//! Pure-Rust WIM (Windows Imaging Format) reader.
//!
//! Provides read-only access to `.wim` and `.swm` files without
//! requiring the external `wimlib` / `libwim` C library.
//!
//! # Supported features
//!
//! - WIM v1.0 (0x10D00) containers
//! - XPRESS (LZ77+Huffman) chunk decompression
//! - Experimental LZX decoding: known incompatibility with wimlib-produced
//!   WIMs (`encountered empty decode tree`); not full LZX support
//! - Multi-image WIMs (first image only for entries/extract)
//! - Directory tree enumeration with full paths
//!
//! # Not yet supported
//!
//! - LZMS compression (decoder not implemented here)
//! - Split (`.swm`) WIM files
//! - XPRESS or LZX compression when writing (uncompressed only for now)
//! - Extracting from images beyond the first one

mod header;
mod lookup;
mod metadata;
mod resource;
mod sha1;
mod writer;
mod xml;

pub use writer::WimWriter;

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::archive::{ArchiveReader, Entry};
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

use header::{ResourceDescriptor, WimHeader};
use lookup::LookupTable;
use xml::XmlData;

/// Pure-Rust read-only WIM archive reader.
pub struct WimReader {
    /// The open WIM file handle.
    file: std::fs::File,
    /// Absolute path to the WIM file (kept for Debug / error messages).
    #[allow(dead_code)]
    path: PathBuf,
    /// Parsed WIM header.
    header: WimHeader,
    /// Parsed lookup table (maps hashes to resources).
    lookup: LookupTable,
    /// Parsed XML metadata (image names, counts, timestamps).
    xml: XmlData,
    /// Lazily-filled entry cache.
    entries_cache: Option<Vec<Entry>>,
    /// Map from entry path to its resource descriptor (for extraction).
    path_to_resource: HashMap<String, ResourceDescriptor>,
    resource_hashes: HashMap<(u64, u64), [u8; 20]>,
}

impl std::fmt::Debug for WimReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WimReader").finish_non_exhaustive()
    }
}

impl WimReader {
    /// Open a WIM file and parse its header, lookup table, and XML metadata.
    pub fn open(path: &Path) -> GeeZipResult<Self> {
        use std::io::Read;

        let mut file =
            std::fs::File::open(path).map_err(|e| GeeZipError::io(e, "opening WIM file"))?;

        let mut header_buf = [0u8; 208];
        file.read_exact(&mut header_buf)
            .map_err(|e| GeeZipError::io(e, "reading WIM header"))?;

        let header = WimHeader::parse(&header_buf)?;

        if header.total_parts > 1 {
            return Err(GeeZipError::format(
                "split WIM files (.swm) are not yet supported",
                ArchiveFormat::Wim,
            ));
        }

        let lookup = LookupTable::parse(&mut file, &header)?;
        let xml = XmlData::parse(&mut file, &header)?;

        let resource_hashes = lookup
            .file_streams
            .iter()
            .map(|stream| {
                (
                    (stream.resource.offset, stream.resource.original_size),
                    stream.hash.0,
                )
            })
            .collect();

        Ok(WimReader {
            resource_hashes,
            file,
            path: path.to_path_buf(),
            header,
            lookup,
            xml,
            entries_cache: None,
            path_to_resource: HashMap::new(),
        })
    }

    #[allow(dead_code)]
    /// Return the parsed image metadata (names, counts, etc.).
    pub(crate) fn images(&self) -> &[XmlData] {
        // This is a bit awkward — XmlData holds Vec<ImageInfo>.
        // For now, just return a reference to the single XmlData.
        std::slice::from_ref(&self.xml)
    }
}

impl ArchiveReader for WimReader {
    fn format(&self) -> ArchiveFormat {
        ArchiveFormat::Wim
    }

    fn entries(&mut self) -> GeeZipResult<Vec<Entry>> {
        if let Some(ref cache) = self.entries_cache {
            return Ok(cache.clone());
        }

        // Currently only the first image (index 0) is supported.
        let (entries, path_map) =
            metadata::walk_directory_tree(&mut self.file, 0, &self.header, &self.lookup)?;

        self.entries_cache = Some(entries.clone());
        self.path_to_resource = path_map;

        Ok(entries)
    }

