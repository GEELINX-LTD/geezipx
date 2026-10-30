//! ACE archive reader (`.ace`).
//!
//! GeeZipX exposes ACE as a read-only archive format backed by the
//! `unarc-rs` crate.  The reader supports listing and extraction;
//! Blowfish-based encryption is supported through `unarc-rs`.
//!
//! # Design notes
//!
//! - **Read-only** — ACE creation is out of scope.
//! - **Path-based** — The reader stores the archive path and re-opens the
//!   file on each operation because [`unarc_rs::unified::UnifiedArchive`]
//!   consumes and owns its `Read + Seek` source.
//! - **No stable magic** — Detection relies on the `.ace` extension.

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

/// Read-only ACE archive reader.
pub struct AceReader {
    path: PathBuf,
    format: ArchiveFormat,
    password: Option<String>,
}

impl fmt::Debug for AceReader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AceReader")
            .field("format", &self.format)
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl AceReader {
    /// Create a new ACE reader for the file at `path`.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            format: ArchiveFormat::Ace,
            password: None,
        }
    }

    fn open_archive(&self) -> GeeZipResult<UnifiedArchive<BufReader<File>>> {
        let file = File::open(&self.path)
            .map_err(|e| GeeZipError::io(e, format!("opening '{}'", self.path.display())))?;
        // The unified setter changes options without updating the ACE backend.
        // Supply the password while opening so ordinary read() also receives it.
        let mut options = ArchiveOptions::default();
        if let Some(password) = &self.password {
            options = options.with_password(password.clone());
        }
        UnifiedArchive::open_with_format_and_options(
            BufReader::new(file),
            UnarcFormat::Ace,
            options,
        )
        .map_err(|e| {
            GeeZipError::format(
                format!("invalid ACE archive '{}': {e}", self.path.display()),
                ArchiveFormat::Ace,
            )
        })
    }

    fn collect_entries(&self) -> GeeZipResult<Vec<Entry>> {
        let mut archive = self.open_archive()?;
        let mut entries = Vec::new();
        while let Some(unarc_entry) = archive
            .next_entry()
            .map_err(|e| GeeZipError::format(format!("ACE read error: {e}"), ArchiveFormat::Ace))?
        {
            entries.push(convert_entry(&unarc_entry));
            archive.skip(&unarc_entry).map_err(|e| {
                GeeZipError::format(format!("ACE skip error: {e}"), ArchiveFormat::Ace)
            })?;
        }
        Ok(entries)
    }
}

impl ArchiveReader for AceReader {
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
        while let Some(unarc_entry) = archive
            .next_entry()
            .map_err(|e| GeeZipError::format(format!("ACE read error: {e}"), ArchiveFormat::Ace))?
        {
            if unarc_entry.name() == entry.path {
                let data = archive.read(&unarc_entry).map_err(|e| {
                    GeeZipError::format(
                        format!("failed to extract ACE entry '{}': {e}", entry.path),
                        ArchiveFormat::Ace,
                    )
                })?;
                let len = data.len() as u64;
                writer.write_all(&data).map_err(|e| {
                    GeeZipError::io(e, format!("writing ACE entry '{}'", entry.path))
                })?;
                return Ok(len);
            }
            let options = archive.options().clone();
            archive
                .skip_with_options(&unarc_entry, &options)
                .map_err(|e| {
                    GeeZipError::format(
                        format!("ACE predecessor decode failed: {e}"),
                        ArchiveFormat::Ace,
                    )
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
        // ACE stores the unfinalized (inverted) CRC32 state.
        crc32: Some(!(unarc_entry.crc() as u32)),
        modified,
        is_dir: unarc_entry.is_directory(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ace_password_reaches_backend_before_open() {
        fn header(data: &[u8]) -> Vec<u8> {
            let mut bytes = ((!crc32fast::hash(data)) as u16).to_le_bytes().to_vec();
            bytes.extend_from_slice(&(data.len() as u16).to_le_bytes());
            bytes.extend_from_slice(data);
            bytes
        }
        let mut main = vec![0, 0, 0];
        main.extend_from_slice(b"**ACE**");
        main.extend_from_slice(&[10, 10, 0, 0]);
        main.extend_from_slice(&[0; 12]);
        let mut archive = header(&main);
        let name = b"encrypted.txt";
        let mut file = vec![1, 0x81, 0]; // file header, payload and encryption flags
        file.extend_from_slice(&8u32.to_le_bytes());
        file.extend_from_slice(&8u32.to_le_bytes());
        file.extend_from_slice(&[0; 8]);
        file.extend_from_slice(&(!crc32fast::hash(b"AAAAAAAA")).to_le_bytes());
        file.extend_from_slice(&[0; 6]); // Store method, quality and reserved fields
        file.extend_from_slice(&(name.len() as u16).to_le_bytes());
        file.extend_from_slice(name);
        archive.extend(header(&file));
        // ACE Blowfish-CBC test vector from MIT-licensed unarc-rs 0.8.0
        // tests/common/ace.rs: password "test", plaintext "AAAAAAAA".
        archive.extend_from_slice(&[0x48, 0x75, 0x48, 0xa7, 0x7c, 0xd6, 0x73, 0x5d]);
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), archive).unwrap();
        let mut reader = AceReader::new(temp.path());
        let entry = reader.entries().unwrap().remove(0);
        assert!(reader.extract(&entry, &mut Vec::new()).is_err());
        reader.set_password("test").unwrap();
        let mut data = Vec::new();
        assert_eq!(reader.extract(&entry, &mut data).unwrap(), 8);
        assert_eq!(data, b"AAAAAAAA");
        reader.set_password("wrong").unwrap();
        assert!(reader.extract(&entry, &mut Vec::new()).is_err());
    }

    #[test]
    fn ace_real_archive_lists_and_extracts() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            temp.path(),
            include_bytes!("../../tests/fixtures/license1.ace"),
        )
        .unwrap();
        let mut reader = AceReader::new(temp.path());
        let entries = reader.entries().unwrap();
        assert!(!entries.is_empty());
        for entry in entries.iter().filter(|e| !e.is_dir) {
            let mut data = Vec::new();
            assert_eq!(reader.extract(entry, &mut data).unwrap(), entry.size);
            assert_eq!(crc32fast::hash(&data), entry.crc32.unwrap());
        }
    }

    #[test]
    fn ace_malformed_fails_cleanly() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("broken.ace");
        std::fs::write(&archive, b"MALFORMED_ACE_FILE").unwrap();
        let mut reader = AceReader::new(&archive);

        let err = reader.entries().unwrap_err();
        assert!(matches!(
            err,
            GeeZipError::Format {
                format: ArchiveFormat::Ace,
                ..
            }
        ));
        assert!(err.to_string().contains("invalid ACE archive"));
    }

    #[test]
    fn ace_trait_object_is_supported() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("trait.ace");
        std::fs::write(&archive, b"FAKE").unwrap();
        let reader: Box<dyn ArchiveReader> = Box::new(AceReader::new(&archive));
        assert_eq!(reader.format(), ArchiveFormat::Ace);
    }

    #[test]
    fn ace_missing_entry_is_handled() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("empty.ace");
        std::fs::write(&archive, b"FAKE").unwrap();
        let mut reader = AceReader::new(&archive);
        let mut sink = Vec::new();

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
