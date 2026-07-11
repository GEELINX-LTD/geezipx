//! ISZ (ISO Zipped) compressed disc image format support.
//!
//! ISZ is a block-compressed ISO wrapper. It does NOT implement
//! `ArchiveReader`/`ArchiveWriter` — it operates on raw byte streams
//! in the same style as the AES, IMG, and BIN single-stream engines.

use std::io::{Read, Seek, Write};

use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

const ISZ_MAGIC: &[u8; 4] = b"IsZ!";
const HEADER_SIZE: usize = 64;
const XOR_KEY: [u8; 4] = [0xb6, 0x8c, 0xa5, 0xde];

#[derive(Debug)]
#[allow(dead_code)]
struct IszHeader {
    header_size: u8,           // offset 4
    version: u8,               // offset 5
    volume_serial_number: u32, // offset 6 (le)
    sector_size: u16,          // offset 10 (le), typically 2048
    total_sectors: u32,        // offset 12 (le)
    encryption_type: u8,       // offset 16
    segment_size: u64,         // offset 17 (le)
    num_blocks: u32,           // offset 25 (le)
    block_size_sectors: u32,   // offset 29 (le) — sectors per block
    pointer_length: u8,        // offset 33 — typically 3
    // file_seg_number: u8,      // offset 34 — unused in MVP
    chunk_pointers_offset: u32,   // offset 35 (le)
    segment_pointers_offset: u32, // offset 39 (le)
    data_offset: u32,             // offset 43 (le)
                                  // checksum1: u32,           // offset 48 — CRC32 of uncompressed data
                                  // size1: u32,               // offset 52 — uncompressed size
                                  // checksum2: u32,           // offset 60 — CRC32 of compressed data
}

/// Decode a 3-byte chunk pointer from the XOR-deobfuscated table.
/// Top 2 bits = data type (0=zeros, 1=data, 2=zlib, 3=bzip2)
/// Lower 22 bits = data size
fn decode_chunk_pointer(bytes: &[u8; 3]) -> (u8, u32) {
    let val = (bytes[0] as u32) | ((bytes[1] as u32) << 8) | ((bytes[2] as u32) << 16);
    let data_type = (val >> 22) as u8;
    let data_size = val & 0x3F_FFFF;
    (data_type, data_size)
}

/// Apply XOR obfuscation to a byte buffer in-place.
fn xor_deobfuscate(data: &mut [u8]) {
    for (i, byte) in data.iter_mut().enumerate() {
        *byte ^= XOR_KEY[i % 4];
    }
}

/// Read and parse the ISZ header from a reader.
fn parse_header<R: Read + Seek>(reader: &mut R) -> GeeZipResult<IszHeader> {
    let mut buf = [0u8; HEADER_SIZE];
    reader
        .seek(std::io::SeekFrom::Start(0))
        .map_err(|e| GeeZipError::io(e, "seeking to ISZ header"))?;
    reader
        .read_exact(&mut buf)
        .map_err(|e| GeeZipError::io(e, "reading ISZ header"))?;

    // Validate magic
    if &buf[0..4] != ISZ_MAGIC {
        return Err(GeeZipError::format(
            "not an ISZ file (bad magic)",
            ArchiveFormat::Isz,
        ));
    }

    Ok(IszHeader {
        header_size: buf[4],
        version: buf[5],
        volume_serial_number: u32::from_le_bytes([buf[6], buf[7], buf[8], buf[9]]),
        sector_size: u16::from_le_bytes([buf[10], buf[11]]),
        total_sectors: u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]),
        encryption_type: buf[16],
        segment_size: u64::from_le_bytes([
            buf[17], buf[18], buf[19], buf[20], buf[21], buf[22], buf[23], buf[24],
        ]),
        num_blocks: u32::from_le_bytes([buf[25], buf[26], buf[27], buf[28]]),
        block_size_sectors: u32::from_le_bytes([buf[29], buf[30], buf[31], buf[32]]),
        pointer_length: buf[33],
        chunk_pointers_offset: u32::from_le_bytes([buf[35], buf[36], buf[37], buf[38]]),
        segment_pointers_offset: u32::from_le_bytes([buf[39], buf[40], buf[41], buf[42]]),
        data_offset: u32::from_le_bytes([buf[43], buf[44], buf[45], buf[46]]),
    })
}

