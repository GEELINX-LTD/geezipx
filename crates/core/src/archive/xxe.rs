//! XXE decode helpers (single-stream).
//!
//! XXencode uses the alphabet: `+-0123456789A-Za-z` (indices 0-63).

use std::fs;
use std::io::Read;
use std::io::Write;
use std::path::Path;

use crate::error::{GeeZipError, GeeZipResult};

const XXE_ALPHABET: &[u8; 64] = b"+-0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

fn build_reverse_table() -> [u8; 256] {
    let mut table = [u8::MAX; 256];
    for (i, &ch) in XXE_ALPHABET.iter().enumerate() {
        table[ch as usize] = i as u8;
    }
    table
}

/// Encode raw binary data into an XXE-encoded body (without header/footer).
///
/// Data is split into 45-byte lines, each prefixed with a length
/// character.  This conforms to the standard XXencode format used by
/// `xxe_decode`.
pub fn xxe_encode(data: &[u8]) -> String {
    let mut out = String::new();
    for chunk in data.chunks(45) {
        // Length byte using XXE alphabet
        out.push(XXE_ALPHABET[chunk.len().min(45)] as char);
        for triple in chunk.chunks(3) {
            let a = triple.first().copied().unwrap_or(0) as u32;
            let b = triple.get(1).copied().unwrap_or(0) as u32;
            let c = triple.get(2).copied().unwrap_or(0) as u32;
            let val = (a << 16) | (b << 8) | c;
            for shift in [18u32, 12, 6, 0] {
                let six = ((val >> shift) & 0x3F) as usize;
                out.push(XXE_ALPHABET[six] as char);
            }
        }
        out.push('\n');
    }
    out
}

/// Encode data from `reader` into a full XXE-encoded stream (with
/// `begin` / `end` headers) and write it to `writer`.
pub fn xxe_encode_to_writer<R: Read, W: Write>(
    reader: &mut R,
    name: &str,
    writer: &mut W,
) -> GeeZipResult<u64> {
    let mut data = Vec::new();
    reader
        .read_to_end(&mut data)
        .map_err(|e| GeeZipError::io(e, "reading input for xxe encoding"))?;
    let body = xxe_encode(&data);
    let output = format!("begin 644 {name}\n{body}+\nend\n");
    writer
        .write_all(output.as_bytes())
        .map_err(|e| GeeZipError::io(e, "writing xxe-encoded output"))?;
    Ok(output.len() as u64)
}

pub fn xxe_decode_file(path: &Path) -> GeeZipResult<(String, Vec<u8>)> {
    let content = fs::read_to_string(path)
        .map_err(|e| GeeZipError::io(e, format!("reading '{}'", path.display())))?;
    xxe_decode(&content).map_err(|e| {
        GeeZipError::format(
            format!("xxencode file '{}': {}", path.display(), e),
            crate::detect::ArchiveFormat::Xxe,
        )
    })
}

pub fn xxe_decode(input: &str) -> Result<(String, Vec<u8>), String> {
    let reverse = build_reverse_table();
    let mut lines = input.lines();

    let header = lines.next().ok_or("missing header line")?;
    let header = header.trim();
    let header = header.strip_prefix("begin ").ok_or("expected 'begin'")?;
    let (mode, filename) = header
        .split_once(char::is_whitespace)
        .ok_or("missing filename")?;
    let filename = filename.trim_start();
    if mode.is_empty() || !mode.bytes().all(|b| matches!(b, b'0'..=b'7')) || filename.is_empty() {
        return Err("invalid mode or filename".into());
    }

    let mut output = Vec::new();
    let mut terminated = false;
    for line in lines {
        let line = line.trim_end_matches('\r');
        if line == "end" {
            return Ok((filename.to_owned(), output));
        }
        if line.is_empty() || line == "+" {
            terminated = true;
            continue;
        }
        if terminated {
            return Err("data after zero-length line".into());
        }
        let bytes = line.as_bytes();
        let encoded_len = reverse[bytes[0] as usize];
        if encoded_len == u8::MAX || encoded_len == 0 {
            return Err("invalid line length character".into());
        }
        let mut remaining = encoded_len as usize;
        let required = remaining.div_ceil(3) * 4;
        let encoded = bytes.get(1..1 + required).ok_or("truncated data line")?;
        if !encoded.iter().all(|b| reverse[*b as usize] != u8::MAX)
            || !bytes[1 + required..].iter().all(u8::is_ascii_whitespace)
        {
            return Err("invalid encoded character".into());
        }
        for chunk in encoded.chunks_exact(4) {
            let a = reverse[chunk[0] as usize] as u32;
            let b = reverse[chunk[1] as usize] as u32;
            let c = reverse[chunk[2] as usize] as u32;
            let d = reverse[chunk[3] as usize] as u32;
            let triple = (a << 18) | (b << 12) | (c << 6) | d;
            for shift in [16, 8, 0].into_iter().take(remaining.min(3)) {
                output.push(((triple >> shift) & 0xFF) as u8);
            }
            remaining = remaining.saturating_sub(3);
        }
    }

    Err("missing end marker".into())
}

