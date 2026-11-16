/* Shared by the desktop app (desktop/app/ui) and the website (web/):
   option presets, the options dialog body, and the file picker in the mirror preview. */
(function (g) {
"use strict";
const esc = s => String(s).replace(/[&<>"]/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));

/* ---------- presets ---------- */
const CODE = "py js mjs cjs ts jsx tsx java c h cpp cc hpp cxx cs go rs rb php swift kt kts scala sh bash zsh bat cmd ps1 psm1 sql yml yaml toml ini cfg conf css scss sass less lua r dart vue svelte graphql gql dockerfile tex log env";
const TYPE_GROUPS = [
  { key: "pdf", label: "PDF", exts: "pdf" },
  { key: "word", label: "Word", exts: "docx docm dotx odt" },
  { key: "slides", label: "PowerPoint", exts: "pptx pptm ppsx" },
  { key: "sheets", label: "Excel & CSV", exts: "xlsx xlsm xlsb xls ods csv tsv" },
  { key: "web", label: "Web pages", exts: "html htm xhtml" },
  { key: "text", label: "Text & Markdown", exts: "txt text md markdown rst adoc org" },
  { key: "ebooks", label: "E-books", exts: "epub" },
  { key: "email", label: "Outlook e-mail", exts: "msg" },
  { key: "data", label: "JSON & XML", exts: "json jsonl geojson xml rss atom plist ipynb" },
  { key: "code", label: "Code & config", exts: CODE },
  { key: "zip", label: "ZIP archives", exts: "zip" },
  { key: "images", label: "Images", exts: "jpg jpeg png gif bmp webp tif tiff heic svg" },
].map(t => ({ ...t, exts: t.exts.split(" ") }));

const SKIP_PRESETS = [
  { key: "hidden", label: "Hidden files & folders", hint: ".git, .env, …", pats: [".*", ".*/**", "**/.*/**"] },
  { key: "build", label: "Build & dependency folders", hint: "node_modules, dist, build, bin, obj, target", pats: ["node_modules", "dist", "build", "bin", "obj", "target", "vendor"] },
  { key: "temp", label: "Temp & log files", hint: "*.tmp, *.bak, *.log, Thumbs.db", pats: ["*.tmp", "*.temp", "*.bak", "*.log", "*~", "Thumbs.db", "desktop.ini"] },
  { key: "backup", label: "Archive & backup folders", hint: "archive, old, backup", pats: ["archive", "archives", "old", "backup", "backups"] },
];

const DEFAULTS = { skipUnchanged: true, combine: false, copyUnsupported: false, types: [], skips: [], custom: "" };

/** Load saved options, upgrading the 1.0 format (free-text include/exclude). */
function load(key) {
  let o = {};
  try { o = JSON.parse(localStorage.getItem(key) || "{}") || {}; } catch {}
  if (Array.isArray(o.include) && !o.types) {
    const want = new Set(o.include.map(s => String(s).toLowerCase().replace(/^\*?\./, "")));
    o.types = TYPE_GROUPS.filter(t => t.exts.some(e => want.has(e))).map(t => t.key);
  }
  if (Array.isArray(o.exclude) && o.custom === undefined) o.custom = o.exclude.join(", ");
  const out = { ...DEFAULTS };
  for (const k of Object.keys(DEFAULTS)) if (o[k] !== undefined) out[k] = o[k];
  return out;
}
function save(key, o) { try { localStorage.setItem(key, JSON.stringify(o)); } catch {} }

const splitList = v => String(v || "").split(/[,;\n]/).map(s => s.trim()).filter(Boolean);

/** What the engine expects (`Options` in desktop/engine/src/folder.rs). */
function engineOptions(o, deselect) {
  const include = [...new Set(TYPE_GROUPS.filter(t => o.types.includes(t.key)).flatMap(t => t.exts))];
  const exclude = [...SKIP_PRESETS.filter(p => o.skips.includes(p.key)).flatMap(p => p.pats), ...splitList(o.custom)];
  return { skipUnchanged: o.skipUnchanged, combine: o.combine, copyUnsupported: o.copyUnsupported, include, exclude, deselect: deselect || [] };
}

function summary(o, { skipUnchanged = true } = {}) {
  const parts = [];
  if (skipUnchanged) parts.push(o.skipUnchanged ? "Skip unchanged" : "Re-convert all");
  parts.push(o.types.length ? "Only " + TYPE_GROUPS.filter(t => o.types.includes(t.key)).map(t => t.label).join(", ") : "All file types");
  if (o.combine) parts.push("Combined file");
  if (o.copyUnsupported) parts.push("Copy others");
  const skips = SKIP_PRESETS.filter(p => o.skips.includes(p.key)).length + splitList(o.custom).length;
  if (skips) parts.push(`${skips} skip rule${skips === 1 ? "" : "s"}`);
  return parts.join(" · ");
}

/* ---------- options dialog body ---------- */
function check(id, on, title, text) {
  return `<label class="check"><input type="checkbox" id="${id}"${on ? " checked" : ""}><div><b>${title}</b><span>${text}</span></div></label>`;
}
function renderOptions(el, o, { skipUnchanged = true } = {}) {
  const chip = (attr, key, label, on, title) => `<button type="button" class="chip${on ? " on" : ""}" ${attr}="${key}"${title ? ` title="${esc(title)}"` : ""}>${esc(label)}</button>`;
  el.innerHTML =
    (skipUnchanged ? check("oSkip", o.skipUnchanged, "Skip unchanged files", "Re-runs only convert new or edited files, so re-syncing a big folder takes seconds.") : "") +
    check("oCombine", o.combine, "Also create one combined file", "Adds <code>_combined.md</code> with every document and a table of contents. Handy for pasting into an AI chat.") +
    check("oCopy", o.copyUnsupported, "Copy files that can't be converted", "Images, media and unknown files are copied as-is instead of skipped, so the output is a complete mirror.") +
    `<div class="dfield"><label>File types</label><div class="chips" id="oTypes">` +
      chip("data-type", "", "All types", !o.types.length) +
      TYPE_GROUPS.map(t => chip("data-type", t.key, t.label, o.types.includes(t.key), "." + t.exts.slice(0, 8).join(" .") + (t.exts.length > 8 ? " …" : ""))).join("") +
    `</div></div>` +
    `<div class="dfield" style="border-bottom:0"><label>Skip</label><div class="chips" id="oSkips">` +
      SKIP_PRESETS.map(p => chip("data-skip", p.key, p.label, o.skips.includes(p.key), p.hint)).join("") +
    `</div><input class="input" id="oCustom" placeholder="Other folders or patterns, e.g. drafts, *.psd" spellcheck="false" value="${esc(o.custom)}"></div>`;

  el.querySelector("#oTypes").onclick = e => {
    const b = e.target.closest(".chip"); if (!b) return;
    const all = el.querySelector('#oTypes [data-type=""]');
    if (!b.dataset.type) { el.querySelectorAll("#oTypes .chip").forEach(c => c.classList.toggle("on", c === all)); return; }
    b.classList.toggle("on");
    const any = el.querySelector('#oTypes .chip.on:not([data-type=""])');
    all.classList.toggle("on", !any);
  };
  el.querySelector("#oSkips").onclick = e => { const b = e.target.closest(".chip"); if (b) b.classList.toggle("on"); };
}
function readOptions(el, prev) {
  const q = s => el.querySelector(s);
  return {
    skipUnchanged: q("#oSkip") ? q("#oSkip").checked : prev.skipUnchanged,
    combine: q("#oCombine").checked,
    copyUnsupported: q("#oCopy").checked,
    types: [...el.querySelectorAll("#oTypes .chip.on")].map(c => c.dataset.type).filter(Boolean),
    skips: [...el.querySelectorAll("#oSkips .chip.on")].map(c => c.dataset.skip),
    custom: splitList(q("#oCustom").value).join(", "),
  };
}

/* ---------- file picker ---------- */
const fmtSize = n => n < 1024 ? `${n} B` : n < 1048576 ? `${(n / 1024).toFixed(0)} KB` : `${(n / 1048576).toFixed(1)} MB`;
const KIND_TAG = { media: ["info", "Only dimensions and metadata are extracted"], other: ["maybe", "Converted if it turns out to be text"] };

class Picker {
  /** files: [{rel, out, size, kind}] */
  constructor(el, onChange) {
    this.el = el; this.onChange = onChange || (() => {});
    this.root = null; this.files = []; this.byPath = new Map();
    el.addEventListener("click", e => this.click(e));
    el.addEventListener("keydown", e => {
      if ((e.key === " " || e.key === "Enter") && e.target.dataset.p !== undefined) { e.preventDefault(); this.click(e, e.key === " "); }
    });
  }
  setFiles(files, keepFrom) {
    const off = new Set(keepFrom ? keepFrom.files.filter(f => !f.on).map(f => f.rel) : []);
    const open = new Set(keepFrom ? [...keepFrom.byPath.values()].filter(n => n.dir && n.open).map(n => n.path) : []);
    this.files = files.map(f => ({ ...f, on: !off.has(f.rel) }));
    this.root = { dir: true, name: "", path: "", depth: -1, open: true, dirs: new Map(), list: [] };
    this.byPath = new Map([["", this.root]]);
    for (const f of this.files) {
      const parts = f.rel.split("/");
      let node = this.root;
      for (let i = 0; i < parts.length - 1; i++) {
        const path = parts.slice(0, i + 1).join("/");
        let d = node.dirs.get(parts[i].toLowerCase());
        if (!d) {
          d = { dir: true, name: parts[i], path, depth: i, open: open.has(path), dirs: new Map(), list: [] };
          node.dirs.set(parts[i].toLowerCase(), d); this.byPath.set(path, d);
        }
        node = d;
      }
      f.name = parts[parts.length - 1]; f.depth = parts.length - 1; f.path = f.rel;
      node.list.push(f); this.byPath.set(f.rel, f);
    }
    // Small trees open fully; big ones open the top level only.
    if (!keepFrom && files.length <= 60) for (const n of this.byPath.values()) if (n.dir) n.open = true;
    this.tally(this.root); this.render(); this.onChange();
  }
  tally(n) {
    n.total = n.list.length; n.on = n.list.filter(f => f.on).length;
    for (const d of n.dirs.values()) { this.tally(d); n.total += d.total; n.on += d.on; }
  }
  sortedDirs(n) { return [...n.dirs.values()].sort((a, b) => a.name.toLowerCase() < b.name.toLowerCase() ? -1 : 1); }
  each(n, fn) { for (const d of n.dirs.values()) this.each(d, fn); n.list.forEach(fn); }
  setNode(n, on) { if (n.dir) this.each(n, f => { f.on = on; }); else n.on = on; }
  selectAll(on) { this.setNode(this.root, on); this.tally(this.root); this.render(); this.onChange(); }
  expandAll(open) { for (const n of this.byPath.values()) if (n.dir && n !== this.root) n.open = open; this.render(); }
  get selected() { return this.root ? this.root.on : 0; }
  get total() { return this.root ? this.root.total : 0; }
  /** The smallest list of relative paths that are unticked: whole folders where possible. */
  deselect() {
    const out = [];
    const walk = n => {
      for (const d of n.dirs.values()) { if (d.on === 0) out.push(d.path); else if (d.on < d.total) walk(d); }
      for (const f of n.list) if (!f.on) out.push(f.rel);
    };
    if (this.root) walk(this.root);
    return out;
  }
  selectedFiles() { return this.files.filter(f => f.on); }
  click(e, toggleOnly) {
    const row = e.target.closest("[data-p]"); if (!row || this.el.classList.contains("locked")) return;
    const n = this.byPath.get(row.dataset.p); if (!n) return;
    if (n.dir && !toggleOnly && !e.target.closest(".cb")) { n.open = !n.open; this.render(); return; }
    this.setNode(n, n.dir ? n.on < n.total : !n.on);
    this.tally(this.root); this.render(); this.onChange();
  }
  row(n) {
    const pad = `padding-left:${n.depth * 16}px`;
    const state = n.dir ? (n.on === 0 ? "" : n.on < n.total ? " mixed" : " on") : (n.on ? " on" : "");
    const offCls = (n.dir ? n.on === 0 : !n.on) ? " off" : "";
    const label = n.dir ? (n.open ? "Collapse " : "Expand ") + n.name : n.name;
    if (n.dir) {
      return `<div class="trow prow dir${offCls}" data-p="${esc(n.path)}" tabindex="0" title="${esc(label)}">` +
        `<span class="pname" style="${pad}"><span class="caret">${n.open ? "▾" : "▸"}</span><span class="cb${state}"></span><span class="ell">${esc(n.name)}/</span></span>` +
        `<span class="a">→</span><span class="ell pout">${esc(n.name)}/</span><span class="pcount">${n.on === n.total ? n.total : `${n.on}/${n.total}`}</span></div>`;
    }
    const tag = KIND_TAG[n.kind];
    const outName = n.out.split("/").pop();
    return `<div class="trow prow${offCls}" data-p="${esc(n.path)}" tabindex="0" title="${esc(n.rel)} · ${fmtSize(n.size || 0)}">` +
      `<span class="pname" style="${pad}"><span class="caret"></span><span class="cb${state}"></span><span class="ell s">${esc(n.name)}</span></span>` +
      `<span class="a">→</span><span class="ell pout">${n.on ? esc(outName) : "not converted"}</span>` +
      (tag ? `<span class="tag ${tag[0]}" title="${tag[1]}">${tag[0]}</span>` : "<span></span>") + `</div>`;
  }
  render() {
    if (!this.root) { this.el.innerHTML = ""; return; }
    const html = [];
    const walk = n => {
      for (const d of this.sortedDirs(n)) { html.push(this.row(d)); if (d.open) walk(d); }
      for (const f of n.list) html.push(this.row(f));
    };
    walk(this.root);
    const top = this.el.scrollTop;
    this.el.innerHTML = html.join("");
    this.el.scrollTop = top;
  }
}

/* ---------- paths & filters (used by the website; the desktop engine does this in Rust) ---------- */
const SKIP_DIRS = new Set([".git", ".svn", ".hg", "node_modules", "__pycache__", ".venv", "venv", ".idea", ".vscode", "$RECYCLE.BIN", "System Volume Information", ".Trash"]);
function extOf(name) {
  const base = name.split("/").pop();
  if (base.toLowerCase() === "dockerfile") return "dockerfile";
  const i = base.lastIndexOf(".");
  return i > 0 ? base.slice(i + 1).toLowerCase() : "";
}
function globRe(pat) {
  let re = "";
  for (let i = 0; i < pat.length; i++) {
    const c = pat[i];
    if (c === "*") { re += ".*"; if (pat[i + 1] === "*") i++; }
    else if (c === "?") re += ".";
    else if (c === "[") { const j = pat.indexOf("]", i); if (j > i) { re += pat.slice(i, j + 1); i = j; } else re += "\\["; }
    else re += c.replace(/[.+^${}()|\\/]/g, "\\$&");
  }
  return new RegExp("^" + re + "$", "i");
}
/** Mirrors `Filter` in desktop/engine/src/folder.rs. */
function makeFilter(eo) {
  const include = new Set(eo.include);
  const res = [];
  for (const raw of eo.exclude) {
    const pat = raw.replace(/\\/g, "/");
    res.push(globRe(pat));
    if (!/[*?\[\/.]/.test(pat)) { res.push(globRe(pat + "/**")); res.push(globRe("**/" + pat + "/**")); }
  }
  return rel => {
    const name = rel.split("/").pop();
    if (name.startsWith("~$")) return false;
    if (res.some(r => r.test(rel) || r.test(name))) return false;
    return !include.size || include.has(extOf(name));
  };
}
/** Mirrors `plan_names`: `a.pdf` + `a.docx` side by side become `a.pdf.md` + `a.docx.md`. */
function planNames(rels) {
  const key = r => { const i = r.lastIndexOf("/"); const dir = r.slice(0, i + 1); const n = r.slice(i + 1); const d = n.lastIndexOf("."); return (dir + (d > 0 ? n.slice(0, d) : n)).toLowerCase(); };
  const count = new Map();
  for (const r of rels) count.set(key(r), (count.get(key(r)) || 0) + 1);
  return rels.map(r => {
    const clash = count.get(key(r)) > 1;
    const i = r.lastIndexOf("/"); const n = r.slice(i + 1); const d = n.lastIndexOf(".");
    return { out: clash || d <= 0 ? r + ".md" : r.slice(0, i + 1) + n.slice(0, d) + ".md", renamed: clash };
  });
}

g.AnyToMd = { esc, TYPE_GROUPS, SKIP_PRESETS, DEFAULTS, load, save, engineOptions, summary, renderOptions, readOptions, Picker, fmtSize, SKIP_DIRS, extOf, makeFilter, planNames };
})(window);
