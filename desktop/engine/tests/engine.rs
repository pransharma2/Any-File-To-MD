use std::fs;
use std::io::Write;
use std::path::Path;

use anytomd_engine::*;

fn write(p: &Path, s: &[u8]) {
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, s).unwrap();
}

fn make_zip(entries: &[(&str, &str)]) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let o = zip::write::SimpleFileOptions::default();
        for (name, body) in entries {
            z.start_file(*name, o).unwrap();
            z.write_all(body.as_bytes()).unwrap();
        }
        z.finish().unwrap();
    }
    buf.into_inner()
}

fn run(src: &Path, out: &Path, opts: &Options) -> Status {
    let job = Job::new();
    convert_folder(src, out, opts, &job);
    job.snapshot(0).0
}

fn tree(root: &Path) {
    write(&root.join("notes.txt"), b"hello");
    write(&root.join("page.html"), b"<html><head><style>x{}</style></head><body><h1>Title</h1><p>Body <b>bold</b></p></body></html>");
    write(&root.join("sub/data.csv"), b"a,b\n1,2\n");
    write(&root.join("sub/deep/script.py"), b"print(1)\n");
    write(&root.join("sub/bad.bin"), &[0, 1, 2, 3]);
    write(&root.join(".git/config"), b"x");
}

#[test]
fn mirrors_structure_and_reports_failures() {
    let t = tempfile::tempdir().unwrap();
    let (src, out) = (t.path().join("src"), t.path().join("out"));
    tree(&src);
    let s = run(&src, &out, &Options::default());
    assert_eq!(s.state, "done");
    assert_eq!(fs::read_to_string(out.join("notes.md")).unwrap(), "hello\n");
    let page = fs::read_to_string(out.join("page.md")).unwrap();
    assert!(page.contains("# Title") && page.contains("**bold**") && !page.contains("x{}"), "{page}");
    assert!(fs::read_to_string(out.join("sub/data.md")).unwrap().contains("| a | b |"));
    assert!(fs::read_to_string(out.join("sub/deep/script.md")).unwrap().contains("```python"));
    assert!(!out.join(".git").exists());
    assert_eq!(s.failed, 1);
    assert!(s.failures[0].file.ends_with("bad.bin"));
    assert!(out.join("_conversion-report.md").exists());
    assert_eq!(fs::read_to_string(src.join("notes.txt")).unwrap(), "hello"); // untouched
}

#[test]
fn skip_unchanged_on_second_run() {
    let t = tempfile::tempdir().unwrap();
    let (src, out) = (t.path().join("s"), t.path().join("o"));
    write(&src.join("a.txt"), b"a");
    write(&src.join("b.txt"), b"b");
    assert_eq!(run(&src, &out, &Options::default()).converted, 2);
    let s = run(&src, &out, &Options::default());
    assert_eq!((s.converted, s.unchanged), (0, 2));
    let all = Options { skip_unchanged: false, ..Options::default() };
    assert_eq!(run(&src, &out, &all).converted, 2);
}

#[test]
fn output_inside_source_is_not_rescanned() {
    let t = tempfile::tempdir().unwrap();
    let src = t.path().join("s");
    write(&src.join("a.txt"), b"a");
    let out = src.join("md");
    run(&src, &out, &Options::default());
    assert_eq!(run(&src, &out, &Options::default()).total, 1);
}

#[test]
fn collisions_keep_both() {
    let t = tempfile::tempdir().unwrap();
    let s = t.path();
    let files = vec![s.join("r.pdf"), s.join("r.docx"), s.join("x.txt")];
    let plan = plan_names(s, &files);
    let names: Vec<String> = plan.iter().map(|p| p.1.to_string_lossy().into_owned()).collect();
    assert_eq!(names, vec!["r.pdf.md", "r.docx.md", "x.md"]);
    assert!(plan[0].2 && !plan[2].2);
}

