use super::{
    checksum::{bad, Checksum},
    Source,
};
use crate::error::GeeZipResult as Result;
use std::io::{BufReader, Read};

pub(super) struct Input<'a> {
    pub(super) source: &'a mut BufReader<Box<dyn Source>>,
    pub(super) position: u64,
    remaining: u64,
    volume: Checksum,
    stream: Checksum,
    shared: bool,
    stream_active: bool,
}
impl<'a> Input<'a> {
    pub(super) fn new(
        source: &'a mut BufReader<Box<dyn Source>>,
        length: u64,
        header: &[u8; 20],
    ) -> Result<Self> {
        let shared = header[3] == header[18];
        let mut value = Self {
            source,
            position: 20,
            remaining: length
                .checked_sub(20)
                .ok_or_else(|| bad("truncated header"))?,
            volume: Checksum::new(header[3])?,
            stream: Checksum::new(if shared { 0 } else { header[18] })?,
            shared,
            stream_active: true,
        };
        value.update(header);
        Ok(value)
    }
    pub(super) fn record(
        source: &'a mut BufReader<Box<dyn Source>>,
        start: u64,
        end: u64,
    ) -> Result<Self> {
        Ok(Self {
            source,
            position: start,
            remaining: end
                .checked_sub(start)
                .ok_or_else(|| bad("invalid record range"))?,
            volume: Checksum::new(0)?,
            stream: Checksum::new(0)?,
            shared: false,
            stream_active: false,
        })
    }
    fn update(&mut self, bytes: &[u8]) {
        self.volume.update(bytes);
        if self.stream_active && !self.shared {
            self.stream.update(bytes);
        }
    }
    pub(super) fn read_exact(&mut self, bytes: &mut [u8]) -> Result<()> {
        self.raw(bytes)?;
        self.update(bytes);
        Ok(())
    }
    fn raw(&mut self, bytes: &mut [u8]) -> Result<()> {
        let len = bytes.len() as u64;
        if len > self.remaining {
            return Err(bad("record exceeds archive bounds"));
        }
        self.source.read_exact(bytes)?;
        self.position += len;
        self.remaining -= len;
        Ok(())
    }
    pub(super) fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut bytes = [0; N];
        self.read_exact(&mut bytes)?;
        Ok(bytes)
    }
    pub(super) fn bytes(&mut self, len: usize) -> Result<Vec<u8>> {
        let mut bytes = vec![0; len];
        self.read_exact(&mut bytes)?;
        Ok(bytes)
    }
    pub(super) fn poll(&mut self) -> Result<()> {
        // Bypass BufReader so an empty read reaches cancellation-aware sources.
        loop {
            match self.source.get_mut().read(&mut []) {
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                result => {
                    result?;
                    break;
                }
            }
        }
        Ok(())
    }
    pub(super) fn finish_stream(&mut self) -> Result<()> {
        let expected = if self.shared {
            self.volume.value()
        } else {
            self.stream.value()
        };
        self.stream_active = false;
        if self.bytes(expected.len())? != expected {
            return Err(bad("stream checksum mismatch"));
        }
        let expected = self.volume.value();
        let mut actual = vec![0; expected.len()];
        self.raw(&mut actual)?;
        if actual != expected {
            return Err(bad("volume checksum mismatch"));
        }
        self.finish_record()
    }
    pub(super) fn finish_record(&self) -> Result<()> {
        if self.remaining != 0 {
            return Err(bad(
                "trailing data or unsupported split/multiple-stream archive",
            ));
        }
        Ok(())
    }
}
