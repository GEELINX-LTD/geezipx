use super::*;
use sha2::{Digest, Sha256};
use std::io::{self, Cursor};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pea")
        .join(name)
}
fn bytes(name: &str) -> Vec<u8> {
    std::fs::read(fixture(name)).unwrap()
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[test]
fn independently_generated_store_deflate_and_all_plain_hashes() {
    for algorithm in [
        "noalgo",
        "adler32",
        "crc32",
        "crc64",
        "md5",
        "ripemd160",
        "sha1",
        "sha256",
        "sha512",
        "whirlpool",
        "sha3_256",
        "sha3_512",
        "blake2s",
        "blake2b",
    ] {
        check_tree(&format!("{algorithm}-pcompress2.pea"));
    }
    check_tree("crc32-pcompress0.pea");
    check_tree("mixed.pea");
    check_tree("level1.pea");
    check_tree("level3.pea");
}
fn check_tree(name: &str) {
    let mut reader =
        PeaReader::open(&fixture(name)).unwrap_or_else(|error| panic!("{name}: {error}"));
    assert_eq!(reader.format(), ArchiveFormat::Pea);
    assert_eq!(reader.verified_stats(), (4, 136256));
    let entries = reader.entries().unwrap();
    assert!(entries
        .iter()
        .any(|entry| entry.path == "source/empty-dir" && entry.is_dir));
    assert!(entries
        .iter()
        .any(|entry| entry.path == "source/empty.bin" && entry.size == 0 && !entry.is_dir));
    let entry = entries
        .iter()
        .find(|entry| entry.path == "source/中文 Ω.txt")
        .unwrap();
    let mut output = Vec::new();
    assert_eq!(reader.extract(entry, &mut output).unwrap(), 136256);
    assert_eq!(
        digest(&output),
        "6050ccb8eb47ba3d20954ffe26ed6a5d1c812518115a7a682bbbba08bc70ec6e"
    );
}

#[test]
fn independently_generated_multiple_blocks_and_raw_final_block() {
    let mut reader = PeaReader::open(&fixture("multiblock.pea")).unwrap();
    let entry = reader.entries().unwrap().remove(0);
    assert_eq!(entry.path, "data.bin");
    let mut output = Vec::new();
    reader.extract(&entry, &mut output).unwrap();
    assert_eq!(output.len(), 1024 * 1024 + 17);
    assert_eq!(
        digest(&output),
        "c29c45cb10645a27d37ba63e00e7f2f1c1e5e860d8a28753dff84d68cf4ef341"
    );
}

#[test]
fn every_proper_prefix_and_extra_trailer_are_rejected() {
    let data = bytes("crc32-pcompress2.pea");
    for length in 0..data.len() {
        assert!(
            PeaReader::new(Cursor::new(data[..length].to_vec())).is_err(),
            "prefix {length}"
        );
    }
    let mut data = bytes("noalgo-pcompress2.pea");
    data.push(0);
    assert!(PeaReader::new(Cursor::new(data)).is_err());
}

#[test]
fn unsupported_headers_and_password_streams_are_rejected() {
    for (position, value) in [
        (0, 0),
        (1, 2),
        (2, 7),
        (3, 255),
        (4, 1),
        (7, 0),
        (8, 128),
        (10, 1),
        (16, 4),
        (17, 1),
        (18, 4),
        (18, 0x31),
        (19, 255),
    ] {
        let mut data = bytes("noalgo-pcompress2.pea");
        data[position] = value;
        assert!(
            PeaReader::new(Cursor::new(data)).is_err(),
            "header {position}={value}"
        );
    }
    let mut data = bytes("noalgo-pcompress2.pea");
    data[20..24].fill(0);
    assert!(PeaReader::new(Cursor::new(data)).is_err());
}

#[test]
fn corrupted_object_stream_and_volume_checksums_are_rejected() {
    let original = bytes("crc32-pcompress2.pea");
    for offset in [original.len() - 1, original.len() - 5, original.len() - 15] {
        let mut data = original.clone();
        data[offset] ^= 1;
        assert!(
            PeaReader::new(Cursor::new(data)).is_err(),
            "checksum at {offset}"
        );
    }
}

#[test]
fn unsafe_paths_duplicates_and_symlinks_are_rejected_without_checksums() {
    let original = bytes("noalgo-pcompress2.pea");
    let mut data = original.clone();
    let start = data.windows(4).position(|value| value == b"/tmp").unwrap();
    data[start..start + 4].copy_from_slice(b"/../");
    assert!(PeaReader::new(Cursor::new(data)).is_err());
    let mut data = original.clone();
    let start = data
        .windows(9)
        .position(|value| value == b"empty.bin")
        .unwrap();
    data[start..start + 9].copy_from_slice(b"empty-dir");
    assert!(PeaReader::new(Cursor::new(data)).is_err());
    let mut data = original.clone();
    let length = u16::from_le_bytes(data[24..26].try_into().unwrap()) as usize;
    data[24 + 2 + length + 4] &= !16;
    let end = 24 + 2 + length + 8;
    data.splice(end..end, [0; 8]);
    let error = PeaReader::new(Cursor::new(data)).err().unwrap();
    assert!(error.to_string().contains("ancestor"));
    let mut data = original;
    let length = u16::from_le_bytes(data[24..26].try_into().unwrap()) as usize;
    data[24 + 2 + length + 4] |= 64;
    assert!(PeaReader::new(Cursor::new(data)).is_err());
}

fn payload_start(data: &[u8]) -> usize {
    let name = "中文 Ω.txt".as_bytes();
    let end = data
        .windows(name.len())
        .position(|value| value == name)
        .unwrap()
        + name.len();
    end + 16
}
#[test]
fn zlib_trailers_lengths_and_zero_blocks_are_rejected() {
    let original = bytes("noalgo-pcompress2.pea");
    let start = payload_start(&original);
    let encoded = u32::from_le_bytes(original[start..start + 4].try_into().unwrap()) as usize;
    let mut data = original.clone();
    data[start..start + 4].fill(0);
    assert!(PeaReader::new(Cursor::new(data)).is_err());
    let mut data = original.clone();
    data[start + 4 + encoded] ^= 1;
    assert!(PeaReader::new(Cursor::new(data)).is_err());
    let mut data = original;
    data.splice(start + 4 + encoded..start + 4 + encoded, [0]);
    data[start..start + 4].copy_from_slice(&((encoded + 1) as u32).to_le_bytes());
    assert!(PeaReader::new(Cursor::new(data)).is_err());
}

struct ShortSource {
    inner: Cursor<Vec<u8>>,
    calls: usize,
    empty: Arc<AtomicUsize>,
    cancel_at: usize,
}
impl Read for ShortSource {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.calls += 1;
        if self.calls.is_multiple_of(7) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        if buffer.is_empty() {
            if self.empty.fetch_add(1, Ordering::Relaxed) + 1 >= self.cancel_at {
                return Err(io::Error::other(GeeZipError::Cancelled));
            }
            return Ok(0);
        }
        let length = buffer.len().min(3);
        self.inner.read(&mut buffer[..length])
    }
}
impl Seek for ShortSource {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.inner.seek(position)
    }
}
#[test]
fn short_reads_and_interrupted_are_supported() {
    let source = ShortSource {
        inner: Cursor::new(bytes("crc32-pcompress2.pea")),
        calls: 0,
        empty: Arc::default(),
        cancel_at: usize::MAX,
    };
    let mut reader = PeaReader::new(source).unwrap();
    let entry = reader
        .entries()
        .unwrap()
        .into_iter()
        .find(|entry| entry.size > 0)
        .unwrap();
    assert_eq!(reader.extract(&entry, &mut io::sink()).unwrap(), 136256);
}
#[test]
fn cancellation_reaches_source_while_inflating() {
    let empty = Arc::new(AtomicUsize::new(0));
    let source = ShortSource {
        inner: Cursor::new(bytes("multiblock.pea")),
        calls: 0,
        empty: empty.clone(),
        cancel_at: 8,
    };
    assert!(matches!(
        PeaReader::new(source),
        Err(GeeZipError::Cancelled)
    ));
    assert_eq!(empty.load(Ordering::Relaxed), 8);
}

