//! WIM resource reading and chunk decompression.
//!
//! Reads compressed or uncompressed resources from the WIM file.
//! Handles XPRESS (Huffman variant) and LZX decompression.

use std::io::{Read, Seek, SeekFrom, Write};

use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

use super::header::{CompressionType, ResourceDescriptor};

/// Read an entire resource from the WIM file, decompressing if needed.
pub(crate) fn read_resource(
    file: &mut std::fs::File,
    desc: &ResourceDescriptor,
    compression: CompressionType,
    chunk_size: u32,
) -> GeeZipResult<Vec<u8>> {
    usize::try_from(desc.original_size).map_err(|_| {
        GeeZipError::format("WIM resource is too large for memory", ArchiveFormat::Wim)
    })?;
    // Do not allocate the untrusted original_size before validating the
    // descriptor and chunk table. Grow only as validated data is produced.
    let mut output = Vec::new();
    extract_resource_range(
        file,
        desc,
        compression,
        chunk_size,
        0,
        desc.original_size,
        &mut output,
    )?;
    Ok(output)
}

fn invalid_resource(message: &str) -> GeeZipError {
    GeeZipError::format(message, ArchiveFormat::Wim)
}

fn read_chunk_offset(
    file: &mut std::fs::File,
    table_start: u64,
    entry_size: u64,
    index: u64,
) -> GeeZipResult<u64> {
    file.seek(SeekFrom::Start(table_start + index * entry_size))
        .map_err(|e| GeeZipError::io(e, "seeking to WIM chunk offset"))?;
    let mut bytes = [0u8; 8];
    file.read_exact(&mut bytes[..entry_size as usize])
        .map_err(|e| GeeZipError::io(e, "reading WIM chunk offset"))?;
    Ok(u64::from_le_bytes(bytes))
}

/// Extract a byte range from a possibly-compressed resource into `writer`.
///
/// Returns the number of bytes written. All lengths and chunk boundaries are
/// validated against the resource and file before reading or allocating.
pub(crate) fn extract_resource_range(
    file: &mut std::fs::File,
    desc: &ResourceDescriptor,
    compression: CompressionType,
    chunk_size: u32,
    offset: u64,
    len: u64,
    writer: &mut dyn Write,
) -> GeeZipResult<u64> {
    let range_end = offset
        .checked_add(len)
        .filter(|end| *end <= desc.original_size)
        .ok_or_else(|| invalid_resource("WIM extraction range exceeds resource"))?;
    let resource_end = desc
        .offset
        .checked_add(desc.compressed_size)
        .ok_or_else(|| invalid_resource("WIM resource offset overflow"))?;
    let file_len = file
        .metadata()
        .map_err(|e| GeeZipError::io(e, "reading WIM file length"))?
        .len();
    if resource_end > file_len {
        return Err(invalid_resource("truncated WIM resource"));
    }

    if !desc.is_compressed() {
        if desc.original_size != desc.compressed_size {
            return Err(invalid_resource("WIM stored resource size mismatch"));
        }
        file.seek(SeekFrom::Start(desc.offset + offset))
            .map_err(|e| GeeZipError::io(e, "seeking to WIM resource"))?;
        let written = std::io::copy(&mut file.take(len), writer)
            .map_err(|e| GeeZipError::io(e, "extracting WIM resource"))?;
        if written != len {
            return Err(invalid_resource("truncated WIM stored resource"));
        }
        return Ok(written);
    }
    if compression == CompressionType::None {
        return Err(invalid_resource("compressed WIM resource has no codec"));
    }
    let chunk_size = if chunk_size == 0 { 32768 } else { chunk_size };
    // Bound workspace to the window sizes supported by our decoders.
    if !chunk_size.is_power_of_two() || chunk_size > 524288 {
        return Err(invalid_resource("unsupported WIM chunk size"));
    }
    let chunk_size_u64 = u64::from(chunk_size);
    let num_chunks = desc.original_size.div_ceil(chunk_size_u64);
    // WIM stores n-1 cumulative offsets at the START of the resource, relative
    // to the payload after the table. The first and last bounds are implicit.
    let entry_size = if desc.original_size > u64::from(u32::MAX) {
        8
    } else {
        4
    };
    let table_size = num_chunks
        .saturating_sub(1)
        .checked_mul(entry_size)
        .filter(|size| *size <= desc.compressed_size)
        .ok_or_else(|| invalid_resource("WIM chunk table exceeds resource"))?;
    let data_start = desc.offset + table_size;
    let data_size = desc.compressed_size - table_size;
    if len == 0 {
        return Ok(0);
    }

    let start_chunk = offset / chunk_size_u64;
    let end_chunk = range_end.div_ceil(chunk_size_u64);
    let mut total_written = 0;
    for ci in start_chunk..end_chunk {
        let chunk_start = ci * chunk_size_u64;
        let expected = (desc.original_size - chunk_start).min(chunk_size_u64);
        let encoded_start = if ci == 0 {
            0
        } else {
            read_chunk_offset(file, desc.offset, entry_size, ci - 1)?
        };
        let encoded_end = if ci + 1 == num_chunks {
            data_size
        } else {
            read_chunk_offset(file, desc.offset, entry_size, ci)?
        };
        let encoded_len = encoded_end
            .checked_sub(encoded_start)
            .filter(|size| *size > 0 && *size <= expected && encoded_end <= data_size)
            .ok_or_else(|| invalid_resource("invalid WIM chunk bounds"))?;

        file.seek(SeekFrom::Start(data_start + encoded_start))
            .map_err(|e| GeeZipError::io(e, "seeking to WIM chunk"))?;
        let mut encoded = vec![0u8; encoded_len as usize];
        file.read_exact(&mut encoded)
            .map_err(|e| GeeZipError::io(e, "reading WIM chunk"))?;
        // WIM stores incompressible chunks directly, without a size prefix.
        let decoded = if encoded_len == expected {
            encoded
        } else {
            match compression {
                CompressionType::Xpress => decompress_xpress(&encoded, expected as usize)?,
                CompressionType::Lzx => decompress_lzx(&encoded, expected as usize, chunk_size)?,
                CompressionType::Lzms => {
                    return Err(invalid_resource("LZMS compression is not yet supported"));
                }
                CompressionType::None => unreachable!("codec was checked above"),
            }
        };
        if decoded.len() as u64 != expected {
            return Err(invalid_resource("WIM chunk decompressed size mismatch"));
        }
        let copy_start = offset.saturating_sub(chunk_start) as usize;
        let copy_end = (range_end - chunk_start).min(expected) as usize;
        writer
            .write_all(&decoded[copy_start..copy_end])
            .map_err(|e| GeeZipError::io(e, "writing extracted WIM data"))?;
        total_written += (copy_end - copy_start) as u64;
    }
    Ok(total_written)
}

