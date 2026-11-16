// Any File To MD in the browser: files are read and converted on this device and never uploaded.
"use strict";
const A = window.AnyToMd;
const $ = id => document.getElementById(id);
const esc = A.esc;
const OPTS_KEY = "anytomd.web.options";
const MAX_BYTES = 300 * 1024 * 1024;
const MEDIA = new Set("jpg jpeg png gif bmp webp tif tiff heic svg ico mp3 wav m4a flac ogg aac mp4 mov avi mkv webm wmv".split(" "));
const DOCS = new Set(A.TYPE_GROUPS.filter(t => t.key !== "images").flatMap(t => t.exts));
const kindOf = rel => { const e = A.extOf(rel); return MEDIA.has(e) ? "media" : DOCS.has(e) ? "doc" : "other"; };

let opts = A.load(OPTS_KEY);
const st = { view: "setup", name: "", entries: [], s: null, rows: 0, cancel: false, reportOpen: true, output: null };

/* ---------- reading a folder ---------- */
// entry: { rel, size, file: () => Promise<File> }
async function fromDirectoryHandle(dir) {
  const out = [];
  async function walk(h, prefix) {
    for await (const [name, child] of h.entries()) {
      if (child.kind === "directory") { if (!A.SKIP_DIRS.has(name)) await walk(child, prefix + name + "/"); }
      else { const f = await child.getFile(); out.push({ rel: prefix + name, size: f.size, file: async () => f }); }
    }
  }
  await walk(dir, "");
  return out;
}
function readAll(reader) {
  return new Promise((res, rej) => { const all = []; const next = () => reader.readEntries(b => { if (!b.length) res(all); else { all.push(...b); next(); } }, rej); next(); });
}
async function fromEntries(items) {
  const out = [];
  const fileOf = e => new Promise((res, rej) => e.file(res, rej));
  async function walk(e, prefix) {
    if (e.isDirectory) {
      if (A.SKIP_DIRS.has(e.name)) return;
      for (const c of await readAll(e.createReader())) await walk(c, prefix + e.name + "/");
    } else {
      const f = await fileOf(e); out.push({ rel: prefix + e.name, size: f.size, file: async () => f });
    }
  }
  if (items.length === 1 && items[0].isDirectory) {
    for (const c of await readAll(items[0].createReader())) await walk(c, "");
    return { name: items[0].name, entries: out };
  }
  for (const e of items) await walk(e, "");
  return { name: "files", entries: out };
}
function fromFileList(files) {
  const list = [...files];
  const first = (list[0] && list[0].webkitRelativePath) || "";
  const name = first.includes("/") ? first.split("/")[0] : "files";
  const entries = list
    .map(f => ({ rel: (f.webkitRelativePath || f.name).split("/").slice(first.includes("/") ? 1 : 0).join("/"), size: f.size, file: async () => f }))
    .filter(e => !e.rel.split("/").slice(0, -1).some(d => A.SKIP_DIRS.has(d)));
  return { name, entries };
}

async function browse() {
  if (window.showDirectoryPicker) {
    let dir;
    try { dir = await window.showDirectoryPicker({ id: "anytomd-src", mode: "read" }); } catch (e) { if (e.name !== "AbortError") setErr(String(e.message || e)); return; }
    await setSource(dir.name, () => fromDirectoryHandle(dir));
  } else $("folderInput").click();
}
$("folderInput").onchange = async e => { const f = e.target.files; if (f && f.length) { const r = fromFileList(f); await setSource(r.name, async () => r.entries); } e.target.value = ""; };

async function setSource(name, load) {
  if (locked()) return;
  setErr("");
  $("tnote").textContent = "Reading your folder…";
  let entries;
  try { entries = await load(); } catch (e) { return setErr("Couldn't read that folder: " + (e.message || e)); }
  st.name = name; st.entries = entries; st.view = "setup";
  $("srcCard").hidden = false; $("drop").hidden = true;
  $("srcRoot").textContent = name; $("srcPath").textContent = `${entries.length} file${entries.length === 1 ? "" : "s"}`;
  relist(false); render();
}

