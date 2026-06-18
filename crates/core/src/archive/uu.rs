//! UU/UUE decode helpers (single-stream).

use std::fs;
use std::io::Write;
use std::path::Path;

use crate::error::{GeeZipError, GeeZipResult};

pub fn uu_decode_file(path: &Path) -> GeeZipResult<(String, Vec<u8>)> {
    let content = fs::read_to_string(path)
        .map_err(|e| GeeZipError::io(e, format!("reading '{}'", path.display())))?;
    uu_decode(&content).ok_or_else(|| {
        GeeZipError::format(
            format!("uuencode file '{}' is empty or malformed", path.display()),
            crate::detect::ArchiveFormat::Uu,
        )
    })
}

pub fn uu_decode(input: &str) -> Option<(String, Vec<u8>)> {
    let mut lines = input.lines();
    let header = lines.next()?;
    let header = header.strip_prefix("begin ")?;
    let filename = header
        .split_whitespace()
        .nth(1)
        .unwrap_or("unknown")
        .to_string();

    let mut output = Vec::new();
    for line in lines {
        let line = line.trim_end_matches('\r');
        if line == "end" || line.is_empty() {
            break;
        }
        let bytes = line.as_bytes();
        if bytes.is_empty() {
            continue;
        }
        let encoded_len = bytes[0];
        if encoded_len < 33 {
            break; // ` = 96 marks an empty line end
        }
        let mut remaining = (encoded_len - 32) as usize;
        if remaining == 0 {
            continue;
        }
        let encoded = &bytes[1..];
        let mut i = 0;
        while i + 3 < encoded.len() && remaining > 0 {
            let a = (encoded[i].wrapping_sub(32)) & 0x3F;
            let b = (encoded[i + 1].wrapping_sub(32)) & 0x3F;
            let c = (encoded[i + 2].wrapping_sub(32)) & 0x3F;
            let d = (encoded[i + 3].wrapping_sub(32)) & 0x3F;
            let triple = ((a as u32) << 18) | ((b as u32) << 12) | ((c as u32) << 6) | (d as u32);
            if remaining > 0 {
                output.push(((triple >> 16) & 0xFF) as u8);
                remaining -= 1;
            }
            if remaining > 0 {
                output.push(((triple >> 8) & 0xFF) as u8);
                remaining -= 1;
            }
            if remaining > 0 {
                output.push((triple & 0xFF) as u8);
                remaining -= 1;
            }
            i += 4;
        }
    }
    if filename.is_empty() && output.is_empty() {
        return None;
    }
    Some((filename, output))
}

pub fn uu_decode_to_writer(path: &Path, writer: &mut dyn Write) -> GeeZipResult<u64> {
    let (_filename, data) = uu_decode_file(path)?;
    let len = data.len() as u64;
    writer.write_all(&data).map_err(|e| {
        GeeZipError::io(
            e,
            format!("writing decoded output for '{}'", path.display()),
        )
    })?;
    Ok(len)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uu_encode(data: &[u8]) -> String {
        let mut out = String::new();
        let total = data.len();
        out.push(((total as u8).min(45) + 32) as char);
        for chunk in data.chunks(3) {
            let a = chunk.first().copied().unwrap_or(0) as u32;
            let b = chunk.get(1).copied().unwrap_or(0) as u32;
            let c = chunk.get(2).copied().unwrap_or(0) as u32;
            let val = (a << 16) | (b << 8) | c;
            for shift in [18u32, 12, 6, 0] {
                let six = ((val >> shift) & 0x3F) as u8;
                out.push((six + 32) as char);
            }
        }
        out
    }

    #[test]
    fn uu_decode_roundtrip() {
        for data in &[
            b"cat".as_slice(),
            b"hello uuencode".as_slice(),
            b"".as_slice(),
            b"x".as_slice(),
        ] {
            let encoded = uu_encode(data);
            let input = format!("begin 644 test.bin\n{}\nend\n", encoded);
            if let Some((filename, decoded)) = uu_decode(&input) {
                assert_eq!(filename, "test.bin");
                assert_eq!(decoded, data.to_vec());
            } else if !data.is_empty() {
                panic!("decode failed for {:?}", data);
            }
        }
    }

    #[test]
    fn uu_decode_empty_fails() {
        assert!(uu_decode("").is_none());
    }

    #[test]
    fn uu_decode_file_test() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("test.uu");
        let data = b"file content";
        let encoded = uu_encode(data);
        let content = format!("begin 644 test.txt\n{}\nend\n", encoded);
        std::fs::write(&path, content).unwrap();
        let (filename, decoded) = uu_decode_file(&path).unwrap();
        assert_eq!(filename, "test.txt");
        assert_eq!(decoded, data.to_vec());
    }
}
