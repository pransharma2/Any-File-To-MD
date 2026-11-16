//! OpenDocument text (.odt): headings, paragraphs, lists and tables.

use anyhow::{anyhow, Result};
use quick_xml::events::Event;
use quick_xml::Reader;

use super::{attr, md_table, open_zip, zip_entry};

pub fn convert(bytes: &[u8]) -> Result<String> {
    let mut zip = open_zip(bytes)?;
    let xml = zip_entry(&mut zip, "content.xml")?.ok_or_else(|| anyhow!("Not an OpenDocument file"))?;
    let mut r = Reader::from_reader(xml.as_slice());
    let mut buf = Vec::new();
    let mut blocks: Vec<String> = Vec::new();
    let mut cur: Option<(String, Option<usize>)> = None; // (text, heading level)
    let mut list_depth = 0usize;
    let mut table: Option<Vec<Vec<String>>> = None;
    let mut cell: Option<String> = None;
    loop {
        match r.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match e.local_name().as_ref() {
                b"h" => cur = Some((String::new(), Some(attr(&e, b"outline-level").and_then(|l| l.parse().ok()).unwrap_or(1)))),
                b"p" => cur = Some((String::new(), None)),
                b"list" => list_depth += 1,
                b"table" => table = Some(Vec::new()),
                b"table-row" => {
                    if let Some(t) = table.as_mut() {
                        t.push(Vec::new());
                    }
                }
                b"table-cell" => cell = Some(String::new()),
                _ => {}
            },
            Ok(Event::Empty(e)) => {
                if let Some((t, _)) = cur.as_mut() {
                    match e.local_name().as_ref() {
                        b"s" => t.push(' '),
                        b"tab" => t.push('\t'),
                        b"line-break" => t.push_str("  \n"),
                        _ => {}
                    }
                }
            }
            Ok(Event::Text(t)) => {
                if let (Some((s, _)), Ok(txt)) = (cur.as_mut(), t.unescape()) {
                    s.push_str(&txt);
                }
            }
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"h" | b"p" => {
                    if let Some((text, level)) = cur.take() {
                        let text = text.trim().to_string();
                        if text.is_empty() {
                            continue;
                        }
                        if let Some(c) = cell.as_mut() {
                            if !c.is_empty() {
                                c.push_str("<br>");
                            }
                            c.push_str(&text);
                        } else if let Some(l) = level {
                            blocks.push(format!("{} {text}", "#".repeat(l.clamp(1, 6))));
                        } else if list_depth > 0 {
                            blocks.push(format!("{}- {text}", "  ".repeat(list_depth - 1)));
                        } else {
                            blocks.push(text);
                        }
                    }
                }
                b"list" => list_depth = list_depth.saturating_sub(1),
                b"table-cell" => {
                    if let (Some(t), Some(c)) = (table.as_mut(), cell.take()) {
                        if let Some(row) = t.last_mut() {
                            row.push(c);
                        }
                    }
                }
                b"table" => {
                    if let Some(t) = table.take() {
                        blocks.push(md_table(&t));
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(e) => return Err(anyhow!("Damaged document XML: {e}")),
            _ => {}
        }
        buf.clear();
    }
    Ok(blocks.join("\n\n") + "\n")
}