/* drag & drop anywhere */
let dragDepth = 0;
document.addEventListener("dragenter", e => { e.preventDefault(); if (++dragDepth === 1) { $("app").classList.add("dragging"); $("drop").classList.add("over"); } });
document.addEventListener("dragleave", () => { if (--dragDepth <= 0) { dragDepth = 0; $("app").classList.remove("dragging"); $("drop").classList.remove("over"); } });
document.addEventListener("dragover", e => e.preventDefault());
document.addEventListener("drop", async e => {
  e.preventDefault(); dragDepth = 0; $("app").classList.remove("dragging"); $("drop").classList.remove("over");
  const items = [...(e.dataTransfer.items || [])].map(i => i.webkitGetAsEntry && i.webkitGetAsEntry()).filter(Boolean);
  if (items.length) { const r = await fromEntries(items).catch(err => (setErr("Couldn't read that: " + (err.message || err)), null)); if (r) await setSource(r.name, async () => r.entries); }
  else if (e.dataTransfer.files.length) { const r = fromFileList(e.dataTransfer.files); await setSource(r.name, async () => r.entries); }
});

/* ---------- picker + options ---------- */
const picker = new A.Picker($("plist"), renderPicker);
function relist(keep) {
  if (!st.entries.length && !st.name) return;
  const allow = A.makeFilter(A.engineOptions(opts));
  const kept = st.entries.filter(e => allow(e.rel)).sort((a, b) => a.rel.toLowerCase() < b.rel.toLowerCase() ? -1 : 1);
  const plan = A.planNames(kept.map(e => e.rel));
  picker.setFiles(kept.map((e, i) => ({ rel: e.rel, out: plan[i].out, size: e.size, kind: kindOf(e.rel) })), keep && picker.root ? picker : null);
}
function renderPicker() {
  const show = !!picker.root && st.view === "setup";
  $("plist").hidden = !show; $("ptool").hidden = !show; $("tlist").hidden = show;
  if (!show) return;
  const n = picker.total, k = picker.selected;
  $("pcount").textContent = n ? `${k} of ${n} file${n === 1 ? "" : "s"} selected` : "No files match your options";
  $("tnote").textContent = "Click a folder to open it. Untick anything you don't want converted.";
  const dirs = [...picker.byPath.values()].filter(d => d.dir && d.depth >= 0);
  $("pExpand").hidden = !dirs.length;
  $("pExpand").textContent = dirs.some(d => !d.open) ? "Expand all" : "Collapse all";
}
$("pAll").onclick = () => picker.selectAll(true);
$("pNone").onclick = () => picker.selectAll(false);
$("pExpand").onclick = () => { picker.expandAll($("pExpand").textContent === "Expand all"); renderPicker(); };

const optsSummary = () => { $("optsSum").textContent = A.summary(opts, { skipUnchanged: false }); };
$("optsBtn").onclick = () => { A.renderOptions($("optsBody"), opts, { skipUnchanged: false }); $("optsDlg").hidden = false; };
$("oCancel").onclick = () => { $("optsDlg").hidden = true; };
$("optsDlg").onclick = e => { if (e.target === $("optsDlg")) $("optsDlg").hidden = true; };
$("oReset").onclick = () => A.renderOptions($("optsBody"), A.DEFAULTS, { skipUnchanged: false });
$("oSave").onclick = () => { opts = A.readOptions($("optsBody"), opts); A.save(OPTS_KEY, opts); optsSummary(); $("optsDlg").hidden = true; if (st.view === "setup") relist(true); };

/* ---------- workers ---------- */
const workerUrl = new URL("worker.js", document.baseURI);
const pool = [];
function spawn() { const w = new Worker(workerUrl, { type: "module" }); w.busy = null; return w; }
function runOn(w, name, buf) {
  return new Promise(res => {
    const id = Math.random();
    const done = r => { w.onmessage = w.onerror = null; res(r); };
    w.onmessage = e => { if (e.data.id === id) done(e.data); };
    w.onerror = ev => { ev.preventDefault(); done({ error: "failed:File appears to be damaged (the converter crashed)", crashed: true }); };
    w.postMessage({ id, name, buf }, [buf]);
  });
}

/* ---------- run ---------- */
const locked = () => st.view === "progress" && st.s && st.s.state === "running";
function setErr(m) { $("err").textContent = m || ""; $("err").hidden = !m; }