/// Decompress ISZ → raw ISO bytes.
pub fn isz_decompress<R: Read + Seek, W: Write>(
    reader: &mut R,
    mut writer: W,
) -> GeeZipResult<u64> {
    let header = parse_header(reader)?;

    if header.version != 1 {
        return Err(GeeZipError::format(
            format!("unsupported ISZ version {}", header.version),
            ArchiveFormat::Isz,
        ));
    }
    if header.encryption_type != 0 {
        return Err(GeeZipError::format(
            "encrypted ISZ files are not supported",
            ArchiveFormat::Isz,
        ));
    }
    if header.pointer_length != 3 {
        return Err(GeeZipError::format(
            format!("unsupported ISZ pointer length {}", header.pointer_length),
            ArchiveFormat::Isz,
        ));
    }

    let block_size_bytes = header.block_size_sectors as u64 * header.sector_size as u64;
    let num_blocks = header.num_blocks as usize;
    let total_uncompressed = (header.total_sectors as u64) * (header.sector_size as u64);

    // Read chunk pointer table
    let cdt_size = num_blocks * 3;
    let mut cdt = vec![0u8; cdt_size];

    let cdt_offset = if header.chunk_pointers_offset != 0 {
        header.chunk_pointers_offset as u64
    } else {
        HEADER_SIZE as u64
    };

    reader
        .seek(std::io::SeekFrom::Start(cdt_offset))
        .map_err(|e| GeeZipError::io(e, "seeking to ISZ chunk table"))?;
    reader
        .read_exact(&mut cdt)
        .map_err(|e| GeeZipError::io(e, "reading ISZ chunk table"))?;

    // Deobfuscate the chunk table
    xor_deobfuscate(&mut cdt);

    // Parse chunk pointers
    let mut chunks: Vec<(u8, u32)> = Vec::with_capacity(num_blocks);
    for i in 0..num_blocks {
        let offset = i * 3;
        let ptr = [cdt[offset], cdt[offset + 1], cdt[offset + 2]];
        chunks.push(decode_chunk_pointer(&ptr));
    }

    // Determine data start offset
    let data_start = if header.data_offset != 0 {
        header.data_offset as u64
    } else {
        cdt_offset + cdt_size as u64
    };

    // Read all compressed data into a buffer
    reader
        .seek(std::io::SeekFrom::Start(data_start))
        .map_err(|e| GeeZipError::io(e, "seeking to ISZ data start"))?;

    let mut compressed_data = Vec::new();
    reader
        .read_to_end(&mut compressed_data)
        .map_err(|e| GeeZipError::io(e, "reading ISZ compressed data"))?;

    // Decompress each chunk
    let mut total_written: u64 = 0;
    let mut data_offset: usize = 0;

    for (chunk_type, chunk_size) in &chunks {
        match chunk_type {
            0 => {
                // Zeros — write block_size_bytes zeros via streaming
                std::io::copy(&mut std::io::repeat(0).take(block_size_bytes), &mut writer)
                    .map_err(|e| GeeZipError::io(e, "writing zero block"))?;
                total_written += block_size_bytes;
            }
            1 => {
                // Raw data
                let size = *chunk_size as usize;
                if data_offset + size > compressed_data.len() {
                    return Err(GeeZipError::format(
                        "truncated ISZ data block",
                        ArchiveFormat::Isz,
                    ));
                }
                writer
                    .write_all(&compressed_data[data_offset..data_offset + size])
                    .map_err(|e| GeeZipError::io(e, "writing raw block"))?;
                total_written += size as u64;
                data_offset += size;
            }
            2 => {
                // Zlib (deflate)
                let size = *chunk_size as usize;
                if data_offset + size > compressed_data.len() {
                    return Err(GeeZipError::format(
                        "truncated ISZ zlib block",
                        ArchiveFormat::Isz,
                    ));
                }
                use flate2::read::ZlibDecoder;
                let compressed = &compressed_data[data_offset..data_offset + size];
                let mut decoder = ZlibDecoder::new(compressed);
                let n = std::io::copy(&mut decoder, &mut writer)
                    .map_err(|e| GeeZipError::io(e, "decompressing ISZ zlib block"))?;
                total_written += n;
                data_offset += size;
            }
            3 => {
                // Bzip2 — prepend "BZh9" magic header.
                // ISZ strips the 4-byte BZip2 magic; "9" requests the max
                // block size (900kB) so the decoder allocates enough workspace.
                let size = *chunk_size as usize;
                if data_offset + size > compressed_data.len() {
                    return Err(GeeZipError::format(
                        "truncated ISZ bzip2 block",
                        ArchiveFormat::Isz,
                    ));
                }
                let mut bz_data = vec![b'B', b'Z', b'h', b'9'];
                bz_data.extend_from_slice(&compressed_data[data_offset..data_offset + size]);
                use bzip2::read::BzDecoder;
                let mut decoder = BzDecoder::new(&bz_data[..]);
                let n = std::io::copy(&mut decoder, &mut writer)
                    .map_err(|e| GeeZipError::io(e, "decompressing ISZ bzip2 block"))?;
                total_written += n;
                data_offset += size;
            }
            _ => {
                return Err(GeeZipError::format(
                    format!("unknown ISZ chunk type {}", chunk_type),
                    ArchiveFormat::Isz,
                ));
            }
        }
    }

    // Verify total size
    if total_written != total_uncompressed {
        return Err(GeeZipError::format(
            format!(
                "ISZ size mismatch: expected {} bytes, got {}",
                total_uncompressed, total_written
            ),
            ArchiveFormat::Isz,
        ));
    }

    Ok(total_written)
}

