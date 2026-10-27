//! ISZ (ISO Zipped) compressed disc image format support.
//!
//! ISZ is a block-compressed ISO wrapper. It does NOT implement
//! `ArchiveReader`/`ArchiveWriter` — it operates on raw byte streams
//! in the same style as the AES, IMG, and BIN single-stream engines.

use std::io::{Read, Seek, Write};

use crate::config::CompressOptions;
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

use flate2::write::ZlibEncoder;
use flate2::Compression;

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

/// Encode a (type, size) pair into a 3-byte chunk pointer (little-endian).
fn encode_chunk_pointer(chunk_type: u8, size: u32) -> [u8; 3] {
    let val = ((chunk_type as u32) << 22) | (size & 0x3F_FFFF);
    [val as u8, (val >> 8) as u8, (val >> 16) as u8]
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

/// Copy exactly one block without allowing a compressed block to exceed its
/// declared output size. Reading once more also validates the codec trailer.
fn copy_block<R: Read, W: Write>(mut reader: R, writer: &mut W, expected: u64) -> GeeZipResult<()> {
    let written = std::io::copy(&mut reader.by_ref().take(expected), writer)
        .map_err(|e| GeeZipError::io(e, "decompressing ISZ block"))?;
    let mut extra = [0u8; 1];
    let extra_len = reader
        .read(&mut extra)
        .map_err(|e| GeeZipError::io(e, "validating ISZ block"))?;
    if written != expected || extra_len != 0 {
        return Err(GeeZipError::format(
            "ISZ block decompressed size mismatch",
            ArchiveFormat::Isz,
        ));
    }
    Ok(())
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
    let total_uncompressed = header.total_sectors as u64 * header.sector_size as u64;
    if block_size_bytes == 0
        || total_uncompressed.div_ceil(block_size_bytes) != u64::from(header.num_blocks)
    {
        return Err(GeeZipError::format(
            "invalid ISZ block geometry",
            ArchiveFormat::Isz,
        ));
    }

    let file_len = reader
        .seek(std::io::SeekFrom::End(0))
        .map_err(|e| GeeZipError::io(e, "reading ISZ file length"))?;
    let cdt_offset = if header.chunk_pointers_offset != 0 {
        u64::from(header.chunk_pointers_offset)
    } else {
        HEADER_SIZE as u64
    };
    let table_end = cdt_offset + u64::from(header.num_blocks) * 3;
    let mut data_offset = if header.data_offset != 0 {
        u64::from(header.data_offset)
    } else {
        table_end
    };
    if cdt_offset < HEADER_SIZE as u64
        || table_end > file_len
        || data_offset < table_end
        || data_offset > file_len
    {
        return Err(GeeZipError::format(
            "invalid or truncated ISZ chunk table/data bounds",
            ArchiveFormat::Isz,
        ));
    }

    // Read one table entry and one encoded block at a time. A three-byte
    // pointer can describe at most 4 MiB of encoded data, independent of the
    // claimed image size. Do not allocate based on num_blocks or read to EOF.
    let mut total_written = 0;
    for index in 0..u64::from(header.num_blocks) {
        reader
            .seek(std::io::SeekFrom::Start(cdt_offset + index * 3))
            .map_err(|e| GeeZipError::io(e, "seeking to ISZ chunk pointer"))?;
        let mut pointer = [0u8; 3];
        reader
            .read_exact(&mut pointer)
            .map_err(|e| GeeZipError::io(e, "reading ISZ chunk pointer"))?;
        for (byte_index, byte) in pointer.iter_mut().enumerate() {
            *byte ^= XOR_KEY[((index * 3 + byte_index as u64) % 4) as usize];
        }
        let (chunk_type, chunk_size) = decode_chunk_pointer(&pointer);
        let expected = block_size_bytes.min(total_uncompressed - total_written);
        let data_end = data_offset + u64::from(chunk_size);
        if data_end > file_len {
            return Err(GeeZipError::format(
                "truncated ISZ data block",
                ArchiveFormat::Isz,
            ));
        }
        if (chunk_type == 0 && chunk_size != 0)
            || (chunk_type == 1 && u64::from(chunk_size) != expected)
        {
            return Err(GeeZipError::format(
                "ISZ block stored size mismatch",
                ArchiveFormat::Isz,
            ));
        }
        if chunk_type == 0 {
            // The final zero block can be shorter than a full block.
            copy_block(std::io::repeat(0).take(expected), &mut writer, expected)?;
        } else {
            reader
                .seek(std::io::SeekFrom::Start(data_offset))
                .map_err(|e| GeeZipError::io(e, "seeking to ISZ block"))?;
            let mut encoded = vec![0u8; chunk_size as usize];
            reader
                .read_exact(&mut encoded)
                .map_err(|e| GeeZipError::io(e, "reading ISZ block"))?;
            match chunk_type {
                1 => copy_block(encoded.as_slice(), &mut writer, expected)?,
                2 => {
                    let mut decoder = flate2::read::ZlibDecoder::new(encoded.as_slice());
                    copy_block(&mut decoder, &mut writer, expected)?;
                    if decoder.total_in() != u64::from(chunk_size) {
                        return Err(GeeZipError::format(
                            "trailing data in ISZ zlib block",
                            ArchiveFormat::Isz,
                        ));
                    }
                }
                3 => {
                    // ISZ strips the four-byte BZip2 magic. Chain it back
                    // without copying the entire encoded block again.
                    let input = (&b"BZh9"[..]).chain(encoded.as_slice());
                    let mut decoder = bzip2::read::BzDecoder::new(input);
                    copy_block(&mut decoder, &mut writer, expected)?;
                    if decoder.total_in() != u64::from(chunk_size) + 4 {
                        return Err(GeeZipError::format(
                            "trailing data in ISZ bzip2 block",
                            ArchiveFormat::Isz,
                        ));
                    }
                }
                _ => unreachable!("chunk type is encoded in two bits"),
            }
        }
        total_written += expected;
        data_offset = data_end;
    }
    Ok(total_written)
}

/// Verify ISZ integrity — decompress to a sink and return total bytes.
pub fn isz_verify<R: Read + Seek>(reader: &mut R) -> GeeZipResult<u64> {
    isz_decompress(reader, std::io::sink())
}

/// Compress raw data into ISZ format.
///
/// Reads all input, pads to 2048-byte sector boundaries, compresses
/// each 2 MiB block with Zlib (Deflate, level from options), and writes
/// a complete ISZ file with header, XOR-obfuscated chunk table, and blocks.
pub fn isz_compress<R: Read, W: Write>(
    reader: &mut R,
    mut writer: W,
    options: &CompressOptions,
) -> GeeZipResult<u64> {
    use std::io::Write as _;

    let mut input = Vec::new();
    reader
        .read_to_end(&mut input)
        .map_err(|e| GeeZipError::io(e, "reading ISZ input"))?;

    const SECTOR_SIZE: u16 = 2048;
    // The three-byte pointer has only 22 size bits. A stored/incompressible
    // 4 MiB block (plus codec overhead) cannot be represented without truncation.
    const BLOCK_SIZE_SECTORS: u32 = 1024;
    let block_size_bytes = BLOCK_SIZE_SECTORS as u64 * SECTOR_SIZE as u64;
    let level = options.level.unwrap_or(6).clamp(0, 9);

    let padded_len = input.len().div_ceil(SECTOR_SIZE as usize) * SECTOR_SIZE as usize;
    if padded_len > u32::MAX as usize {
        return Err(GeeZipError::format(
            "ISZ input too large",
            ArchiveFormat::Isz,
        ));
    }

    let total_sectors = (padded_len / SECTOR_SIZE as usize) as u32;
    let num_blocks = if total_sectors == 0 {
        0
    } else {
        total_sectors.div_ceil(BLOCK_SIZE_SECTORS)
    };

    let padded = if padded_len > input.len() {
        let mut p = input;
        p.resize(padded_len, 0u8);
        p
    } else {
        input
    };

    let mut block_data: Vec<u8> = Vec::new();
    let mut chunk_table: Vec<u8> = Vec::with_capacity(num_blocks as usize * 3);

    for i in 0..num_blocks {
        let start = (i as u64 * block_size_bytes) as usize;
        let end = std::cmp::min(start + block_size_bytes as usize, padded_len);
        let block = &padded[start..end];

        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(level));
        encoder
            .write_all(block)
            .map_err(|e| GeeZipError::io(e, "compressing ISZ block"))?;
        let compressed = encoder
            .finish()
            .map_err(|e| GeeZipError::io(e, "finishing ISZ block"))?;

        let ptr = encode_chunk_pointer(2, compressed.len() as u32);
        chunk_table.extend_from_slice(&ptr);
        block_data.extend_from_slice(&compressed);
    }

    xor_deobfuscate(&mut chunk_table);

    let checksum1 = crc32fast::hash(&padded);
    let checksum2 = crc32fast::hash(&block_data);
    let size1 = padded_len as u32;

    let cdt_offset: u32 = 64;
    let data_offset: u32 = cdt_offset + chunk_table.len() as u32;

    let mut header = [0u8; 64];
    header[0..4].copy_from_slice(b"IsZ!");
    header[4] = 64;
    header[5] = 1;
    header[10..12].copy_from_slice(&SECTOR_SIZE.to_le_bytes());
    header[12..16].copy_from_slice(&total_sectors.to_le_bytes());
    header[25..29].copy_from_slice(&num_blocks.to_le_bytes());
    header[29..33].copy_from_slice(&BLOCK_SIZE_SECTORS.to_le_bytes());
    header[33] = 3;
    header[35..39].copy_from_slice(&cdt_offset.to_le_bytes());
    header[43..47].copy_from_slice(&data_offset.to_le_bytes());
    header[48..52].copy_from_slice(&checksum1.to_le_bytes());
    header[52..56].copy_from_slice(&size1.to_le_bytes());
    header[60..64].copy_from_slice(&checksum2.to_le_bytes());

    let mut total: u64 = 0;
    writer
        .write_all(&header)
        .map_err(|e| GeeZipError::io(e, "writing ISZ header"))?;
    total += 64;
    writer
        .write_all(&chunk_table)
        .map_err(|e| GeeZipError::io(e, "writing ISZ chunk table"))?;
    total += chunk_table.len() as u64;
    writer
        .write_all(&block_data)
        .map_err(|e| GeeZipError::io(e, "writing ISZ block data"))?;
    total += block_data.len() as u64;
    Ok(total)
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

            let ptr = encode_chunk_pointer(2, compressed.len() as u32);
            chunk_table.extend_from_slice(&ptr);

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

    fn single_chunk_isz(kind: u8, encoded: &[u8], block_sectors: u32) -> Vec<u8> {
        let mut data = build_test_isz(b"fixture");
        data.truncate(64);
        data[29..33].copy_from_slice(&block_sectors.to_le_bytes());
        let mut table = encode_chunk_pointer(kind, encoded.len() as u32);
        xor_deobfuscate(&mut table);
        data.extend_from_slice(&table);
        data.extend_from_slice(encoded);
        data
    }

    #[test]
    fn isz_final_zero_block_uses_remaining_sectors() {
        let data = single_chunk_isz(0, b"", 2);
        let mut output = Vec::new();
        assert_eq!(
            isz_decompress(&mut Cursor::new(data), &mut output).unwrap(),
            2048
        );
        assert_eq!(output, vec![0u8; 2048]);
    }

    #[test]
    fn isz_limits_each_decoded_block_and_validates_trailer() {
        for size in [1024, 4096] {
            let mut encoded = Vec::new();
            let mut encoder = ZlibEncoder::new(&mut encoded, Compression::default());
            encoder.write_all(&vec![b'A'; size]).unwrap();
            encoder.finish().unwrap();
            let data = single_chunk_isz(2, &encoded, 1);
            let mut output = Vec::new();
            assert!(isz_decompress(&mut Cursor::new(data), &mut output).is_err());
            assert!(output.len() <= 2048);
        }
        let mut data = build_test_isz(b"CRC test");
        *data.last_mut().unwrap() ^= 1;
        assert!(isz_verify(&mut Cursor::new(data)).is_err());
        let data = single_chunk_isz(1, &[0u8; 2047], 1);
        let mut output = Vec::new();
        assert!(isz_decompress(&mut Cursor::new(data), &mut output).is_err());
        assert!(output.is_empty());
    }

    #[test]
    fn isz_rejects_huge_or_truncated_tables_without_allocating_them() {
        let mut data = build_test_isz(b"small");
        data[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
        data[25..29].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(isz_verify(&mut Cursor::new(data)).is_err());
        let mut data = build_test_isz(b"small");
        data[29..33].copy_from_slice(&0u32.to_le_bytes());
        assert!(isz_verify(&mut Cursor::new(data)).is_err());
    }

    #[test]
    fn isz_does_not_read_unreferenced_trailing_data() {
        struct CountedReader {
            inner: Cursor<Vec<u8>>,
            bytes_read: usize,
        }
        impl Read for CountedReader {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                let read = self.inner.read(buf)?;
                self.bytes_read += read;
                Ok(read)
            }
        }
        impl Seek for CountedReader {
            fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
                self.inner.seek(pos)
            }
        }
        let mut data = build_test_isz(b"small");
        let referenced_size = data.len();
        data.resize(referenced_size + 1024 * 1024, 0);
        let mut reader = CountedReader {
            inner: Cursor::new(data),
            bytes_read: 0,
        };
        assert_eq!(isz_verify(&mut reader).unwrap(), 2048);
        assert!(reader.bytes_read <= referenced_size);
    }

    #[test]
    fn isz_level_zero_full_blocks_fit_chunk_pointer() {
        let original = vec![b'A'; 4 * 1024 * 1024];
        let options = CompressOptions {
            level: Some(0),
            ..Default::default()
        };
        let mut encoded = Vec::new();
        isz_compress(&mut Cursor::new(&original), &mut encoded, &options).unwrap();
        let mut output = Vec::new();
        assert_eq!(
            isz_decompress(&mut Cursor::new(encoded), &mut output).unwrap(),
            original.len() as u64
        );
        assert_eq!(output, original);
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

    #[test]
    fn isz_compress_empty() {
        let input = b"";
        let mut reader = Cursor::new(input);
        let mut output = Vec::new();
        let written = isz_compress(&mut reader, &mut output, &CompressOptions::default()).unwrap();
        assert!(written >= 64);
        assert_eq!(&output[0..4], b"IsZ!");
    }

    #[test]
    fn isz_compress_roundtrip_small() {
        let original = b"Hello, ISZ compression! Roundtrip test.";
        let mut reader = Cursor::new(original);
        let mut compressed = Vec::new();
        isz_compress(&mut reader, &mut compressed, &CompressOptions::default()).unwrap();

        let mut reader = Cursor::new(&compressed);
        let mut decompressed = Vec::new();
        isz_decompress(&mut reader, &mut decompressed).unwrap();

        assert_eq!(&decompressed[..original.len()], original);
    }

    #[test]
    fn isz_compress_roundtrip_1mb() {
        // 1 MB of deterministic data
        let original: Vec<u8> = (0..(1024 * 1024)).map(|i| (i % 251) as u8).collect();
        let mut reader = Cursor::new(&original);
        let mut compressed = Vec::new();
        isz_compress(&mut reader, &mut compressed, &CompressOptions::default()).unwrap();

        let mut reader = Cursor::new(&compressed);
        let mut decompressed = Vec::new();
        let bytes = isz_decompress(&mut reader, &mut decompressed).unwrap();
        assert_eq!(&decompressed[..original.len()], &original[..]);
        assert!(bytes >= original.len() as u64);
    }

    #[test]
    fn isz_compress_verify() {
        let original = b"ISZ verify test for compression path";
        let mut reader = Cursor::new(original);
        let mut compressed = Vec::new();
        isz_compress(&mut reader, &mut compressed, &CompressOptions::default()).unwrap();

        let mut reader = Cursor::new(&compressed);
        let verified = isz_verify(&mut reader).unwrap();
        assert!(verified > 0);
    }
}
