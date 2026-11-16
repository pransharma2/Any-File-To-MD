//! EPUB e-books: chapters in reading (spine) order.

use std::collections::HashMap;

use anyhow::{anyhow, Result};
use quick_xml::events::Event;
use quick_xml::Reader;

use super::text::decode;
use super::{attr, open_zip, zip_entry};

pub fn convert(bytes: &[u8]) -> Result<String> {
    let mut zip = open_zip(bytes)?;
    let container = zip_entry(&mut zip, "META-INF/container.xml")?.ok_or_else(|| anyhow!("Not an EPUB (container.xml missing)"))?;
    let opf_path = find_attr(&container, b"rootfile", b"full-path").ok_or_else(|| anyhow!("EPUB has no package file"))?;
    let base = opf_path.rsplit_once('/').map(|(d, _)| format!("{d}/")).unwrap_or_default();
    let opf = zip_entry(&mut zip, &opf_path)?.ok_or_else(|| anyhow!("EPUB package file missing"))?;

    let mut manifest: HashMap<String, String> = HashMap::new();
    let mut spine: Vec<String> = Vec::new();
    let mut title = String::new();
    let mut in_title = false;
    let mut r = Reader::from_reader(opf.as_slice());
    let mut buf = Vec::new();
    loop {
        match r.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match e.local_name().as_ref() {
                b"item" => {
                    if let (Some(id), Some(href)) = (attr(&e, b"id"), attr(&e, b"href")) {
                        manifest.insert(id, href);
                    }
                }
                b"itemref" => {
                    if let Some(id) = attr(&e, b"idref") {
                        spine.push(id);
                    }
                }
                b"title" if title.is_empty() => in_title = true,
                _ => {}
            },
            Ok(Event::Text(t)) if in_title => {
                title = t.unescape().map(|s| s.trim().to_string()).unwrap_or_default();
                in_title = false;
            }
            Ok(Event::End(_)) => in_title = false,
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    let mut parts = Vec::new();
    if !title.is_empty() {
        parts.push(format!("# {title}"));
    }
    for id in spine {
        let Some(href) = manifest.get(&id) else { continue };
        let path = normalize(&format!("{base}{}", href.split('#').next().unwrap_or(href)));
        let Some(html) = zip_entry(&mut zip, &path)? else { continue };
        let md = super::html::convert(&decode(&html))?;
        if !md.trim().is_empty() {
            parts.push(md.trim().to_string());
        }
    }
    if parts.is_empty() {
        return Err(anyhow!("EPUB has no readable chapters"));
    }
    Ok(parts.join("\n\n") + "\n")
}

fn find_attr(xml: &[u8], tag: &[u8], key: &[u8]) -> Option<String> {
    let mut r = Reader::from_reader(xml);
    let mut buf = Vec::new();
    loop {
        match r.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if e.local_name().as_ref() == tag => return attr(&e, key),
            Ok(Event::Eof) | Err(_) => return None,
            _ => {}
        }
        buf.clear();
    }
}

/// Resolve `a/b/../c` and URL escapes in zip paths.
fn normalize(p: &str) -> String {
    let p = p.replace("%20", " ");
    let mut parts: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}
