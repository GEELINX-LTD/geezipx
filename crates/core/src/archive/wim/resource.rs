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
    if !desc.is_compressed() || compression == CompressionType::None {
        file.seek(SeekFrom::Start(desc.offset))
            .map_err(|e| GeeZipError::io(e, "seeking to WIM resource"))?;
        let mut buf = vec![0u8; desc.original_size as usize];
        file.read_exact(&mut buf)
            .map_err(|e| GeeZipError::io(e, "reading WIM resource"))?;
        return Ok(buf);
    }

    let mut output = Vec::with_capacity(desc.original_size as usize);
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

/// Extract a byte range from a possibly-compressed resource into `writer`.
///
/// Returns the number of bytes written.
pub(crate) fn extract_resource_range(
    file: &mut std::fs::File,
    desc: &ResourceDescriptor,
    compression: CompressionType,
    chunk_size: u32,
    offset: u64,
    len: u64,
    writer: &mut dyn Write,
) -> GeeZipResult<u64> {
    if !desc.is_compressed() || compression == CompressionType::None {
        file.seek(SeekFrom::Start(desc.offset + offset))
            .map_err(|e| GeeZipError::io(e, "seeking to WIM resource"))?;
        let mut buf = vec![0u8; len as usize];
        file.read_exact(&mut buf)
            .map_err(|e| GeeZipError::io(e, "reading WIM resource"))?;
        writer
            .write_all(&buf)
            .map_err(|e| GeeZipError::io(e, "writing extracted WIM data"))?;
        return Ok(len);
    }

    let chunk_size = if chunk_size == 0 {
        32768u32
    } else {
        chunk_size
    };
    let chunk_size_u64 = chunk_size as u64;
    let num_chunks = desc.original_size.div_ceil(chunk_size_u64) as usize;

    // The chunk table lives at the end of the compressed resource data.
    let table_size = num_chunks * 4;
    let table_start = desc.offset + desc.compressed_size - table_size as u64;

    file.seek(SeekFrom::Start(table_start))
        .map_err(|e| GeeZipError::io(e, "seeking to WIM chunk table"))?;
    let mut table_bytes = vec![0u8; table_size];
    file.read_exact(&mut table_bytes)
        .map_err(|e| GeeZipError::io(e, "reading WIM chunk table"))?;

    let mut chunk_offsets: Vec<u64> = Vec::with_capacity(num_chunks);
    for i in 0..num_chunks {
        let off = u32::from_le_bytes([
            table_bytes[i * 4],
            table_bytes[i * 4 + 1],
            table_bytes[i * 4 + 2],
            table_bytes[i * 4 + 3],
        ]) as u64;
        chunk_offsets.push(off);
    }

    let start_chunk = (offset / chunk_size_u64) as usize;
    let end_chunk = (offset + len)
        .div_ceil(chunk_size_u64)
        .min(num_chunks as u64) as usize;

    let mut total_written: u64 = 0;
    let mut current_uncompressed_offset: u64 = start_chunk as u64 * chunk_size_u64;

    for ci in start_chunk..end_chunk {
        let chunk_decompressed_size = if ci == num_chunks - 1 {
            desc.original_size - ci as u64 * chunk_size_u64
        } else {
            chunk_size_u64
        };

        // Calculate compressed chunk bounds within the resource
        let chunk_data_offset = table_start + chunk_offsets[ci];
        let compressed_len = if ci + 1 < num_chunks {
            (table_start + chunk_offsets[ci + 1]) - chunk_data_offset
        } else {
            (desc.offset + desc.compressed_size) - chunk_data_offset
        };

        // Uncompressed chunk: offset same as previous chunk
        let is_uncompressed_chunk = ci > 0 && chunk_offsets[ci] == chunk_offsets[ci - 1];

        // Read the compressed (or stored) chunk data
        file.seek(SeekFrom::Start(chunk_data_offset))
            .map_err(|e| GeeZipError::io(e, "seeking to WIM chunk"))?;
        let mut compressed = vec![0u8; compressed_len as usize];
        file.read_exact(&mut compressed)
            .map_err(|e| GeeZipError::io(e, "reading WIM chunk"))?;

        let decompressed: Vec<u8> = if is_uncompressed_chunk {
            // Skip the 4-byte size prefix, rest is raw data
            let mut raw = compressed[4..].to_vec();
            raw.truncate(chunk_decompressed_size as usize);
            raw
        } else {
            match compression {
                CompressionType::Xpress => {
                    decompress_xpress(&compressed, chunk_decompressed_size as usize)?
                }
                CompressionType::Lzx => {
                    decompress_lzx(&compressed, chunk_decompressed_size as usize, chunk_size)?
                }
                CompressionType::Lzms => {
                    return Err(GeeZipError::format(
                        "LZMS compression is not yet supported",
                        ArchiveFormat::Wim,
                    ));
                }
                CompressionType::None => compressed,
            }
        };

        // Copy the relevant portion of this chunk to output
        let chunk_end = current_uncompressed_offset + chunk_decompressed_size;
        let copy_start = if chunk_end <= offset || current_uncompressed_offset >= offset + len {
            0 // chunk entirely outside the requested range
        } else {
            (offset.saturating_sub(current_uncompressed_offset)) as usize
        };

        let range_end = (offset + len).min(current_uncompressed_offset + chunk_decompressed_size);
        let copy_end = (range_end - current_uncompressed_offset) as usize;

        if copy_end > copy_start && copy_start < decompressed.len() {
            let actual_end = copy_end.min(decompressed.len());
            writer
                .write_all(&decompressed[copy_start..actual_end])
                .map_err(|e| GeeZipError::io(e, "writing extracted WIM data"))?;
            total_written += (actual_end - copy_start) as u64;
        }

        current_uncompressed_offset += chunk_decompressed_size;
    }

    Ok(total_written)
}

/// Decompress a single XPRESS (LZ77+Huffman) chunk.
fn decompress_xpress(compressed: &[u8], decompressed_size: usize) -> GeeZipResult<Vec<u8>> {
    // Some WIM writers store uncompressed data with a 4-byte size prefix
    if compressed.len() >= 4 {
        let prefix =
            u32::from_le_bytes([compressed[0], compressed[1], compressed[2], compressed[3]])
                as usize;
        if prefix == decompressed_size && compressed.len() == decompressed_size + 4 {
            return Ok(compressed[4..].to_vec());
        }
    }

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
    // Some WIM writers store uncompressed data with a 4-byte size prefix
    if compressed.len() >= 4 {
        let prefix =
            u32::from_le_bytes([compressed[0], compressed[1], compressed[2], compressed[3]])
                as usize;
        if prefix == decompressed_size && compressed.len() == decompressed_size + 4 {
            return Ok(compressed[4..].to_vec());
        }
    }

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
