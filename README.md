# Any File To MD

Point it at a folder. Get a mirrored folder of clean Markdown. Your originals are never touched.

Everything runs on your own computer: no accounts, no uploads, no API keys.

There are two ways to run it:

| | **Desktop app** (Windows, Rust) | **Web app** (any OS, Python) |
| --- | --- | --- |
| Install | Run the installer `.exe` | Double-click `Start.cmd` / run `./start.sh` |
| Speed | ~190× faster (350 files in 0.5 s) | markitdown, highest fidelity on complex PDFs |
| Right-click "Convert to Markdown" in Explorer | Yes | No |
| Skip unchanged · combined file · filters · CLI | Yes | No |

## Desktop app (Windows)

Download `Any File To MD_1.0.0_x64-setup.exe` from the [Releases](https://github.com/pransharma2/Any-File-To-MD/releases) page and run it. It installs for your user only, so no admin rights are needed. A portable `.exe` is also provided.

- **Drop a folder** onto the window, or click to browse. Press **Convert to Markdown** (or `Ctrl+Enter`).
- **Right-click in Explorer** (on Windows 11, under *Show more options*):
  - on a folder, or inside one: **Convert folder to Markdown** opens the app and starts right away;
  - on a PDF, Word, PowerPoint, Excel, HTML, EPUB, CSV, ZIP or `.msg` file: **Convert to Markdown** writes `name.md` beside it.

  The installer adds these entries. With the portable `.exe`, click *Add to right-click menu* in the status bar.
- **Options** (saved between runs):
  - **Skip unchanged files.** Re-runs only convert new or edited files.
  - **Combined file.** Also writes `_combined.md`, every document in one file with a table of contents, ready to paste into an AI chat.
  - **Copy files that can't be converted.** Images, media and unknown files are copied as-is, so the output is a complete mirror.
  - **Only these file types / Skip these paths.** For example `pdf, docx` or `drafts, *.log`.
- **Command line:** `anytomd.exe` is installed next to the app.

  ```
  anytomd "C:\Docs\Reports"                      # -> C:\Docs\Reports-markdown
  anytomd "C:\Docs\Reports" -o D:\md --combine --exclude drafts
  anytomd report.pdf                              # -> report.md
  anytomd --help
  ```

  Exit code `0` means success, `1` means an error, and `2` means it finished but some files were skipped.

### Build it yourself

You need [Rust](https://rustup.rs) and the Visual Studio C++ Build Tools. Then run:

```
powershell -ExecutionPolicy Bypass -File desktop\build.ps1
```

The installer, the portable app and `anytomd.exe` land in `desktop\dist\`. A GitHub Actions workflow (`.github/workflows/desktop.yml`) builds the same files on every push and attaches them to tagged releases.

> **Windows Smart App Control** blocks the Rust compiler's unsigned build helpers, and blocks unsigned apps in general. To build locally, or to run an unsigned build, Smart App Control must be off. Releases that should install cleanly for everyone need a code-signing certificate.

## Web app (Windows, Mac, Linux)

**Windows:** double-click **`Start.cmd`**. **Mac / Linux:** run `./start.sh` (you may need `chmod +x start.sh` once).

The first launch downloads a small helper ([uv](https://docs.astral.sh/uv/)) into this folder, which then fetches Python and the converters. This takes about a minute and needs internet. After that, launching is instant and works offline. Nothing is installed system-wide, and deleting this folder removes everything. Your browser opens to the app. Leave the console window open while you use it.

## How the output looks

The folder structure is copied exactly, with each file becoming a `.md`:

```
Reports/                  Reports-markdown/
  q1.pdf            →       q1.md
  budget.xlsx       →       budget.md
  slides/deck.pptx  →       slides/deck.md
```

If `a.pdf` and `a.docx` sit side by side, they become `a.pdf.md` and `a.docx.md` so neither overwrites the other. Files that can't be converted are listed in the app and in `_conversion-report.md`; one bad file never stops the batch.

## Supported files

PDF, Word (.docx), PowerPoint (.pptx, with speaker notes), Excel (.xlsx/.xls/.xlsb), OpenDocument (.odt/.ods), CSV/TSV, HTML, JSON, XML, EPUB, ZIP (each file inside), Outlook (.msg), Jupyter notebooks (.ipynb), images (dimensions and EXIF metadata), plain text, Markdown, and common source-code and config files (in fenced code blocks). Scanned PDFs without a text layer produce no text, because there is no OCR.

The web app converts with Microsoft's [markitdown](https://github.com/microsoft/markitdown). The desktop app has its own pure-Rust engine (`desktop/engine`).

## Development

```
.tools\uv.exe run --extra dev pytest           # web app tests
cargo test -p anytomd-engine                   # desktop engine tests (run inside desktop\)
```

Layout:

- `src/anytomd/`: the Python web app (`converter.py`, `server.py`, `static/index.html`).
- `desktop/engine/`: the Rust conversion engine and the `anytomd` CLI.
- `desktop/app/`: the Tauri desktop app (`src/main.rs`, `src/shell.rs` for the Explorer menu, `ui/index.html`).

MIT License. See [LICENSE](LICENSE).
