use super::*;
use crate::archive::udf::UdfReader;
use crate::archive::ArchiveReader;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

struct Generated {
    left: u64,
    interrupt: bool,
}
impl Read for Generated {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        assert!(buf.len() <= BUFFER_SIZE, "unbounded read buffer");
        if self.interrupt {
            self.interrupt = false;
            return Err(io::ErrorKind::Interrupted.into());
        }
        let n = self.left.min(buf.len() as u64) as usize;
        buf[..n].fill(b'Q');
        self.left -= n as u64;
        Ok(n)
    }
}
#[derive(Debug)]
struct WriteOnly(File);
impl Write for WriteOnly {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

struct PatternOutput(u64);
impl Write for PatternOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        assert!(bytes.iter().all(|&b| b == b'Q'));
        self.0 += bytes.len() as u64;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn extract(path: &Path, name: &str) -> Vec<u8> {
    let mut reader = UdfReader::new(path).unwrap();
    let entry = reader
        .entries()
        .unwrap()
        .into_iter()
        .find(|e| e.path == name)
        .unwrap();
    let mut data = Vec::new();
    reader.extract(&entry, &mut data).unwrap();
    data
}

#[test]
fn udf_reads_independent_cs0_and_legacy_utf8_fixtures() {
    fn fixture(bytes: &[u8]) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        let decoder = flate2::read::GzDecoder::new(bytes);
        io::copy(&mut decoder.take(2 * 1024 * 1024), file.as_file_mut()).unwrap();
        file
    }
    let old = fixture(include_bytes!(
        "../../../../tests/fixtures/udf/legacy-utf8.udf.gz"
    ));
    assert_eq!(extract(old.path(), "旧版中文.txt"), b"legacy");
    let external = fixture(include_bytes!(
        "../../../../tests/fixtures/udf/genisoimage.udf.gz"
    ));
    assert_eq!(
        extract(external.path(), "中文目录/你好.txt"),
        b"Chinese fixture\n"
    );
    assert_eq!(extract(external.path(), "café.txt"), b"accent fixture\n");
    assert_eq!(
        extract(external.path(), "Â©.txt"),
        b"Latin-1 ambiguity fixture\n"
    );
    let entries = UdfReader::new(external.path()).unwrap().entries().unwrap();
    assert!(entries
        .iter()
        .any(|entry| entry.path == "empty" && entry.is_dir));
}

#[test]
fn udf_streams_bounded_input_to_nonseekable_output_and_exact_image() {
    // The API output implements only Write; the image never needs a payload Vec.
    let file = tempfile::NamedTempFile::new().unwrap();
    let mut writer = UdfWriter::new(WriteOnly(file.reopen().unwrap()));
    let size = 8 * 1024 * 1024 + 17;
    writer
        .add_entry_from_reader(
            Path::new("data.bin"),
            &mut Generated {
                left: size,
                interrupt: true,
            },
        )
        .unwrap();
    let (bytes, _) = writer.finalize().unwrap();
    assert_eq!(bytes, file.as_file().metadata().unwrap().len());
    assert_eq!(bytes % 2048, 0);
    let mut reader = UdfReader::new(file.path()).unwrap();
    let entry = reader.entries().unwrap().into_iter().next().unwrap();
    assert_eq!(entry.size, size);
    let mut sink = PatternOutput(0);
    assert_eq!(reader.extract(&entry, &mut sink).unwrap(), size);
    assert_eq!(sink.0, size);
    let mut source = file.reopen().unwrap();
    let mut tag = [0; 16];
    source.seek(SeekFrom::End(-2048)).unwrap();
    source.read_exact(&mut tag).unwrap();
    assert_eq!(u16::from_le_bytes(tag[..2].try_into().unwrap()), 2);
    assert_eq!(
        u32::from_le_bytes(tag[12..].try_into().unwrap()) as u64,
        bytes / 2048 - 1
    );
    source.seek(SeekFrom::Start(269 * 2048)).unwrap();
    let mut integrity = [0; 134];
    source.read_exact(&mut integrity).unwrap();
    assert_eq!(
        u32::from_le_bytes(integrity[120..124].try_into().unwrap()),
        1
    );
    assert_eq!(
        u32::from_le_bytes(integrity[124..128].try_into().unwrap()),
        1
    );
    assert_eq!(
        u16::from_le_bytes(integrity[128..130].try_into().unwrap()),
        0x0102
    );
}

#[test]
fn udf_unicode_long_names_empty_entries_and_multiple_directory_sectors() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let mut writer = UdfWriter::new(file.reopen().unwrap());
    writer.add_directory(Path::new("中文/empty")).unwrap();
    writer.add_directory(Path::new("only-empty")).unwrap();
    let mut expected = Vec::new();
    for i in 0..40 {
        let name = format!("中文/{i:02}-{}.txt", "长".repeat(64));
        let data = if i % 2 == 0 {
            Vec::new()
        } else {
            format!("data-{i}").into_bytes()
        };
        writer
            .add_entry_from_reader(Path::new(&name), &mut data.as_slice())
            .unwrap();
        expected.push((name, data));
    }
    for name in ["中文/😊.txt", "中文/café.txt", "中文/Ω.txt"] {
        writer
            .add_entry_from_reader(Path::new(name), &mut &b"unicode"[..])
            .unwrap();
        expected.push((name.into(), b"unicode".to_vec()));
    }
    writer.finalize().unwrap();
    let mut reader = UdfReader::new(file.path()).unwrap();
    let entries = reader.entries().unwrap();
    assert_eq!(entries.iter().filter(|e| !e.is_dir).count(), expected.len());
    assert!(entries.iter().any(|e| e.path == "中文/empty" && e.is_dir));
    assert!(entries.iter().any(|e| e.path == "only-empty" && e.is_dir));
    for (name, data) in expected {
        assert_eq!(extract(file.path(), &name), data, "{name}");
    }
}