/// Verify ISZ integrity — decompress to a sink and return total bytes.
pub fn isz_verify<R: Read + Seek>(reader: &mut R) -> GeeZipResult<u64> {
    isz_decompress(reader, std::io::sink())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// Build a minimal valid ISZ file in memory with Zlib-compressed blocks.
    /// Pads input data to full sector boundaries (matching real ISZ behavior).
    fn build_test_isz(data: &[u8]) -> Vec<u8> {
        use flate2::write::ZlibEncoder;
        use flate2::Compression;
        use std::io::Write;

        let sector_size: u16 = 2048;
        let block_size_sectors: u32 = 1; // 1 sector = 2048 bytes per block
        let total_sectors = (data.len() as u64).div_ceil(2048) as u32;
        let num_blocks = total_sectors;

        // Pad data to full sector size
        let padded_len = num_blocks as usize * 2048;
        let mut padded = vec![0u8; padded_len];
        padded[..data.len()].copy_from_slice(data);

        // Compress data blocks
        let mut block_data = Vec::new();
        let mut chunk_table = Vec::new();

        for i in 0..num_blocks {
            let start = (i * 2048) as usize;
            let block = &padded[start..start + 2048];

            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(6));
            encoder.write_all(block).unwrap();
            let compressed = encoder.finish().unwrap();

            // Build 3-byte pointer: type=2 (zlib), size=compressed.len()
            let size = compressed.len() as u32;
            let raw = size | (2u32 << 22); // type 2 in top 2 bits
            chunk_table.push(raw as u8);
            chunk_table.push((raw >> 8) as u8);
            chunk_table.push((raw >> 16) as u8);

            block_data.extend_from_slice(&compressed);
        }

        // XOR obfuscate the chunk table
        xor_deobfuscate(&mut chunk_table);

        let cdt_offset: u32 = 64; // right after header

        // Build header
        let mut header = vec![0u8; 64];
        header[0..4].copy_from_slice(b"IsZ!");
        header[4] = 64; // header_size
        header[5] = 1; // version
                       // volume_serial_number stays 0
        header[10..12].copy_from_slice(&sector_size.to_le_bytes());
        header[12..16].copy_from_slice(&total_sectors.to_le_bytes());
        // encryption_type stays 0
        // segment_size stays 0
        header[25..29].copy_from_slice(&num_blocks.to_le_bytes());
        header[29..33].copy_from_slice(&block_size_sectors.to_le_bytes());
        header[33] = 3; // pointer_length
        header[35..39].copy_from_slice(&cdt_offset.to_le_bytes());
        // data_offset = cdt_offset + chunk_table.len()
        let data_off: u32 = cdt_offset + chunk_table.len() as u32;
        header[43..47].copy_from_slice(&data_off.to_le_bytes());

        // Assemble
        let mut isz = Vec::new();
        isz.extend_from_slice(&header);
        isz.extend_from_slice(&chunk_table);
        isz.extend_from_slice(&block_data);
        isz
    }

    #[test]
    fn isz_decompress_small() {
        let original = b"Hello, ISZ world! This is test data.";
        let isz_data = build_test_isz(original);

        let mut reader = Cursor::new(isz_data);
        let mut output = Vec::new();
        let bytes = isz_decompress(&mut reader, &mut output).unwrap();

        assert!(bytes >= original.len() as u64);
        assert_eq!(&output[..original.len()], original);
    }

    #[test]
    fn isz_bad_magic() {
        let mut data = vec![0u8; 64];
        data[0..4].copy_from_slice(b"BAD!");
        let mut reader = Cursor::new(data);
        let mut output = Vec::new();
        let err = isz_decompress(&mut reader, &mut output).unwrap_err();
        assert!(err.to_string().contains("bad magic"), "got: {err}");
    }

    #[test]
    fn isz_unsupported_version() {
        let original = b"test";
        let mut isz_data = build_test_isz(original);
        isz_data[5] = 99; // corrupt version
        let mut reader = Cursor::new(isz_data);
        let mut output = Vec::new();
        let err = isz_decompress(&mut reader, &mut output).unwrap_err();
        assert!(err.to_string().contains("version"), "got: {err}");
    }

    #[test]
    fn isz_roundtrip_data_block() {
        // Test with Zlib-compressed blocks
        let data = b"ISZ raw block roundtrip test data 1234567890";
        let isz_data = build_test_isz(data);
        let mut reader = Cursor::new(isz_data);
        let mut output = Vec::new();
        let _bytes = isz_decompress(&mut reader, &mut output).unwrap();
        assert_eq!(&output[..data.len()], data);
    }

    #[test]
    fn isz_verify_works() {
        let data = b"ISZ verify test data";
        let isz_data = build_test_isz(data);
        let mut reader = Cursor::new(isz_data);
        let verified = isz_verify(&mut reader).unwrap();
        assert!(verified > 0);
    }
}
