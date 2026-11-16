//! ZIP archives: every convertible entry, one section each. Guarded against zip bombs.

use std::io::Read;

use anyhow::{anyhow, Result};

use super::open_zip;
use crate::convert::{convert_depth, ConvertError};

const MAX_ENTRIES: usize = 2000;
const MAX_TOTAL: u64 = 512 * 1024 * 1024;
const MAX_DEPTH: u8 = 2;

pub fn convert(name: &str, bytes: &[u8], depth: u8) -> Result<String> {
    if depth >= MAX_DEPTH {
        return Err(anyhow!("Archive nested too deeply"));
    }
    let mut zip = open_zip(bytes)?;
    let mut parts = vec![format!("# Contents of {name}")];
    let mut total = 0u64;
    let mut converted = 0;
    for i in 0..zip.len().min(MAX_ENTRIES) {
        let mut f = zip.by_index(i)?;
        if f.is_dir() {
            continue;
        }
        let inner = f.name().to_string();
        if inner.starts_with("__MACOSX/") || inner.rsplit('/').next().is_some_and(|b| b.starts_with('.')) {
            continue;
        }
        total += f.size();
        if total > MAX_TOTAL {
            parts.push("_Archive truncated: too much data to expand._".into());
            break;
        }
        let mut data = Vec::with_capacity(f.size() as usize);
        f.read_to_end(&mut data)?;
        drop(f);
        match convert_depth(&inner, &data, depth + 1) {
            Ok(md) => {
                converted += 1;
                parts.push(format!("## {inner}\n\n{}", demote(md.trim())));
            }
            Err(ConvertError::Unsupported(_)) => parts.push(format!("## {inner}\n\n_Skipped: no converter for this file type._")),
            Err(e) => parts.push(format!("## {inner}\n\n_Could not convert: {e}_")),
        }
    }
    if converted == 0 && parts.len() == 1 {
        return Err(anyhow!("Archive is empty"));
    }
    Ok(parts.join("\n\n") + "\n")
}

/// Push headings down two levels so inner documents nest under their `##` entry.
pub(crate) fn demote(md: &str) -> String {
    let mut fence = false;
    md.lines()
        .map(|l| {
            if l.trim_start().starts_with("```") {
                fence = !fence;
            }
            if !fence && l.starts_with('#') {
                let n = l.chars().take_while(|c| *c == '#').count();
                if l[n..].starts_with(' ') {
                    return format!("{}{}", "#".repeat((n + 2).min(6)), &l[n..]);
                }
            }
            l.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
