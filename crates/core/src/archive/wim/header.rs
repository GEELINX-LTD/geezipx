//! WIM header parsing.
//!
//! Parses the 208-byte WIM header and resource descriptors that
//! locate the lookup table, XML data, and metadata resources.

use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

/// WIM file magic bytes: "MSWIM\0\0\0"
pub(crate) const WIM_MAGIC: &[u8; 8] = b"MSWIM\x00\x00\x00";

/// Parsed WIM header (208 bytes at the start of a .wim file).
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct WimHeader {
    /// Number of images stored in this WIM.
    pub image_count: u32,
    /// Header flags (compression type encoded in bits 16-18).
    pub flags: u32,
    /// Chunk size for compressed resources (default 32768).
    pub chunk_size: u32,
    /// Globally unique identifier for this WIM.
    pub guid: [u8; 16],
    /// Part number (0 for non-split WIMs, 1-based for split).
    pub part_number: u16,
    /// Total number of parts in the split set (1 = non-split).
    pub total_parts: u16,
    /// Resource locating the lookup table.
    pub offset_table: ResourceDescriptor,
    /// Resource locating the XML metadata.
    pub xml_data: ResourceDescriptor,
    /// Resource locating the boot metadata.
    pub boot_metadata: ResourceDescriptor,
    /// 1-based index of the bootable image (0 = none).
    pub boot_index: u32,
    /// Resource locating the integrity table.
    pub integrity: ResourceDescriptor,
}

/// Describes where a WIM resource lives in the file and its sizes.
///
/// On-disk layout: 24 bytes, little-endian.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ResourceDescriptor {
    /// Flags in the top byte (0x02 = metadata, 0x04 = compressed).
    pub flags: u8,
    /// Compressed size as stored in the file (lower 56 bits).
    pub compressed_size: u64,
    /// Absolute byte offset of this resource within the file.
    pub offset: u64,
    /// Uncompressed (original) size in bytes.
    pub original_size: u64,
}

/// Compression algorithm used for chunked resources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompressionType {
    None,
    Xpress,
    Lzx,
    Lzms,
}

impl WimHeader {
    /// Parse a WIM header from exactly 208 bytes.
    pub fn parse(data: &[u8; 208]) -> GeeZipResult<Self> {
        if &data[0..8] != WIM_MAGIC.as_slice() {
            return Err(GeeZipError::format("not a WIM file", ArchiveFormat::Wim));
        }

        let header_size = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
        if header_size < 208 {
            return Err(GeeZipError::format(
                "WIM header too small",
                ArchiveFormat::Wim,
            ));
        }

        let version = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
        // WIM version 1.0 = 0x10D00
        if version != 0x10D00 {
            return Err(GeeZipError::format(
                format!("unsupported WIM version {version:#x}"),
                ArchiveFormat::Wim,
            ));
        }

        let flags = u32::from_le_bytes([data[16], data[17], data[18], data[19]]);
        let chunk_size = u32::from_le_bytes([data[20], data[21], data[22], data[23]]);

        let mut guid = [0u8; 16];
        guid.copy_from_slice(&data[24..40]);

        let part_number = u16::from_le_bytes([data[40], data[41]]);
        let total_parts = u16::from_le_bytes([data[42], data[43]]);
        let image_count = u32::from_le_bytes([data[44], data[45], data[46], data[47]]);

        let offset_table = ResourceDescriptor::parse(&data[48..72])?;
        let xml_data = ResourceDescriptor::parse(&data[72..96])?;
        let boot_metadata = ResourceDescriptor::parse(&data[96..120])?;

        let boot_index = u32::from_le_bytes([data[120], data[121], data[122], data[123]]);

        let integrity = ResourceDescriptor::parse(&data[124..148])?;

        Ok(WimHeader {
            image_count,
            flags,
            chunk_size,
            guid,
            part_number,
            total_parts,
            offset_table,
            xml_data,
            boot_metadata,
            boot_index,
            integrity,
        })
    }

