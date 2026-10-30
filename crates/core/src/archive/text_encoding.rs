//! Bounded line-oriented UU/XX encoding; payloads never accumulate in memory.
use std::io::{BufRead, Read, Write};

pub(super) const UU: &[u8; 64] =
    b" !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_";

pub(super) fn encode<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    name: &str,
    alphabet: &[u8; 64],
    zero: u8,
) -> std::io::Result<u64> {
    if name.is_empty() || name.contains(['\r', '\n']) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "invalid encoded filename",
        ));
    }
    let header = format!("begin 644 {name}\n");
    writer.write_all(header.as_bytes())?;
    let mut count = header.len() as u64;
    loop {
        let mut block = Vec::with_capacity(45);
        reader.take(45).read_to_end(&mut block)?;
        if block.is_empty() {
            break;
        }
        let mut line = [0u8; 62];
        line[0] = alphabet[block.len()];
        let mut len = 1;
        for triple in block.chunks(3) {
            let value = ((triple[0] as u32) << 16)
                | ((triple.get(1).copied().unwrap_or(0) as u32) << 8)
                | triple.get(2).copied().unwrap_or(0) as u32;
            for shift in [18, 12, 6, 0] {
                line[len] = alphabet[((value >> shift) & 63) as usize];
                len += 1;
            }
        }
        line[len] = b'\n';
        writer.write_all(&line[..=len])?;
        count += (len + 1) as u64;
    }
    writer.write_all(&[zero, b'\n', b'e', b'n', b'd', b'\n'])?;
    Ok(count + 6)
}

fn line<R: BufRead>(reader: &mut R, data: &mut Vec<u8>) -> std::io::Result<usize> {
    data.clear();
    let n = reader.take(1025).read_until(b'\n', data)?;
    if n > 1024 {
        return Err(invalid("encoded line exceeds 1024 bytes"));
    }
    while matches!(data.last(), Some(b'\n' | b'\r')) {
        data.pop();
    }
    Ok(n)
}
fn invalid(message: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message)
}

pub(super) fn decode<R: BufRead, W: Write + ?Sized>(
    reader: &mut R,
    writer: &mut W,
    alphabet: &[u8; 64],
    uu: bool,
) -> std::io::Result<u64> {
    let mut data = Vec::with_capacity(128);
    line(reader, &mut data)?;
    let header = std::str::from_utf8(&data).map_err(|_| invalid("invalid encoding header"))?;
    let (mode, name) = header
        .strip_prefix("begin ")
        .and_then(|h| h.split_once(char::is_whitespace))
        .ok_or_else(|| invalid("missing begin header"))?;
    if mode.is_empty() || !mode.bytes().all(|b| matches!(b, b'0'..=b'7')) || name.trim().is_empty()
    {
        return Err(invalid("invalid mode or filename"));
    }
    let mut reverse = [255u8; 256];
    for (i, b) in alphabet.iter().enumerate() {
        reverse[*b as usize] = i as u8;
    }
    if uu {
        reverse[b'`' as usize] = 0;
    }
    let mut total = 0;
    let mut terminated = false;
    while line(reader, &mut data)? != 0 {
        if data == b"end" {
            return Ok(total);
        }
        if data.is_empty() || (data.len() == 1 && reverse[data[0] as usize] == 0) {
            terminated = true;
            continue;
        }
        if terminated {
            return Err(invalid("data after zero-length line"));
        }
        let len = reverse[data[0] as usize] as usize;
        if len == 0 || len > 63 {
            return Err(invalid("invalid line length"));
        }
        let required = len.div_ceil(3) * 4;
        let encoded = data
            .get(1..1 + required)
            .ok_or_else(|| invalid("truncated encoded line"))?;
        if encoded.iter().any(|b| reverse[*b as usize] == 255)
            || !data[1 + required..].iter().all(u8::is_ascii_whitespace)
        {
            return Err(invalid("invalid encoded character"));
        }
        let mut output = [0u8; 63];
        let mut index = 0;
        for quad in encoded.chunks_exact(4) {
            let value = quad
                .iter()
                .fold(0u32, |value, b| (value << 6) | reverse[*b as usize] as u32);
            for shift in [16, 8, 0] {
                if index < len {
                    output[index] = (value >> shift) as u8;
                    index += 1;
                }
            }
        }
        writer.write_all(&output[..len])?;
        total += len as u64;
    }
    Err(invalid("missing end marker"))
}
