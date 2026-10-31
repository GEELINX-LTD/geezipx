use super::*;
use crate::archive::{iso::IsoReader, ArchiveReader};
use std::io::Cursor;

#[test]
fn rock_ridge_continuations_are_forward_and_confined_to_sectors() {
    let mut writer = IsoWriter::new(Vec::new());
    for i in 0..20 {
        let name = format!("{i:02}{}", "n".repeat(190));
        let data: &[u8] = if i % 2 == 0 { b"" } else { b"x" };
        writer
            .add_entry_from_reader(Path::new(&name), &mut &data[..])
            .unwrap();
    }
    writer
        .add_entry_from_reader(Path::new("sub/child"), &mut &b"child"[..])
        .unwrap();
    let (_, image) = writer.finalize().unwrap();
    let number = |at| u32::from_le_bytes(image[at..at + 4].try_into().unwrap()) as usize;
    let extent = number(16 * 2048 + 158) * 2048;
    let end = extent + number(16 * 2048 + 166);
    let mut pos = extent;
    let mut continuations = 0;
    while pos < end {
        let len = image[pos] as usize;
        if len == 0 {
            pos = (pos / 2048 + 1) * 2048;
            continue;
        }
        assert!(
            pos % 2048 + len <= 2048,
            "directory record crosses a sector"
        );
        let id_len = image[pos + 32] as usize;
        let mut field = pos + (33 + id_len).next_multiple_of(2);
        while field + 4 <= pos + len {
            let size = image[field + 2] as usize;
            assert!(size >= 4 && field + size <= pos + len);
            if &image[field..field + 2] == b"CE" {
                let block = number(field + 4) * 2048;
                let offset = number(field + 12);
                let bytes = number(field + 20);
                assert!(block >= end, "CE must follow the directory");
                assert!(offset + bytes <= 2048, "CE must fit inside its sector");
                assert!(block + offset + bytes <= image.len());
                if image[pos + 25] & 2 == 0 {
                    assert!(
                        block + offset + bytes <= number(pos + 2) * 2048,
                        "CE must precede even an empty regular file's data extent"
                    );
                }
                continuations += 1;
            }
            field += size;
        }
        pos += len;
    }
    assert!(
        continuations >= 21,
        "exercise multiple continuation sectors"
    );
    let mut reader = IsoReader::new(Cursor::new(image));
    let entries = reader.entries().unwrap();
    assert_eq!(entries.iter().filter(|e| !e.is_dir).count(), 21);
    for entry in entries.iter().filter(|e| !e.is_dir) {
        let mut output = Vec::new();
        reader.extract(entry, &mut output).unwrap();
        assert_eq!(output.len() as u64, entry.size);
    }
}

#[test]
fn unicode_tree_and_ranges_have_exact_volume_length() {
    let mut writer = IsoWriter::new(Vec::new());
    writer.add_directory(Path::new("空目录")).unwrap();
    writer.add_directory(Path::new("目录/深层")).unwrap();
    let cases = [
        ("目录/深层/你好.txt", 140_003, 0x35),
        ("目录/空.txt", 0, 0),
        ("second.bin", 65_537, 0xa5),
    ];
    for (name, size, byte) in cases {
        writer
            .add_entry_from_reader(Path::new(name), &mut io::repeat(byte).take(size))
            .unwrap();
    }
    let (size, image) = writer.finalize().unwrap();
    assert_eq!(size, image.len() as u64);
    for sector in [16, 17] {
        let start = sector * 2048 + 80;
        let little = u32::from_le_bytes(image[start..start + 4].try_into().unwrap());
        let big = u32::from_be_bytes(image[start + 4..start + 8].try_into().unwrap());
        assert_eq!(little, big);
        assert_eq!(
            u64::from(little) * 2048,
            size,
            "descriptor must match physical length"
        );
    }
    let mut reader = IsoReader::new(Cursor::new(image));
    let entries = reader.entries().unwrap();
    assert!(entries
        .iter()
        .any(|entry| entry.path == "空目录" && entry.is_dir));
    for (name, size, byte) in cases {
        let entry = entries.iter().find(|entry| entry.path == name).unwrap();
        let mut output = Vec::new();
        assert_eq!(reader.extract(entry, &mut output).unwrap(), size);
        assert_eq!(output, vec![byte; size as usize]);
    }
}

