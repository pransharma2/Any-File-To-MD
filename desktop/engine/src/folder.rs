//! Folder -> mirrored folder of Markdown, converted in parallel.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, UNIX_EPOCH};

use globset::{Glob, GlobSet, GlobSetBuilder};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::convert::{convert_path, ext_of, is_media, ConvertError};
use crate::formats::archive::demote;

pub const REPORT: &str = "_conversion-report.md";
pub const COMBINED: &str = "_combined.md";
pub const MANIFEST: &str = ".anytomd.json";

const SKIP_DIRS: &[&str] = &[
    ".git", ".svn", ".hg", "node_modules", "__pycache__", ".venv", "venv", ".idea", ".vscode",
    "$RECYCLE.BIN", "System Volume Information", ".Trash", ".DS_Store",
];

// ---------------------------------------------------------------- options

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Options {
    /// Re-runs only convert new or modified files.
    pub skip_unchanged: bool,
    /// Also write `_combined.md`: every document in one file with a table of contents.
    pub combine: bool,
    /// Copy files that can't be converted (images, media, ...) unchanged, so links still work.
    pub copy_unsupported: bool,
    /// Only these extensions (no dot, case-insensitive). Empty = everything.
    pub include: Vec<String>,
    /// Glob patterns to skip, matched against the relative path and the file name (e.g. `*.log`, `drafts/**`).
    pub exclude: Vec<String>,
    /// Relative paths (files or folders, `/`-separated) unticked in the file picker.
    pub deselect: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Options { skip_unchanged: true, combine: false, copy_unsupported: false, include: vec![], exclude: vec![], deselect: vec![] }
    }
}

struct Filter {
    include: Vec<String>,
    exclude: GlobSet,
    deselect: std::collections::HashSet<String>,
}

impl Filter {
    fn new(o: &Options) -> Result<Self, String> {
        let include = o
            .include
            .iter()
            .flat_map(|s| s.split([',', ' ', ';']))
            .map(|s| s.trim().trim_start_matches("*.").trim_start_matches('.').to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        let mut b = GlobSetBuilder::new();
        for pat in o.exclude.iter().flat_map(|s| s.split([',', ';', '\n'])).map(str::trim).filter(|s| !s.is_empty()) {
            let pat = pat.replace('\\', "/");
            let g = Glob::new(&pat).map_err(|e| format!("Bad exclude pattern \"{pat}\": {e}"))?;
            b.add(g);
            // A bare folder name like "drafts" also excludes everything under it.
            if !pat.contains(['*', '?', '[', '/', '.']) {
                b.add(Glob::new(&format!("{pat}/**")).map_err(|e| e.to_string())?);
                b.add(Glob::new(&format!("**/{pat}/**")).map_err(|e| e.to_string())?);
            }
        }
        let deselect = o.deselect.iter().map(|d| d.replace('\\', "/").trim_matches('/').to_lowercase()).filter(|d| !d.is_empty()).collect();
        Ok(Filter { include, exclude: b.build().map_err(|e| e.to_string())?, deselect })
    }

    fn allows(&self, rel: &str) -> bool {
        let name = rel.rsplit('/').next().unwrap_or(rel);
        if self.exclude.is_match(rel) || self.exclude.is_match(name) {
            return false;
        }
        if !self.deselect.is_empty() {
            // The file itself, or any folder above it, was unticked.
            let lower = rel.to_lowercase();
            let mut cut = lower.as_str();
            loop {
                if self.deselect.contains(cut) {
                    return false;
                }
                match cut.rfind('/') {
                    Some(i) => cut = &cut[..i],
                    None => break,
                }
            }
        }
        self.include.is_empty() || self.include.contains(&ext_of(name))
    }
}

// ---------------------------------------------------------------- job state

#[derive(Clone, Debug, Default, Serialize)]
pub struct Failure {
    pub file: String,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Item {
    pub src: String,
    pub out: String,
    /// done | renamed | unchanged | copied | skipped
    pub tag: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct Status {
    /// idle | scanning | running | done | cancelled | error
    pub state: String,
    pub total: usize,
    pub done: usize,
    pub converted: usize,
    pub failed: usize,
    pub unchanged: usize,
    pub copied: usize,
    pub current: String,
    pub output: String,
    pub combined: Option<String>,
    pub error: String,
    pub seconds: f64,
    pub failures: Vec<Failure>,
}

impl Default for Status {
    fn default() -> Self {
        Status {
            state: "idle".into(),
            total: 0,
            done: 0,
            converted: 0,
            failed: 0,
            unchanged: 0,
            copied: 0,
            current: String::new(),
            output: String::new(),
            combined: None,
            error: String::new(),
            seconds: 0.0,
            failures: vec![],
        }
    }
}

#[derive(Default)]
struct Inner {
    status: Status,
    items: Vec<Item>,
    started: Option<Instant>,
    finished: Option<Instant>,
}

/// Shared, thread-safe progress for one conversion run.
#[derive(Default)]
pub struct Job {
    inner: Mutex<Inner>,
    cancel: AtomicBool,
}

impl Job {
    pub fn new() -> Arc<Job> {
        Arc::new(Job::default())
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
    pub fn is_running(&self) -> bool {
        let s = &self.lock().status.state;
        s == "scanning" || s == "running"
    }
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }
    /// Current status plus every processed item from index `since` onward.
    pub fn snapshot(&self, since: usize) -> (Status, Vec<Item>, usize) {
        let g = self.lock();
        let mut s = g.status.clone();
        if let Some(t0) = g.started {
            let end = g.finished.unwrap_or_else(Instant::now);
            s.seconds = ((end - t0).as_secs_f64() * 10.0).round() / 10.0;
        }
        let since = since.min(g.items.len());
        (s, g.items[since..].to_vec(), g.items.len())
    }
    fn update(&self, f: impl FnOnce(&mut Inner)) {
        f(&mut self.lock())
    }
}

// ---------------------------------------------------------------- paths

fn absolute(p: &Path) -> PathBuf {
    let abs = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    // Resolve `.` / `..` lexically.
    let mut out = PathBuf::new();
    for c in abs.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            c => out.push(c),
        }
    }
    out
}

fn key(p: &Path) -> String {
    let s = absolute(p).to_string_lossy().replace('\\', "/");
    let s = s.trim_end_matches('/').to_string();
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s
    }
}

