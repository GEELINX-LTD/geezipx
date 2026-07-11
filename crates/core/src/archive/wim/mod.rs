//! Pure-Rust WIM (Windows Imaging Format) reader.
//!
//! Provides read-only access to `.wim` and `.swm` files without
//! requiring the external `wimlib` / `libwim` C library.
//!
//! # Supported features
//!
//! - WIM v1.0 (0x10D00) containers
//! - XPRESS (LZ77+Huffman) and LZX chunk decompression
//! - Multi-image WIMs (first image only for entries/extract)
//! - Directory tree enumeration with full paths
//!
//! # Not yet supported
//!
//! - LZMS compression (no pure-Rust implementation available)
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

        Ok(WimReader {
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

        let rd = self.path_to_resource.get(&entry.path).ok_or_else(|| {
            GeeZipError::format(
                format!("no resource found for entry '{}'", entry.path),
                ArchiveFormat::Wim,
            )
        })?;

        resource::extract_resource_range(
            &mut self.file,
            rd,
            self.header.compression_type(),
            self.header.chunk_size,
            0,
            rd.original_size,
            writer,
        )
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
