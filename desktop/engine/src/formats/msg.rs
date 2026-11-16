//! Outlook .msg e-mails (OLE compound files).

use std::io::{Cursor, Read};

use anyhow::{anyhow, Result};

fn prop(comp: &mut cfb::CompoundFile<Cursor<&[u8]>>, id: &str) -> Option<String> {
    for (suffix, utf16) in [("001F", true), ("001E", false)] {
        let path = format!("/__substg1.0_{id}{suffix}");
        if let Ok(mut s) = comp.open_stream(&path) {
            let mut buf = Vec::new();
            if s.read_to_end(&mut buf).is_err() {
                continue;
            }
            let text = if utf16 {
                encoding_rs::UTF_16LE.decode(&buf).0.into_owned()
            } else {
                encoding_rs::WINDOWS_1252.decode(&buf).0.into_owned()
            };
            let text = text.trim_end_matches('\0').trim().to_string();
            if !text.is_empty() {
                return Some(text);
            }
        }
    }
    None
}

pub fn convert(bytes: &[u8]) -> Result<String> {
    let mut comp = cfb::CompoundFile::open(Cursor::new(bytes)).map_err(|e| anyhow!("Not a valid Outlook message: {e}"))?;
    let subject = prop(&mut comp, "0037");
    let from_name = prop(&mut comp, "0C1A");
    let from_addr = prop(&mut comp, "0C1F").or_else(|| prop(&mut comp, "5D01"));
    let to = prop(&mut comp, "0E04");
    let cc = prop(&mut comp, "0E03");
    let body = prop(&mut comp, "1000");
    let html_body = if body.is_none() {
        prop(&mut comp, "1013").and_then(|h| super::html::convert(&h).ok())
    } else {
        None
    };
    if subject.is_none() && body.is_none() && html_body.is_none() {
        return Err(anyhow!("Message has no readable content"));
    }
    let from = match (from_name, from_addr) {
        (Some(n), Some(a)) if n != a => Some(format!("{n} <{a}>")),
        (Some(n), _) => Some(n),
        (None, a) => a,
    };
    let mut out = String::from("# Email Message\n\n");
    for (label, v) in [("From", from), ("To", to), ("Cc", cc), ("Subject", subject)] {
        if let Some(v) = v {
            out.push_str(&format!("**{label}:** {v}  \n"));
        }
    }
    out.push_str("\n## Content\n\n");
    out.push_str(body.or(html_body).unwrap_or_default().trim());
    out.push('\n');
    Ok(out)
}