fn is_inside(child: &Path, parent: &Path) -> bool {
    let (c, p) = (key(child), key(parent));
    c.len() > p.len() && c.starts_with(&p) && c.as_bytes()[p.len()] == b'/'
}

fn rel_string(rel: &Path) -> String {
    rel.to_string_lossy().replace('\\', "/")
}

pub fn default_output(source: &Path) -> PathBuf {
    let s = absolute(source);
    let name = s.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "output".into());
    s.parent().unwrap_or(&s).join(format!("{name}-markdown"))
}

/// Return an error message, or None when the pair is safe to use.
pub fn validate(source: &Path, output: &Path) -> Option<String> {
    if !source.is_dir() {
        return Some("That source folder doesn't exist.".into());
    }
    if key(source) == key(output) {
        return Some("The output folder can't be the same as the source folder.".into());
    }
    if is_inside(source, output) {
        return Some("The output folder can't contain the source folder.".into());
    }
    None // output inside source is fine; it is excluded while scanning
}

/// Every file under `source` to convert, sorted case-insensitively by path.
pub fn collect_files(source: &Path, output: &Path, opts: &Options) -> Result<Vec<PathBuf>, String> {
    let filter = Filter::new(opts)?;
    let out_key = key(output);
    let mut files: Vec<PathBuf> = walkdir::WalkDir::new(source)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            if e.depth() == 0 {
                return true;
            }
            if e.file_type().is_dir() {
                let name = e.file_name().to_string_lossy();
                return !SKIP_DIRS.contains(&name.as_ref()) && key(e.path()) != out_key;
            }
            true
        })
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter(|e| !e.file_name().to_string_lossy().starts_with("~$"))
        .filter(|e| {
            let rel = e.path().strip_prefix(source).map(rel_string).unwrap_or_default();
            filter.allows(&rel)
        })
        .map(|e| e.into_path())
        .collect();
    files.sort_by_key(|p| p.strip_prefix(source).map(|r| rel_string(r).to_lowercase()).unwrap_or_default());
    Ok(files)
}