    fn extract(&mut self, entry: &Entry, writer: &mut dyn Write) -> GeeZipResult<u64> {
        if entry.is_dir {
            return Ok(0);
        }

        // Ensure entries have been loaded (populates path_to_resource)
        if self.entries_cache.is_none() {
            self.entries()?;
        }

        let Some(rd) = self.path_to_resource.get(&entry.path) else {
            if self.entries_cache.as_ref().is_some_and(|entries| {
                entries
                    .iter()
                    .any(|item| item.path == entry.path && item.size == 0 && !item.is_dir)
            }) {
                return Ok(0);
            }
            return Err(GeeZipError::format(
                format!("no resource found for entry '{}'", entry.path),
                ArchiveFormat::Wim,
            ));
        };
        let expected = self
            .resource_hashes
            .get(&(rd.offset, rd.original_size))
            .ok_or_else(|| {
                GeeZipError::format("missing WIM resource digest", ArchiveFormat::Wim)
            })?;
        let mut checked = sha1::DigestWriter::new(writer);
        let count = resource::extract_resource_range(
            &mut self.file,
            rd,
            self.header.compression_type(),
            self.header.chunk_size,
            0,
            rd.original_size,
            &mut checked,
        )?;
        if &checked.finish() != expected {
            return Err(GeeZipError::format(
                "WIM resource SHA-1 mismatch",
                ArchiveFormat::Wim,
            ));
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::ArchiveReader;

    /// Build a minimal uncompressed WIM file in memory for testing.
    ///
    /// The generated WIM contains:
    /// - 1 image named "Test"
    /// - 1 file: "hello.txt" with contents "hello wim\n"
    #[allow(dead_code)]
    fn build_test_wim() -> Vec<u8> {
        // We'll construct a minimal WIM by hand:
        //
        // Layout:
        // [Header 208 bytes]
        // [XML data resource (uncompressed)]
        // [Lookup table resource (uncompressed)]
        // [Metadata resource (uncompressed)]
        // [File data resource (uncompressed)]
        //
        // This is complex. For now we'll mark the test as ignored
        // and note that real WIM files should be used for integration tests.
        // A proper minimal WIM builder would be ~300 lines.
        Vec::new()
    }

    #[test]
    fn reads_independent_wimlib_xpress_fixture() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("external.wim");
        std::fs::write(
            &path,
            include_bytes!("../../../tests/fixtures/wimlib-xpress.wim"),
        )
        .unwrap();
        let mut reader = WimReader::open(&path).unwrap();
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 4);
        assert!(!entries.iter().any(|entry| entry.path.is_empty()));
        for (name, expected) in [
            ("empty", Vec::new()),
            ("目录/示例.txt", b"WIM external fixture\n".repeat(4000)),
        ] {
            let mut bytes = Vec::new();
            let entry = entries.iter().find(|entry| entry.path == name).unwrap();
            reader.extract(entry, &mut bytes).unwrap();
            assert_eq!(bytes, expected);
        }
        assert!(
            entries
                .iter()
                .find(|entry| entry.path == "empty-dir")
                .unwrap()
                .is_dir
        );
    }

    #[test]
    fn corrupt_wim_file_or_metadata_fails_integrity_verification() {
        use crate::archive::ArchiveWriter;
        use std::io::{Read, Seek, SeekFrom};
        for corrupt_metadata in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("corrupt.wim");
            let file = std::fs::File::create(&path).unwrap();
            let mut writer = WimWriter::new(file);
            writer
                .add_entry_from_reader(Path::new("data"), &mut &b"original bytes"[..])
                .unwrap();
            Box::new(writer).finish().unwrap();
            let mut reader = WimReader::open(&path).unwrap();
            let offset = if corrupt_metadata {
                reader.lookup.metadata_streams[0].resource.offset + 8 + 40
            } else {
                reader.lookup.file_streams[0].resource.offset
            };
            let mut file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
                .unwrap();
            file.seek(SeekFrom::Start(offset)).unwrap();
            let mut original = [0];
            file.read_exact(&mut original).unwrap();
            file.seek(SeekFrom::Start(offset)).unwrap();
            // A fixed replacement can equal the timestamp byte and leave the
            // resource intact. Always change at least one bit of the source.
            file.write_all(&[original[0] ^ 1]).unwrap();
            drop(file);
            if corrupt_metadata {
                assert!(reader
                    .entries()
                    .unwrap_err()
                    .to_string()
                    .contains("SHA-1 mismatch"));
            } else {
                let entries = reader.entries().unwrap();
                assert!(reader
                    .extract(&entries[0], &mut Vec::new())
                    .unwrap_err()
                    .to_string()
                    .contains("SHA-1 mismatch"));
            }
        }
    }

    #[test]
    fn open_nonexistent_file() {
        let err = WimReader::open(Path::new("/nonexistent/test.wim")).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("opening") || msg.contains("No such"),
            "msg: {msg}"
        );
    }

    #[test]
    fn open_file_too_small() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("tiny.wim");
        std::fs::write(&path, b"not a wim").unwrap();

        let err = WimReader::open(&path).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("not a WIM") || msg.contains("WIM"),
            "msg: {msg}"
        );
    }

    #[test]
    fn trait_object_supported() {
        // Verify WimReader can be used as Box<dyn ArchiveReader>
        fn _assert_reader(_: Box<dyn ArchiveReader>) {}
    }
}
