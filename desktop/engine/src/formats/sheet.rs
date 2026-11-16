//! Spreadsheets (.xlsx/.xls/.xlsb/.ods) and delimited text (.csv/.tsv).

use std::io::Cursor;

use anyhow::{anyhow, Result};
use calamine::{open_workbook_auto_from_rs, Data, Reader};

use super::md_table;
use super::text::decode;

fn fmt_cell(d: &Data) -> String {
    match d {
        Data::Empty => String::new(),
        Data::String(s) => s.clone(),
        Data::Float(f) => {
            if f.fract() == 0.0 && f.abs() < 1e15 {
                format!("{}", *f as i64)
            } else {
                format!("{f}")
            }
        }
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => b.to_string().to_uppercase(),
        Data::DateTime(dt) => match dt.as_datetime() {
            Some(t) => {
                let s = t.format("%Y-%m-%d %H:%M:%S").to_string();
                s.strip_suffix(" 00:00:00").map(str::to_string).unwrap_or(s)
            }
            None => dt.to_string(),
        },
        Data::DateTimeIso(s) | Data::DurationIso(s) => s.clone(),
        Data::Error(e) => format!("#{e:?}"),
    }
}

pub fn convert(bytes: &[u8]) -> Result<String> {
    let mut wb = open_workbook_auto_from_rs(Cursor::new(bytes.to_vec())).map_err(|e| {
        let m = e.to_string();
        if m.to_lowercase().contains("password") || m.to_lowercase().contains("encrypt") {
            anyhow!("File is password-protected")
        } else {
            anyhow!("Could not open spreadsheet: {m}")
        }
    })?;
    let names = wb.sheet_names().to_vec();
    let mut out = Vec::new();
    for name in names {
        let range = match wb.worksheet_range(&name) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let rows: Vec<Vec<String>> = range.rows().map(|r| r.iter().map(fmt_cell).collect()).collect();
        let table = md_table(&rows);
        if table.is_empty() {
            continue;
        }
        out.push(format!("## {name}\n\n{table}"));
    }
    if out.is_empty() {
        return Err(anyhow!("Spreadsheet is empty"));
    }
    Ok(out.join("\n"))
}

pub fn csv_to_md(bytes: &[u8], delim: u8) -> Result<String> {
    let text = decode(bytes);
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(delim)
        .has_headers(false)
        .flexible(true)
        .from_reader(text.as_bytes());
    let mut rows = Vec::new();
    for rec in rdr.records() {
        let rec = rec.map_err(|e| anyhow!("Malformed CSV: {e}"))?;
        rows.push(rec.iter().map(str::to_string).collect::<Vec<_>>());
    }
    let t = md_table(&rows);
    if t.is_empty() {
        return Err(anyhow!("File is empty"));
    }
    Ok(t)
}
