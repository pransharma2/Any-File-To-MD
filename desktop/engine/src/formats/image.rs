//! Images: dimensions and EXIF metadata (no OCR). Audio/video: unsupported.

use std::io::Cursor;

use crate::convert::{ConvertError, Result};

pub fn convert(name: &str, ext: &str, bytes: &[u8]) -> Result<String> {
    let is_image = matches!(ext, "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "tif" | "tiff" | "heic" | "ico");
    if !is_image {
        return Err(ConvertError::Unsupported(format!(".{ext} media has no text to extract")));
    }
    let mut lines = Vec::new();
    if let Ok(size) = imagesize::blob_size(bytes) {
        lines.push(format!("- **Dimensions:** {} × {} px", size.width, size.height));
    }
    lines.push(format!("- **File size:** {}", human(bytes.len() as u64)));
    if let Ok(ex) = exif::Reader::new().read_from_container(&mut Cursor::new(bytes)) {
        let get = |tag| {
            ex.get_field(tag, exif::In::PRIMARY)
                .map(|f| f.display_value().with_unit(&ex).to_string().trim_matches('"').trim().to_string())
                .filter(|s| !s.is_empty())
        };
        let camera = match (get(exif::Tag::Make), get(exif::Tag::Model)) {
            (Some(m), Some(md)) if md.starts_with(&m) => Some(md),
            (Some(m), Some(md)) => Some(format!("{m} {md}")),
            (a, b) => a.or(b),
        };
        for (label, v) in [
            ("Camera", camera),
            ("Date taken", get(exif::Tag::DateTimeOriginal).or_else(|| get(exif::Tag::DateTime))),
            ("Description", get(exif::Tag::ImageDescription)),
            ("Artist", get(exif::Tag::Artist)),
            ("Exposure", get(exif::Tag::ExposureTime)),
            ("Aperture", get(exif::Tag::FNumber)),
            ("ISO", get(exif::Tag::PhotographicSensitivity)),
            ("Latitude", get(exif::Tag::GPSLatitude).zip(get(exif::Tag::GPSLatitudeRef)).map(|(a, b)| format!("{a} {b}"))),
            ("Longitude", get(exif::Tag::GPSLongitude).zip(get(exif::Tag::GPSLongitudeRef)).map(|(a, b)| format!("{a} {b}"))),
        ] {
            if let Some(v) = v {
                lines.push(format!("- **{label}:** {v}"));
            }
        }
    }
    Ok(format!("# {name}\n\n{}\n", lines.join("\n")))
}

fn human(n: u64) -> String {
    match n {
        n if n >= 1 << 20 => format!("{:.1} MB", n as f64 / (1u64 << 20) as f64),
        n if n >= 1 << 10 => format!("{:.0} KB", n as f64 / 1024.0),
        n => format!("{n} bytes"),
    }
}
