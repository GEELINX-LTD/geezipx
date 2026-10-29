//! Regressions for the object-safe streaming APIs used by native frontends.
use geezipx_core::{
    archive::{self, ArchiveReader, ArchiveWriter},
    config::{CompressOptions, SevenZipOptions},
    GeeZipError,
};
use std::{
    io::{self, Cursor, Read, Write},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

#[test]
fn sized_tar_writes_its_header_before_consuming_the_payload() {
    struct Output(Arc<AtomicBool>, Vec<u8>);
    impl Write for Output {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.store(true, Ordering::Relaxed);
            self.1.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    struct Input(Arc<AtomicBool>, Cursor<Vec<u8>>);
    impl Read for Input {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            assert!(
                self.0.load(Ordering::Relaxed),
                "payload was buffered before its TAR header"
            );
            self.1.read(bytes)
        }
    }
    let written = Arc::new(AtomicBool::new(false));
    let mut writer = archive::tar::TarWriter::new(Output(written.clone(), Vec::new()));
    let mut input = Input(written, Cursor::new(vec![42; 1024 * 1024]));
    writer
        .add_entry_with_size(Path::new("large.bin"), &mut input, 1024 * 1024)
        .unwrap();
    Box::new(writer).finish().unwrap();
}

#[test]
fn sized_tar_rejects_short_and_growing_inputs() {
    let mut writer = archive::tar::TarWriter::new(Vec::new());
    assert!(writer
        .add_entry_with_size(Path::new("short.txt"), &mut Cursor::new(b"abc"), 10)
        .is_err());
    let mut writer = archive::tar::TarWriter::new(Vec::new());
    assert!(writer
        .add_entry_with_size(Path::new("growing.txt"), &mut Cursor::new(b"abcdef"), 3)
        .is_err());
}

#[test]
fn tar_visitor_drains_partially_read_entries_and_honors_early_stop() {
    let mut writer = archive::tar::TarWriter::new(Vec::new());
    for name in ["first.txt", "second.txt", "third.txt"] {
        writer
            .add_entry_with_size(Path::new(name), &mut Cursor::new(b"hello"), 5)
            .unwrap();
    }
    let (_, data) = writer.finalize().unwrap();
    let mut reader = archive::tar::TarReader::new(Cursor::new(data));
    let mut visited = Vec::new();
    reader
        .visit_entries(
            &mut |entry, data| {
                visited.push(entry.path.clone());
                let mut byte = [0];
                data.read_exact(&mut byte)?;
                assert_eq!(byte[0], b'h');
                Ok(visited.len() < 2)
            },
            &|| false,
        )
        .unwrap();
    assert_eq!(visited, ["first.txt", "second.txt"]);
    assert!(matches!(
        reader.visit_entries(&mut |_, _| Ok(true), &|| true),
        Err(GeeZipError::Cancelled)
    ));
}

#[test]
fn seven_zip_visitor_stops_across_multiple_blocks_and_preserves_callback_errors() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("blocks.7z");
    let mut writer = archive::seven_zip::SevenZipWriter::new(
        std::fs::File::create(&path).unwrap(),
        &CompressOptions::default(),
        &SevenZipOptions::default(),
    )
    .unwrap();
    for name in ["first.txt", "second.txt", "third.txt"] {
        writer
            .add_entry_from_reader(Path::new(name), &mut Cursor::new(b"hello"))
            .unwrap();
    }
    Box::new(writer).finish().unwrap();
    let mut reader = archive::seven_zip::SevenZipReader::new(path);
    let mut calls = 0;
    reader
        .visit_entries(
            &mut |_, _| {
                calls += 1;
                Ok(false)
            },
            &|| false,
        )
        .unwrap();
    assert_eq!(
        calls, 1,
        "false must stop all blocks, not just the current block"
    );
    let result = reader.visit_entries(&mut |_, _| Err(GeeZipError::Cancelled), &|| false);
    assert!(matches!(result, Err(GeeZipError::Cancelled)));
}