#[test]
fn source_metadata_changes_and_output_failure_are_reported() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(temp.path(), bytes("noalgo-pcompress2.pea")).unwrap();
    let mut reader = PeaReader::open(temp.path()).unwrap();
    let entry = reader
        .entries()
        .unwrap()
        .into_iter()
        .find(|entry| entry.size > 0)
        .unwrap();
    let start = reader.index[reader.lookup[&entry.path]].start;
    let mut file = temp.reopen().unwrap();
    file.seek(SeekFrom::Start(start + 2)).unwrap();
    file.write_all(b"*").unwrap();
    let mut output = Vec::new();
    assert!(reader.extract(&entry, &mut output).is_err());
    assert!(output.is_empty());
    struct Fail;
    impl Write for Fail {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("PEA destination full"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut reader = PeaReader::open(&fixture("crc32-pcompress2.pea")).unwrap();
    let error = reader.extract(&entry, &mut Fail).unwrap_err();
    assert!(error.to_string().contains("PEA destination full"));
}

#[test]
fn failed_integrity_preserves_existing_extraction_target() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    let mut data = bytes("crc32-pcompress0.pea");
    std::fs::write(temp.path(), &data).unwrap();
    let mut reader = PeaReader::open(temp.path()).unwrap();
    let entry = reader
        .entries()
        .unwrap()
        .into_iter()
        .find(|entry| entry.size > 0)
        .unwrap();
    let start = payload_start(&data);
    data[start] ^= 1;
    std::fs::write(temp.path(), data).unwrap();
    let dest = tempfile::tempdir().unwrap();
    let target = dest.path().join(&entry.path);
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, b"keep old destination").unwrap();
    let report = reader.extract_all(dest.path(), true).unwrap();
    assert!(!report.errors.is_empty());
    assert_eq!(std::fs::read(target).unwrap(), b"keep old destination");
}

#[test]
fn metadata_budget_is_checked_before_reading_the_name() {
    let data = bytes("noalgo-pcompress2.pea");
    let length = data.len() as u64;
    let mut source: BufReader<Box<dyn Source>> = BufReader::new(Box::new(Cursor::new(data)));
    let mut header = [0; 20];
    source.read_exact(&mut header).unwrap();
    let mut input = input::Input::new(&mut source, length, &header).unwrap();
    let block_size = u32::from_le_bytes(input.array::<4>().unwrap());
    let settings = Settings {
        compression: 2,
        block_size,
        object_hash: 0,
    };
    let error = record::read(&mut input, &settings, &mut io::sink(), None, 0)
        .err()
        .unwrap();
    assert!(error.to_string().contains("budget"));
    assert_eq!(input.position, 26);
}
