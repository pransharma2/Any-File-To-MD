//! Browser bindings: the website converts files entirely in the visitor's browser.

use anytomd_engine::{convert_bytes, ConvertError};
use wasm_bindgen::prelude::*;

/// Convert one file. Errors start with `unsupported:` or `failed:`.
#[wasm_bindgen]
pub fn convert(name: &str, bytes: &[u8]) -> Result<String, JsValue> {
    convert_bytes(name, bytes).map_err(|e| {
        JsValue::from_str(&match e {
            ConvertError::Unsupported(m) => format!("unsupported:{m}"),
            ConvertError::Failed(m) => format!("failed:{m}"),
        })
    })
}

/// `doc`, `media` or `other`, for the file picker.
#[wasm_bindgen]
pub fn kind(name: &str) -> String {
    anytomd_engine::file_kind(&anytomd_engine::convert::ext_of(name)).to_string()
}

#[wasm_bindgen]
pub fn version() -> String {
    anytomd_engine::VERSION.to_string()
}