/// One entry of the file picker in the app.
#[derive(Clone, Debug, Serialize)]
pub struct Listed {
    /// Relative path, `/`-separated.
    pub rel: String,
    /// Planned relative output path.
    pub out: String,
    pub size: u64,
    /// doc | media | other (see [`crate::convert::file_kind`]).
    pub kind: &'static str,
}

/// Every file the current options would pick up, ignoring the picker's own `deselect` list.
/// Returns at most `limit` files, and whether there were more.
pub fn list_files(source: &Path, output: &Path, opts: &Options, limit: usize) -> Result<(Vec<Listed>, bool), String> {
    if !source.is_dir() {
        return Err("That source folder doesn't exist.".into());
    }
    let o = Options { deselect: vec![], ..opts.clone() };
    let files = collect_files(source, output, &o)?;
    let truncated = files.len() > limit;
    let files = &files[..files.len().min(limit)];
    Ok((
        plan_names(source, files)
            .into_iter()
            .map(|(src, out, _)| {
                let rel = rel_string(src.strip_prefix(source).unwrap_or(&src));
                Listed {
                    kind: crate::convert::file_kind(&ext_of(&rel)),
                    size: fs::metadata(&src).map(|m| m.len()).unwrap_or(0),
                    out: rel_string(&out),
                    rel,
                }
            })
            .collect(),
        truncated,
    ))
}

/// Map each file to (relative .md path, renamed?).
/// `report.pdf` -> `report.md`; `a.pdf` + `a.docx` side by side -> `a.pdf.md` + `a.docx.md`.
pub fn plan_names(source: &Path, files: &[PathBuf]) -> Vec<(PathBuf, PathBuf, bool)> {
    let mut groups: HashMap<(PathBuf, String), usize> = HashMap::new();
    let rel = |f: &PathBuf| f.strip_prefix(source).map(Path::to_path_buf).unwrap_or_else(|_| f.clone());
    let stem_key = |r: &Path| {
        (
            r.parent().map(Path::to_path_buf).unwrap_or_default(),
            r.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default(),
        )
    };
    for f in files {
        *groups.entry(stem_key(&rel(f))).or_default() += 1;
    }
    files
        .iter()
        .map(|f| {
            let r = rel(f);
            let clash = groups.get(&stem_key(&r)).copied().unwrap_or(0) > 1;
            let name = r.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let out = if clash { r.with_file_name(format!("{name}.md")) } else { r.with_extension("md") };
            (f.clone(), out, clash)
        })
        .collect()
}

// ---------------------------------------------------------------- manifest (skip unchanged)

#[derive(Clone, Serialize, Deserialize, PartialEq)]
struct Entry {
    size: u64,
    mtime: u64,
    out: String,
}

#[derive(Default, Serialize, Deserialize)]
struct Manifest {
    engine: String,
    files: BTreeMap<String, Entry>,
}

fn load_manifest(out: &Path) -> Manifest {
    fs::read(out.join(MANIFEST))
        .ok()
        .and_then(|b| serde_json::from_slice::<Manifest>(&b).ok())
        .filter(|m| m.engine == crate::VERSION)
        .unwrap_or_default()
}

fn fingerprint(p: &Path) -> Option<(u64, u64)> {
    let m = fs::metadata(p).ok()?;
    let mtime = m.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs();
    Some((m.len(), mtime))
}

// ---------------------------------------------------------------- conversion

enum Outcome {
    Converted,
    Unchanged,
    Copied(String),
    Failed(String),
}

fn write_atomic(dest: &Path, text: &str) -> std::io::Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(dest, text.as_bytes())
}

