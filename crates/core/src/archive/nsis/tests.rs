use super::*;
use std::io::{self, Cursor};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/nsis")
            .join(format!("{name}.bin")),
    )
    .unwrap()
}
fn all() -> [&'static str; 7] {
    [
        "store-nonsolid",
        "zlib-nonsolid",
        "zlib-solid",
        "bzip2-nonsolid",
        "bzip2-solid",
        "lzma-nonsolid",
        "lzma-solid",
    ]
}
fn refresh_crc(bytes: &mut [u8]) {
    let end = bytes.len() - 4;
    let crc = crc32fast::hash(&bytes[512..end]);
    bytes[end..].copy_from_slice(&crc.to_le_bytes());
}
fn start(bytes: &[u8]) -> usize {
    input::layout(&mut Cursor::new(bytes)).unwrap().start as usize
}

#[test]
fn native_nsis_all_codecs_solid_unicode_empty_and_trait_object() {
    let manifest: serde_json::Value =
        serde_json::from_slice(include_bytes!("../../../tests/fixtures/nsis/manifest.json"))
            .unwrap();
    for name in all() {
        let bytes = fixture(name);
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            manifest[name]["sha256"].as_str().unwrap(),
            "{name}"
        );
        let mut source = Cursor::new(bytes.clone());
        assert_eq!(
            crate::detect::detect_seekable_format(&mut source).unwrap(),
            Some(ArchiveFormat::Nsis),
            "{name}"
        );
        assert_eq!(source.position(), 0);
        let mut reader =
            NsisReader::new(Cursor::new(bytes)).unwrap_or_else(|e| panic!("{name}: {e}"));
        let mut entries = reader.entries().unwrap();
        entries.sort_unstable_by(|a, b| a.path.cmp(&b.path));
        assert_eq!(
            entries.iter().map(|e| e.path.as_str()).collect::<Vec<_>>(),
            [
                "empty",
                "empty.bin",
                "hello.txt",
                "mixed.bin",
                "nested",
                "nested/中文 Ω.txt"
            ],
            "{name}"
        );
        let expected_size: u64 = ["hello.txt", "empty.bin", "mixed.bin", "中文 Ω.txt"]
            .iter()
            .map(|key| manifest[key]["bytes"].as_u64().unwrap())
            .sum();
        assert_eq!(reader.verified_stats(), (6, expected_size, true), "{name}");
        let mut boxed: Box<dyn ArchiveReader> = Box::new(reader);
        assert_eq!(boxed.format(), ArchiveFormat::Nsis);
        for entry in entries {
            let mut output = Vec::new();
            assert_eq!(
                boxed.extract(&entry, &mut output).unwrap(),
                entry.size,
                "{name}: {}",
                entry.path
            );
            if entry.is_dir {
                assert!(output.is_empty());
                continue;
            }
            let key = entry.path.strip_prefix("nested/").unwrap_or(&entry.path);
            assert_eq!(
                entry.size,
                manifest[key]["bytes"].as_u64().unwrap(),
                "{name}: {key}"
            );
            assert_eq!(
                format!("{:x}", Sha256::digest(&output)),
                manifest[key]["sha256"].as_str().unwrap(),
                "{name}: {key}"
            );
        }
    }
}

