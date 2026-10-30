//! UU/UUE decode helpers (single-stream).

use std::fs;
use std::io::Read;
use std::io::Write;
use std::path::Path;

use crate::error::{GeeZipError, GeeZipResult};

/// Encode raw binary data into a UU-encoded body (without header/footer).
///
/// Data is split into 45-byte lines, each prefixed with a length
/// character.  This conforms to the standard UUencoding format used by
/// `uu_decode`.
pub fn uu_encode(data: &[u8]) -> String {
    let mut out = String::new();
    for chunk in data.chunks(45) {
        // Length byte: chunk.len() + 32 (printable ASCII range)
        out.push(((chunk.len() as u8).saturating_add(32)) as char);
        for triple in chunk.chunks(3) {
            let a = triple.first().copied().unwrap_or(0) as u32;
            let b = triple.get(1).copied().unwrap_or(0) as u32;
            let c = triple.get(2).copied().unwrap_or(0) as u32;
            let val = (a << 16) | (b << 8) | c;
            for shift in [18u32, 12, 6, 0] {
                let six = ((val >> shift) & 0x3F) as u8;
                out.push((six + 32) as char);
            }
        }
        out.push('\n');
    }
    out
}

/// Encode data from `reader` into a full UU-encoded stream (with
/// `begin` / `end` headers) and write it to `writer`.
pub fn uu_encode_to_writer<R: Read, W: Write>(
    reader: &mut R,
    name: &str,
    writer: &mut W,
) -> GeeZipResult<u64> {
    super::text_encoding::encode(reader, writer, name, super::text_encoding::UU, b'`')
        .map_err(|e| GeeZipError::io(e, "streaming uu encoding"))
}

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
    let (mode, filename) = header.split_once(char::is_whitespace)?;
    let filename = filename.trim_start();
    if mode.is_empty() || !mode.bytes().all(|b| matches!(b, b'0'..=b'7')) || filename.is_empty() {
        return None;
    }

    let mut output = Vec::new();
    let mut terminated = false;
    for line in lines {
        let line = line.trim_end_matches('\r');
        if line == "end" {
            return Some((filename.to_owned(), output));
        }
        // Accept the conventional space/backtick zero-length line (and an
        // empty separator), but require the following end marker.
        if line.is_empty() || line == " " || line == "`" {
            terminated = true;
            continue;
        }
        if terminated {
            return None;
        }
        let bytes = line.as_bytes();
        if !(b'!'..=b'_').contains(&bytes[0]) {
            return None;
        }
        let mut remaining = (bytes[0] - 32) as usize;
        let required = remaining.div_ceil(3) * 4;
        let encoded = bytes.get(1..1 + required)?;
        if !encoded.iter().all(|b| (b' '..=b'`').contains(b))
            || !bytes[1 + required..].iter().all(u8::is_ascii_whitespace)
        {
            return None;
        }
        for chunk in encoded.as_chunks::<4>().0 {
            let a = (chunk[0] - 32) & 0x3F;
            let b = (chunk[1] - 32) & 0x3F;
            let c = (chunk[2] - 32) & 0x3F;
            let d = (chunk[3] - 32) & 0x3F;
            let triple = ((a as u32) << 18) | ((b as u32) << 12) | ((c as u32) << 6) | (d as u32);
            for shift in [16, 8, 0].into_iter().take(remaining.min(3)) {
                output.push(((triple >> shift) & 0xFF) as u8);
            }
            remaining = remaining.saturating_sub(3);
        }
    }
    None
}

pub fn uu_decode_to_writer(path: &Path, writer: &mut dyn Write) -> GeeZipResult<u64> {
    let file = fs::File::open(path).map_err(|e| GeeZipError::io(e, "opening uu input"))?;
    super::text_encoding::decode(
        &mut std::io::BufReader::new(file),
        writer,
        super::text_encoding::UU,
        true,
    )
    .map_err(|e| GeeZipError::io(e, "streaming uu decoding"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uu_streaming_decode_validates_termination() {
        let data = b"bounded line encoding ".repeat(10000);
        let mut encoded = Vec::new();
        uu_encode_to_writer(&mut data.as_slice(), "large.bin", &mut encoded).unwrap();
        let mut decoded = Vec::new();
        super::super::text_encoding::decode(
            &mut encoded.as_slice(),
            &mut decoded,
            super::super::text_encoding::UU,
            true,
        )
        .unwrap();
        assert_eq!(decoded, data);
        encoded.truncate(encoded.len() - 4);
        assert!(super::super::text_encoding::decode(
            &mut encoded.as_slice(),
            &mut Vec::new(),
            super::super::text_encoding::UU,
            true
        )
        .is_err());
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
    fn uu_writer_roundtrip_preserves_padding_and_line_boundaries() {
        for data in [vec![], vec![0], vec![0; 45], vec![0; 46], b"hello".to_vec()] {
            let mut encoded = Vec::new();
            let count =
                uu_encode_to_writer(&mut &data[..], "name with spaces.bin", &mut encoded).unwrap();
            assert_eq!(count, encoded.len() as u64);
            let (name, decoded) = uu_decode(std::str::from_utf8(&encoded).unwrap()).unwrap();
            assert_eq!(name, "name with spaces.bin");
            assert_eq!(decoded, data);
            assert!(encoded.ends_with(b"\n`\nend\n"));
        }
    }

    #[test]
    fn uu_rejects_truncated_or_invalid_body() {
        for input in [
            "begin 644 f\n#8V\nend\n",
            "begin 644 f\n#8V%T\n",
            "begin 644 f\n#8V~T\nend\n",
            "begin 644 f\n`\n#8V%T\nend\n",
            "begin 644\nend\n",
        ] {
            assert!(uu_decode(input).is_none(), "accepted {input:?}");
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