fn copy_into(src: &Path, out_root: &Path, rel: &Path) -> Result<String, String> {
    let dest = out_root.join(rel);
    if let Some(p) = dest.parent() {
        fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    fs::copy(src, &dest).map_err(|e| format!("Could not copy: {e}"))?;
    Ok(rel_string(rel))
}

/// Convert a whole folder. Blocks until finished; progress is visible through `job`.
pub fn convert_folder(source: &Path, output: &Path, opts: &Options, job: &Job) {
    let source = absolute(source);
    let output = absolute(output);
    job.update(|g| {
        *g = Inner::default();
        g.started = Some(Instant::now());
        g.status.state = "scanning".into();
        g.status.output = output.to_string_lossy().into_owned();
    });
    let fail = |msg: String| {
        job.update(|g| {
            g.status.state = "error".into();
            g.status.error = msg;
            g.finished = Some(Instant::now());
        })
    };
    if let Some(e) = validate(&source, &output) {
        return fail(e);
    }
    let files = match collect_files(&source, &output, opts) {
        Ok(f) => f,
        Err(e) => return fail(e),
    };
    let plan = plan_names(&source, &files);
    if let Err(e) = fs::create_dir_all(&output) {
        return fail(format!("Could not create the output folder: {e}"));
    }
    let old = if opts.skip_unchanged { load_manifest(&output) } else { Manifest::default() };
    job.update(|g| {
        g.status.total = plan.len();
        g.status.state = "running".into();
    });

    let results: Vec<Option<(String, Outcome, String, Option<(u64, u64)>)>> = plan
        .par_iter()
        .map(|(src, out_rel, renamed)| {
            if job.is_cancelled() {
                return None;
            }
            let rel = rel_string(src.strip_prefix(&source).unwrap_or(src));
            let out_s = rel_string(out_rel);
            job.update(|g| g.status.current = rel.clone());
            let fp = fingerprint(src);
            let ext = ext_of(&rel);

            let outcome = if opts.skip_unchanged
                && fp.is_some_and(|(size, mtime)| {
                    old.files.get(&rel).is_some_and(|e| e.size == size && e.mtime == mtime && output.join(&e.out).is_file())
                }) {
                Outcome::Unchanged
            } else if opts.copy_unsupported && is_media(&ext) {
                match copy_into(src, &output, src.strip_prefix(&source).unwrap_or(src)) {
                    Ok(p) => Outcome::Copied(p),
                    Err(e) => Outcome::Failed(e),
                }
            } else {
                match convert_path(src) {
                    Ok(md) => match write_atomic(&output.join(out_rel), &md) {
                        Ok(()) => Outcome::Converted,
                        Err(e) => Outcome::Failed(format!("Could not write output: {e}")),
                    },
                    Err(ConvertError::Unsupported(m)) if opts.copy_unsupported => {
                        let _ = m;
                        match copy_into(src, &output, src.strip_prefix(&source).unwrap_or(src)) {
                            Ok(p) => Outcome::Copied(p),
                            Err(e) => Outcome::Failed(e),
                        }
                    }
                    Err(e) => Outcome::Failed(e.to_string()),
                }
            };

            let (tag, shown_out) = match &outcome {
                Outcome::Converted => (if *renamed { "renamed" } else { "done" }, out_s.clone()),
                Outcome::Unchanged => ("unchanged", old.files.get(&rel).map(|e| e.out.clone()).unwrap_or(out_s.clone())),
                Outcome::Copied(p) => ("copied", p.clone()),
                Outcome::Failed(_) => ("skipped", "not converted".to_string()),
            };
            job.update(|g| {
                let s = &mut g.status;
                s.done += 1;
                match &outcome {
                    Outcome::Converted => s.converted += 1,
                    Outcome::Unchanged => s.unchanged += 1,
                    Outcome::Copied(_) => s.copied += 1,
                    Outcome::Failed(reason) => {
                        s.failed += 1;
                        s.failures.push(Failure { file: rel.clone(), reason: reason.clone() });
                    }
                }
                g.items.push(Item { src: rel.clone(), out: shown_out.clone(), tag });
            });
            Some((rel, outcome, shown_out, fp))
        })
        .collect();

    // Manifest: keep entries for files we didn't touch this run (e.g. after Cancel).
    let cancelled = job.is_cancelled();
    let mut manifest = Manifest { engine: crate::VERSION.into(), files: BTreeMap::new() };
    let in_plan: std::collections::HashSet<String> =
        plan.iter().map(|(s, _, _)| rel_string(s.strip_prefix(&source).unwrap_or(s))).collect();
    for (k, v) in old.files {
        if in_plan.contains(&k) {
            manifest.files.insert(k, v);
        }
    }
    let mut combined_order: Vec<(String, String)> = Vec::new();
    for r in results.into_iter().flatten() {
        let (rel, outcome, out, fp) = r;
        match outcome {
            Outcome::Converted | Outcome::Unchanged => {
                if let Some((size, mtime)) = fp {
                    manifest.files.insert(rel.clone(), Entry { size, mtime, out: out.clone() });
                }
                combined_order.push((rel, out));
            }
            Outcome::Copied(_) => {
                manifest.files.remove(&rel);
            }
            Outcome::Failed(_) => {
                manifest.files.remove(&rel);
            }
        }
    }
    if let Ok(json) = serde_json::to_vec_pretty(&manifest) {
        let _ = fs::write(output.join(MANIFEST), json);
    }

    let failures = job.snapshot(usize::MAX).0.failures;
    write_report(&output, &failures);

    let mut combined = None;
    if opts.combine && !cancelled && !combined_order.is_empty() {
        let title = source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if write_combined(&output, &title, &combined_order).is_ok() {
            combined = Some(output.join(COMBINED).to_string_lossy().into_owned());
        }
    }

    job.update(|g| {
        g.status.state = if cancelled { "cancelled" } else { "done" }.into();
        g.status.current.clear();
        g.status.combined = combined;
        g.status.failures.sort_by(|a, b| a.file.to_lowercase().cmp(&b.file.to_lowercase()));
        g.finished = Some(Instant::now());
    });
}

fn write_report(output: &Path, failures: &[Failure]) {
    let path = output.join(REPORT);
    if failures.is_empty() {
        let _ = fs::remove_file(path); // stale report from an earlier run
        return;
    }
    let mut f: Vec<&Failure> = failures.iter().collect();
    f.sort_by_key(|x| x.file.to_lowercase());
    let mut s = format!("# Conversion report\n\n{} file(s) could not be converted:\n\n", f.len());
    for x in f {
        s.push_str(&format!("- `{}`: {}\n", x.file, x.reason));
    }
    let _ = fs::write(path, s);
}

/// GitHub-style heading anchor.
fn slug(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter_map(|c| match c {
            c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
            ' ' => Some('-'),
            _ => None,
        })
        .collect()
}

fn write_combined(output: &Path, title: &str, docs: &[(String, String)]) -> std::io::Result<()> {
    let mut docs = docs.to_vec();
    docs.sort_by_key(|(src, _)| src.to_lowercase());
    let mut s = format!("# {title}\n\nAll {} documents from `{title}` in one file, generated by Any File To MD.\n\n## Contents\n\n", docs.len());
    for (src, _) in &docs {
        s.push_str(&format!("- [{src}](#{})\n", slug(src)));
    }
    for (src, out) in &docs {
        let body = fs::read_to_string(output.join(out)).unwrap_or_default();
        s.push_str(&format!("\n---\n\n## {src}\n\n{}\n", demote(body.trim())));
    }
    fs::write(output.join(COMBINED), s)
}

/// Convert one file. Default output: `name.md` beside it (`name.ext.md` if that would be the input).
pub fn convert_single_file(path: &Path, output: Option<&Path>) -> Result<PathBuf, String> {
    if !path.is_file() {
        return Err("That file doesn't exist.".into());
    }
    let dest = match output {
        Some(o) if o.is_dir() => o.join(path.with_extension("md").file_name().unwrap_or_default()),
        Some(o) => o.to_path_buf(),
        None => path.with_extension("md"),
    };
    let dest = if key(&dest) == key(path) {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        path.with_file_name(format!("{name}.md"))
    } else {
        dest
    };
    let md = convert_path(path).map_err(|e| e.to_string())?;
    write_atomic(&dest, &md).map_err(|e| format!("Could not write output: {e}"))?;
    Ok(dest)
}