async function go() {
  setErr("");
  if (!st.name) return setErr("Pick a folder first.");
  const files = picker.selectedFiles();
  if (!files.length) return setErr(picker.total ? "Nothing is selected. Tick at least one file in the preview." : "No files match your options.");
  const byRel = new Map(st.entries.map(e => [e.rel, e]));
  const plan = A.planNames(files.map(f => f.rel));
  const t0 = performance.now();
  st.view = "progress"; st.cancel = false; st.rows = 0; st.reportOpen = true;
  st.s = { state: "running", total: files.length, done: 0, converted: 0, failed: 0, copied: 0, current: "", failures: [], seconds: 0 };
  st.output = { docs: [], copies: [] };
  $("tlist").innerHTML = ""; renderPicker(); render();

  const queue = files.map((f, i) => ({ rel: f.rel, out: plan[i].out, renamed: plan[i].renamed }));
  const n = Math.max(1, Math.min(4, navigator.hardwareConcurrency || 2, queue.length));
  while (pool.length < n) pool.push(spawn());
  let next = 0;
  const row = (src, out, tag) => {
    const box = $("tlist");
    const atBottom = box.scrollHeight - box.scrollTop - box.clientHeight < 40;
    box.insertAdjacentHTML("beforeend", `<div class="trow"><span class="ell s">${esc(src)}</span><span class="a">→</span><span class="ell" style="color:${tag === "skipped" ? "var(--bad)" : "var(--ink)"}">${esc(out)}</span><span class="tag ${tag}">${tag}</span></div>`);
    st.rows++;
    while (box.childElementCount > 2000) box.firstElementChild.remove();
    if (atBottom) box.scrollTop = box.scrollHeight;
  };
  const finish = (item, outcome, reason) => {
    const s = st.s; s.done++;
    if (outcome === "copied") { s.copied++; row(item.rel, item.rel, "copied"); }
    else if (outcome === "done") { s.converted++; row(item.rel, item.out, item.renamed ? "renamed" : "done"); }
    else { s.failed++; s.failures.push({ file: item.rel, reason }); row(item.rel, "not converted", "skipped"); }
    s.seconds = Math.round((performance.now() - t0) / 100) / 10;
    render();
  };
  async function lane(slot) {
    while (!st.cancel && next < queue.length) {
      const item = queue[next++];
      st.s.current = item.rel; render();
      const entry = byRel.get(item.rel);
      let file;
      try { file = await entry.file(); } catch (e) { finish(item, "failed", "Could not read file: " + (e.message || e)); continue; }
      if (opts.copyUnsupported && MEDIA.has(A.extOf(item.rel))) { st.output.copies.push({ path: item.rel, data: file }); finish(item, "copied"); continue; }
      if (file.size > MAX_BYTES) { finish(item, "failed", "Too large for the browser version (over 300 MB). Use the desktop app."); continue; }
      const buf = await file.arrayBuffer();
      const r = await runOn(pool[slot], item.rel.split("/").pop(), buf);
      if (r.crashed || r.fatal) { pool[slot].terminate(); pool[slot] = spawn(); }
      if (r.md !== undefined) { st.output.docs.push({ src: item.rel, path: item.out, text: r.md }); finish(item, "done"); }
      else if (opts.copyUnsupported && r.error.startsWith("unsupported:")) { st.output.copies.push({ path: item.rel, data: file }); finish(item, "copied"); }
      else finish(item, "failed", r.error.startsWith("unsupported:") ? "Unsupported: " + r.error.slice(12) : r.error.replace(/^failed:/, ""));
    }
  }
  await Promise.all(pool.slice(0, n).map((_, i) => lane(i)));
  st.s.state = st.cancel ? "cancelled" : "done"; st.s.current = "";
  st.s.failures.sort((a, b) => a.file.toLowerCase() < b.file.toLowerCase() ? -1 : 1);
  if (opts.combine && !st.cancel && st.output.docs.length) st.output.combined = combined(st.name, st.output.docs);
  render();
}

