//! One module per family of file formats.

pub mod archive;
pub mod docx;
pub mod epub;
pub mod html;
pub mod image;
pub mod msg;
pub mod odt;
pub mod pdf;
pub mod pptx;
pub mod sheet;
pub mod text;

use std::io::{Cursor, Read};

use anyhow::{anyhow, Context};
use quick_xml::events::BytesStart;

/// Read one entry of a zip container into memory (None if missing).
pub(crate) fn zip_entry(
    zip: &mut zip::ZipArchive<Cursor<&[u8]>>,
    name: &str,
) -> anyhow::Result<Option<Vec<u8>>> {
    let mut f = match zip.by_name(name) {
        Ok(f) => f,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    if f.size() > 512 * 1024 * 1024 {
        return Err(anyhow!("Entry {name} is too large"));
    }
    let mut buf = Vec::with_capacity(f.size() as usize);
    f.read_to_end(&mut buf).with_context(|| format!("reading {name}"))?;
    Ok(Some(buf))
}

pub(crate) fn open_zip(bytes: &[u8]) -> anyhow::Result<zip::ZipArchive<Cursor<&[u8]>>> {
    zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| {
        let m = e.to_string();
        if m.contains("encrypt") || m.contains("password") {
            anyhow!("File is password-protected")
        } else {
            anyhow!("Not a valid Office/zip file ({m})")
        }
    })
}

/// Value of an attribute matched by local name (ignores namespace prefix).
pub(crate) fn attr(e: &BytesStart, key: &[u8]) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.local_name().as_ref() == key)
        .and_then(|a| a.unescape_value().ok().map(|v| v.into_owned()))
}

/// Escape characters that would break a Markdown table cell.
pub(crate) fn cell(s: &str) -> String {
    s.trim().replace('|', "\\|").replace("\r\n", "<br>").replace('\n', "<br>")
}

/// Render rows as a GitHub-flavoured Markdown table, first row as the header.
pub(crate) fn md_table(rows: &[Vec<String>]) -> String {
    let rows: Vec<&Vec<String>> = rows.iter().filter(|r| r.iter().any(|c| !c.trim().is_empty())).collect();
    if rows.is_empty() {
        return String::new();
    }
    let width = rows.iter().map(|r| r.len()).max().unwrap_or(0).max(1);
    let line = |r: &Vec<String>| {
        let mut s = String::from("|");
        for i in 0..width {
            s.push(' ');
            s.push_str(&cell(r.get(i).map(String::as_str).unwrap_or("")));
            s.push_str(" |");
        }
        s
    };
    let mut out = vec![line(rows[0]), format!("|{}", " --- |".repeat(width))];
    out.extend(rows[1..].iter().map(|r| line(r)));
    out.join("\n") + "\n"
}
