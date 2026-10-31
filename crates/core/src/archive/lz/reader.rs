//! Strict Lzip v1 member framing around liblzma's bounded LZMA decoder.
//! BufRead consumption preserves trailers and the next member without seeking.
use std::io::{self, BufRead, BufReader, Read};
use xz2::stream::{Action, Status, Stream};

pub struct LzipReader<R> {
    source: BufReader<R>,
    stream: Option<Stream>,
    crc: crc32fast::Hasher,
    size: u64,
    members: u64,
    failed: bool,
    finished: bool,
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
impl<R: Read> LzipReader<R> {
    /// Decode Lzip v1 members with a 256 MiB LZMA memory budget.
    /// Empty inputs, incomplete members and trailing non-member data are errors.
    pub fn new(source: R) -> Self {
        Self {
            source: BufReader::with_capacity(64 * 1024, source),
            stream: None,
            crc: crc32fast::Hasher::new(),
            size: 0,
            members: 0,
            failed: false,
            finished: false,
        }
    }
    fn start(&mut self) -> io::Result<bool> {
        let mut header = [0; 6];
        loop {
            match self.source.read(&mut header[..1]) {
                Ok(0) if self.members > 0 => return Ok(false),
                Ok(0) => return Err(invalid("missing Lzip member")),
                Ok(_) => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
        self.source.read_exact(&mut header[1..])?;
        if &header[..4] != b"LZIP" || header[4] != 1 {
            return Err(invalid("invalid Lzip magic/version or trailing data"));
        }
        let exponent = header[5] & 31;
        if !(12..=29).contains(&exponent) {
            return Err(invalid("invalid Lzip dictionary size"));
        }
        let base = 1u32 << exponent;
        let dictionary = base - (base / 16) * u32::from(header[5] >> 5);
        if dictionary < 4096 || u64::from(dictionary) > super::super::xz::LZMA_MEMORY_LIMIT {
            return Err(invalid("Lzip dictionary exceeds memory limit"));
        }
        let mut alone = [0xff; 13];
        alone[0] = 0x5d; // LZMA-302eos: lc=3, lp=0, pb=2.
        alone[1..5].copy_from_slice(&dictionary.to_le_bytes());
        let mut stream = Stream::new_lzma_decoder(super::super::xz::LZMA_MEMORY_LIMIT)?;
        // The synthesized Alone header has no payload and cannot produce output.
        stream.process(&alone, &mut [0; 1], Action::Run)?;
        if stream.total_in() != 13 || stream.total_out() != 0 {
            return Err(invalid("Lzip decoder could not consume header"));
        }
        self.stream = Some(stream);
        self.crc = crc32fast::Hasher::new();
        self.size = 0;
        Ok(true)
    }
    fn finish(&mut self) -> io::Result<()> {
        let stream = self.stream.take().expect("active Lzip member");
        let mut trailer = [0; 20];
        self.source.read_exact(&mut trailer)?;
        let crc = u32::from_le_bytes(trailer[..4].try_into().unwrap());
        let size = u64::from_le_bytes(trailer[4..12].try_into().unwrap());
        let member_size = u64::from_le_bytes(trailer[12..].try_into().unwrap());
        // Replace the synthetic 13-byte Alone header with the 6-byte Lzip header.
        let consumed = stream
            .total_in()
            .checked_add(13)
            .ok_or_else(|| invalid("Lzip member size overflow"))?;
        if std::mem::replace(&mut self.crc, crc32fast::Hasher::new()).finalize() != crc
            || self.size != size
            || consumed != member_size
        {
            return Err(invalid("Lzip CRC32, data size or member size mismatch"));
        }
        self.members += 1;
        Ok(())
    }
    fn decode(&mut self, output: &mut [u8]) -> io::Result<usize> {
        loop {
            if self.finished {
                return Ok(0);
            }
            if self.stream.is_none() && !self.start()? {
                self.finished = true;
                return Ok(0);
            }
            let input = self.source.fill_buf()?;
            let stream = self.stream.as_mut().expect("active Lzip member");
            let before_in = stream.total_in();
            let before_out = stream.total_out();
            let action = if input.is_empty() {
                Action::Finish
            } else {
                Action::Run
            };
            let status = stream.process(input, output, action)?;
            let consumed = (stream.total_in() - before_in) as usize;
            let produced = (stream.total_out() - before_out) as usize;
            self.source.consume(consumed);
            self.crc.update(&output[..produced]);
            self.size = self
                .size
                .checked_add(produced as u64)
                .ok_or_else(|| invalid("Lzip output size overflow"))?;
            if status == Status::StreamEnd {
                self.finish()?;
            } else if consumed == 0 && produced == 0 {
                return Err(invalid("incomplete Lzip compressed stream"));
            }
            if produced > 0 {
                return Ok(produced);
            }
        }
    }
}
impl<R: Read> Read for LzipReader<R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        if self.failed {
            return Err(invalid("Lzip reader previously failed"));
        }
        let result = loop {
            match self.decode(output) {
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                result => break result,
            }
        };
        if result.is_err() {
            self.failed = true;
        }
        result
    }
}
