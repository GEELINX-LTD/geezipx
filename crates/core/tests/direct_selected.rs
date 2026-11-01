use geezipx_core::{
    archive::{lzh::LzhWriter, tar::TarWriter, ArchiveReader, ArchiveWriter, Entry},
    detect::ArchiveFormat,
    GeeZipResult,
};
use std::{
    io::{self, Cursor, Read, Seek, SeekFrom, Write},
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

struct Counted {
    input: Cursor<Vec<u8>>,
    bytes: Arc<AtomicU64>,
}
impl Read for Counted {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let n = self.input.read(output)?;
        self.bytes.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }
}
impl Seek for Counted {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.input.seek(position)
    }
}

#[test]
fn tar_and_lzh_metadata_and_selected_reads_skip_large_unselected_payloads() {
    let payload = vec![42; 4 * 1024 * 1024];
    let mut tar = TarWriter::new(Vec::new());
    let mut lzh = LzhWriter::new_store_only(Cursor::new(Vec::new()));
    for (name, data) in [
        ("large.bin", payload.as_slice()),
        ("目录/last.txt", b"last".as_slice()),
    ] {
        tar.add_entry_with_size(Path::new(name), &mut &data[..], data.len() as u64)
            .unwrap();
        // LZH's level-0 names are ASCII only.
        let lzh_name = if name == "large.bin" {
            name
        } else {
            "last.txt"
        };
        lzh.add_entry_with_size(Path::new(lzh_name), &mut &data[..], data.len() as u64)
            .unwrap();
    }
    let (_, tar) = tar.finalize().unwrap();
    let (_, lzh) = lzh.finalize().unwrap();
    for (format, data) in [
        (ArchiveFormat::Tar, tar),
        (ArchiveFormat::Lzh, lzh.into_inner()),
    ] {
        let bytes = Arc::new(AtomicU64::new(0));
        let input = Counted {
            input: Cursor::new(data),
            bytes: bytes.clone(),
        };
        let mut reader: Box<dyn ArchiveReader> = match format {
            ArchiveFormat::Tar => Box::new(geezipx_core::archive::tar::TarReader::new(input)),
            _ => Box::new(geezipx_core::archive::lzh::LzhReader::new(input)),
        };
        let entries = reader.entries().unwrap();
        assert!(
            bytes.load(Ordering::Relaxed) < 16 * 1024,
            "{format} read payloads while listing"
        );
        bytes.store(0, Ordering::Relaxed);
        let mut output = Vec::new();
        reader
            .extract_entries(
                &entries[1..],
                &mut |entry, extract| {
                    assert!(entry.path.ends_with("last.txt"));
                    assert_eq!(extract(&mut output)?, 4);
                    Ok(true)
                },
                &|| false,
            )
            .unwrap();
        assert_eq!(output, b"last");
        assert!(
            bytes.load(Ordering::Relaxed) < 16 * 1024,
            "{format} read unselected data"
        );
    }
}

struct CompatibilityReader {
    extracted: usize,
}
impl ArchiveReader for CompatibilityReader {
    fn format(&self) -> ArchiveFormat {
        ArchiveFormat::Asar
    }
    fn entries(&mut self) -> GeeZipResult<Vec<Entry>> {
        panic!("selection must use supplied metadata")
    }
    fn extract(&mut self, _: &Entry, writer: &mut dyn Write) -> GeeZipResult<u64> {
        self.extracted += 1;
        writer.write_all(b"payload")?;
        Ok(7)
    }
}
#[test]
fn compatibility_output_is_lazy_direct_and_cancellable() {
    let entry = Entry {
        path: "selected".into(),
        size: 7,
        compressed_size: 7,
        crc32: None,
        modified: None,
        is_dir: false,
    };
    let mut reader = CompatibilityReader { extracted: 0 };
    reader
        .extract_entries(std::slice::from_ref(&entry), &mut |_, _| Ok(true), &|| {
            false
        })
        .unwrap();
    assert_eq!(reader.extracted, 0, "declined entry must not be decoded");
    let mut data = Vec::new();
    reader
        .extract_entries(
            std::slice::from_ref(&entry),
            &mut |_, extract| {
                extract(&mut data)?;
                Ok(true)
            },
            &|| false,
        )
        .unwrap();
    assert_eq!(reader.extracted, 1);
    assert_eq!(data, b"payload");
    let result = reader.extract_entries(&[entry], &mut |_, _| panic!("cancelled"), &|| true);
    assert!(matches!(result, Err(geezipx_core::GeeZipError::Cancelled)));
}