/// Decompress a single XPRESS (LZ77+Huffman) chunk.
fn decompress_xpress(compressed: &[u8], decompressed_size: usize) -> GeeZipResult<Vec<u8>> {
    xpress_huffman::decompress(compressed, decompressed_size).map_err(|e| {
        GeeZipError::format(
            format!("XPRESS decompression error: {e}"),
            ArchiveFormat::Wim,
        )
    })
}

/// Decompress a single LZX chunk.
fn decompress_lzx(
    compressed: &[u8],
    decompressed_size: usize,
    window_size: u32,
) -> GeeZipResult<Vec<u8>> {
    let ws = match window_size {
        0 | 32768 => lzxd::WindowSize::KB32,
        65536 => lzxd::WindowSize::KB64,
        131072 => lzxd::WindowSize::KB128,
        262144 => lzxd::WindowSize::KB256,
        524288 => lzxd::WindowSize::KB512,
        w if w <= 32768 => lzxd::WindowSize::KB32,
        w if w <= 65536 => lzxd::WindowSize::KB64,
        w if w <= 131072 => lzxd::WindowSize::KB128,
        w if w <= 262144 => lzxd::WindowSize::KB256,
        _ => lzxd::WindowSize::KB512,
    };

    let mut decomp = lzxd::Lzxd::new(ws);
    decomp
        .decompress_next(compressed, decompressed_size)
        .map(|data| data.to_vec())
        .map_err(|e| {
            GeeZipError::format(format!("LZX decompression error: {e}"), ArchiveFormat::Wim)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn resource_file(
        data: &[u8],
        original_size: u64,
        compressed: bool,
    ) -> (std::fs::File, ResourceDescriptor) {
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(data).unwrap();
        let desc = ResourceDescriptor {
            flags: if compressed { 4 } else { 0 },
            compressed_size: data.len() as u64,
            offset: 0,
            original_size,
        };
        (file, desc)
    }

    #[test]
    fn stored_resource_ranges_stay_within_descriptor() {
        let (mut file, mut desc) = resource_file(b"abcdefSECRET", 6, false);
        desc.compressed_size = 6;
        let mut output = Vec::new();
        assert_eq!(
            extract_resource_range(
                &mut file,
                &desc,
                CompressionType::None,
                0,
                2,
                3,
                &mut output
            )
            .unwrap(),
            3
        );
        assert_eq!(output, b"cde");
        output.clear();
        assert!(extract_resource_range(
            &mut file,
            &desc,
            CompressionType::None,
            0,
            4,
            4,
            &mut output
        )
        .is_err());
        assert!(output.is_empty());
    }

    #[test]
    fn resource_lengths_reject_overflow_and_truncation_before_allocation() {
        let (mut file, desc) = resource_file(b"", u64::MAX, false);
        assert!(read_resource(&mut file, &desc, CompressionType::None, 0).is_err());
        let mut overflowing = desc;
        overflowing.offset = u64::MAX;
        overflowing.compressed_size = 2;
        assert!(extract_resource_range(
            &mut file,
            &overflowing,
            CompressionType::None,
            0,
            0,
            1,
            &mut Vec::new()
        )
        .is_err());
        assert!(extract_resource_range(
            &mut file,
            &desc,
            CompressionType::None,
            0,
            u64::MAX,
            1,
            &mut Vec::new()
        )
        .is_err());
        let mut truncated = desc;
        truncated.compressed_size = 10;
        truncated.original_size = 10;
        assert!(read_resource(&mut file, &truncated, CompressionType::None, 0).is_err());
    }

    #[test]
    fn single_stored_chunk_in_compressed_resource_has_no_table() {
        let (mut file, desc) = resource_file(b"single chunk", 12, true);
        assert_eq!(
            read_resource(&mut file, &desc, CompressionType::Xpress, 32768).unwrap(),
            b"single chunk"
        );
    }

    #[test]
    fn xpress_and_stored_chunks_use_leading_cumulative_offsets() {
        // A valid two-symbol, one-bit XPRESS Huffman tree. All-zero bit words
        // encode literal 'A'; reserve four bytes for the decoder's lookahead.
        let mut encoded = vec![0u8; 256 + 32768 / 8 + 4];
        encoded[32] = 0x10; // symbol 65 ('A') has code 0
        encoded[33] = 0x01; // symbol 66 ('B') has code 1
        let mut data = (encoded.len() as u32).to_le_bytes().to_vec();
        data.extend_from_slice(&encoded);
        data.extend_from_slice(b"bbbbb");
        let (mut file, desc) = resource_file(&data, 32768 + 5, true);
        let mut expected = vec![b'A'; 32768];
        expected.extend_from_slice(b"bbbbb");
        assert_eq!(
            read_resource(&mut file, &desc, CompressionType::Xpress, 32768).unwrap(),
            expected
        );
        let mut output = Vec::new();
        assert_eq!(
            extract_resource_range(
                &mut file,
                &desc,
                CompressionType::Xpress,
                32768,
                32766,
                7,
                &mut output
            )
            .unwrap(),
            7
        );
        assert_eq!(output, b"AAbbbbb");
    }

    #[test]
    fn invalid_chunk_tables_and_short_decodes_are_errors() {
        // An advertised multi-chunk resource cannot fit its table.
        let (mut file, desc) = resource_file(b"", 65536, true);
        assert!(read_resource(&mut file, &desc, CompressionType::Xpress, 32768).is_err());
        // Out-of-range and decreasing cumulative offsets must not underflow.
        for offsets in [[u32::MAX, 1], [32768, 1]] {
            let mut data = offsets
                .iter()
                .flat_map(|offset| offset.to_le_bytes())
                .collect::<Vec<_>>();
            data.resize(data.len() + 65536, b'A');
            let (mut file, desc) = resource_file(&data, 98304, true);
            assert!(read_resource(&mut file, &desc, CompressionType::Xpress, 32768).is_err());
        }
        // A code-length table with no encoded symbols decodes to zero bytes,
        // which must not be accepted as a successful 32768-byte extraction.
        let (mut file, desc) = resource_file(&[0u8; 256], 32768, true);
        assert!(read_resource(&mut file, &desc, CompressionType::Xpress, 32768).is_err());
    }
}