#[test]
fn failed_short_read_rolls_back_before_next_file() {
    struct Broken {
        left: usize,
        interrupted: bool,
    }
    impl Read for Broken {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if !self.interrupted {
                self.interrupted = true;
                return Err(io::ErrorKind::Interrupted.into());
            }
            if self.left == 0 {
                return Err(io::Error::other("input failed"));
            }
            let count = self.left.min(buf.len()).min(3);
            buf[..count].fill(1);
            self.left -= count;
            Ok(count)
        }
    }
    let mut writer = IsoWriter::new(Vec::new());
    writer
        .add_entry_from_reader(Path::new("first"), &mut &b"first"[..])
        .unwrap();
    let mut broken = Broken {
        left: BUFFER_SIZE + 5,
        interrupted: false,
    };
    assert!(writer
        .add_entry_from_reader(Path::new("broken"), &mut broken)
        .is_err());
    assert_eq!(writer.input.as_ref().unwrap().metadata().unwrap().len(), 5);
    writer
        .add_entry_from_reader(Path::new("last"), &mut &b"last"[..])
        .unwrap();
    let (_, image) = writer.finalize().unwrap();
    let mut reader = IsoReader::new(Cursor::new(image));
    let entries = reader.entries().unwrap();
    assert_eq!(entries.len(), 2);
    for name in ["first", "last"] {
        let mut output = Vec::new();
        reader
            .extract(
                entries.iter().find(|entry| entry.path == name).unwrap(),
                &mut output,
            )
            .unwrap();
        assert_eq!(output, name.as_bytes());
    }
}

#[test]
fn conflicts_and_depth_fail_before_consuming_input() {
    struct NoRead;
    impl Read for NoRead {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            panic!("invalid metadata consumed input")
        }
    }
    let mut writer = IsoWriter::new(Vec::new());
    writer
        .add_entry_from_reader(Path::new("a/b"), &mut &b"x"[..])
        .unwrap();
    writer.add_directory(Path::new("a")).unwrap(); // synthetic parent, idempotent
    for name in ["a/./b", "a//b", "a", "a/b/c", "a/b/c/d/e/f/g/h/i", "././"] {
        assert!(
            writer
                .add_entry_from_reader(Path::new(name), &mut NoRead)
                .is_err(),
            "{name}"
        );
    }
    assert!(writer.add_directory(Path::new("a/b")).is_err());
}

#[test]
fn input_limit_and_truncation_leave_output_untouched() {
    let mut file = tempfile::tempfile().unwrap();
    assert!(spool_entry(&mut file, &mut io::repeat(0).take(9), 8).is_err());
    let mut output = Vec::new();
    let mut writer = IsoWriter::new(&mut output);
    writer
        .add_entry_from_reader(Path::new("file"), &mut &b"contents"[..])
        .unwrap();
    writer.input.as_mut().unwrap().set_len(0).unwrap();
    assert!(writer.finalize().is_err());
    assert!(output.is_empty());
}

#[test]
fn output_cancellation_is_checked_during_formatting() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Output {
        checks: Arc<AtomicUsize>,
        written: Arc<AtomicUsize>,
    }
    impl Write for Output {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.written.fetch_add(buf.len(), Ordering::Relaxed);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            if self.checks.fetch_add(1, Ordering::Relaxed) >= 3 {
                return Err(io::Error::other("cancelled"));
            }
            Ok(())
        }
    }
    let checks = Arc::new(AtomicUsize::new(0));
    let written = Arc::new(AtomicUsize::new(0));
    let mut writer = IsoWriter::new(Output {
        checks: checks.clone(),
        written: written.clone(),
    });
    writer
        .add_entry_from_reader(Path::new("large"), &mut io::repeat(7).take(1024 * 1024))
        .unwrap();
    assert!(writer
        .finalize()
        .err()
        .unwrap()
        .to_string()
        .contains("cancelled"));
    assert_eq!(
        written.load(Ordering::Relaxed),
        0,
        "cancellation must precede final copy"
    );
    assert_eq!(checks.load(Ordering::Relaxed), 4);
}
