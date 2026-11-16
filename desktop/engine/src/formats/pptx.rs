//! PowerPoint .pptx: slide titles, text, tables and speaker notes.

use anyhow::{anyhow, Result};
use quick_xml::events::Event;
use quick_xml::Reader;

use super::docx::relationships;
use super::{attr, md_table, open_zip, zip_entry};

pub fn convert(bytes: &[u8]) -> Result<String> {
    let mut zip = open_zip(bytes)?;
    let mut slides: Vec<(u32, String)> = zip
        .file_names()
        .filter_map(|n| {
            let num = n.strip_prefix("ppt/slides/slide")?.strip_suffix(".xml")?;
            Some((num.parse().ok()?, n.to_string()))
        })
        .collect();
    if slides.is_empty() {
        return Err(anyhow!("Presentation has no slides"));
    }
    slides.sort();

    let mut out = Vec::new();
    for (num, name) in slides {
        let xml = zip_entry(&mut zip, &name)?.unwrap_or_default();
        let (title, body) = slide_text(&xml, false);
        let mut s = format!("<!-- Slide {num} -->\n");
        if let Some(t) = title {
            s.push_str(&format!("# {t}\n"));
        }
        if !body.is_empty() {
            s.push('\n');
            s.push_str(&body.join("\n\n"));
            s.push('\n');
        }
        // Speaker notes via the slide's relationships.
        let rels_name = format!("ppt/slides/_rels/slide{num}.xml.rels");
        if let Some(rels) = zip_entry(&mut zip, &rels_name)? {
            let notes = relationships(&rels)
                .into_values()
                .find(|t| t.contains("notesSlide"))
                .map(|t| format!("ppt/notesSlides/{}", t.rsplit('/').next().unwrap_or("")));
            if let Some(n) = notes {
                if let Some(nx) = zip_entry(&mut zip, &n)? {
                    let (_, note_body) = slide_text(&nx, true);
                    if !note_body.is_empty() {
                        s.push_str(&format!("\n### Notes\n\n{}\n", note_body.join("\n\n")));
                    }
                }
            }
        }
        out.push(s);
    }
    Ok(out.join("\n"))
}

/// Returns (title, paragraphs/tables). For notes, only body placeholders count.
fn slide_text(xml: &[u8], notes: bool) -> (Option<String>, Vec<String>) {
    let mut r = Reader::from_reader(xml);
    let mut buf = Vec::new();
    let mut title: Option<String> = None;
    let mut body = Vec::new();

    let mut shape_kind: Option<String> = None; // placeholder type of the current shape
    let mut in_shape = false;
    let mut shape_paras: Vec<(usize, String)> = Vec::new();
    let mut para = String::new();
    let mut in_t = false;
    let mut level = 0usize;
    let mut table: Option<Vec<Vec<String>>> = None;
    let mut cell: Option<String> = None;

    loop {
        match r.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match e.local_name().as_ref() {
                b"sp" => {
                    in_shape = true;
                    shape_kind = None;
                    shape_paras.clear();
                }
                b"ph" => shape_kind = Some(attr(&e, b"type").unwrap_or_else(|| "body".into())),
                b"tbl" => table = Some(Vec::new()),
                b"tr" => {
                    if let Some(t) = table.as_mut() {
                        t.push(Vec::new());
                    }
                }
                b"tc" => cell = Some(String::new()),
                b"p" => {
                    para.clear();
                    level = 0;
                }
                b"pPr" => level = attr(&e, b"lvl").and_then(|l| l.parse().ok()).unwrap_or(0),
                b"t" => in_t = true,
                _ => {}
            },
            Ok(Event::Empty(e)) => match e.local_name().as_ref() {
                b"ph" => shape_kind = Some(attr(&e, b"type").unwrap_or_else(|| "body".into())),
                b"pPr" => level = attr(&e, b"lvl").and_then(|l| l.parse().ok()).unwrap_or(0),
                b"br" => para.push(' '),
                _ => {}
            },
            Ok(Event::Text(t)) if in_t => {
                if let Ok(s) = t.unescape() {
                    para.push_str(&s);
                }
            }
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"t" => in_t = false,
                b"p" => {
                    let p = para.trim().to_string();
                    if p.is_empty() {
                        continue;
                    }
                    if let Some(c) = cell.as_mut() {
                        if !c.is_empty() {
                            c.push(' ');
                        }
                        c.push_str(&p);
                    } else if in_shape {
                        shape_paras.push((level, p));
                    }
                }
                b"tc" => {
                    if let (Some(t), Some(c)) = (table.as_mut(), cell.take()) {
                        if let Some(row) = t.last_mut() {
                            row.push(c);
                        }
                    }
                }
                b"tbl" => {
                    if let Some(t) = table.take() {
                        let md = md_table(&t);
                        if !md.is_empty() && !notes {
                            body.push(md.trim_end().to_string());
                        }
                    }
                }
                b"sp" => {
                    in_shape = false;
                    let placeholder = shape_kind.is_some();
                    let kind = shape_kind.take().unwrap_or_default();
                    // Body placeholders are bullet lists; free text boxes are paragraphs.
                    let bullets = placeholder && kind != "title" && kind != "ctrTitle" && kind != "subTitle"
                        && (shape_paras.len() > 1 || shape_paras.iter().any(|(l, _)| *l > 0));
                    let text = if bullets {
                        shape_paras.iter().map(|(l, p)| format!("{}- {p}", "  ".repeat((*l).min(6)))).collect::<Vec<_>>().join("\n")
                    } else if kind == "title" || kind == "ctrTitle" {
                        shape_paras.iter().map(|(_, p)| p.as_str()).collect::<Vec<_>>().join(" ")
                    } else {
                        shape_paras.iter().map(|(_, p)| p.as_str()).collect::<Vec<_>>().join("\n\n")
                    };
                    if text.trim().is_empty() {
                        continue;
                    }
                    if notes {
                        if kind == "body" {
                            body.push(text);
                        }
                    } else if (kind == "title" || kind == "ctrTitle") && title.is_none() {
                        title = Some(text.replace('\n', " "));
                    } else if kind != "sldNum" && kind != "dt" && kind != "ftr" {
                        body.push(text);
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    (title, body)
}
