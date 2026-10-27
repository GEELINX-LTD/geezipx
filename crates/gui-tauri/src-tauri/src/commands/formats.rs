//! `get_formats` reports the capabilities of this GUI build.
//!
//! Core/CLI support is broader. Creation follows the GUI command parser and
//! extraction follows the same reader factory used by list/extract/preview.

use geezipx_core::detect::ArchiveFormat;
use serde::Serialize;

use super::compress::parse_gui_compress_format;
use super::list::supports_gui_read;

/// Information about a format available to the desktop bridge.
#[derive(Debug, Serialize)]
pub struct FormatInfo {
    pub name: String,
    pub can_compress: bool,
    pub can_decompress: bool,
    pub supports_encryption: bool,
    pub level_hint: Option<String>,
}

/// Report actual GUI capabilities, including features enabled in this build.
#[tauri::command]
pub fn get_formats() -> Vec<FormatInfo> {
    use ArchiveFormat as Af;
    let mut catalog = vec![
        ("zip", Af::Zip, true, Some("6 (0-9)")),
        ("zipx", Af::Zip, true, Some("6 (0-9)")),
        ("tar", Af::Tar, false, None),
        ("tar.gz", Af::TarGz, false, Some("6 (0-9)")),
        ("gzip", Af::Gzip, false, Some("6 (0-9)")),
        ("bzip2", Af::Bzip2, false, Some("9 (1-9)")),
        ("tar.bz2", Af::TarBz2, false, Some("9 (1-9)")),
        ("brotli", Af::Brotli, false, Some("11 (0-11)")),
        ("tar.br", Af::TarBr, false, Some("11 (0-11)")),
        ("lz4", Af::Lz4, false, None),
        ("tar.lz4", Af::TarLz4, false, None),
        ("zstd", Af::Zstd, false, Some("3 (1-22)")),
        ("tar.zst", Af::TarZst, false, Some("3 (1-22)")),
        ("xz", Af::Xz, false, Some("6 (0-9)")),
        ("tar.xz", Af::TarXz, false, Some("6 (0-9)")),
        ("lzma", Af::Lzma, false, Some("6 (0-9)")),
        ("lz", Af::Lz, false, Some("6 (0-9)")),
        ("7z", Af::SevenZip, true, None),
        ("rar", Af::Rar, true, None),
        ("cab", Af::Cab, false, None),
        ("asar", Af::Asar, false, None),
        ("deb", Af::Deb, false, None),
        ("lzh", Af::Lzh, false, Some("2 (0-4)")),
        ("iso", Af::Iso, false, None),
        ("udf", Af::Udf, false, None),
        ("isz", Af::Isz, false, Some("6 (0-9)")),
        ("cpio", Af::Cpio, false, None),
        ("arj", Af::Arj, false, None),
        ("ace", Af::Ace, false, None),
        ("arc", Af::Arc, false, None),
        ("z", Af::Z, false, None),
        ("alz", Af::Alz, false, None),
        ("uu", Af::Uu, false, None),
        ("xxe", Af::Xxe, false, None),
    ];
    if cfg!(feature = "zpaq") {
        catalog.push(("zpaq", Af::Zpaq, false, None));
    }
    if cfg!(feature = "wim") {
        catalog.push(("wim", Af::Wim, false, None));
    }
    catalog
        .into_iter()
        .map(|(name, format, encryption, level)| {
            let can_compress = parse_gui_compress_format(name).is_ok();
            let can_decompress = supports_gui_read(format);
            FormatInfo {
                name: name.into(),
                can_compress,
                can_decompress,
                supports_encryption: encryption && (can_compress || can_decompress),
                level_hint: level.map(str::to_string),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{compress::create_gui_writer, list::open_reader};
    use geezipx_core::config::CompressOptions;
    use std::{fs, io::Cursor};

    #[test]
    fn capabilities_reflect_this_gui_build() {
        let formats = get_formats();
        assert_eq!(
            formats.len(),
            34 + usize::from(cfg!(feature = "zpaq")) + usize::from(cfg!(feature = "wim"))
        );
        for name in [
            "zip", "zipx", "tar", "tar.gz", "tar.bz2", "tar.br", "tar.lz4", "tar.zst", "tar.xz",
            "7z", "lzh", "iso",
        ] {
            let format = formats.iter().find(|f| f.name == name).unwrap();
            assert!(format.can_compress && format.can_decompress, "{name}");
        }
        for name in [
            "gzip", "bzip2", "brotli", "lz4", "zstd", "xz", "lzma", "lz", "udf", "isz", "arj",
            "ace", "arc", "z", "alz", "uu", "xxe",
        ] {
            let format = formats.iter().find(|f| f.name == name).unwrap();
            assert!(!format.can_compress && !format.can_decompress, "{name}");
        }
        for name in ["cab", "asar", "deb", "cpio"] {
            let format = formats.iter().find(|f| f.name == name).unwrap();
            assert!(!format.can_compress && format.can_decompress, "{name}");
        }
        let rar = formats.iter().find(|f| f.name == "rar").unwrap();
        assert!(!rar.can_compress);
        assert_eq!(rar.can_decompress, cfg!(feature = "rar"));
        assert_eq!(rar.supports_encryption, cfg!(feature = "rar"));
        if let Some(wim) = formats.iter().find(|f| f.name == "wim") {
            assert!(!wim.can_compress && !wim.can_decompress);
        }
    }

    #[test]
    fn every_advertised_writer_round_trips_through_the_gui_reader() {
        let temp = tempfile::tempdir().unwrap();
        let content = b"GUI capability round trip\n";
        for format in get_formats().into_iter().filter(|f| f.can_compress) {
            let af = parse_gui_compress_format(&format.name).unwrap();
            let path = temp.path().join(format!("sample.{}", format.name));
            let file = fs::File::create(&path).unwrap();
            let mut writer = create_gui_writer(file, af, CompressOptions::default())
                .unwrap_or_else(|err| panic!("{}: {err}", format.name));
            writer
                .add_entry_from_reader(std::path::Path::new("hello.txt"), &mut Cursor::new(content))
                .unwrap();
            writer.finish().unwrap();
            let mut reader =
                open_reader(&path, af, None).unwrap_or_else(|err| panic!("{}: {err}", format.name));
            let entries = reader.entries().unwrap();
            let entry = entries.iter().find(|entry| !entry.is_dir).unwrap();
            let mut output = Vec::new();
            reader.extract(entry, &mut output).unwrap();
            assert_eq!(output, content, "{}", format.name);
        }
    }
}
