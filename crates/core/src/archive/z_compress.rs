//! Unix compress (`.Z`) reader — single-stream LZW decompression.
//!
//! GeeZipX exposes Unix compress as a read-only single-stream format backed
//! by the `unarc-rs` crate.  Unlike archive containers, `.Z` files contain
//! a single compressed stream; the reader synthesises a single entry whose
//! name is derived from the input filename.
//!
//! # Design notes
//!
//! - **Read-only** — Unix compress creation is out of scope.
//! - **Single-stream** — Behaves like gzip / bzip2: one synthetic entry,
//!   decompress to stdout / file.
//! - **Path-based** — The reader stores the archive path and re-opens the
//!   file on each operation.
//! - **No stable magic** — Detection relies on the `.Z` extension.

use std::fmt;
use std::fs::File;
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};

use unarc_rs::unified::{ArchiveEntry as UnarcEntry, ArchiveFormat as UnarcFormat, UnifiedArchive};

use crate::archive::{ArchiveReader, Entry};
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

/// Read-only Unix compress reader.
pub struct ZReader {
    path: PathBuf,
    format: ArchiveFormat,
}

impl fmt::Debug for ZReader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ZReader")
            .field("format", &self.format)
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl ZReader {
    /// Create a new Unix-compress reader for the file at `path`.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            format: ArchiveFormat::Z,
        }
    }

    /// Derive a synthetic entry name from the archive path.
    ///
    /// Strips the `.Z` suffix (case-sensitive) and uses the resulting
    /// filename as the single entry name.  Falls back to `"output"`.
    fn synthetic_name(&self) -> String {
        let name = self
            .path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "output".to_string());
        if name.ends_with(".Z") {
            name[..name.len() - 2].to_string()
        } else {
            name
        }
    }

    fn open_archive(&self) -> GeeZipResult<UnifiedArchive<BufReader<File>>> {
        let file = File::open(&self.path)
            .map_err(|e| GeeZipError::io(e, format!("opening '{}'", self.path.display())))?;
        UnifiedArchive::open_with_format(BufReader::new(file), UnarcFormat::Z).map_err(|e| {
            GeeZipError::format(
                format!("invalid Z archive '{}': {e}", self.path.display()),
                ArchiveFormat::Z,
            )
        })
    }

    fn collect_entries(&self) -> GeeZipResult<Vec<Entry>> {
        let mut archive = self.open_archive()?;
        // .Z is a single-stream format — expect exactly one entry.
        match archive
            .next_entry()
            .map_err(|e| GeeZipError::format(format!("Z read error: {e}"), ArchiveFormat::Z))?
        {
            Some(unarc_entry) => {
                let name = if unarc_entry.name().is_empty() {
                    self.synthetic_name()
                } else {
                    unarc_entry.name().to_owned()
                };
                Ok(vec![convert_entry(&unarc_entry, &name)])
            }
            None => Ok(vec![]),
        }
    }
}

impl ArchiveReader for ZReader {
    fn format(&self) -> ArchiveFormat {
        self.format
    }

    fn entries(&mut self) -> GeeZipResult<Vec<Entry>> {
        self.collect_entries()
    }

    fn extract(&mut self, _entry: &Entry, writer: &mut dyn Write) -> GeeZipResult<u64> {
        let mut archive = self.open_archive()?;
        match archive
            .next_entry()
            .map_err(|e| GeeZipError::format(format!("Z read error: {e}"), ArchiveFormat::Z))?
        {
            Some(unarc_entry) => {
                let data = archive.read(&unarc_entry).map_err(|e| {
                    GeeZipError::format(
                        format!("failed to decompress Z file: {e}"),
                        ArchiveFormat::Z,
                    )
                })?;
                let len = data.len() as u64;
                writer
                    .write_all(&data)
                    .map_err(|e| GeeZipError::io(e, "writing decompressed Z data"))?;
                Ok(len)
            }
            None => Err(GeeZipError::format("empty Z archive", ArchiveFormat::Z)),
        }
    }
}

/// Infer the decompressed filename for a `.Z` file by stripping the
/// `.Z` suffix from the filename.
pub fn z_output_filename(archive: &Path) -> PathBuf {
    let name = archive
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".to_string());
    let stripped = if name.ends_with(".Z") {
        name[..name.len() - 2].to_string()
    } else {
        name
    };
    PathBuf::from(stripped)
}

fn convert_entry(unarc_entry: &UnarcEntry, name: &str) -> Entry {
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
        path: name.to_owned(),
        size: unarc_entry.original_size(),
        compressed_size: unarc_entry.compressed_size(),
        crc32: Some(unarc_entry.crc() as u32),
        modified,
        is_dir: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn z_malformed_fails_cleanly() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("broken.Z");
        std::fs::write(&archive, b"MALFORMED_Z").unwrap();
        let mut reader = ZReader::new(&archive);

        let err = reader.entries().unwrap_err();
        assert!(matches!(
            err,
            GeeZipError::Format {
                format: ArchiveFormat::Z,
                ..
            }
        ));
        assert!(err.to_string().contains("invalid Z archive"));
    }

    #[test]
    fn z_trait_object_is_supported() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("trait.Z");
        std::fs::write(&archive, b"FAKE").unwrap();
        let reader: Box<dyn ArchiveReader> = Box::new(ZReader::new(&archive));
        assert_eq!(reader.format(), ArchiveFormat::Z);
    }

    #[test]
    fn z_output_filename_strips_suffix() {
        assert_eq!(z_output_filename(Path::new("hello.Z")), Path::new("hello"));
        assert_eq!(
            z_output_filename(Path::new("/tmp/data.Z")),
            Path::new("data")
        );
    }

    #[test]
    fn z_synthetic_name_derived_from_path() {
        let reader = ZReader::new("/tmp/archive.Z");
        assert_eq!(reader.synthetic_name(), "archive");
    }
}