/* ---------- outputs ---------- */
const slug = s => s.toLowerCase().replace(/ /g, "-").replace(/[^\p{L}\p{N}_-]/gu, "");
function demote(md) {
  let fence = false;
  return md.split("\n").map(l => {
    if (l.trimStart().startsWith("```")) fence = !fence;
    const m = !fence && /^(#+)( .*)$/.exec(l);
    return m ? "#".repeat(Math.min(m[1].length + 2, 6)) + m[2] : l;
  }).join("\n");
}
function combined(title, docs) {
  const d = [...docs].sort((a, b) => a.src.toLowerCase() < b.src.toLowerCase() ? -1 : 1);
  let s = `# ${title}\n\nAll ${d.length} documents from \`${title}\` in one file, generated by Any File To MD.\n\n## Contents\n\n`;
  for (const x of d) s += `- [${x.src}](#${slug(x.src)})\n`;
  for (const x of d) s += `\n---\n\n## ${x.src}\n\n${demote(x.text.trim())}\n`;
  return s;
}
function report(failures) {
  let s = `# Conversion report\n\n${failures.length} file(s) could not be converted:\n\n`;
  for (const f of failures) s += `- \`${f.file}\`: ${f.reason}\n`;
  return s;
}
function outputFiles() {
  const o = st.output, files = [];
  for (const d of o.docs) files.push({ path: d.path, data: d.text });
  for (const c of o.copies) files.push({ path: c.path, data: c.data });
  if (st.s.failures.length) files.push({ path: "_conversion-report.md", data: report(st.s.failures) });
  if (o.combined) files.push({ path: "_combined.md", data: o.combined });
  return files;
}

// Minimal ZIP writer (stored, no compression): Markdown is small and this keeps the page dependency-free.
const CRC = (() => { const t = new Uint32Array(256); for (let n = 0; n < 256; n++) { let c = n; for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1; t[n] = c >>> 0; } return t; })();
function crc32(b) { let c = 0xffffffff; for (let i = 0; i < b.length; i++) c = CRC[(c ^ b[i]) & 0xff] ^ (c >>> 8); return (c ^ 0xffffffff) >>> 0; }
async function zip(files, root) {
  const enc = new TextEncoder(), parts = [], central = [];
  const now = new Date();
  const time = (now.getHours() << 11) | (now.getMinutes() << 5) | (now.getSeconds() >> 1);
  const date = ((now.getFullYear() - 1980) << 9) | ((now.getMonth() + 1) << 5) | now.getDate();
  let offset = 0;
  for (const f of files) {
    const name = enc.encode(root + "/" + f.path);
    const data = typeof f.data === "string" ? enc.encode(f.data) : new Uint8Array(await f.data.arrayBuffer());
    const crc = crc32(data);
    const head = (sig, extra) => { const h = new DataView(new ArrayBuffer(extra)); h.setUint32(0, sig, true); return h; };
    const l = head(0x04034b50, 30);
    l.setUint16(4, 20, true); l.setUint16(6, 0x0800, true); l.setUint16(8, 0, true); l.setUint16(10, time, true); l.setUint16(12, date, true);
    l.setUint32(14, crc, true); l.setUint32(18, data.length, true); l.setUint32(22, data.length, true); l.setUint16(26, name.length, true);
    parts.push(l, name, data);
    const c = head(0x02014b50, 46);
    c.setUint16(4, 20, true); c.setUint16(6, 20, true); c.setUint16(8, 0x0800, true); c.setUint16(12, time, true); c.setUint16(14, date, true);
    c.setUint32(16, crc, true); c.setUint32(20, data.length, true); c.setUint32(24, data.length, true); c.setUint16(28, name.length, true); c.setUint32(42, offset, true);
    central.push(c, name);
    offset += 30 + name.length + data.length;
  }
  const size = central.reduce((n, p) => n + p.byteLength, 0);
  const e = new DataView(new ArrayBuffer(22));
  e.setUint32(0, 0x06054b50, true); e.setUint16(8, files.length, true); e.setUint16(10, files.length, true); e.setUint32(12, size, true); e.setUint32(16, offset, true);
  return new Blob([...parts, ...central, e], { type: "application/zip" });
}
function download(blob, name) {
  const a = document.createElement("a"); a.href = URL.createObjectURL(blob); a.download = name;
  document.body.appendChild(a); a.click(); a.remove(); setTimeout(() => URL.revokeObjectURL(a.href), 60000);
}
async function saveZip() {
  $("open").disabled = true; $("open").textContent = "Preparing .zip…";
  try { download(await zip(outputFiles(), st.name + "-markdown"), st.name + "-markdown.zip"); }
  catch (e) { setErr("Couldn't build the .zip: " + (e.message || e)); }
  finally { $("open").disabled = false; $("open").textContent = "Download .zip"; }
}
async function saveFolder() {
  let dir;
  try { dir = await window.showDirectoryPicker({ id: "anytomd-out", mode: "readwrite", startIn: "documents" }); } catch (e) { if (e.name !== "AbortError") setErr(String(e.message || e)); return; }
  $("saveFolder").disabled = true; $("saveFolder").textContent = "Saving…";
  try {
    const root = await dir.getDirectoryHandle(st.name + "-markdown", { create: true });
    for (const f of outputFiles()) {
      const parts = f.path.split("/"); let d = root;
      for (const p of parts.slice(0, -1)) d = await d.getDirectoryHandle(p, { create: true });
      const w = await (await d.getFileHandle(parts[parts.length - 1], { create: true })).createWritable();
      await w.write(f.data); await w.close();
    }
    $("tnote").textContent = `Saved to ${dir.name}/${st.name}-markdown`;
  } catch (e) { setErr("Couldn't save: " + (e.message || e)); }
  finally { $("saveFolder").disabled = false; $("saveFolder").textContent = "Save to a folder…"; }
}

$("srcCard").onclick = browse;
$("drop").onclick = browse;
$("go").onclick = go;
$("cancel").onclick = () => { st.cancel = true; render(); };
$("open").onclick = saveZip;
$("saveFolder").onclick = saveFolder;
$("openCombined").onclick = () => download(new Blob([st.output.combined], { type: "text/markdown" }), "_combined.md");
$("again").onclick = () => { st.view = "setup"; st.s = null; renderExample(); relist(true); render(); };
$("reportBtn").onclick = () => { st.reportOpen = !st.reportOpen; render(); };
document.addEventListener("keydown", e => {
  if (e.key === "Escape") $("optsDlg").hidden = true;
  if (e.key === "Enter" && e.ctrlKey && !locked()) go();
});

/* ---------- render ---------- */
const EXAMPLE = ["q1.pdf", "budget.xlsx", "slides/deck.pptx", "notes/a.pdf", "notes/a.docx"];
function renderExample() {
  const plan = A.planNames(EXAMPLE);
  $("tlist").innerHTML = EXAMPLE.map((f, i) => `<div class="trow"><span class="ell s">${esc(f)}</span><span class="a">→</span><span class="ell">${esc(plan[i].out)}</span>${plan[i].renamed ? '<span class="tag renamed">renamed</span>' : "<span></span>"}</div>`).join("");
  $("tnote").textContent = "Example. Folders are copied exactly; same-name files keep their extension.";
}
function render() {
  const s = st.s || {};
  const inProg = st.view === "progress";
  const running = locked();
  const finished = inProg && ["done", "cancelled"].includes(s.state);
  const failures = s.failures || [];
  const pct = s.total ? Math.round(100 * s.done / s.total) : 0;
  let title = "Mirror preview", phase = "Ready", dot = "#b9b4af";
  if (running) { title = `Converting… ${pct}%`; phase = "Converting"; dot = "#3f8a4f"; }
  else if (finished) {
    title = s.state === "cancelled" ? "Stopped" : s.failed > 0 ? "Done, with a few skipped" : "All done";
    phase = s.state === "cancelled" ? "Stopped" : s.failed > 0 ? "Completed with warnings" : "Completed";
    dot = s.failed > 0 || s.state === "cancelled" ? "#846e2d" : "#3f8a4f";
  }
  $("ptitle").textContent = title; $("phase").textContent = phase;
  $("dot").style.background = dot; $("dot").style.animation = running ? "blink 1.2s ease-in-out infinite" : "none";
  $("count").textContent = inProg ? `${s.done || 0} / ${s.total || 0} files` : "";
  $("tSrc").textContent = (st.name || "source") + "/"; $("tOut").textContent = (st.name ? st.name + "-markdown" : "source-markdown") + "/";
  $("barWrap").hidden = !running; $("fill").style.width = pct + "%";
  $("cur").textContent = s.current ? "‎" + s.current : " ";
  $("statsWrap").hidden = !finished;
  $("sOk").textContent = s.converted || 0; $("sBad").textContent = s.failed || 0;
  $("sBad").style.color = s.failed ? "var(--bad)" : "var(--ink)"; $("sTime").textContent = (s.seconds || 0) + "s";
  $("extra").hidden = !(finished && s.copied); $("extra").textContent = s.copied ? `${s.copied} copied as-is` : "";
  if (inProg) $("tnote").textContent = running ? "Converting on this device. Nothing is uploaded." : `Ready to save: ${(s.converted || 0) + (s.copied || 0)} file${(s.converted || 0) + (s.copied || 0) === 1 ? "" : "s"}.`;
  $("openCombined").hidden = !(finished && st.output && st.output.combined);
  $("report").hidden = !(finished && failures.length);
  $("caret").textContent = st.reportOpen ? "▾" : "▸";
  $("failSum").textContent = `${s.failed} file${s.failed === 1 ? "" : "s"} couldn't be converted`;
  $("failList").hidden = !st.reportOpen;
  $("failList").innerHTML = failures.map(f => `<div class="fail"><span class="f">${esc(f.file)}</span><span class="r">${esc(f.reason)}</span></div>`).join("");
  $("idleNote").hidden = inProg;
  $("cancel").hidden = !running; $("cancel").disabled = st.cancel; $("cancel").textContent = st.cancel ? "Stopping…" : "Cancel";
  const hasOut = finished && ((s.converted || 0) + (s.copied || 0) > 0);
  $("open").hidden = !hasOut; $("saveFolder").hidden = !(hasOut && window.showDirectoryPicker); $("again").hidden = !finished;
  for (const id of ["srcCard", "drop", "go", "optsBtn"]) $(id).disabled = running;
  $("go").textContent = running ? "Converting…" : "Convert to Markdown";
  renderPicker();
}

optsSummary(); renderExample(); render();
