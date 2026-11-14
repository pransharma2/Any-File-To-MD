from pathlib import Path

from anytomd import converter
from anytomd.converter import Progress, convert_folder, plan_names, validate


def make_tree(root: Path):
    (root / "sub" / "deep").mkdir(parents=True)
    (root / "notes.txt").write_text("hello", encoding="utf-8")
    (root / "page.html").write_text("<h1>Title</h1><p>Body</p>", encoding="utf-8")
    (root / "sub" / "data.csv").write_text("a,b\n1,2\n", encoding="utf-8")
    (root / "sub" / "deep" / "script.py").write_text("print(1)\n", encoding="utf-8")
    (root / "sub" / "bad.bin").write_bytes(b"\x00\x01\x02\x03")
    (root / ".git").mkdir()
    (root / ".git" / "config").write_text("x", encoding="utf-8")


def test_mirrors_structure_and_reports_failures(tmp_path):
    src, out = tmp_path / "src", tmp_path / "out"
    src.mkdir()
    make_tree(src)
    p = convert_folder(src, out, Progress())
    assert p.state == "done"
    assert (out / "notes.md").read_text(encoding="utf-8") == "hello"
    assert "# Title" in (out / "page.md").read_text(encoding="utf-8")
    assert "| a | b |" in (out / "sub" / "data.md").read_text(encoding="utf-8")
    assert "```python" in (out / "sub" / "deep" / "script.md").read_text(encoding="utf-8")
    assert not (out / ".git").exists()
    assert p.failed == 1 and p.failures[0]["file"].endswith("bad.bin")
    assert (out / "_conversion_report.md").exists() or (out / "_conversion-report.md").exists()
    # originals untouched
    assert (src / "notes.txt").read_text(encoding="utf-8") == "hello"


def test_output_inside_source_is_not_rescanned(tmp_path):
    src = tmp_path / "src"
    src.mkdir()
    (src / "a.txt").write_text("a", encoding="utf-8")
    out = src / "md"
    convert_folder(src, out, Progress())
    p = convert_folder(src, out, Progress())  # second run must not convert its own output
    assert p.total == 1


def test_same_stem_collision_keeps_both(tmp_path):
    src = tmp_path / "s"
    src.mkdir()
    a, b = src / "r.pdf", src / "r.docx"
    a.write_bytes(b"x")
    b.write_bytes(b"x")
    plan = plan_names(src, [a, b])
    assert {v.name for v in plan.values()} == {"r.pdf.md", "r.docx.md"}


def test_validate_rejects_bad_pairs(tmp_path):
    assert validate(tmp_path / "nope", tmp_path / "o")
    assert validate(tmp_path, tmp_path)
    child = tmp_path / "c"
    child.mkdir()
    assert validate(child, tmp_path)  # output would contain source


def test_cancel(tmp_path):
    import threading

    src = tmp_path / "s"
    src.mkdir()
    for i in range(5):
        (src / f"{i}.txt").write_text("x", encoding="utf-8")
    ev = threading.Event()
    ev.set()
    p = convert_folder(src, tmp_path / "o", Progress(), ev)
    assert p.state == "cancelled" and p.converted == 0


def test_real_office_formats(tmp_path):
    """Exercises markitdown with a generated .docx, if python-docx is available."""
    import pytest

    docx = pytest.importorskip("docx")
    src = tmp_path / "s"
    src.mkdir()
    d = docx.Document()
    d.add_heading("Quarterly Report", 1)
    d.add_paragraph("Revenue grew.")
    d.save(src / "report.docx")
    p = convert_folder(src, tmp_path / "o", Progress())
    assert p.converted == 1
    assert "Quarterly Report" in (tmp_path / "o" / "report.md").read_text(encoding="utf-8")
