//! WIM lookup table parsing.
//!
//! The lookup table maps SHA-1 hashes to resource descriptors and
//! separates metadata streams from file data streams.

use std::io::{Read, Seek, SeekFrom};

use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

use super::header::{ResourceDescriptor, WimHeader};
use super::resource;

#[allow(dead_code)]
/// A 20-byte SHA-1 hash identifying a file stream.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Sha1Hash(pub [u8; 20]);

#[allow(dead_code)]
/// Describes a single stream in the WIM lookup table.
#[derive(Debug, Clone)]
pub(crate) struct StreamDescriptor {
    /// Resource where this stream's data lives.
    pub resource: ResourceDescriptor,
    /// Part number (0 for non-split WIMs).
    pub part_number: u16,
    /// Number of directory entries referencing this stream.
    pub ref_count: u32,
    /// SHA-1 hash of the uncompressed stream data.
    pub hash: Sha1Hash,
}

/// The parsed lookup table, split into metadata and file streams.
#[derive(Debug, Clone)]
pub(crate) struct LookupTable {
    /// Streams flagged as metadata (contain directory entry trees).
    pub metadata_streams: Vec<StreamDescriptor>,
    /// Streams containing file data.
    pub file_streams: Vec<StreamDescriptor>,
}

impl LookupTable {
    /// Parse the lookup table from the offset table resource in `file`.
    pub fn parse(file: &mut std::fs::File, header: &WimHeader) -> GeeZipResult<Self> {
        let data = resource::read_resource(
            file,
            &header.offset_table,
            header.compression_type(),
            header.chunk_size,
        )?;

        if data.len() % 50 != 0 {
            return Err(GeeZipError::format(
                "lookup table size not a multiple of 50",
                ArchiveFormat::Wim,
            ));
        }

        let count = data.len() / 50;
        let mut metadata_streams = Vec::new();
        let mut file_streams = Vec::new();

        for i in 0..count {
            let base = i * 50;

            let resource = ResourceDescriptor::parse(&data[base..base + 24])?;

            let part_number = u16::from_le_bytes([data[base + 24], data[base + 25]]);

            let ref_count = u32::from_le_bytes([
                data[base + 26],
                data[base + 27],
                data[base + 28],
                data[base + 29],
            ]);

            let mut hash = [0u8; 20];
            hash.copy_from_slice(&data[base + 30..base + 50]);

            let sd = StreamDescriptor {
                resource,
                part_number,
                ref_count,
                hash: Sha1Hash(hash),
            };

            if sd.resource.is_metadata() {
                metadata_streams.push(sd);
            } else {
                file_streams.push(sd);
            }
        }

        Ok(LookupTable {
            metadata_streams,
            file_streams,
        })
    }

    #[allow(dead_code)]
    /// Find a file stream by its SHA-1 hash.
    pub fn find_file_by_hash(&self, hash: &Sha1Hash) -> Option<&StreamDescriptor> {
        self.file_streams.iter().find(|s| &s.hash == hash)
    }
}

#[allow(dead_code)]
/// Move the file cursor and read exactly `len` bytes.
pub(crate) fn read_at(file: &mut std::fs::File, offset: u64, len: usize) -> GeeZipResult<Vec<u8>> {
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| GeeZipError::io(e, "seeking in WIM file"))?;
    let mut buf = vec![0u8; len];
    file.read_exact(&mut buf)
        .map_err(|e| GeeZipError::io(e, "reading from WIM file"))?;
    Ok(buf)
}
