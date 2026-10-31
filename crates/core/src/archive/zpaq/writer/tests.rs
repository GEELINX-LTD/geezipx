use super::*;
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};

#[test]
fn solid_archive_keeps_names_empty_entries_unicode_and_payloads() {
    let data: Vec<u8> = (0..160_000).map(|i| ((i * 17) % 251) as u8).collect();
    let mut writer = ZpaqWriter::new(Vec::new(), Some(1));
    writer.add_directory(Path::new("目录")).unwrap();
    writer
        .add_entry_from_reader(Path::new("目录/α.bin"), &mut Cursor::new(&data))
        .unwrap();
    writer
        .add_entry_from_reader(Path::new("empty"), &mut io::empty())
        .unwrap();
    writer
        .add_entry_from_reader(Path::new("repeat"), &mut Cursor::new(&data))
        .unwrap();
    let (size, archive) = writer.finalize().unwrap();
    assert_eq!(size, archive.len() as u64);
    assert_eq!(
        zpaq_rs::archive_read_file_bytes(&archive, "目录/α.bin").unwrap(),
        data
    );
    assert!(zpaq_rs::archive_read_file_bytes(&archive, "empty")
        .unwrap()
        .is_empty());
    assert_eq!(
        zpaq_rs::archive_read_file_bytes(&archive, "repeat").unwrap(),
        data
    );
}

struct BrokenInput(bool);
impl Read for BrokenInput {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.0 {
            return Err(io::Error::other("source failure"));
        }
        self.0 = true;
        buf[0] = 9;
        Ok(1)
    }
}
struct NeverRead;
impl Read for NeverRead {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        panic!("invalid metadata read its payload");
    }
}
#[test]
fn source_failure_rolls_back_and_preserves_existing_members() {
    let mut writer = ZpaqWriter::new(Vec::new(), Some(1));
    writer
        .add_entry_from_reader(Path::new("good"), &mut Cursor::new(b"good"))
        .unwrap();
    assert!(writer
        .add_entry_from_reader(Path::new("bad"), &mut BrokenInput(false))
        .is_err());
    assert_eq!(writer.entries.len(), 1);
    assert_eq!(
        writer.staging.as_ref().unwrap().metadata().unwrap().len(),
        4
    );
    writer
        .add_entry_from_reader(Path::new("bad"), &mut Cursor::new(b"retry"))
        .unwrap();
    let (_, archive) = writer.finalize().unwrap();
    assert_eq!(
        zpaq_rs::archive_read_file_bytes(&archive, "good").unwrap(),
        b"good"
    );
    assert_eq!(
        zpaq_rs::archive_read_file_bytes(&archive, "bad").unwrap(),
        b"retry"
    );
}

#[test]
fn normalized_path_storage_respects_the_metadata_budget() {
    let mut writer = ZpaqWriter::new(Vec::new(), None);
    let path = format!("{}short", "./".repeat(4000));
    writer
        .add_entry_from_reader(Path::new(&path), &mut io::empty())
        .unwrap();
    assert_eq!(writer.entries[0].path, "short");
    assert!(writer.entries[0].path.capacity() <= writer.metadata_bytes);
}

#[test]
fn metadata_validation_precedes_payload_reads() {
    let mut writer = ZpaqWriter::new(Vec::new(), None);
    for name in ["", ".", "../bad", "/absolute", "nul\0name", "file/"] {
        assert!(writer
            .add_entry_from_reader(Path::new(name), &mut NeverRead)
            .is_err());
    }
    writer
        .add_entry_from_reader(Path::new("parent"), &mut io::empty())
        .unwrap();
    assert!(writer.add_directory(Path::new("parent/child")).is_err());
    assert!(writer.add_directory(Path::new("parent")).is_err());
    writer.add_directory(Path::new("folder")).unwrap();
    writer.add_directory(Path::new("folder")).unwrap();
    writer.metadata_bytes = METADATA_BUDGET;
    let error = writer
        .add_entry_from_reader(Path::new("budget"), &mut NeverRead)
        .unwrap_err();
    assert!(error.to_string().contains("budget"));
}

struct CancelAfterInput {
    finished: Arc<AtomicBool>,
    done: bool,
}
impl Read for CancelAfterInput {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        if self.done {
            self.finished.store(true, Ordering::Relaxed);
            return Ok(0);
        }
        self.done = true;
        b[0] = 7;
        Ok(1)
    }
}
#[derive(Debug)]
struct CancelledOutput(Arc<AtomicBool>);
impl Write for CancelledOutput {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if self.0.load(Ordering::Relaxed) {
            Err(io::Error::other("cancelled after input"))
        } else {
            Ok(b.len())
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn cancellation_is_observed_after_source_eof() {
    let flag = Arc::new(AtomicBool::new(false));
    let mut writer = ZpaqWriter::new(CancelledOutput(Arc::clone(&flag)), None);
    writer
        .add_entry_from_reader(
            Path::new("file"),
            &mut CancelAfterInput {
                finished: flag,
                done: false,
            },
        )
        .unwrap();
    let error = writer.finalize().unwrap_err();
    assert!(error.to_string().contains("cancelled after input"));
}

#[test]
fn stored_segment_checksum_is_verified_when_reading() {
    let mut writer = ZpaqWriter::new(Vec::new(), None);
    writer
        .add_entry_from_reader(Path::new("file"), &mut Cursor::new(b"protected payload"))
        .unwrap();
    let (_, mut archive) = writer.finalize().unwrap();
    assert_eq!(archive[archive.len() - 22], 253); // stored SHA-1 marker
    let checksum_byte = archive.len() - 2;
    archive[checksum_byte] ^= 1;
    let error = zpaq_rs::archive_read_file_bytes(&archive, "file").unwrap_err();
    assert!(error.to_string().contains("checksum"));
}

#[test]
fn staging_truncation_is_not_encoded_as_a_short_successful_member() {
    let mut writer = ZpaqWriter::new(Vec::new(), None);
    writer
        .add_entry_from_reader(Path::new("file"), &mut Cursor::new(b"contents"))
        .unwrap();
    writer.staging.as_mut().unwrap().set_len(0).unwrap();
    assert!(writer.finalize().is_err());
}

#[test]
fn high_compatibility_levels_encode_with_the_strongest_supported_solid_model() {
    for level in [3, 4, 5] {
        let mut writer = ZpaqWriter::new(Vec::new(), Some(level));
        writer
            .add_entry_from_reader(Path::new("file"), &mut Cursor::new(b"strong model"))
            .unwrap();
        let (_, archive) = writer.finalize().unwrap();
        assert_eq!(
            zpaq_rs::archive_read_file_bytes(&archive, "file").unwrap(),
            b"strong model"
        );
    }
}
