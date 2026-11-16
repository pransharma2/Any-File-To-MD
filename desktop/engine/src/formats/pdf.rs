//! PDF text extraction (pure Rust, no OCR).

use anyhow::{anyhow, Result};

pub fn convert(bytes: &[u8]) -> Result<String> {
    let text = pdf_extract::extract_text_from_mem(bytes).map_err(|e| {
        let m = e.to_string();
        if m.to_lowercase().contains("encrypt") || m.to_lowercase().contains("password") {
            anyhow!("File is password-protected")
        } else {
            anyhow!("Could not read PDF: {m}")
        }
    })?;
    let cleaned = clean(&text);
    if cleaned.trim().is_empty() {
        return Err(anyhow!("No text layer (scanned PDF?). OCR is not supported"));
    }
    Ok(cleaned)
}

/// Re-flow extracted text: join hard-wrapped lines inside paragraphs,
/// keep blank lines as paragraph breaks, and mark page breaks.
fn clean(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for (i, page) in raw.split('\u{c}').enumerate() {
        if page.trim().is_empty() {
            continue;
        }
        if i > 0 && !out.is_empty() {
            out.push_str("\n\n---\n\n");
        }
        let mut para = String::new();
        for line in page.lines() {
            let l = line.trim();
            if l.is_empty() {
                if !para.is_empty() {
                    out.push_str(para.trim());
                    out.push_str("\n\n");
                    para.clear();
                }
                continue;
            }
            if para.ends_with('-') && !para.ends_with(" -") {
                para.pop(); // de-hyphenate "conver-\nsion"
            } else if !para.is_empty() {
                para.push(' ');
            }
            para.push_str(l);
        }
        if !para.is_empty() {
            out.push_str(para.trim());
            out.push_str("\n\n");
        }
    }
    out
}
