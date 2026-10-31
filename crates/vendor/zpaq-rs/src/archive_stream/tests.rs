use super::*;
use std::io::Cursor;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

struct ShortInput {
    data: Cursor<Vec<u8>>,
    calls: usize,
}
impl Read for ShortInput {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        self.calls += 1;
        if self.calls % 11 == 0 {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let len = out.len().min(7);
        self.data.read(&mut out[..len])
    }
}
#[test]
fn short_interrupted_named_readers_keep_a_shared_model() {
    let payload: Vec<u8> = (0..90_000).map(|i| (i % 251) as u8).collect();
    let entries = ["目录/one", "目录/two"]
        .into_iter()
        .map(|path| ReaderArchiveEntry {
            path: path.into(),
            reader: ShortInput {
                data: Cursor::new(payload.clone()),
                calls: 0,
            },
            size: payload.len() as u64,
            comment: None,
        });
    let mut out = Vec::new();
    archive_from_reader_entries(entries, &mut out, "1").unwrap();
    for path in ["目录/one", "目录/two"] {
        assert_eq!(crate::archive_read_file_bytes(&out, path).unwrap(), payload);
    }
}

struct FailedWriter;
impl Write for FailedWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if data.is_empty() {
            Ok(0)
        } else {
            Err(io::Error::other("disk write failed"))
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn final_cpp_buffer_error_is_returned_without_throwing_from_destruction() {
    let entries = [ReaderArchiveEntry {
        path: "small".into(),
        reader: Cursor::new(b"small"),
        size: 5,
        comment: None,
    }];
    assert!(archive_from_reader_entries(entries, FailedWriter, "1").is_err());
}

#[test]
fn declared_sizes_reject_short_and_long_sources() {
    for expected in [2, 4] {
        let entries = [ReaderArchiveEntry {
            path: "size".into(),
            reader: Cursor::new(b"abc"),
            size: expected,
            comment: None,
        }];
        let error = archive_from_reader_entries(entries, Vec::new(), "1").unwrap_err();
        assert!(error.to_string().contains("size mismatch"));
    }
}

struct InputCounter(Arc<AtomicUsize>);
impl Read for InputCounter {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        out.fill(0);
        self.0.fetch_add(out.len(), Ordering::Relaxed);
        Ok(out.len())
    }
}
struct CancelOutput(usize);
impl Write for CancelOutput {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if data.is_empty() {
            self.0 += 1;
            if self.0 > 3 {
                return Err(io::Error::other("step cancelled"));
            }
        }
        Ok(data.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn cancellation_polls_during_solid_compression_not_only_at_final_output() {
    let read = Arc::new(AtomicUsize::new(0));
    let entries = [ReaderArchiveEntry {
        path: "cancel".into(),
        reader: InputCounter(Arc::clone(&read)),
        size: 1024 * 1024,
        comment: None,
    }];
    let error = archive_from_reader_entries(entries, CancelOutput(0), "1").unwrap_err();
    assert!(error.to_string().contains("step cancelled"));
    assert!((1..1024 * 1024).contains(&read.load(Ordering::Relaxed)));
}

struct PanicInput;
impl Read for PanicInput {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        panic!("simulated read panic");
    }
}
#[test]
fn user_reader_panics_do_not_cross_the_cpp_boundary() {
    let entries = [ReaderArchiveEntry {
        path: "panic".into(),
        reader: PanicInput,
        size: 1,
        comment: None,
    }];
    assert!(archive_from_reader_entries(entries, Vec::new(), "1").is_err());
}

struct FlushError;
impl Write for FlushError {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        Ok(data.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::other("flush failure"))
    }
}
#[test]
fn underlying_writer_flush_errors_are_propagated() {
    assert!(
        archive_from_reader_entries(
            std::iter::empty::<ReaderArchiveEntry<Cursor<Vec<u8>>>>(),
            FlushError,
            "1"
        )
        .unwrap_err()
        .to_string()
        .contains("flush failure")
    );
    let entries = [ReaderArchiveEntry {
        path: "flush".into(),
        reader: Cursor::new(b"x"),
        size: 1,
        comment: None,
    }];
    assert!(
        archive_from_reader_entries(entries, FlushError, "1")
            .unwrap_err()
            .to_string()
            .contains("flush failure")
    );
}
