"""Folder -> mirrored folder of Markdown. No UI code in here."""

from __future__ import annotations

import threading
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Callable

SKIP_DIRS = {
    ".git", ".svn", ".hg", "node_modules", "__pycache__", ".venv", "venv",
    ".idea", ".vscode", "$RECYCLE.BIN", "System Volume Information",
}

# Plain text that is already Markdown-friendly: copied through unchanged.
PASSTHROUGH = {".md", ".markdown", ".txt"}

# Source code / config: wrapped in a fenced block with a language hint.
CODE_LANGS = {
    ".py": "python", ".js": "javascript", ".ts": "typescript", ".jsx": "jsx",
    ".tsx": "tsx", ".java": "java", ".c": "c", ".h": "c", ".cpp": "cpp",
    ".hpp": "cpp", ".cs": "csharp", ".go": "go", ".rs": "rust", ".rb": "ruby",
    ".php": "php", ".swift": "swift", ".kt": "kotlin", ".sh": "bash",
    ".bat": "bat", ".cmd": "bat", ".ps1": "powershell", ".sql": "sql",
    ".yml": "yaml", ".yaml": "yaml", ".toml": "toml", ".ini": "ini",
    ".cfg": "ini", ".css": "css", ".scss": "scss", ".lua": "lua", ".r": "r",
    ".log": "",
}


# Binary formats markitdown knows how to read; any other binary file is reported as unsupported.
BINARY_OK = {
    ".pdf", ".docx", ".pptx", ".xlsx", ".xls", ".epub", ".zip", ".msg",
    ".jpg", ".jpeg", ".png", ".gif", ".bmp", ".webp", ".tif", ".tiff",
    ".wav", ".mp3", ".m4a", ".mp4",
}


@dataclass
class Progress:
    state: str = "idle"  # idle | scanning | running | done | cancelled | error
    total: int = 0
    done: int = 0
    converted: int = 0
    failed: int = 0
    current: str = ""
    output: str = ""
    error: str = ""
    started: float = 0.0
    finished: float = 0.0
    failures: list = field(default_factory=list)  # [{"file":..., "reason":...}]

    def snapshot(self) -> dict:
        end = self.finished or time.time()
        return {
            "state": self.state, "total": self.total, "done": self.done,
            "converted": self.converted, "failed": self.failed,
            "current": self.current, "output": self.output, "error": self.error,
            "seconds": round(end - self.started, 1) if self.started else 0,
            "failures": self.failures[-50:],
        }


def default_output(source: Path) -> Path:
    return source.parent / f"{source.name}-markdown"


def validate(source: Path, output: Path) -> str | None:
    """Return an error message, or None when the pair is safe to use."""
    if not source.is_dir():
        return "That source folder doesn't exist."
    s, o = source.resolve(), output.resolve()
    if s == o:
        return "The output folder can't be the same as the source folder."
    if o in s.parents:
        return "The output folder can't contain the source folder."
    return None  # output inside source is fine; it is excluded while scanning


def collect_files(source: Path, output: Path) -> list[Path]:
    out = output.resolve()
    files: list[Path] = []
    stack = [source]
    while stack:
        d = stack.pop()
        try:
            entries = list(d.iterdir())
        except OSError:
            continue
        for p in entries:
            try:
                if p.is_dir():
                    if p.name in SKIP_DIRS or p.resolve() == out:
                        continue
                    stack.append(p)
                elif p.is_file() and not p.name.startswith("~$"):
                    files.append(p)
            except OSError:
                continue
    files.sort(key=lambda p: [x.lower() for x in p.relative_to(source).parts])
    return files


def plan_names(source: Path, files: list[Path]) -> dict[Path, Path]:
    """Map each source file to a relative .md path.

    report.pdf -> report.md. If report.pdf and report.docx sit side by side,
    they become report.pdf.md and report.docx.md so neither overwrites the other.
    """
    groups: dict[tuple, list[Path]] = {}
    for f in files:
        rel = f.relative_to(source)
        groups.setdefault((rel.parent, rel.stem.lower()), []).append(f)
    plan: dict[Path, Path] = {}
    for group in groups.values():
        for f in group:
            rel = f.relative_to(source)
            plan[f] = rel.with_name(rel.name + ".md") if len(group) > 1 else rel.with_suffix(".md")
    return plan


