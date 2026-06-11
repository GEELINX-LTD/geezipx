//! `get_formats` command — return the list of supported archive formats.

use serde::Serialize;

/// Information about a single archive/compression format.
#[derive(Debug, Serialize)]
pub struct FormatInfo {
    /// Machine-readable format name (e.g. `"zip"`, `"tar.gz"`, `"7z"`).
    pub name: String,
    /// Whether the current GUI compression command can create this format.
    ///
    /// In the current GUI release, single-stream `gzip`, `bzip2`, `brotli`,
    /// `lz4`, `zstd`, `xz`, and `lzma` entries remain decompress-only.
    /// Read-only archive formats still include `rar`, `cab`, `asar`, `deb`,
    /// `iso`, `cpio`, and `zpaq`; `lzh` is createable via a store-only MVP.
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
    let formats = vec![
        FormatInfo {
            name: "zip".into(),
            can_compress: true,
            can_decompress: true,
        },
        FormatInfo {
            name: "zipx".into(),
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
            name: "bzip2".into(),
            can_compress: false,
            can_decompress: true,
        },
        FormatInfo {
            name: "tar.bz2".into(),
            can_compress: true,
            can_decompress: true,
        },
        FormatInfo {
            name: "brotli".into(),
            can_compress: false,
            can_decompress: true,
        },
        FormatInfo {
            name: "tar.br".into(),
            can_compress: true,
            can_decompress: true,
        },
        FormatInfo {
            name: "lz4".into(),
            can_compress: false,
            can_decompress: true,
        },
        FormatInfo {
            name: "tar.lz4".into(),
            can_compress: true,
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
            can_compress: true,
            can_decompress: true,
        },
        FormatInfo {
            name: "rar".into(),
            can_compress: false,
            can_decompress: true,
        },
        FormatInfo {
            name: "cab".into(),
            can_compress: false,
            can_decompress: true,
        },
        FormatInfo {
            name: "asar".into(),
            can_compress: false,
            can_decompress: true,
        },
        FormatInfo {
            name: "deb".into(),
            can_compress: false,
            can_decompress: true,
        },
        FormatInfo {
            name: "lzh".into(),
            can_compress: true,
            can_decompress: true,
        },
        FormatInfo {
            name: "iso".into(),
            can_compress: false,
            can_decompress: true,
        },
        FormatInfo {
            name: "cpio".into(),
            can_compress: false,
            can_decompress: true,
        },
    ];

    #[cfg(feature = "zpaq")]
    let formats = {
        let mut formats = formats;
        formats.push(FormatInfo {
            name: "zpaq".into(),
            can_compress: false,
            can_decompress: true,
        });
        formats
    };

    formats
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_formats_returns_expected_count() {
        let formats = get_formats();
        let expected = if cfg!(feature = "zpaq") { 25 } else { 24 };
        assert_eq!(formats.len(), expected, "unexpected supported format count");
    }

    #[test]
    fn get_formats_zip_compress_decompress() {
        let formats = get_formats();
        let zip = formats.iter().find(|f| f.name == "zip").unwrap();
        assert!(zip.can_compress);
        assert!(zip.can_decompress);
    }

    #[test]
    fn get_formats_zipx_compress_decompress() {
        let formats = get_formats();
        let zipx = formats.iter().find(|f| f.name == "zipx").unwrap();
        assert!(zipx.can_compress);
        assert!(zipx.can_decompress);
    }

    #[test]
    fn get_formats_seven_zip_compress_decompress() {
        let formats = get_formats();
        let sz = formats.iter().find(|f| f.name == "7z").unwrap();
        assert!(sz.can_compress);
        assert!(sz.can_decompress);
    }

    #[test]
    fn get_formats_bzip2_decompress_only() {
        let formats = get_formats();
        let bz2 = formats.iter().find(|f| f.name == "bzip2").unwrap();
        assert!(!bz2.can_compress);
        assert!(bz2.can_decompress);
    }

    #[test]
    fn get_formats_tarbz2_compress_decompress() {
        let formats = get_formats();
        let tarbz2 = formats.iter().find(|f| f.name == "tar.bz2").unwrap();
        assert!(tarbz2.can_compress);
        assert!(tarbz2.can_decompress);
    }

    #[test]
    fn get_formats_brotli_decompress_only() {
        let formats = get_formats();
        let br = formats.iter().find(|f| f.name == "brotli").unwrap();
        assert!(!br.can_compress);
        assert!(br.can_decompress);
    }

    #[test]
    fn get_formats_tarbr_compress_decompress() {
        let formats = get_formats();
        let tarbr = formats.iter().find(|f| f.name == "tar.br").unwrap();
        assert!(tarbr.can_compress);
        assert!(tarbr.can_decompress);
    }

    #[test]
    fn get_formats_lz4_decompress_only() {
        let formats = get_formats();
        let lz4 = formats.iter().find(|f| f.name == "lz4").unwrap();
        assert!(!lz4.can_compress);
        assert!(lz4.can_decompress);
    }

    #[test]
    fn get_formats_tarlz4_compress_decompress() {
        let formats = get_formats();
        let tarlz4 = formats.iter().find(|f| f.name == "tar.lz4").unwrap();
        assert!(tarlz4.can_compress);
        assert!(tarlz4.can_decompress);
    }

    #[test]
    fn get_formats_rar_decompress_only() {
        let formats = get_formats();
        let rar = formats.iter().find(|f| f.name == "rar").unwrap();
        assert!(!rar.can_compress);
        assert!(rar.can_decompress);
    }

    #[test]
    fn get_formats_cab_decompress_only() {
        let formats = get_formats();
        let cab = formats.iter().find(|f| f.name == "cab").unwrap();
        assert!(!cab.can_compress);
        assert!(cab.can_decompress);
    }

    #[test]
    fn get_formats_asar_decompress_only() {
        let formats = get_formats();
        let asar = formats.iter().find(|f| f.name == "asar").unwrap();
        assert!(!asar.can_compress);
        assert!(asar.can_decompress);
    }

    #[test]
    fn get_formats_deb_decompress_only() {
        let formats = get_formats();
        let deb = formats.iter().find(|f| f.name == "deb").unwrap();
        assert!(!deb.can_compress);
        assert!(deb.can_decompress);
    }

    #[test]
    fn get_formats_lzh_compress_decompress() {
        let formats = get_formats();
        let lzh = formats.iter().find(|f| f.name == "lzh").unwrap();
        assert!(lzh.can_compress);
        assert!(lzh.can_decompress);
    }

    #[test]
    fn get_formats_iso_decompress_only() {
        let formats = get_formats();
        let iso = formats.iter().find(|f| f.name == "iso").unwrap();
        assert!(!iso.can_compress);
        assert!(iso.can_decompress);
    }

    #[test]
    fn get_formats_cpio_decompress_only() {
        let formats = get_formats();
        let cpio = formats.iter().find(|f| f.name == "cpio").unwrap();
        assert!(!cpio.can_compress);
        assert!(cpio.can_decompress);
    }

    #[cfg(feature = "zpaq")]
    #[test]
    fn get_formats_zpaq_decompress_only() {
        let formats = get_formats();
        let zpaq = formats.iter().find(|f| f.name == "zpaq").unwrap();
        assert!(!zpaq.can_compress);
        assert!(zpaq.can_decompress);
    }

    #[cfg(not(feature = "zpaq"))]
    #[test]
    fn get_formats_omits_zpaq_without_feature() {
        let formats = get_formats();
        assert!(formats.iter().all(|f| f.name != "zpaq"));
    }
}