#[test]
fn validate_rejects_bad_pairs() {
    let t = tempfile::tempdir().unwrap();
    assert!(validate(&t.path().join("nope"), &t.path().join("o")).is_some());
    assert!(validate(t.path(), t.path()).is_some());
    let child = t.path().join("c");
    fs::create_dir(&child).unwrap();
    assert!(validate(&child, t.path()).is_some());
    assert!(validate(&child, &t.path().join("c-markdown")).is_none());
}

#[test]
fn filters_combine_and_copy() {
    let t = tempfile::tempdir().unwrap();
    let (src, out) = (t.path().join("s"), t.path().join("o"));
    write(&src.join("a.txt"), b"# Alpha\ntext");
    write(&src.join("b.csv"), b"x,y\n1,2");
    write(&src.join("drafts/c.txt"), b"draft");
    write(&src.join("app.exe"), &[0x4d, 0x5a, 0, 0, 1]);
    let opts = Options { exclude: vec!["drafts".into()], combine: true, copy_unsupported: true, ..Options::default() };
    let s = run(&src, &out, &opts);
    assert_eq!(s.total, 3, "drafts excluded");
    assert_eq!(s.copied, 1);
    assert!(out.join("app.exe").exists());
    let combined = fs::read_to_string(out.join("_combined.md")).unwrap();
    assert!(combined.contains("## Contents") && combined.contains("### Alpha") && combined.contains("## b.csv"), "{combined}");

    let only_csv = Options { include: vec!["csv".into()], ..Options::default() };
    assert_eq!(run(&src, &t.path().join("o2"), &only_csv).total, 1);
}

#[test]
fn picker_list_and_deselect() {
    let t = tempfile::tempdir().unwrap();
    let src = t.path().join("s");
    write(&src.join("a.txt"), b"alpha");
    write(&src.join("Sub/b.txt"), b"beta");
    write(&src.join("Sub/deep/c.txt"), b"gamma");
    write(&src.join("pic.png"), b"\x89PNG");
    let (listed, more) = list_files(&src, &t.path().join("o"), &Options::default(), 100).unwrap();
    assert!(!more);
    assert_eq!(listed.len(), 4);
    assert_eq!(listed.iter().find(|l| l.rel == "pic.png").unwrap().kind, "media");
    assert_eq!(listed.iter().find(|l| l.rel == "Sub/b.txt").unwrap().out, "Sub/b.md");

    // Untick a whole folder (case-insensitive) and one file.
    let opts = Options { deselect: vec!["sub/deep".into(), "pic.png".into()], skip_unchanged: false, ..Options::default() };
    let s = run(&src, &t.path().join("o"), &opts);
    assert_eq!(s.total, 2);
    assert!(t.path().join("o/Sub/b.md").exists() && !t.path().join("o/Sub/deep").exists());
    // The picker itself still lists everything.
    assert_eq!(list_files(&src, &t.path().join("o"), &opts, 100).unwrap().0.len(), 4);
}

#[test]
fn docx_headings_lists_tables() {
    let doc = r#"<?xml version="1.0"?><w:document xmlns:w="w" xmlns:r="r"><w:body>
      <w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Quarterly Report</w:t></w:r></w:p>
      <w:p><w:r><w:t xml:space="preserve">Revenue </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>grew</w:t></w:r><w:r><w:t>.</w:t></w:r></w:p>
      <w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>First</w:t></w:r></w:p>
      <w:p><w:pPr><w:numPr><w:ilvl w:val="1"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>Nested</w:t></w:r></w:p>
      <w:tbl><w:tr><w:tc><w:p><w:r><w:t>Q</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Rev</w:t></w:r></w:p></w:tc></w:tr>
             <w:tr><w:tc><w:p><w:r><w:t>Q1</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>10</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
      <w:p><w:hyperlink r:id="rId9"><w:r><w:t>site</w:t></w:r></w:hyperlink></w:p>
    </w:body></w:document>"#;
    let rels = r#"<Relationships><Relationship Id="rId9" Target="https://example.com"/></Relationships>"#;
    let bytes = make_zip(&[("word/document.xml", doc), ("word/_rels/document.xml.rels", rels)]);
    let md = convert_bytes("r.docx", &bytes).unwrap();
    assert!(md.contains("# Quarterly Report"), "{md}");
    assert!(md.contains("Revenue **grew**."), "{md}");
    assert!(md.contains("- First\n  - Nested"), "{md}");
    assert!(md.contains("| Q | Rev |\n| --- | --- |\n| Q1 | 10 |"), "{md}");
    assert!(md.contains("[site](https://example.com)"), "{md}");
}