#[test]
fn udf_failed_reads_roll_back_and_paths_validate_before_payload() {
    struct PanicRead;
    impl Read for PanicRead {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            panic!("invalid path consumed input")
        }
    }
    struct Broken(bool);
    impl Read for Broken {
        fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
            if self.0 {
                return Err(io::Error::other("input failure"));
            }
            self.0 = true;
            b[..4096].fill(1);
            Ok(4096)
        }
    }
    let file = tempfile::NamedTempFile::new().unwrap();
    let mut writer = UdfWriter::new(file.reopen().unwrap());
    writer
        .add_entry_from_reader(Path::new("ok.txt"), &mut &b"ok"[..])
        .unwrap();
    for path in ["/absolute", "../escape", "ok.txt", "ok.txt/child"] {
        assert!(writer
            .add_entry_from_reader(Path::new(path), &mut PanicRead)
            .is_err());
    }
    let long = "长".repeat(128);
    assert!(writer
        .add_entry_from_reader(Path::new(&long), &mut PanicRead)
        .is_err());
    assert!(writer
        .add_entry_from_reader(Path::new("bad/partial"), &mut Broken(false))
        .is_err());
    assert!(!writer.entries.contains_key("bad"));
    writer
        .add_entry_from_reader(Path::new("bad/partial"), &mut &b"retry"[..])
        .unwrap();
    assert!(writer.add_directory(Path::new("ok.txt")).is_err());
    writer.add_directory(Path::new("bad")).unwrap();
    writer.finalize().unwrap();
    assert_eq!(extract(file.path(), "ok.txt"), b"ok");
    assert_eq!(extract(file.path(), "bad/partial"), b"retry");
}

#[derive(Debug)]
struct OutputProbe {
    bytes: Arc<AtomicU64>,
    flushes: Arc<AtomicUsize>,
    cancel_after: usize,
    fail_write: bool,
}
impl Write for OutputProbe {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if self.fail_write {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        self.bytes.fetch_add(b.len() as u64, Ordering::Relaxed);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.flushes.fetch_add(1, Ordering::Relaxed) >= self.cancel_after {
            return Err(io::Error::other("cancelled UDF output"));
        }
        Ok(())
    }
}
fn output(cancel_after: usize, fail_write: bool) -> OutputProbe {
    OutputProbe {
        bytes: Arc::new(AtomicU64::new(0)),
        flushes: Arc::new(AtomicUsize::new(0)),
        cancel_after,
        fail_write,
    }
}

#[test]
fn udf_checks_cancellation_while_formatting_before_output() {
    let sink = output(4, false);
    let bytes = sink.bytes.clone();
    let flushes = sink.flushes.clone();
    let mut writer = UdfWriter::new(sink);
    writer
        .add_entry_from_reader(
            Path::new("large"),
            &mut Generated {
                left: 8 * 1024 * 1024,
                interrupt: false,
            },
        )
        .unwrap();
    let err = writer.finalize().unwrap_err();
    assert!(err.to_string().contains("cancelled UDF output"));
    assert!(flushes.load(Ordering::Relaxed) > 4);
    assert_eq!(bytes.load(Ordering::Relaxed), 0);
}

#[test]
fn udf_rejects_source_truncation_and_propagates_output_errors() {
    let sink = output(usize::MAX, false);
    let bytes = sink.bytes.clone();
    let mut writer = UdfWriter::new(sink);
    writer
        .add_entry_from_reader(Path::new("data"), &mut &b"data"[..])
        .unwrap();
    writer.input.as_ref().unwrap().set_len(0).unwrap();
    assert!(writer.finalize().is_err());
    assert_eq!(bytes.load(Ordering::Relaxed), 0);
    let mut writer = UdfWriter::new(output(usize::MAX, true));
    writer.add_directory(Path::new("empty")).unwrap();
    assert!(writer.finalize().unwrap_err().to_string().contains("pipe"));
}

#[test]
fn udf_small_limits_reject_oversize_without_registering_partial_data() {
    let mut file = tempfile::tempfile().unwrap();
    assert!(spool_entry(&mut file, &mut &b"12345"[..], 4).is_err());
    assert_eq!(
        spool_entry(&mut tempfile::tempfile().unwrap(), &mut &b"1234"[..], 4).unwrap(),
        4
    );
    let mut writer = UdfWriter::new(io::sink());
    assert!(writer
        .check_path(Path::new(&format!("{}/file", "d/".repeat(130))), false)
        .is_err());
    writer.add_directory(Path::new("empty")).unwrap();
    assert!(writer
        .add_entry_from_reader(Path::new("empty/../alias"), &mut &b"x"[..])
        .is_err());
}