#[test]
fn native_nsis_crc_truncation_and_declared_header_budget() {
    for name in all() {
        let original = fixture(name);
        let mut bytes = original.clone();
        let end = bytes.len();
        bytes[end - 5] ^= 0x40;
        assert!(
            NsisReader::new(Cursor::new(bytes))
                .unwrap_err_text()
                .contains("CRC32"),
            "{name}"
        );
        for cut in [0, 63, 511, start(&original) + 27, original.len() - 1] {
            assert!(
                NsisReader::new(Cursor::new(original[..cut].to_vec())).is_err(),
                "{name}: {cut}"
            );
        }
        let mut bytes = original;
        let offset = start(&bytes);
        bytes[offset + 20..offset + 24]
            .copy_from_slice(&(METADATA_BUDGET as u32 + 1).to_le_bytes());
        assert!(
            NsisReader::new(Cursor::new(bytes))
                .unwrap_err_text()
                .contains("metadata budget"),
            "{name}"
        );
    }
}
trait ErrorText {
    fn unwrap_err_text(self) -> String;
}
impl ErrorText for Result<NsisReader> {
    fn unwrap_err_text(self) -> String {
        match self {
            Err(e) => e.to_string(),
            Ok(_) => panic!("expected malformed installer rejection"),
        }
    }
}
fn replace_utf16(bytes: &mut [u8], before: &str, after: &str) {
    let encoded = |text: &str| {
        text.encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>()
    };
    let before = encoded(before);
    let after = encoded(after);
    assert_eq!(before.len(), after.len());
    let offset = bytes
        .windows(before.len())
        .position(|data| data == before)
        .unwrap();
    bytes[offset..offset + before.len()].copy_from_slice(&after);
    refresh_crc(bytes);
}
#[test]
fn native_nsis_rejects_traversal_and_case_aliases_before_extraction() {
    let mut bytes = fixture("store-nonsolid");
    replace_utf16(&mut bytes, "nested", "..\\bad");
    assert!(NsisReader::new(Cursor::new(bytes))
        .unwrap_err_text()
        .contains("unsafe"));
    let mut bytes = fixture("store-nonsolid");
    replace_utf16(&mut bytes, "empty.bin", "HELLO.TXT");
    assert!(NsisReader::new(Cursor::new(bytes))
        .unwrap_err_text()
        .contains("duplicate"));
}
#[test]
fn native_nsis_rejects_file_parents_even_with_intervening_sorted_names() {
    let mut bytes = fixture("store-nonsolid");
    let header = start(&bytes) + 32;
    let entry_offset =
        u32::from_le_bytes(bytes[header + 20..header + 24].try_into().unwrap()) as usize;
    let out_dir = header + entry_offset + 4 * 28;
    assert_eq!(
        i32::from_le_bytes(bytes[out_dir..out_dir + 4].try_into().unwrap()),
        nsis::opcode::EW_CREATEDIR
    );
    // Keep CreateDirectory nested, but stop it changing the current output
    // directory. The later file "aa" must conflict with earlier "aa/x",
    // even though "aa.0" falls between the two in a lexical sort.
    bytes[out_dir + 8..out_dir + 12].copy_from_slice(&0_u32.to_le_bytes());
    replace_utf16(&mut bytes, "mixed.bin", "aa/x\0\0\0\0\0");
    replace_utf16(&mut bytes, "empty.bin", "aa.0\0\0\0\0\0");
    replace_utf16(&mut bytes, "中文 Ω.txt", "aa\0\0\0\0\0\0");
    assert!(NsisReader::new(Cursor::new(bytes))
        .unwrap_err_text()
        .contains("conflicts"));
}
#[test]
fn native_nsis_rejects_windows_devices_and_runtime_dependent_paths() {
    let mut bytes = fixture("store-nonsolid");
    replace_utf16(&mut bytes, "empty.bin", "CON\0\0\0\0\0\0");
    assert!(NsisReader::new(Cursor::new(bytes))
        .unwrap_err_text()
        .contains("unsafe"));
    let mut bytes = fixture("store-nonsolid");
    let header = start(&bytes) + 32;
    let variable = [3, 0, 0x95, 0x80]; // Unicode $INSTDIR, NSIS coded short 21.
    let offset = header
        + bytes[header..]
            .windows(4)
            .position(|data| data == variable)
            .unwrap();
    bytes[offset + 2] = 0x8a; // Unresolved $R0 (10), not literal text.
    refresh_crc(&mut bytes);
    assert!(NsisReader::new(Cursor::new(bytes))
        .unwrap_err_text()
        .contains("runtime-dependent"));
}
#[test]
fn native_nsis_without_container_crc_reports_the_limit() {
    let mut bytes = fixture("store-nonsolid");
    let offset = start(&bytes);
    let flags = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    bytes[offset..offset + 4].copy_from_slice(&((flags | 4) & !8).to_le_bytes());
    let size = u32::from_le_bytes(bytes[offset + 24..offset + 28].try_into().unwrap()) - 4;
    bytes[offset + 24..offset + 28].copy_from_slice(&size.to_le_bytes());
    bytes.truncate(bytes.len() - 4);
    let reader = NsisReader::new(Cursor::new(bytes)).unwrap();
    assert!(!reader.verified_stats().2);
}
struct Checked {
    input: Cursor<Vec<u8>>,
    cancel: Arc<AtomicBool>,
    largest: Arc<AtomicUsize>,
}
impl Read for Checked {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.largest.fetch_max(bytes.len(), Ordering::Relaxed);
        if self.cancel.load(Ordering::Relaxed) {
            return Err(io::Error::other(GeeZipError::Cancelled));
        }
        self.input.read(bytes)
    }
}
impl Seek for Checked {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.input.seek(position)
    }
}
#[test]
fn native_nsis_bounded_reads_and_cancellation_survive_solid_staging() {
    for name in ["zlib-nonsolid", "bzip2-solid", "lzma-solid"] {
        let cancel = Arc::new(AtomicBool::new(false));
        let largest = Arc::new(AtomicUsize::new(0));
        let input = Checked {
            input: Cursor::new(fixture(name)),
            cancel: cancel.clone(),
            largest: largest.clone(),
        };
        let mut reader = NsisReader::new(input).unwrap();
        assert!(largest.load(Ordering::Relaxed) <= 65536, "{name}");
        let entry = reader
            .entries()
            .unwrap()
            .into_iter()
            .find(|e| e.path == "mixed.bin")
            .unwrap();
        cancel.store(true, Ordering::Relaxed);
        assert!(
            reader
                .extract(&entry, &mut Vec::new())
                .unwrap_err()
                .to_string()
                .contains("cancelled"),
            "{name}"
        );
    }
}
#[test]
fn native_nsis_propagates_output_errors_and_preserves_directories() {
    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "fixture writer failed",
            ))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut reader = NsisReader::new(Cursor::new(fixture("zlib-solid"))).unwrap();
    let entries = reader.entries().unwrap();
    let entry = entries.iter().find(|e| e.path == "hello.txt").unwrap();
    assert!(reader
        .extract(entry, &mut Broken)
        .unwrap_err()
        .to_string()
        .contains("fixture writer failed"));
    let destination = tempfile::tempdir().unwrap();
    reader.extract_all(destination.path(), true).unwrap();
    assert!(destination.path().join("empty").is_dir());
    assert!(destination.path().join("nested/中文 Ω.txt").is_file());
}
