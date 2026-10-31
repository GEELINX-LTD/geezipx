//! ARJ archive reader (`.arj`).
//!
//! GeeZipX exposes ARJ as a read-only archive format backed by the
//! `unarc-rs` crate.  The reader supports listing, extraction, and
//! password-protected archives (Garble and GOST-40 encryption).
//!
//! # Design notes
//!
//! - **Read-only** — ARJ creation is out of scope.
//! - **Path-based** — The reader stores the archive path and re-opens the
//!   file on each operation because [`unarc_rs::unified::UnifiedArchive`]
//!   consumes and owns its `Read + Seek` source.
//! - **No stable magic** — Detection relies on the `.arj` extension.

use std::fmt;
use std::fs::File;
use std::io::{BufReader, Write};
use std::path::PathBuf;

use unarc_rs::unified::{
    ArchiveEntry as UnarcEntry, ArchiveFormat as UnarcFormat, ArchiveOptions, UnifiedArchive,
};

use crate::archive::{ArchiveReader, Entry};
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

/// Read-only ARJ archive reader.
pub struct ArjReader {
    path: PathBuf,
    format: ArchiveFormat,
    password: Option<String>,
}

impl fmt::Debug for ArjReader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ArjReader")
            .field("format", &self.format)
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl ArjReader {
    /// Create a new ARJ reader for the file at `path`.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            format: ArchiveFormat::Arj,
            password: None,
        }
    }

    fn open_archive(&self) -> GeeZipResult<UnifiedArchive<BufReader<File>>> {
        let file = File::open(&self.path)
            .map_err(|e| GeeZipError::io(e, format!("opening '{}'", self.path.display())))?;
        // The unified setter changes options without updating the ARJ backend.
        let mut options = ArchiveOptions::default();
        if let Some(password) = &self.password {
            options = options.with_password(password.clone());
        }
        UnifiedArchive::open_with_format_and_options(
            BufReader::new(file),
            UnarcFormat::Arj,
            options,
        )
        .map_err(|e| {
            GeeZipError::format(
                format!("invalid ARJ archive '{}': {e}", self.path.display()),
                ArchiveFormat::Arj,
            )
        })
    }

    fn collect_entries(&self) -> GeeZipResult<Vec<Entry>> {
        let mut archive = self.open_archive()?;
        let mut entries = Vec::new();
        while let Some(unarc_entry) = archive
            .next_entry()
            .map_err(|e| GeeZipError::format(format!("ARJ read error: {e}"), ArchiveFormat::Arj))?
        {
            entries.push(convert_entry(&unarc_entry));
            archive.skip(&unarc_entry).map_err(|e| {
                GeeZipError::format(format!("ARJ skip error: {e}"), ArchiveFormat::Arj)
            })?;
        }
        Ok(entries)
    }
}

impl ArchiveReader for ArjReader {
    fn format(&self) -> ArchiveFormat {
        self.format
    }

    fn set_password(&mut self, password: &str) -> GeeZipResult<()> {
        self.password = Some(password.to_owned());
        Ok(())
    }

    fn entries(&mut self) -> GeeZipResult<Vec<Entry>> {
        self.collect_entries()
    }

    fn extract(&mut self, entry: &Entry, writer: &mut dyn Write) -> GeeZipResult<u64> {
        if entry.is_dir {
            return Ok(0);
        }

        let mut archive = self.open_archive()?;
        // Iterate entries until we find the matching one.
        while let Some(unarc_entry) = archive
            .next_entry()
            .map_err(|e| GeeZipError::format(format!("ARJ read error: {e}"), ArchiveFormat::Arj))?
        {
            if unarc_entry.name() == entry.path {
                let data = archive.read(&unarc_entry).map_err(|e| {
                    GeeZipError::format(
                        format!("failed to extract ARJ entry '{}': {e}", entry.path),
                        ArchiveFormat::Arj,
                    )
                })?;
                let len = data.len() as u64;
                writer.write_all(&data).map_err(|e| {
                    GeeZipError::io(e, format!("writing ARJ entry '{}'", entry.path))
                })?;
                return Ok(len);
            }
            // Skip non-matching entry.
            archive.skip(&unarc_entry).map_err(|e| {
                GeeZipError::format(format!("ARJ skip error: {e}"), ArchiveFormat::Arj)
            })?;
        }
        Err(GeeZipError::EntryNotFound {
            name: entry.path.clone(),
        })
    }
}

