//! ARC/PAK archive reader (`.arc`, `.pak`).
//!
//! GeeZipX exposes ARC as a read-only archive format backed by the
//! `unarc-rs` crate.  The reader supports listing and extraction;
//! password-based XOR decryption is handled by `unarc-rs` when a password is set.
//!
//! # Design notes
//!
//! - **Read-only** — ARC/PAK creation is out of scope.
//! - **Path-based** — The reader stores the archive path and re-opens the
//!   file on each operation because [`unarc_rs::unified::UnifiedArchive`]
//!   consumes and owns its `Read + Seek` source.
//! - **No stable magic** — Detection relies on the `.arc` / `.pak` extension.

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

/// Read-only ARC/PAK archive reader.
pub struct ArcReader {
    path: PathBuf,
    format: ArchiveFormat,
    password: Option<String>,
}

impl fmt::Debug for ArcReader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ArcReader")
            .field("format", &self.format)
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl ArcReader {
    /// Create a new ARC reader for the file at `path`.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            format: ArchiveFormat::Arc,
            password: None,
        }
    }

    fn open_archive(&self) -> GeeZipResult<UnifiedArchive<BufReader<File>>> {
        let file = File::open(&self.path)
            .map_err(|e| GeeZipError::io(e, format!("opening '{}'", self.path.display())))?;
        // UnifiedArchive::set_password only changes its own options; these
        // backends receive the password when they are opened.
        let mut options = ArchiveOptions::default();
        if let Some(password) = &self.password {
            options = options.with_password(password.clone());
        }
        UnifiedArchive::open_with_format_and_options(
            BufReader::new(file),
            UnarcFormat::Arc,
            options,
        )
        .map_err(|e| {
            GeeZipError::format(
                format!("invalid ARC archive '{}': {e}", self.path.display()),
                ArchiveFormat::Arc,
            )
        })
    }

    fn collect_entries(&self) -> GeeZipResult<Vec<Entry>> {
        let mut archive = self.open_archive()?;
        let mut entries = Vec::new();
        while let Some(unarc_entry) = archive
            .next_entry()
            .map_err(|e| GeeZipError::format(format!("ARC read error: {e}"), ArchiveFormat::Arc))?
        {
            entries.push(convert_entry(&unarc_entry));
            archive.skip(&unarc_entry).map_err(|e| {
                GeeZipError::format(format!("ARC skip error: {e}"), ArchiveFormat::Arc)
            })?;
        }
        Ok(entries)
    }
}

impl ArchiveReader for ArcReader {
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
            .map_err(|e| GeeZipError::format(format!("ARC read error: {e}"), ArchiveFormat::Arc))?
        {
            if unarc_entry.name() == entry.path {
                let data = archive.read(&unarc_entry).map_err(|e| {
                    GeeZipError::format(
                        format!("failed to extract ARC entry '{}': {e}", entry.path),
                        ArchiveFormat::Arc,
                    )
                })?;
                let len = data.len() as u64;
                writer.write_all(&data).map_err(|e| {
                    GeeZipError::io(e, format!("writing ARC entry '{}'", entry.path))
                })?;
                return Ok(len);
            }
            archive.skip(&unarc_entry).map_err(|e| {
                GeeZipError::format(format!("ARC skip error: {e}"), ArchiveFormat::Arc)
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
        // ARC stores CRC16, which unarc verifies while decoding. Do not
        // expose it as CRC32 to the shared verifier.
        crc32: None,
        modified,
        is_dir: unarc_entry.original_size() == 0 && unarc_entry.name().ends_with('/'),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stored_arc(password: Option<&str>) -> Vec<u8> {
        let mut archive = Vec::new();
        for (name, data) in [
            ("first", &b"payload with a fake header: \x1a\x02"[..]),
            ("second", &b"second payload"[..]),
        ] {
            let mut crc = 0u16;
            for &byte in data {
                crc ^= u16::from(byte);
                for _ in 0..8 {
                    crc = (crc >> 1) ^ if crc & 1 != 0 { 0xa001 } else { 0 };
                }
            }
            archive.extend_from_slice(&[0x1a, 2]);
            let mut filename = [0; 13];
            filename[..name.len()].copy_from_slice(name.as_bytes());
            archive.extend_from_slice(&filename);
            archive.extend_from_slice(&(data.len() as u32).to_le_bytes());
            archive.extend_from_slice(&[0, 0, 0, 0]);
            archive.extend_from_slice(&crc.to_le_bytes());
            archive.extend_from_slice(&(data.len() as u32).to_le_bytes());
            for (i, &byte) in data.iter().enumerate() {
                archive.push(byte ^ password.map_or(0, |p| p.as_bytes()[i % p.len()]));
            }
        }
        archive.extend_from_slice(&[0x1a, 0]);
        archive
    }

    #[test]
    fn arc_listing_skips_header_like_payload_and_password_is_forwarded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stored.arc");
        for password in [None, Some("secret")] {
            std::fs::write(&path, stored_arc(password)).unwrap();
            let mut reader = ArcReader::new(&path);
            if let Some(password) = password {
                reader.set_password(password).unwrap();
            }
            let entries = reader.entries().unwrap();
            assert_eq!(
                entries.iter().map(|e| e.path.as_str()).collect::<Vec<_>>(),
                ["first", "second"]
            );
            assert!(entries.iter().all(|e| e.crc32.is_none()));
            let mut output = Vec::new();
            reader.extract(&entries[1], &mut output).unwrap();
            assert_eq!(output, b"second payload");
            assert!(!format!("{reader:?}").contains("secret"));
            let mut bad = stored_arc(password);
            bad[23] ^= 1; // first entry CRC16
            std::fs::write(&path, bad).unwrap();
            assert!(reader.extract(&entries[0], &mut std::io::sink()).is_err());
        }
    }

    #[test]
    fn arc_malformed_fails_cleanly() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("broken.arc");
        std::fs::write(&archive, b"MALFORMED_ARC_FILE").unwrap();
        let mut reader = ArcReader::new(&archive);

        let err = reader.entries().unwrap_err();
        assert!(matches!(
            err,
            GeeZipError::Format {
                format: ArchiveFormat::Arc,
                ..
            }
        ));
        // The error format may vary — just ensure it's a clean Format error.
        let msg = err.to_string();
        assert!(
            msg.contains("arc") || msg.contains("format error"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn arc_trait_object_is_supported() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("trait.arc");
        std::fs::write(&archive, b"FAKE").unwrap();
        let reader: Box<dyn ArchiveReader> = Box::new(ArcReader::new(&archive));
        assert_eq!(reader.format(), ArchiveFormat::Arc);
    }

    #[test]
    fn arc_missing_entry_is_handled() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("empty.arc");
        std::fs::write(&archive, b"FAKE").unwrap();
        let mut reader = ArcReader::new(&archive);
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
