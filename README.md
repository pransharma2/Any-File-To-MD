# Any File To MD

Point it at a folder. Get a mirrored folder of clean Markdown. Your originals are never touched.

Everything runs on your own computer: no accounts, no uploads, no API keys.

## Install and run

**Windows:** double-click **`Start.cmd`**.

**Mac / Linux:** run `./start.sh` (you may need `chmod +x start.sh` once).

The first launch downloads a small helper ([uv](https://docs.astral.sh/uv/)) into this folder, which then fetches Python and the converters. This takes about a minute and needs internet. After that, launching is instant and works offline. Nothing is installed system-wide, and deleting this folder removes everything.

Your browser opens to the app. Leave the black window open while you use it.

## Use it

1. **Pick a folder** by pasting a path or clicking *Browse…*
2. **Check the output location.** It defaults to `<folder>-markdown` next to the original.
3. Click **Convert to Markdown**.

The folder structure is copied exactly, with each file becoming a `.md`:

```
Reports/                  Reports-markdown/
  q1.pdf            →       q1.md
  budget.xlsx       →       budget.md
  slides/deck.pptx  →       slides/deck.md
```

If `a.pdf` and `a.docx` sit side by side, they become `a.pdf.md` and `a.docx.md` so neither overwrites the other. Files that can't be converted are listed in the app and in `_conversion-report.md`; one bad file never stops the batch.

## Supported files

PDF, Word (.docx), PowerPoint (.pptx), Excel (.xlsx/.xls), CSV, HTML, JSON, XML, EPUB, ZIP, Outlook (.msg), images (metadata only), plain text, Markdown, and common source-code and config files (wrapped in fenced code blocks). Scanned PDFs without a text layer produce little or no text, because there is no OCR.

Conversion uses Microsoft's [markitdown](https://github.com/microsoft/markitdown).

## Development

```
.tools\uv.exe run --extra dev pytest      # Windows
./.tools/uv run --extra dev pytest        # Mac / Linux
```

Layout: `src/anytomd/converter.py` (conversion logic), `server.py` (local-only web server, token-protected, localhost-only), `static/index.html` (the UI).
