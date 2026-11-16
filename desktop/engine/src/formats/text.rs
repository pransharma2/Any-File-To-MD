//! Plain text, code, JSON/XML and Jupyter notebooks.

use anyhow::{anyhow, Result};

/// Decode bytes as text: BOM-aware UTF-8/UTF-16, then UTF-8, then Windows-1252.
pub fn decode(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return encoding_rs::UTF_16LE.decode(bytes).0.into_owned();
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return encoding_rs::UTF_16BE.decode(bytes).0.into_owned();
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_owned(),
        Err(_) => encoding_rs::WINDOWS_1252.decode(bytes).0.into_owned(),
    }
}

/// `# name` followed by the content in a fenced code block.
pub fn fenced(name: &str, lang: &str, bytes: &[u8]) -> String {
    let body = decode(bytes);
    let body = body.trim_end();
    // Use a fence longer than any backtick run inside the content.
    let longest = body
        .split(|c| c != '`')
        .map(str::len)
        .max()
        .unwrap_or(0);
    let fence = "`".repeat(longest.max(2) + 1);
    format!("# {name}\n\n{fence}{lang}\n{body}\n{fence}\n")
}

/// Normalise line endings, strip trailing spaces, and collapse runs of blank lines.
pub fn tidy(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut blank = 0;
    let mut in_fence = false;
    for line in s.replace("\r\n", "\n").replace('\r', "\n").split('\n') {
        let t = line.trim_end();
        if t.trim_start().starts_with("```") {
            in_fence = !in_fence;
        }
        if t.is_empty() && !in_fence {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(if in_fence { line } else { t });
        out.push('\n');
    }
    let trimmed = out.trim_matches('\n');
    format!("{trimmed}\n")
}

/// Jupyter notebook: markdown cells as-is, code cells fenced.
pub fn ipynb(bytes: &[u8]) -> Result<String> {
    let v: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| anyhow!("Invalid notebook JSON: {e}"))?;
    let lang = v
        .pointer("/metadata/kernelspec/language")
        .or_else(|| v.pointer("/metadata/language_info/name"))
        .and_then(|l| l.as_str())
        .unwrap_or("python")
        .to_string();
    let src = |c: &serde_json::Value| -> String {
        match c.get("source") {
            Some(serde_json::Value::Array(a)) => a.iter().filter_map(|x| x.as_str()).collect(),
            Some(serde_json::Value::String(s)) => s.clone(),
            _ => String::new(),
        }
    };
    let cells = v.get("cells").and_then(|c| c.as_array()).ok_or_else(|| anyhow!("Notebook has no cells"))?;
    let mut out = Vec::new();
    for c in cells {
        let text = src(c);
        if text.trim().is_empty() {
            continue;
        }
        match c.get("cell_type").and_then(|t| t.as_str()) {
            Some("markdown") => out.push(text.trim_end().to_string()),
            Some("code") => out.push(format!("```{lang}\n{}\n```", text.trim_end())),
            _ => out.push(text.trim_end().to_string()),
        }
    }
    Ok(out.join("\n\n") + "\n")
}