def _decode(raw: bytes) -> str:
    if raw.startswith((b"\xff\xfe", b"\xfe\xff")):
        return raw.decode("utf-16")
    for enc in ("utf-8-sig", "cp1252"):
        try:
            return raw.decode(enc)
        except UnicodeDecodeError:
            continue
    return raw.decode("utf-8", errors="replace")


def _read_text(path: Path) -> str:
    return _decode(path.read_bytes())


class Engine:
    """Converts one file to Markdown. The heavy markitdown import happens lazily, once."""

    def __init__(self):
        self._md = None

    def __call__(self, path: Path) -> str:
        ext = path.suffix.lower()
        if ext in PASSTHROUGH:
            return _read_text(path)
        if ext in CODE_LANGS:
            return f"# {path.name}\n\n```{CODE_LANGS[ext]}\n{_read_text(path).rstrip()}\n```\n"
        if ext not in BINARY_OK:
            with path.open("rb") as fh:
                if b"\x00" in fh.read(4096):
                    raise ValueError("Unsupported binary format")
        if self._md is None:
            from markitdown import MarkItDown

            self._md = MarkItDown(enable_plugins=False)
        try:
            text = self._md.convert(str(path)).text_content
        except Exception as exc:
            if type(exc).__name__ == "UnsupportedFormatException":
                return self._text_fallback(path)
            raise
        return text if text and text.strip() else self._text_fallback(path)

    @staticmethod
    def _text_fallback(path: Path) -> str:
        raw = path.read_bytes()
        if not raw.strip():
            raise ValueError("File is empty")
        if b"\x00" in raw[:4096]:
            raise ValueError("Unsupported binary format")
        return f"# {path.name}\n\n```\n{_decode(raw).rstrip()}\n```\n"


def convert_folder(
    source: Path,
    output: Path,
    progress: Progress,
    cancel: threading.Event | None = None,
    convert: Callable[[Path], str] | None = None,
) -> Progress:
    source, output = Path(source), Path(output)
    cancel = cancel or threading.Event()
    progress.started, progress.state, progress.output = time.time(), "scanning", str(output)
    err = validate(source, output)
    if err:
        progress.state, progress.error, progress.finished = "error", err, time.time()
        return progress
    try:
        files = collect_files(source, output)
        plan = plan_names(source, files)
        progress.total, progress.state = len(files), "running"
        output.mkdir(parents=True, exist_ok=True)
        engine = convert or Engine()
        for f in files:
            if cancel.is_set():
                progress.state = "cancelled"
                break
            rel = f.relative_to(source)
            progress.current = str(rel)
            try:
                text = engine(f)
                dest = output / plan[f]
                dest.parent.mkdir(parents=True, exist_ok=True)
                dest.write_text(text, encoding="utf-8", newline="\n")
                progress.converted += 1
            except Exception as exc:  # one bad file must never stop the batch
                progress.failed += 1
                progress.failures.append({"file": str(rel), "reason": _short(exc)})
            progress.done += 1
        else:
            progress.state = "done"
    except Exception as exc:
        progress.state, progress.error = "error", _short(exc)
    progress.current, progress.finished = "", time.time()
    if progress.failures:
        _write_report(output, progress)
    return progress


def _short(exc: Exception) -> str:
    text = str(exc).strip()
    msg = text.splitlines()[0] if text else ""
    return f"{type(exc).__name__}: {msg}"[:240]


def _write_report(output: Path, progress: Progress) -> None:
    try:
        lines = ["# Conversion report", "", f"{progress.failed} file(s) could not be converted:", ""]
        lines += [f"- `{x['file']}`: {x['reason']}" for x in progress.failures]
        (output / "_conversion-report.md").write_text("\n".join(lines) + "\n", encoding="utf-8")
    except OSError:
        pass