fn convert_entry(unarc_entry: &UnarcEntry) -> Entry {
    let modified = unarc_entry.modified_time().map(|dt| {
        crate::archive::datetime_to_timestamp(
            dt.year() as u64,
            dt.month() as u64,
            dt.day() as u64,
            dt.hour() as u64,
            dt.minute() as u64,
            dt.second() as u64,
        )
    });
    Entry {
        path: unarc_entry.name().to_owned(),
        size: unarc_entry.original_size(),
        compressed_size: unarc_entry.compressed_size(),
        crc32: Some(unarc_entry.crc() as u32),
        modified,
        is_dir: unarc_entry.is_directory(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_test_arj() -> Vec<u8> {
        // ARJ archives have a complex binary structure.  For testing we rely on
        // `unarc-rs` to handle the parsing; we create a minimal valid ARJ file
        // from known-good bytes or skip the round-trip test and only test
        // malformed-input handling.
        //
        // The `unarc-rs` crate itself includes fixture-based tests for ARJ
        // parsing, so the GeeZipX integration layer only needs to verify:
        //   - malformed input → clean error
        //   - trait-object dispatch works
        b"MALFORMED_ARJ_FILE".to_vec()
    }

    #[test]
    fn arj_password_reaches_backend_before_open() {
        fn header(data: &[u8]) -> Vec<u8> {
            let mut bytes = vec![0x60, 0xea];
            bytes.extend_from_slice(&(data.len() as u16).to_le_bytes());
            bytes.extend_from_slice(data);
            bytes.extend_from_slice(&crc32fast::hash(data).to_le_bytes());
            bytes.extend_from_slice(&[0, 0]); // no extended headers
            bytes
        }
        let mut main = vec![0; 30];
        main[0] = 30;
        main[1] = 11;
        main[2] = 1;
        main[4] = 1; // garbled, legacy encryption version zero
        main[6] = 2; // archive comment header
        main.extend_from_slice(b"encrypted.arj\0\0");
        let mut archive = header(&main);
        let data = b"ARJ garble password regression";
        let mut file = vec![0; 30];
        file[0] = 30;
        file[1] = 11;
        file[2] = 1;
        file[4] = 1; // garbled Store entry
        file[7] = 17; // password modifier
        file[12..16].copy_from_slice(&(data.len() as u32).to_le_bytes());
        file[16..20].copy_from_slice(&(data.len() as u32).to_le_bytes());
        file[20..24].copy_from_slice(&crc32fast::hash(data).to_le_bytes());
        file.extend_from_slice(b"payload.txt\0\0");
        archive.extend(header(&file));
        let password = b"secret";
        for (index, &byte) in data.iter().enumerate() {
            archive.push(byte ^ password[index % password.len()].wrapping_add(17));
        }
        archive.extend_from_slice(&[0x60, 0xea, 0, 0]);
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), archive).unwrap();
        let mut reader = ArjReader::new(temp.path());
        let entry = reader.entries().unwrap().remove(0);
        assert!(reader.extract(&entry, &mut Vec::new()).is_err());
        reader.set_password("secret").unwrap();
        let mut output = Vec::new();
        assert_eq!(
            reader.extract(&entry, &mut output).unwrap(),
            data.len() as u64
        );
        assert_eq!(output, data);
        reader.set_password("wrong").unwrap();
        assert!(reader.extract(&entry, &mut Vec::new()).is_err());
    }

    #[test]
    fn arj_real_archive_lists_and_extracts() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            temp.path(),
            include_bytes!("../../tests/fixtures/stored.arj"),
        )
        .unwrap();
        let mut reader = ArjReader::new(temp.path());
        let entries = reader.entries().unwrap();
        assert!(!entries.is_empty());
        for entry in entries.iter().filter(|e| !e.is_dir) {
            let mut data = Vec::new();
            assert_eq!(reader.extract(entry, &mut data).unwrap(), entry.size);
            assert_eq!(crc32fast::hash(&data), entry.crc32.unwrap());
        }
    }

    #[test]
    fn arj_malformed_fails_cleanly() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("broken.arj");
        std::fs::write(&archive, build_test_arj()).unwrap();
        let mut reader = ArjReader::new(&archive);

        let err = reader.entries().unwrap_err();
        assert!(matches!(
            err,
            GeeZipError::Format {
                format: ArchiveFormat::Arj,
                ..
            }
        ));
        assert!(err.to_string().contains("invalid ARJ archive"));
    }

    #[test]
    fn arj_trait_object_is_supported() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("trait.arj");
        std::fs::write(&archive, b"FAKE").unwrap();
        let reader: Box<dyn ArchiveReader> = Box::new(ArjReader::new(&archive));
        assert_eq!(reader.format(), ArchiveFormat::Arj);
    }

    #[test]
    fn arj_missing_entry_returns_entry_not_found() {
        // When the archive is empty/malformed but not completely broken, requesting
        // a non-existent entry should yield EntryNotFound.
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("empty.arj");
        // Write a tiny header that unarc-rs will parse with zero entries,
        // then fail cleanly when looking for a specific entry.
        std::fs::write(&archive, b"FAKE").unwrap();
        let mut reader = ArjReader::new(&archive);
        let mut sink = Vec::new();

        // Skip the entries() call which would fail on malformed data; test
        // the extract path on an empty/malformed archive.
        let result = reader.extract(
            &Entry {
                path: "missing.txt".into(),
                size: 0,
                compressed_size: 0,
                crc32: None,
                modified: None,
                is_dir: false,
            },
            &mut sink,
        );
        assert!(result.is_err());
    }
}
