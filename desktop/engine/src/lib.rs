//! Any File To MD engine: convert one file, or mirror a whole folder into Markdown.
//!
//! Rules (kept identical to the Python app's `converter.py`):
//! * the output is a mirror of the source tree, one `.md` per file;
//! * `a.pdf` + `a.docx` in the same folder become `a.pdf.md` / `a.docx.md`;
//! * one bad file never stops the batch; failures go to `_conversion-report.md`;
//! * originals are only ever read.

pub mod convert;
pub mod folder;
pub mod formats;

pub use convert::{convert_bytes, convert_path, is_media, ConvertError};
pub use folder::{
    collect_files, convert_folder, convert_single_file, default_output, plan_names, validate,
    Item, Job, Options, Status,
};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