    /// Determine the compression type from header flags.
    pub fn compression_type(&self) -> CompressionType {
        if self.flags & 0x0000_0002 == 0 {
            return CompressionType::None;
        }
        if self.flags & 0x0004_0000 != 0 {
            return CompressionType::Lzx;
        }
        if self.flags & 0x0002_0000 != 0 {
            return CompressionType::Xpress;
        }
        if self.flags & 0x0008_0000 != 0 {
            return CompressionType::Lzms;
        }
        CompressionType::None
    }

    /// Serialize to a 208-byte on-disk header (little-endian).
    pub(crate) fn to_bytes(&self) -> [u8; 208] {
        let mut buf = [0u8; 208];
        // Magic
        buf[0..8].copy_from_slice(WIM_MAGIC);
        // header_size = 208
        buf[8..12].copy_from_slice(&208u32.to_le_bytes());
        // version = 0x10D00
        buf[12..16].copy_from_slice(&0x10D00u32.to_le_bytes());
        // flags
        buf[16..20].copy_from_slice(&self.flags.to_le_bytes());
        // chunk_size
        buf[20..24].copy_from_slice(&self.chunk_size.to_le_bytes());
        // guid
        buf[24..40].copy_from_slice(&self.guid);
        // part_number
        buf[40..42].copy_from_slice(&self.part_number.to_le_bytes());
        // total_parts
        buf[42..44].copy_from_slice(&self.total_parts.to_le_bytes());
        // image_count
        buf[44..48].copy_from_slice(&self.image_count.to_le_bytes());
        // offset_table
        buf[48..72].copy_from_slice(&self.offset_table.to_bytes());
        // xml_data
        buf[72..96].copy_from_slice(&self.xml_data.to_bytes());
        // boot_metadata
        buf[96..120].copy_from_slice(&self.boot_metadata.to_bytes());
        // boot_index
        buf[120..124].copy_from_slice(&self.boot_index.to_le_bytes());
        // integrity
        buf[124..148].copy_from_slice(&self.integrity.to_bytes());
        // bytes 148..208 are reserved (zeros)
        buf
    }
}

impl ResourceDescriptor {
    /// Parse a 24-byte resource descriptor.
    pub(crate) fn parse(data: &[u8]) -> GeeZipResult<Self> {
        if data.len() < 24 {
            return Err(GeeZipError::format(
                "resource descriptor too short",
                ArchiveFormat::Wim,
            ));
        }

        let flags_and_size = u64::from_le_bytes([
            data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
        ]);
        let flags = ((flags_and_size >> 56) & 0xFF) as u8;
        let compressed_size = flags_and_size & 0x00FF_FFFF_FFFF_FFFF;

        let offset = u64::from_le_bytes([
            data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15],
        ]);

        let original_size = u64::from_le_bytes([
            data[16], data[17], data[18], data[19], data[20], data[21], data[22], data[23],
        ]);

