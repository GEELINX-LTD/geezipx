//! `get_formats` command — return the list of supported archive formats.

use serde::Serialize;

/// Information about a single archive/compression format.
#[derive(Debug, Serialize)]
pub struct FormatInfo {
    /// Machine-readable format name (e.g. `"zip"`, `"tar.gz"`, `"7z"`).
    pub name: String,
    pub can_compress: bool,
    pub can_decompress: bool,
    /// Whether this format supports password/encryption.
    pub supports_encryption: bool,
    /// Human-readable level hint (e.g. "6 (0-9)"), or None if not applicable.
    pub level_hint: Option<String>,
}

/// Return the list of all formats supported by the GeeZipX engine.
///
/// This is a no-argument introspection command that the frontend can call
/// at startup to populate format selectors.
#[tauri::command]
pub fn get_formats() -> Vec<FormatInfo> {
    let formats = vec![
        FormatInfo {
            name: "zip".into(),
            can_compress: true,
            can_decompress: true,
            supports_encryption: true,
            level_hint: Some("6 (0-9)".into()),
        },
        FormatInfo {
            name: "zipx".into(),
            can_compress: true,
            can_decompress: true,
            supports_encryption: true,
            level_hint: Some("6 (0-9)".into()),
        },
        FormatInfo {
            name: "tar".into(),
            can_compress: true,
            can_decompress: true,
            supports_encryption: false,
            level_hint: None,
        },
        FormatInfo {
            name: "tar.gz".into(),
            can_compress: true,
            can_decompress: true,
            supports_encryption: false,
            level_hint: Some("6 (0-9)".into()),
        },
        FormatInfo {
            name: "gzip".into(),
            can_compress: false,
            can_decompress: true,
            supports_encryption: false,
            level_hint: Some("6 (0-9)".into()),
        },
        FormatInfo {
            name: "bzip2".into(),
            can_compress: false,
            can_decompress: true,
            supports_encryption: false,
            level_hint: Some("9 (1-9)".into()),
        },
        FormatInfo {
            name: "tar.bz2".into(),
            can_compress: true,
            can_decompress: true,
            supports_encryption: false,
            level_hint: Some("9 (1-9)".into()),
        },
        FormatInfo {
            name: "brotli".into(),
            can_compress: false,
            can_decompress: true,
            supports_encryption: false,
            level_hint: Some("11 (0-11)".into()),
        },
        FormatInfo {
            name: "tar.br".into(),
            can_compress: true,
            can_decompress: true,
            supports_encryption: false,
            level_hint: Some("11 (0-11)".into()),
        },
        FormatInfo {
            name: "lz4".into(),
            can_compress: false,
            can_decompress: true,
            supports_encryption: false,
            level_hint: None,
        },
        FormatInfo {
            name: "tar.lz4".into(),
            can_compress: true,
            can_decompress: true,
            supports_encryption: false,
            level_hint: None,
        },
        FormatInfo {
            name: "zstd".into(),
            can_compress: false,
            can_decompress: true,
            supports_encryption: false,
            level_hint: Some("3 (1-22)".into()),
        },
        FormatInfo {
            name: "tar.zst".into(),
            can_compress: true,
            can_decompress: true,
            supports_encryption: false,
            level_hint: Some("3 (1-22)".into()),
        },
        FormatInfo {
            name: "xz".into(),
            can_compress: false,
            can_decompress: true,
            supports_encryption: false,
            level_hint: Some("6 (0-9)".into()),
        },
        FormatInfo {
            name: "tar.xz".into(),
            can_compress: true,
            can_decompress: true,
            supports_encryption: false,
            level_hint: Some("6 (0-9)".into()),
        },
        FormatInfo {
            name: "lzma".into(),
            can_compress: false,
            can_decompress: true,
            supports_encryption: false,
            level_hint: Some("6 (0-9)".into()),
        },
        FormatInfo {
            name: "7z".into(),
            can_compress: true,
            can_decompress: true,
            supports_encryption: true,
            level_hint: None,
        },
        FormatInfo {
            name: "rar".into(),
            can_compress: false,
            can_decompress: true,
            supports_encryption: true,
            level_hint: None,
        },
        FormatInfo {
            name: "cab".into(),
            can_compress: false,
            can_decompress: true,
            supports_encryption: false,
            level_hint: None,
        },
        FormatInfo {
            name: "asar".into(),
            can_compress: false,
            can_decompress: true,
            supports_encryption: false,
            level_hint: None,
        },
        FormatInfo {
            name: "deb".into(),
            can_compress: false,
            can_decompress: true,
            supports_encryption: false,
            level_hint: None,
        },
        FormatInfo {
            name: "lzh".into(),
            can_compress: true,
            can_decompress: true,
            supports_encryption: false,
            level_hint: Some("2 (0-4)".into()),
        },
        FormatInfo {
            name: "iso".into(),
            can_compress: true,
            can_decompress: true,
            supports_encryption: false,
            level_hint: None,
        },
        FormatInfo {
            name: "cpio".into(),
            can_compress: true,
            can_decompress: true,
            supports_encryption: false,
            level_hint: None,
        },
    ];

    #[cfg(feature = "zpaq")]
    let formats = {
        let mut formats = formats;
        formats.push(FormatInfo {
            name: "zpaq".into(),
            can_compress: true,
            can_decompress: true,
            supports_encryption: false,
            level_hint: None,
        });
        formats
    };

    #[cfg(feature = "wim")]
    let formats = {
        let mut formats = formats;
        formats.push(FormatInfo {
            name: "wim".into(),
            can_compress: false,
            can_decompress: true,
            supports_encryption: false,
            level_hint: None,
        });
        formats
    };

    // Retro/historical formats (read-only)
    formats.push(FormatInfo {
        name: "arj".into(),
        can_compress: false,
        can_decompress: true,
        supports_encryption: false,
        level_hint: None,
    });
    formats.push(FormatInfo {
        name: "ace".into(),
        can_compress: false,
        can_decompress: true,
        supports_encryption: false,
        level_hint: None,
    });
    formats.push(FormatInfo {
        name: "arc".into(),
        can_compress: false,
        can_decompress: true,
        supports_encryption: false,
        level_hint: None,
    });
    formats.push(FormatInfo {
        name: "z".into(),
        can_compress: false,
        can_decompress: true,
        supports_encryption: false,
        level_hint: None,
    });
    formats
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_formats_returns_expected_count() {
        let formats = get_formats();
        let expected = if cfg!(feature = "zpaq") { 26 } else { 25 };
        assert_eq!(formats.len(), expected, "unexpected supported format count");
    }

    #[test]
    fn get_formats_zip_compress_decompress() {
        let formats = get_formats();
        let zip = formats.iter().find(|f| f.name == "zip").unwrap();
        assert!(zip.can_compress);
        assert!(zip.can_decompress);
        assert!(zip.supports_encryption);
        assert_eq!(zip.level_hint.as_deref(), Some("6 (0-9)"));
    }

    #[test]
    fn get_formats_zipx_compress_decompress() {
        let formats = get_formats();
        let zipx = formats.iter().find(|f| f.name == "zipx").unwrap();
        assert!(zipx.can_compress);
        assert!(zipx.can_decompress);
        assert!(zipx.supports_encryption);
    }

    #[test]
    fn get_formats_seven_zip_compress_decompress() {
        let formats = get_formats();
        let sz = formats.iter().find(|f| f.name == "7z").unwrap();
        assert!(sz.can_compress);
        assert!(sz.can_decompress);
        assert!(sz.supports_encryption);
        assert!(sz.level_hint.is_none());
    }

    #[test]
    fn get_formats_bzip2_decompress_only() {
        let formats = get_formats();
        let bz2 = formats.iter().find(|f| f.name == "bzip2").unwrap();
        assert!(!bz2.can_compress);
        assert!(bz2.can_decompress);
        assert_eq!(bz2.level_hint.as_deref(), Some("9 (1-9)"));
    }

    #[test]
    fn get_formats_tarbz2_compress_decompress() {
        let formats = get_formats();
        let tarbz2 = formats.iter().find(|f| f.name == "tar.bz2").unwrap();
        assert!(tarbz2.can_compress);
        assert!(tarbz2.can_decompress);
        assert!(!tarbz2.supports_encryption);
    }

    #[test]
    fn get_formats_tar_no_level() {
        let formats = get_formats();
        let tar = formats.iter().find(|f| f.name == "tar").unwrap();
        assert!(tar.can_compress);
        assert!(tar.can_decompress);
        assert!(!tar.supports_encryption);
        assert!(tar.level_hint.is_none());
    }

    #[test]
    fn get_formats_rar_decompress_only() {
        let formats = get_formats();
        let rar = formats.iter().find(|f| f.name == "rar").unwrap();
        assert!(!rar.can_compress);
        assert!(rar.can_decompress);
        assert!(rar.supports_encryption);
    }
}
