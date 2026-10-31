use super::{
    checksum::{bad, Checksum},
    input::Input,
    Settings,
};
use crate::error::GeeZipResult as Result;
use flate2::{Decompress, FlushDecompress, Status};
use sha2::{Digest, Sha256};
use std::io::Write;

pub(super) struct Record {
    pub(super) name: String,
    pub(super) size: u64,
    pub(super) is_dir: bool,
    pub(super) fingerprint: [u8; 32],
    pub(super) encoded_size: u64,
}

pub(super) fn read(
    input: &mut Input<'_>,
    settings: &Settings,
    writer: &mut dyn Write,
    expected: Option<&[u8; 32]>,
    metadata_remaining: usize,
) -> Result<Option<Record>> {
    let &Settings {
        compression,
        block_size,
        object_hash,
    } = settings;
    input.poll()?;
    let name_size = input.array::<2>()?;
    let len = u16::from_le_bytes(name_size) as usize;
    if len == 0 {
        if input.array::<4>()? != *b"EOA\0" {
            return Err(bad("unsupported stream/volume trigger"));
        }
        return Ok(None);
    }
    if len * 3 + 256 > metadata_remaining {
        return Err(bad("metadata exceeds 64 MiB budget"));
    }
    let mut prefix = name_size.to_vec();
    let name = input.bytes(len)?;
    if name.contains(&0) {
        return Err(bad("NUL in entry name"));
    }
    let name = String::from_utf8(name).map_err(|_| bad("unsupported non-UTF-8 ANSI filename"))?;
    prefix.extend_from_slice(name.as_bytes());
    prefix.extend_from_slice(&input.array::<8>()?); // time and attributes
    let attributes = u32::from_le_bytes(prefix[prefix.len() - 4..].try_into().unwrap());
    if attributes & (8 | 64) != 0 {
        return Err(bad("unsupported volume-label or symlink entry"));
    }
    let is_dir = attributes & 16 != 0;
    let size = if is_dir {
        0
    } else {
        let bytes = input.array::<8>()?;
        prefix.extend_from_slice(&bytes);
        u64::from_le_bytes(bytes)
    };
    let fingerprint: [u8; 32] = Sha256::digest(&prefix).into();
    if expected.is_some_and(|expected| *expected != fingerprint) {
        return Err(bad("entry metadata changed after indexing"));
    }
    let mut checksum = Checksum::new(object_hash)?;
    checksum.update(&prefix);
    let data_start = input.position;
    if !is_dir && size != 0 {
        if compression == 0 {
            stored(input, writer, &mut checksum, size)?;
        } else {
            let mut remaining = size;
            let mut last = 0;
            while remaining != 0 {
                input.poll()?;
                let expected = remaining.min(block_size as u64);
                let field = input.array::<4>()?;
                checksum.update(&field);
                let encoded = u32::from_le_bytes(field) as u64;
                if encoded == 0 {
                    return Err(bad("zero compressed block size"));
                }
                if encoded == expected {
                    stored(input, writer, &mut checksum, expected)?;
                } else {
                    inflate(input, writer, &mut checksum, encoded, expected)?;
                }
                remaining -= expected;
                last = expected as u32;
            }
            let field = input.array::<4>()?;
            checksum.update(&field);
            if u32::from_le_bytes(field) != last {
                return Err(bad("incorrect final block length"));
            }
        }
    }
    let encoded_size = input.position - data_start;
    let tag = checksum.value();
    if input.bytes(tag.len())? != tag {
        return Err(bad("object checksum mismatch"));
    }
    Ok(Some(Record {
        name,
        size,
        is_dir,
        fingerprint,
        encoded_size,
    }))
}

fn stored(
    input: &mut Input<'_>,
    writer: &mut dyn Write,
    checksum: &mut Checksum,
    mut remaining: u64,
) -> Result<()> {
    let mut buffer = [0u8; 64 * 1024];
    while remaining != 0 {
        input.poll()?;
        let len = remaining.min(buffer.len() as u64) as usize;
        input.read_exact(&mut buffer[..len])?;
        checksum.update(&buffer[..len]);
        writer.write_all(&buffer[..len])?;
        remaining -= len as u64;
    }
    Ok(())
}

fn inflate(
    input: &mut Input<'_>,
    writer: &mut dyn Write,
    checksum: &mut Checksum,
    mut remaining: u64,
    expected: u64,
) -> Result<()> {
    let mut decoder = Decompress::new(true);
    let mut compressed = [0; 64 * 1024];
    let mut output = [0; 64 * 1024];
    let (mut offset, mut available) = (0, 0);
    loop {
        input.poll()?;
        if offset == available && remaining != 0 {
            available = remaining.min(compressed.len() as u64) as usize;
            offset = 0;
            input.read_exact(&mut compressed[..available])?;
            remaining -= available as u64;
        }
        let before_in = decoder.total_in();
        let before_out = decoder.total_out();
        let flush = if remaining == 0 && offset == available {
            FlushDecompress::Finish
        } else {
            FlushDecompress::None
        };
        let status = decoder
            .decompress(&compressed[offset..available], &mut output, flush)
            .map_err(|error| bad(format!("invalid zlib block: {error}")))?;
        let consumed = (decoder.total_in() - before_in) as usize;
        let produced = (decoder.total_out() - before_out) as usize;
        offset += consumed;
        if decoder.total_out() > expected {
            return Err(bad("zlib output exceeds declared block length"));
        }
        checksum.update(&output[..produced]);
        writer.write_all(&output[..produced])?;
        if status == Status::StreamEnd {
            if remaining != 0 || offset != available || decoder.total_out() != expected {
                return Err(bad("zlib block length mismatch or trailing data"));
            }
            return Ok(());
        }
        if consumed == 0 && produced == 0 && (offset < available || remaining == 0) {
            return Err(bad("unfinished zlib block"));
        }
    }
}