pub fn xxe_decode_to_writer(path: &Path, writer: &mut dyn Write) -> GeeZipResult<u64> {
    let (_filename, data) = xxe_decode_file(path)?;
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

    #[test]
    fn xxe_decode_roundtrip() {
        for data in &[
            b"cat".as_slice(),
            b"hello xxencode".as_slice(),
            b"x".as_slice(),
        ] {
            let encoded = xxe_encode(data);
            let input = format!("begin 644 test.bin\n{}\nend\n", encoded);
            let (filename, decoded) = xxe_decode(&input).unwrap();
            assert_eq!(filename, "test.bin");
            assert_eq!(decoded, data.to_vec());
        }
    }

    #[test]
    fn xxe_writer_roundtrip_preserves_line_boundaries() {
        for data in [vec![], vec![0], vec![0; 45], vec![0; 46], b"hello".to_vec()] {
            let mut encoded = Vec::new();
            let count =
                xxe_encode_to_writer(&mut &data[..], "name with spaces.bin", &mut encoded).unwrap();
            assert_eq!(count, encoded.len() as u64);
            let (name, decoded) = xxe_decode(std::str::from_utf8(&encoded).unwrap()).unwrap();
            assert_eq!(name, "name with spaces.bin");
            assert_eq!(decoded, data);
            assert!(encoded.ends_with(b"\n+\nend\n"));
        }
    }

    #[test]
    fn xxe_rejects_truncated_or_invalid_body() {
        let body = xxe_encode(b"cat");
        assert!(xxe_decode(&format!("begin 644 f\n{body}")).is_err());
        assert!(xxe_decode(&format!("begin 644 f\n+\n{body}end\n")).is_err());
        for input in [
            "begin 644 f\n1Mk\nend\n",
            "begin 644 f\n1!!!!\nend\n",
            "begin 644 f\n!\nend\n",
            "begin 644\nend\n",
        ] {
            assert!(xxe_decode(input).is_err(), "accepted {input:?}");
        }
    }

    #[test]
    fn xxe_decode_malformed_fails() {
        assert!(xxe_decode("not xxencode").is_err());
        assert!(xxe_decode("").is_err());
    }

    #[test]
    fn xxe_decode_empty_data() {
        let encoded = xxe_encode(b"");
        let input = format!("begin 644 empty.txt\n{}\nend\n", encoded);
        let (filename, data) = xxe_decode(&input).unwrap();
        assert_eq!(filename, "empty.txt");
        assert!(data.is_empty());
    }

    #[test]
    fn xxe_decode_file_roundtrip() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("test.xxe");
        let data = b"test content";
        let encoded = xxe_encode(data);
        let content = format!("begin 644 test.txt\n{}\nend\n", encoded);
        std::fs::write(&path, content).unwrap();
        let (filename, decoded) = xxe_decode_file(&path).unwrap();
        assert_eq!(filename, "test.txt");
        assert_eq!(decoded, data.to_vec());
    }
}
