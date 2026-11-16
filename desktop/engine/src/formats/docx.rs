//! Word .docx: headings, lists, bold/italic, links and tables.

use std::collections::HashMap;

use anyhow::{anyhow, Result};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use super::{attr, md_table, open_zip, zip_entry};

#[derive(Clone, Default, PartialEq)]
struct Fmt {
    bold: bool,
    italic: bool,
    link: Option<String>,
}

#[derive(Default)]
struct Para {
    style: Option<String>,
    list_level: Option<usize>,
    runs: Vec<(String, Fmt)>,
}

pub fn convert(bytes: &[u8]) -> Result<String> {
    let mut zip = open_zip(bytes)?;
    let doc = zip_entry(&mut zip, "word/document.xml")?.ok_or_else(|| anyhow!("Not a Word document (word/document.xml missing)"))?;
    let styles = zip_entry(&mut zip, "word/styles.xml")?.map(|s| style_names(&s)).unwrap_or_default();
    let rels = zip_entry(&mut zip, "word/_rels/document.xml.rels")?.map(|r| relationships(&r)).unwrap_or_default();
    body_to_md(&doc, &styles, &rels)
}

/// styleId -> lower-cased style name ("Heading1" -> "heading 1").
fn style_names(xml: &[u8]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut r = Reader::from_reader(xml);
    let mut buf = Vec::new();
    let mut cur: Option<String> = None;
    loop {
        match r.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match e.local_name().as_ref() {
                b"style" => cur = attr(&e, b"styleId"),
                b"name" => {
                    if let (Some(id), Some(n)) = (cur.clone(), attr(&e, b"val")) {
                        map.insert(id, n.to_lowercase());
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    map
}

/// Relationship id -> target (used for hyperlinks).
pub(crate) fn relationships(xml: &[u8]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut r = Reader::from_reader(xml);
    let mut buf = Vec::new();
    loop {
        match r.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if e.local_name().as_ref() == b"Relationship" => {
                if let (Some(id), Some(t)) = (attr(&e, b"Id"), attr(&e, b"Target")) {
                    map.insert(id, t);
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    map
}

fn heading_level(style: &str, names: &HashMap<String, String>) -> Option<usize> {
    let name = names.get(style).map(String::as_str).unwrap_or(style).to_lowercase();
    let name = name.replace(' ', "");
    if name == "title" {
        return Some(1);
    }
    if name == "subtitle" {
        return Some(2);
    }
    name.strip_prefix("heading").and_then(|n| n.parse::<usize>().ok()).map(|n| n.clamp(1, 6))
}

fn on_off(e: &BytesStart) -> bool {
    !matches!(attr(e, b"val").as_deref(), Some("0") | Some("false") | Some("none"))
}

fn render_runs(runs: &[(String, Fmt)]) -> String {
    // Merge neighbouring runs with identical formatting, then wrap.
    let mut merged: Vec<(String, Fmt)> = Vec::new();
    for (t, f) in runs {
        match merged.last_mut() {
            Some((lt, lf)) if lf == f => lt.push_str(t),
            _ => merged.push((t.clone(), f.clone())),
        }
    }
    let mut out = String::new();
    for (t, f) in merged {
        if t.trim().is_empty() {
            out.push_str(&t);
            continue;
        }
        // Keep surrounding whitespace outside the markers so `** text**` never happens.
        let lead = &t[..t.len() - t.trim_start().len()];
        let trail = &t[t.trim_end().len()..];
        let mut core = t.trim().to_string();
        if f.bold && f.italic {
            core = format!("***{core}***");
        } else if f.bold {
            core = format!("**{core}**");
        } else if f.italic {
            core = format!("*{core}*");
        }
        if let Some(href) = &f.link {
            core = format!("[{core}]({href})");
        }
        out.push_str(lead);
        out.push_str(&core);
        out.push_str(trail);
    }
    out
}

fn body_to_md(xml: &[u8], styles: &HashMap<String, String>, rels: &HashMap<String, String>) -> Result<String> {
    let mut r = Reader::from_reader(xml);
    let mut buf = Vec::new();
    let mut blocks: Vec<String> = Vec::new();

    // Table state (a stack so nested tables flatten into their parent cell).
    let mut tables: Vec<Vec<Vec<String>>> = Vec::new();
    let mut cell_text: Vec<Vec<String>> = Vec::new(); // paragraphs of the current cell, per depth

    let mut para: Option<Para> = None;
    let mut fmt = Fmt::default();
    let mut in_rpr = false;
    let mut in_text = false;
    let mut link_stack: Vec<Option<String>> = Vec::new();

    loop {
        let ev = r.read_event_into(&mut buf).map_err(|e| anyhow!("Damaged document XML: {e}"))?;
        match ev {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty = matches!(ev, Event::Empty(_));
                match e.local_name().as_ref() {
                    b"tbl" if !empty => {
                        tables.push(Vec::new());
                    }
                    b"tr" if !empty => {
                        if let Some(t) = tables.last_mut() {
                            t.push(Vec::new());
                        }
                    }
                    b"tc" if !empty => cell_text.push(Vec::new()),
                    b"p" => {
                        para = Some(Para::default());
                        if empty {
                            para = None;
                        }
                    }
                    b"pStyle" => {
                        if let Some(p) = para.as_mut() {
                            p.style = attr(e, b"val");
                        }
                    }
                    b"ilvl" => {
                        if let Some(p) = para.as_mut() {
                            p.list_level = Some(attr(e, b"val").and_then(|v| v.parse().ok()).unwrap_or(0));
                        }
                    }
                    b"numId" => {
                        if let Some(p) = para.as_mut() {
                            let id = attr(e, b"val").unwrap_or_default();
                            if id == "0" {
                                p.list_level = None;
                            } else if p.list_level.is_none() {
                                p.list_level = Some(0);
                            }
                        }
                    }
                    b"r" if !empty => {
                        fmt.bold = false;
                        fmt.italic = false;
                    }
                    b"rPr" if !empty => in_rpr = true,
                    b"b" if in_rpr => fmt.bold = on_off(e),
                    b"i" if in_rpr => fmt.italic = on_off(e),
                    b"t" if !empty => in_text = true,
                    b"tab" if !in_rpr => push_text(&mut para, "\t", &fmt),
                    b"br" | b"cr" => push_text(&mut para, "  \n", &fmt),
                    b"hyperlink" if !empty => {
                        let target = attr(e, b"id").and_then(|id| rels.get(&id).cloned());
                        link_stack.push(target.clone());
                        fmt.link = target;
                    }
                    _ => {}
                }
            }
            Event::Text(t) if in_text => {
                let s = t.unescape().map_err(|e| anyhow!("Damaged text: {e}"))?;
                push_text(&mut para, &s, &fmt);
            }
            Event::End(ref e) => match e.local_name().as_ref() {
                b"t" => in_text = false,
                b"rPr" => in_rpr = false,
                b"hyperlink" => {
                    link_stack.pop();
                    fmt.link = link_stack.last().cloned().flatten();
                }
                b"p" => {
                    if let Some(p) = para.take() {
                        let line = finish_para(&p, styles, !cell_text.is_empty());
                        if let Some(cell) = cell_text.last_mut() {
                            if !line.trim().is_empty() {
                                cell.push(line);
                            }
                        } else if !line.trim().is_empty() {
                            blocks.push(line);
                        }
                    }
                }
                b"tc" => {
                    let text = cell_text.pop().unwrap_or_default().join("<br>");
                    if let Some(row) = tables.last_mut().and_then(|t| t.last_mut()) {
                        row.push(text);
                    }
                }
                b"tbl" => {
                    if let Some(t) = tables.pop() {
                        let md = md_table(&t);
                        if let Some(cell) = cell_text.last_mut() {
                            // Nested table: flatten its text into the enclosing cell.
                            let flat: Vec<String> = t.into_iter().map(|r| r.join(" / ")).collect();
                            cell.push(flat.join("<br>"));
                        } else if !md.is_empty() {
                            blocks.push(md);
                        }
                    }
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(join_blocks(&blocks))
}

fn push_text(para: &mut Option<Para>, s: &str, fmt: &Fmt) {
    if let Some(p) = para.as_mut() {
        p.runs.push((s.to_string(), fmt.clone()));
    }
}

fn finish_para(p: &Para, styles: &HashMap<String, String>, in_cell: bool) -> String {
    let text = render_runs(&p.runs);
    if in_cell {
        return text.trim().to_string();
    }
    if let Some(level) = p.style.as_deref().and_then(|s| heading_level(s, styles)) {
        // Headings: drop bold/italic markers, they add noise.
        let plain: String = p.runs.iter().map(|(t, _)| t.as_str()).collect();
        return format!("{} {}", "#".repeat(level), plain.trim());
    }
    let is_list_style = p
        .style
        .as_deref()
        .map(|s| styles.get(s).map(String::as_str).unwrap_or(s).to_lowercase().contains("list"))
        .unwrap_or(false);
    if let Some(level) = p.list_level.or(if is_list_style { Some(0) } else { None }) {
        return format!("{}- {}", "  ".repeat(level.min(8)), text.trim());
    }
    text.trim().to_string()
}

/// Join blocks with blank lines, but keep consecutive list items together.
fn join_blocks(blocks: &[String]) -> String {
    let is_item = |s: &str| s.trim_start().starts_with("- ");
    let mut out = String::new();
    for (i, b) in blocks.iter().enumerate() {
        if i > 0 {
            out.push_str(if is_item(b) && is_item(&blocks[i - 1]) { "\n" } else { "\n\n" });
        }
        out.push_str(b);
    }
    out.push('\n');
    out
}