        Ok(ResourceDescriptor {
            flags,
            compressed_size,
            offset,
            original_size,
        })
    }

    /// Returns true if this resource is compressed (flag 0x04 set).
    pub fn is_compressed(&self) -> bool {
        self.flags & 0x04 != 0
    }

    /// Returns true if this resource contains metadata (flag 0x02 set).
    pub fn is_metadata(&self) -> bool {
        self.flags & 0x02 != 0
    }

    /// Serialize to a 24-byte little-endian buffer.
    pub(crate) fn to_bytes(self) -> [u8; 24] {
        let mut buf = [0u8; 24];
        let flags_and_size: u64 =
            ((self.flags as u64) << 56) | (self.compressed_size & 0x00FF_FFFF_FFFF_FFFF);
        buf[0..8].copy_from_slice(&flags_and_size.to_le_bytes());
        buf[8..16].copy_from_slice(&self.offset.to_le_bytes());
        buf[16..24].copy_from_slice(&self.original_size.to_le_bytes());
        buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_header() {
        let mut buf = [0u8; 208];
        buf[0..8].copy_from_slice(WIM_MAGIC);

        // header_size = 208 (0xD0)
        buf[8] = 0xD0;
        // version = 0x10D00
        buf[12] = 0x00;
        buf[13] = 0x0D;
        buf[14] = 0x01;
        buf[16] = 0x02; // FLAG_HEADER_COMPRESSION
        buf[18] = 0x02; // 0x00020000
                        // chunk_size = 32768
        buf[20] = 0x00;
        buf[21] = 0x80;
        // image_count = 1
        buf[44] = 1;
        // total_parts = 1 (u16 at offset 42, low byte first)
        buf[42] = 1;

        let h = WimHeader::parse(&buf).expect("should parse");
        assert_eq!(h.image_count, 1);
        assert_eq!(h.total_parts, 1);
        assert_eq!(h.flags, 0x00020002);
        assert_eq!(h.chunk_size, 32768);
        assert_eq!(h.compression_type(), CompressionType::Xpress);
    }

    #[test]
    fn bad_magic() {
        let buf = [0u8; 208];
        let err = WimHeader::parse(&buf).unwrap_err();
        assert!(err.to_string().contains("not a WIM"));
    }

    #[test]
    fn resource_descriptor_roundtrip() {
        let original = ResourceDescriptor {
            flags: 0x06, // metadata + compressed
            compressed_size: 1024,
            offset: 512,
            original_size: 2048,
        };
        let bytes = original.to_bytes();
        let parsed = ResourceDescriptor::parse(&bytes).unwrap();
        assert_eq!(parsed.flags, 0x06);
        assert!(parsed.is_metadata());
        assert!(parsed.is_compressed());
        assert_eq!(parsed.compressed_size, 1024);
        assert_eq!(parsed.offset, 512);
        assert_eq!(parsed.original_size, 2048);
    }

    #[test]
    fn resource_descriptor_to_bytes_known_pattern() {
        // Also test against a known-good raw pattern
        let mut raw = [0u8; 24];
        let fns: u64 = (0x06u64 << 56) | 1024;
        raw[0..8].copy_from_slice(&fns.to_le_bytes());
        raw[8..16].copy_from_slice(&512u64.to_le_bytes());
        raw[16..24].copy_from_slice(&2048u64.to_le_bytes());

        let parsed = ResourceDescriptor::parse(&raw).unwrap();
        assert_eq!(parsed.flags, 0x06);
    }

    #[test]
    fn header_roundtrip() {
        let mut guid = [0u8; 16];
        guid[0] = 0x42;
        let original = WimHeader {
            image_count: 1,
            flags: 0,
            chunk_size: 32768,
            guid,
            part_number: 1,
            total_parts: 1,
            offset_table: ResourceDescriptor {
                flags: 0,
                compressed_size: 100,
                offset: 500,
                original_size: 100,
            },
            xml_data: ResourceDescriptor {
                flags: 0,
                compressed_size: 200,
                offset: 600,
                original_size: 200,
            },
            boot_metadata: ResourceDescriptor {
                flags: 0,
                compressed_size: 0,
                offset: 0,
                original_size: 0,
            },
            boot_index: 0,
            integrity: ResourceDescriptor {
                flags: 0,
                compressed_size: 0,
                offset: 0,
                original_size: 0,
            },
        };

        let bytes = original.to_bytes();
        let parsed = WimHeader::parse(&bytes).unwrap();

        assert_eq!(parsed.image_count, 1);
        assert_eq!(parsed.flags, 0);
        assert_eq!(parsed.chunk_size, 32768);
        assert_eq!(parsed.guid[0], 0x42);
        assert_eq!(parsed.part_number, 1);
        assert_eq!(parsed.total_parts, 1);
        assert_eq!(parsed.offset_table.compressed_size, 100);
        assert_eq!(parsed.offset_table.offset, 500);
        assert_eq!(parsed.offset_table.original_size, 100);
        assert_eq!(parsed.xml_data.compressed_size, 200);
        assert_eq!(parsed.xml_data.offset, 600);
        assert_eq!(parsed.xml_data.original_size, 200);
        assert_eq!(parsed.compression_type(), CompressionType::None);
    }
}
