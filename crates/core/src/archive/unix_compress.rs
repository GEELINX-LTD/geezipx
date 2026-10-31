//! Streaming Unix compress (.Z) decoder. The dictionary is limited to 65536
//! entries and codes are read in groups of eight (the ncompress alignment rule).
use crate::error::{GeeZipError, GeeZipResult};
use std::io::{self, Read, Write};

struct Codes<R> {
    reader: R,
    data: [u8; 16],
    bytes: usize,
    bit: usize,
}
impl<R: Read> Codes<R> {
    fn next(&mut self, width: usize) -> io::Result<Option<usize>> {
        if self.bit + width > self.bytes * 8 {
            self.bit = 0;
            self.bytes = 0;
            while self.bytes < width {
                match self.reader.read(&mut self.data[self.bytes..width]) {
                    Ok(0) => break,
                    Ok(n) => self.bytes += n,
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(e),
                }
            }
            if self.bytes * 8 < width {
                return Ok(None);
            }
        }
        let mut code = 0;
        for i in 0..width {
            let pos = self.bit + i;
            code |= (((self.data[pos / 8] >> (pos % 8)) & 1) as usize) << i;
        }
        self.bit += width;
        Ok(Some(code))
    }
    fn align(&mut self) {
        self.bytes = 0;
        self.bit = 0;
    }
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

pub fn z_decompress<R: Read, W: Write + ?Sized>(
    reader: &mut R,
    writer: &mut W,
) -> GeeZipResult<u64> {
    decode(reader, writer).map_err(|e| GeeZipError::io(e, "Unix compress decoding failed"))
}
fn decode<R: Read, W: Write + ?Sized>(reader: &mut R, writer: &mut W) -> io::Result<u64> {
    let mut header = [0; 3];
    reader.read_exact(&mut header)?;
    let maxbits = (header[2] & 31) as usize;
    if header[..2] != [0x1f, 0x9d] || header[2] & 0x60 != 0 || !(9..=16).contains(&maxbits) {
        return Err(invalid("invalid Unix compress header"));
    }
    let block = header[2] & 0x80 != 0;
    let capacity = 1usize << maxbits;
    let mut prefix = vec![0u16; capacity];
    let mut suffix = vec![0u8; capacity];
    let mut stack = vec![0u8; capacity];
    let mut codes = Codes {
        reader,
        data: [0; 16],
        bytes: 0,
        bit: 0,
    };
    let mut output = io::BufWriter::with_capacity(64 * 1024, writer);
    let mut width = 9;
    let mut free = if block { 257 } else { 256 };
    let Some(mut old) = codes.next(width)? else {
        return Ok(0);
    };
    if old > 255 {
        return Err(invalid("first LZW code must be literal"));
    }
    let mut first = old as u8;
    output.write_all(&[first])?;
    let mut total = 1;
    while let Some(raw) = codes.next(width)? {
        if block && raw == 256 {
            codes.align();
            width = 9;
            free = 257;
            let Some(literal) = codes.next(width)? else {
                break;
            };
            if literal > 255 {
                return Err(invalid("LZW CLEAR must be followed by literal"));
            }
            old = literal;
            first = literal as u8;
            output.write_all(&[first])?;
            total += 1;
            continue;
        }
        if raw > free || raw >= capacity {
            return Err(invalid("invalid LZW dictionary reference"));
        }
        let mut code = raw;
        let mut len = 0;
        if code == free {
            stack[len] = first;
            len += 1;
            code = old;
        }
        while code >= 256 {
            if code >= free || len >= capacity - 1 {
                return Err(invalid("invalid LZW prefix chain"));
            }
            stack[len] = suffix[code];
            len += 1;
            code = prefix[code] as usize;
        }
        first = code as u8;
        stack[len] = first;
        len += 1;
        stack[..len].reverse();
        output.write_all(&stack[..len])?;
        total += len as u64;
        if free < capacity {
            prefix[free] = old as u16;
            suffix[free] = first;
            free += 1;
            if free > (1 << width) - 1 && width < maxbits {
                codes.align();
                width += 1;
            }
        }
        old = raw;
    }
    output.flush()?;
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ncompress_fixture_width_transitions() {
        let input = include_bytes!("../../tests/fixtures/ncompress.Z");
        let expected: Vec<u8> = (0..16384)
            .map(|i| ((i * 73 + i / 13) % 256) as u8)
            .collect();
        let mut output = Vec::new();
        assert_eq!(
            z_decompress(&mut input.as_slice(), &mut output).unwrap(),
            expected.len() as u64
        );
        assert_eq!(output, expected);
    }
    #[test]
    fn invalid_header_and_code() {
        for input in [
            vec![0x1f],
            vec![0x1f, 0x9d, 8],
            vec![0x1f, 0x9d, 0x90, 255, 1],
        ] {
            assert!(z_decompress(&mut input.as_slice(), &mut Vec::new()).is_err());
        }
    }
}