#[test]
fn pptx_titles_and_notes() {
    let slide = r#"<p:sld xmlns:p="p" xmlns:a="a"><p:cSld><p:spTree>
      <p:sp><p:nvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr><p:txBody><a:p><a:r><a:t>Roadmap</a:t></a:r></a:p></p:txBody></p:sp>
      <p:sp><p:nvSpPr><p:nvPr><p:ph idx="1"/></p:nvPr></p:nvSpPr><p:txBody><a:p><a:r><a:t>Ship it</a:t></a:r></a:p></p:txBody></p:sp>
    </p:spTree></p:cSld></p:sld>"#;
    let rels = r#"<Relationships><Relationship Id="rId2" Target="../notesSlides/notesSlide1.xml"/></Relationships>"#;
    let notes = r#"<p:notes xmlns:p="p" xmlns:a="a"><p:cSld><p:spTree>
      <p:sp><p:nvSpPr><p:nvPr><p:ph type="body"/></p:nvPr></p:nvSpPr><p:txBody><a:p><a:r><a:t>Say hi</a:t></a:r></a:p></p:txBody></p:sp>
    </p:spTree></p:cSld></p:notes>"#;
    let bytes = make_zip(&[
        ("ppt/slides/slide1.xml", slide),
        ("ppt/slides/_rels/slide1.xml.rels", rels),
        ("ppt/notesSlides/notesSlide1.xml", notes),
    ]);
    let md = convert_bytes("d.pptx", &bytes).unwrap();
    assert!(md.contains("<!-- Slide 1 -->\n# Roadmap\n\nShip it"), "{md}");
    assert!(md.contains("### Notes\n\nSay hi"), "{md}");
}

#[test]
fn zip_and_notebook_and_json() {
    let z = make_zip(&[("inner/a.txt", "# Inner\nhi"), ("b.csv", "k,v\n1,2")]);
    let md = convert_bytes("pack.zip", &z).unwrap();
    assert!(md.contains("# Contents of pack.zip") && md.contains("## inner/a.txt") && md.contains("### Inner"), "{md}");
    let nb = br##"{"cells":[{"cell_type":"markdown","source":["# T"]},{"cell_type":"code","source":"x=1"}],"metadata":{}}"##;
    assert_eq!(convert_bytes("n.ipynb", nb).unwrap(), "# T\n\n```python\nx=1\n```\n");
    assert!(convert_bytes("c.json", b"{\"a\":1}").unwrap().contains("```json"));
}

#[test]
fn single_file_and_damaged_input() {
    let t = tempfile::tempdir().unwrap();
    let f = t.path().join("note.html");
    write(&f, b"<p>Hi</p>");
    let dest = convert_single_file(&f, None).unwrap();
    assert_eq!(dest, t.path().join("note.md"));
    assert!(matches!(convert_bytes("x.docx", b"not a zip"), Err(ConvertError::Failed(_))));
    assert!(matches!(convert_bytes("x.pdf", b"%PDF-garbage"), Err(ConvertError::Failed(_))));
    assert!(matches!(convert_bytes("x.exe", &[0x4d, 0x5a, 0, 0]), Err(ConvertError::Unsupported(_))));
}
