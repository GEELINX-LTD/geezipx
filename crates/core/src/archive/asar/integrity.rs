//! Verify optional Electron integrity metadata with constant working memory.
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};
use asar::header::FileIntegrity;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};

pub(super) fn copy_verified(
    reader: &mut dyn Read,
    writer: &mut dyn Write,
    integrity: Option<&FileIntegrity>,
) -> GeeZipResult<u64> {
    let Some(meta) = integrity else {
        return Ok(std::io::copy(reader, writer)?);
    };
    let expected = meta.blocks();
    let check_blocks = !expected.is_empty();
    if check_blocks && meta.block_size() == 0 {
        return Err(mismatch());
    }
    let mut full = Sha256::new();
    let mut block = Sha256::new();
    let mut in_block = 0usize;
    let mut index = 0usize;
    let mut count = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = match reader.read(&mut buffer) {
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if n == 0 {
            break;
        }
        full.update(&buffer[..n]);
        if check_blocks {
            let mut remaining = &buffer[..n];
            while !remaining.is_empty() {
                let take = remaining.len().min(meta.block_size() - in_block);
                block.update(&remaining[..take]);
                remaining = &remaining[take..];
                in_block += take;
                if in_block == meta.block_size() {
                    if expected.get(index).map(|d| d.as_slice())
                        != Some(&block.finalize_reset()[..])
                    {
                        return Err(mismatch());
                    }
                    index += 1;
                    in_block = 0;
                }
            }
        }
        writer.write_all(&buffer[..n])?;
        count += n as u64;
    }
    if meta.hash() != &full.finalize()[..] {
        return Err(mismatch());
    }
    if check_blocks {
        // Rust ASAR omits a final empty block; Electron includes it. Verify
        // either representation, but reject missing nonempty or extra blocks.
        if in_block != 0 || index < expected.len() {
            if expected.get(index).map(|d| d.as_slice()) != Some(&block.finalize()[..]) {
                return Err(mismatch());
            }
            index += 1;
        }
        if index != expected.len() {
            return Err(mismatch());
        }
    }
    Ok(count)
}
fn mismatch() -> GeeZipError {
    GeeZipError::format(
        "ASAR SHA-256 integrity mismatch or invalid block table",
        ArchiveFormat::Asar,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn block_table_and_whole_hash_are_both_checked() {
        let data = b"block table payload";
        let hashes: Vec<_> = data
            .chunks(7)
            .map(|b| crate::archive::asar::bytes_to_hex(&Sha256::digest(b)))
            .collect();
        let value = serde_json::json!({"algorithm":"SHA256", "hash":crate::archive::asar::bytes_to_hex(&Sha256::digest(data)), "blockSize":7, "blocks":hashes});
        let valid: FileIntegrity = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(
            copy_verified(&mut &data[..], &mut std::io::sink(), Some(&valid)).unwrap(),
            data.len() as u64
        );
        for mutation in 0..4 {
            let mut bad = value.clone();
            match mutation {
                0 => bad["hash"] = "00".repeat(32).into(),
                1 => bad["blocks"][0] = "00".repeat(32).into(),
                2 => bad["blockSize"] = 0.into(),
                _ => {
                    bad["blocks"].as_array_mut().unwrap().pop();
                }
            }
            let meta = serde_json::from_value(bad).unwrap();
            assert!(copy_verified(&mut &data[..], &mut std::io::sink(), Some(&meta)).is_err());
        }
    }
}
