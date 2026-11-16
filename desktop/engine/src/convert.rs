//! Single-file conversion: picks a converter by extension and returns Markdown text.

use std::fmt;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;

use crate::formats;

#[derive(Debug)]
pub enum ConvertError {
    /// No converter for this kind of file (e.g. an executable or a video).
    Unsupported(String),
    /// The file is recognised but could not be read.
    Failed(String),
}

impl fmt::Display for ConvertError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConvertError::Unsupported(m) => write!(f, "Unsupported: {m}"),
            ConvertError::Failed(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for ConvertError {}

impl From<anyhow::Error> for ConvertError {
    fn from(e: anyhow::Error) -> Self {
        ConvertError::Failed(first_line(&format!("{e:#}")))
    }
}

pub type Result<T> = std::result::Result<T, ConvertError>;

fn first_line(s: &str) -> String {
    let s = s.trim().lines().next().unwrap_or("").to_string();
    if s.chars().count() > 240 {
        s.chars().take(240).collect()
    } else {
        s
    }
}

/// Markdown-ish text copied through unchanged.
const PASSTHROUGH: &[&str] = &["md", "markdown", "txt", "text", "rst", "adoc", "org"];

/// Images, audio and video: no text to extract (images give metadata).
const MEDIA: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "bmp", "webp", "tif", "tiff", "heic", "svg", "ico", "mp3", "wav",
    "m4a", "flac", "ogg", "aac", "mp4", "mov", "avi", "mkv", "webm", "wmv",
];

pub fn is_media(ext: &str) -> bool {
    MEDIA.contains(&ext)
}

pub fn code_lang(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "py" => "python",
        "js" | "mjs" | "cjs" => "javascript",
        "ts" => "typescript",
        "jsx" => "jsx",
        "tsx" => "tsx",
        "java" => "java",
        "c" | "h" => "c",
        "cpp" | "cc" | "hpp" | "cxx" => "cpp",
        "cs" => "csharp",
        "go" => "go",
        "rs" => "rust",
        "rb" => "ruby",
        "php" => "php",
        "swift" => "swift",
        "kt" | "kts" => "kotlin",
        "scala" => "scala",
        "sh" | "bash" | "zsh" => "bash",
        "bat" | "cmd" => "bat",
        "ps1" | "psm1" => "powershell",
        "sql" => "sql",
        "yml" | "yaml" => "yaml",
        "toml" => "toml",
        "ini" | "cfg" | "conf" => "ini",
        "css" => "css",
        "scss" | "sass" => "scss",
        "less" => "less",
        "lua" => "lua",
        "r" => "r",
        "dart" => "dart",
        "vue" => "vue",
        "svelte" => "svelte",
        "graphql" | "gql" => "graphql",
        "dockerfile" => "dockerfile",
        "tex" => "latex",
        "log" | "env" => "",
        _ => return None,
    })
}

pub fn ext_of(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    if base.eq_ignore_ascii_case("dockerfile") {
        return "dockerfile".into();
    }
    match base.rfind('.') {
        Some(i) if i > 0 => base[i + 1..].to_ascii_lowercase(),
        _ => String::new(),
    }
}

/// Convert a file on disk.
pub fn convert_path(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).map_err(|e| ConvertError::Failed(format!("Could not read file: {e}")))?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    convert_bytes(&name, &bytes)
}

/// Convert in-memory bytes. `name` is used for the extension and for headings.
pub fn convert_bytes(name: &str, bytes: &[u8]) -> Result<String> {
    convert_depth(name, bytes, 0)
}

pub(crate) fn convert_depth(name: &str, bytes: &[u8], depth: u8) -> Result<String> {
    // Third-party parsers can panic on malformed input; turn that into a normal failure.
    match catch_unwind(AssertUnwindSafe(|| dispatch(name, bytes, depth))) {
        Ok(r) => r,
        Err(p) => {
            let msg = p
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| p.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "parser crashed".into());
            Err(ConvertError::Failed(format!("File appears to be damaged ({})", first_line(&msg))))
        }
    }
}

fn dispatch(name: &str, bytes: &[u8], depth: u8) -> Result<String> {
    let ext = ext_of(name);
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    if bytes.is_empty() {
        return Err(ConvertError::Failed("File is empty".into()));
    }
    let out = match ext.as_str() {
        e if PASSTHROUGH.contains(&e) => formats::text::decode(bytes),
        "pdf" => formats::pdf::convert(bytes)?,
        "docx" | "docm" | "dotx" => formats::docx::convert(bytes)?,
        "pptx" | "pptm" | "ppsx" => formats::pptx::convert(bytes)?,
        "xlsx" | "xlsm" | "xlsb" | "xls" | "ods" => formats::sheet::convert(bytes)?,
        "odt" => formats::odt::convert(bytes)?,
        "csv" => formats::sheet::csv_to_md(bytes, b',')?,
        "tsv" => formats::sheet::csv_to_md(bytes, b'\t')?,
        "html" | "htm" | "xhtml" => formats::html::convert(&formats::text::decode(bytes))?,
        "epub" => formats::epub::convert(bytes)?,
        "zip" => formats::archive::convert(base, bytes, depth)?,
        "msg" => formats::msg::convert(bytes)?,
        "ipynb" => formats::text::ipynb(bytes)?,
        "json" | "jsonl" | "geojson" => formats::text::fenced(base, "json", bytes),
        "xml" | "rss" | "atom" | "plist" => formats::text::fenced(base, "xml", bytes),
        e if is_media(e) => formats::image::convert(base, &ext, bytes)?,
        e => match code_lang(e) {
            Some(lang) => formats::text::fenced(base, lang, bytes),
            None => {
                if looks_binary(bytes) {
                    return Err(ConvertError::Unsupported("binary format with no converter".into()));
                }
                formats::text::fenced(base, "", bytes)
            }
        },
    };
    let out = formats::text::tidy(&out);
    if out.trim().is_empty() {
        return Err(ConvertError::Failed("No text could be extracted".into()));
    }
    Ok(out)
}

pub fn looks_binary(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(4096)];
    if head.starts_with(&[0xFF, 0xFE]) || head.starts_with(&[0xFE, 0xFF]) {
        return false; // UTF-16 text
    }
    head.contains(&0)
}
