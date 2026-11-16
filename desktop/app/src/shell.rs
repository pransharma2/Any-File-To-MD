//! Explorer right-click integration (per-user, no admin rights needed).
//!
//! * Right-click a folder            -> "Convert folder to Markdown"
//! * Right-click inside a folder     -> "Convert this folder to Markdown"
//! * Right-click a supported file    -> "Convert to Markdown" (writes name.md beside it)
//!
//! On Windows 11 these entries live under "Show more options" (or Shift+right-click).

pub const FILE_TYPES: &[&str] = &[
    "pdf", "docx", "pptx", "xlsx", "xls", "ods", "odt", "csv", "tsv", "html", "htm", "epub", "zip",
    "msg", "ipynb", "json", "xml", "txt",
];

#[cfg(windows)]
mod imp {
    use super::FILE_TYPES;
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    const VERB: &str = "AnyFileToMD";

    fn entries() -> Vec<(String, &'static str, &'static str)> {
        let mut v = vec![
            (format!(r"Software\Classes\Directory\shell\{VERB}"), "Convert folder to Markdown", "%1"),
            (format!(r"Software\Classes\Directory\Background\shell\{VERB}"), "Convert this folder to Markdown", "%V"),
        ];
        for ext in FILE_TYPES {
            v.push((format!(r"Software\Classes\SystemFileAssociations\.{ext}\shell\{VERB}"), "Convert to Markdown", "%1"));
        }
        v
    }

    fn exe() -> Result<String, String> {
        std::env::current_exe().map(|p| p.display().to_string()).map_err(|e| e.to_string())
    }

    pub fn register() -> Result<(), String> {
        let exe = exe()?;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        for (path, label, arg) in entries() {
            let (key, _) = hkcu.create_subkey(&path).map_err(|e| e.to_string())?;
            key.set_value("", &label).map_err(|e| e.to_string())?;
            key.set_value("Icon", &format!("\"{exe}\",0")).map_err(|e| e.to_string())?;
            let (cmd, _) = key.create_subkey("command").map_err(|e| e.to_string())?;
            cmd.set_value("", &format!("\"{exe}\" --convert \"{arg}\"")).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn unregister() -> Result<(), String> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        for (path, _, _) in entries() {
            match hkcu.delete_subkey_all(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        Ok(())
    }

    /// True when the folder entry exists and points at this executable.
    pub fn is_registered() -> bool {
        let Ok(exe) = exe() else { return false };
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(format!(r"Software\Classes\Directory\shell\{VERB}\command"))
            .and_then(|k| k.get_value::<String, _>(""))
            .map(|v| v.to_lowercase().contains(&exe.to_lowercase()))
            .unwrap_or(false)
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn register() -> Result<(), String> {
        Err("Right-click integration is only available on Windows.".into())
    }
    pub fn unregister() -> Result<(), String> {
        Ok(())
    }
    pub fn is_registered() -> bool {
        false
    }
}

pub use imp::{is_registered, register, unregister};

pub const SUPPORTED: bool = cfg!(windows);
