use super::*;
use std::io::Cursor;

struct Generated {
    left: usize,
    short: usize,
}
impl Read for Generated {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        assert!(buf.len() <= BUFFER, "unbounded read request");
        let n = self.left.min(buf.len()).min(self.short);
        buf[..n].fill(b'x');
        self.left -= n;
        Ok(n)
    }
}

#[test]
fn bounded_sources_integrity_and_nonseek_output() {
    let size = HASH_BLOCK * 2 + 17;
    let mut writer = AsarWriter::new(Vec::new());
    writer
        .add_entry_from_reader(
            Path::new("目录/é-😀.bin"),
            &mut Generated {
                left: size,
                short: 777,
            },
        )
        .unwrap();
    writer
        .add_entry_from_reader(Path::new("empty"), &mut io::empty())
        .unwrap();
    writer
        .add_entry_from_reader(
            Path::new("boundary"),
            &mut Generated {
                left: HASH_BLOCK,
                short: BUFFER,
            },
        )
        .unwrap();
    writer.add_directory(Path::new("empty-dir/sub")).unwrap();
    let (written, bytes) = writer.finalize().unwrap();
    assert_eq!(written, bytes.len() as u64);
    let parsed = asar::AsarReader::new(&bytes, None).unwrap();
    let file = parsed.read(Path::new("目录/é-😀.bin")).unwrap();
    assert_eq!(file.data().len(), size);
    assert_eq!(file.integrity().unwrap().blocks().len(), 3);
    let integrity = file.integrity().unwrap();
    let empty = parsed
        .read(Path::new("empty"))
        .unwrap()
        .integrity()
        .unwrap();
    assert_eq!(empty.blocks().len(), 1);
    assert_eq!(empty.blocks()[0].as_slice(), &Sha256::digest([])[..]);
    let boundary = parsed
        .read(Path::new("boundary"))
        .unwrap()
        .integrity()
        .unwrap();
    assert_eq!(boundary.blocks().len(), 2);
    assert_eq!(boundary.blocks()[1].as_slice(), &Sha256::digest([])[..]);
    assert_eq!(integrity.hash(), &Sha256::digest(file.data())[..]);
    for (chunk, digest) in file.data().chunks(HASH_BLOCK).zip(integrity.blocks()) {
        assert_eq!(digest.as_slice(), &Sha256::digest(chunk)[..]);
    }
    let (header, _) = asar::Header::read(&mut Cursor::new(&bytes)).unwrap();
    let json = serde_json::to_value(header).unwrap();
    assert_eq!(
        json["files"]["empty-dir"]["files"]["sub"]["files"],
        serde_json::json!({})
    );
}

struct Broken(bool);
impl Read for Broken {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.0 {
            return Err(io::Error::other("source failed"));
        }
        self.0 = true;
        buf[0] = 1;
        Ok(1)
    }
}

#[test]
fn invalid_paths_do_not_consume_and_failed_reads_roll_back() {
    struct NoRead;
    impl Read for NoRead {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            panic!("unexpected read")
        }
    }
    let mut writer = AsarWriter::new(Vec::new());
    for name in ["../bad", "C:\\bad", "/bad", "a/./b", "a//b", "", "a\0b"] {
        assert!(writer
            .add_entry_from_reader(Path::new(name), &mut NoRead)
            .is_err());
    }
    writer
        .add_entry_from_reader(Path::new("ok"), &mut Cursor::new(b"first"))
        .unwrap();
    assert!(writer
        .add_entry_from_reader(Path::new("ok"), &mut NoRead)
        .is_err());
    assert!(writer
        .add_entry_from_reader(Path::new("ok/child"), &mut NoRead)
        .is_err());
    assert!(writer
        .add_entry_from_reader(Path::new("retry"), &mut Broken(false))
        .is_err());
    writer
        .add_entry_from_reader(Path::new("retry"), &mut Cursor::new(b"second"))
        .unwrap();
    let (_, bytes) = writer.finalize().unwrap();
    let parsed = asar::AsarReader::new(&bytes, None).unwrap();
    assert_eq!(parsed.read(Path::new("retry")).unwrap().data(), b"second");
    assert_eq!(parsed.read(Path::new("ok")).unwrap().data(), b"first");
}

#[test]
fn packed_corruption_is_rejected_by_reader() {
    use crate::archive::ArchiveReader;
    let mut writer = AsarWriter::new(Vec::new());
    writer
        .add_entry_from_reader(Path::new("payload"), &mut Cursor::new(b"hello world"))
        .unwrap();
    let (_, mut bytes) = writer.finalize().unwrap();
    let temp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(temp.path(), &bytes).unwrap();
    let mut archive = crate::archive::asar::AsarReader::new(temp.path());
    let entry = archive.entries().unwrap().remove(0);
    assert_eq!(archive.extract(&entry, &mut io::sink()).unwrap(), 11);
    *bytes.last_mut().unwrap() ^= 1;
    std::fs::write(temp.path(), &bytes).unwrap();
    let mut archive = crate::archive::asar::AsarReader::new(temp.path());
    let entry = archive.entries().unwrap().remove(0);
    assert!(archive.extract(&entry, &mut io::sink()).is_err());
}

#[test]
fn empty_archive_and_output_failures() {
    let (_, bytes) = AsarWriter::new(Vec::new()).finalize().unwrap();
    assert!(asar::AsarReader::new(&bytes, None)
        .unwrap()
        .files()
        .is_empty());
    struct Failed;
    impl Write for Failed {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("cancelled output"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    assert!(AsarWriter::new(Failed).finalize().is_err());
    let mut writer = AsarWriter::new(Vec::new());
    writer.index_cost = INDEX_BUDGET;
    assert!(writer
        .add_entry_from_reader(Path::new("x"), &mut io::empty())
        .is_err());
}
