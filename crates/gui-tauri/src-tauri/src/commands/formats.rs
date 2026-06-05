//! `get_formats` command — return the list of supported archive formats.

use serde::Serialize;

/// Information about a single archive/compression format.
#[derive(Debug, Serialize)]
pub struct FormatInfo {
    /// Machine-readable format name (e.g. `"zip"`, `"tar.gz"`, `"7z"`).
    pub name: String,
    /// Whether this format supports creating archives (compression).
    pub can_compress: bool,
    /// Whether this format supports extracting archives (decompression).
    pub can_decompress: bool,
}

/// Return the list of all formats supported by the GeeZipX engine.
///
/// This is a no-argument introspection command that the frontend can call
/// at startup to populate format selectors.
#[tauri::command]
pub fn get_formats() -> Vec<FormatInfo> {
    vec![
        FormatInfo {
            name: "zip".into(),
            can_compress: true,
            can_decompress: true,
        },
        FormatInfo {
            name: "tar".into(),
            can_compress: true,
            can_decompress: true,
        },
        FormatInfo {
            name: "tar.gz".into(),
            can_compress: true,
            can_decompress: true,
        },
        FormatInfo {
            name: "gzip".into(),
            can_compress: false,
            can_decompress: true,
        },
        FormatInfo {
            name: "zstd".into(),
            can_compress: false,
            can_decompress: true,
        },
        FormatInfo {
            name: "tar.zst".into(),
            can_compress: true,
            can_decompress: true,
        },
        FormatInfo {
            name: "xz".into(),
            can_compress: false,
            can_decompress: true,
        },
        FormatInfo {
            name: "tar.xz".into(),
            can_compress: true,
            can_decompress: true,
        },
        FormatInfo {
            name: "lzma".into(),
            can_compress: false,
            can_decompress: true,
        },
        FormatInfo {
            name: "7z".into(),
            can_compress: false,
            can_decompress: true,
        },
        FormatInfo {
            name: "rar".into(),
            can_compress: false,
            can_decompress: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_formats_returns_expected_count() {
        let formats = get_formats();
        assert_eq!(formats.len(), 11, "expected 11 supported formats");
    }

    #[test]
    fn get_formats_zip_compress_decompress() {
        let formats = get_formats();
        let zip = formats.iter().find(|f| f.name == "zip").unwrap();
        assert!(zip.can_compress);
        assert!(zip.can_decompress);
    }

    #[test]
    fn get_formats_seven_zip_decompress_only() {
        let formats = get_formats();
        let sz = formats.iter().find(|f| f.name == "7z").unwrap();
        assert!(!sz.can_compress);
        assert!(sz.can_decompress);
    }

    #[test]
    fn get_formats_rar_decompress_only() {
        let formats = get_formats();
        let rar = formats.iter().find(|f| f.name == "rar").unwrap();
        assert!(!rar.can_compress);
        assert!(rar.can_decompress);
    }
}
