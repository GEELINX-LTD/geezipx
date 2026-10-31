use super::{bad, Input, Result, Source};
use nsis::decompress::{bzip2::Bzip2Reader, CompressionMethod};
use std::io::{self, BufRead, BufReader, Read, Write};

pub(super) fn poll(input: &mut Input<'_>) -> io::Result<()> {
    input.get_mut().get_mut().read(&mut []).map(|_| ())
}
fn pump<D: Read>(
    decoder: &mut D,
    output: &mut dyn Write,
    limit: u64,
    mut check: impl FnMut(&mut D) -> io::Result<()>,
) -> Result<u64> {
    let mut bytes = [0; 64 * 1024];
    let mut total = 0u64;
    loop {
        check(decoder)?;
        let count = decoder.read(&mut bytes)?;
        if count == 0 {
            return Ok(total);
        }
        total = total
            .checked_add(count as u64)
            .ok_or_else(|| bad("decoded size overflow"))?;
        if total > limit {
            return Err(bad("decoded block exceeds its size bound"));
        }
        output.write_all(&bytes[..count])?;
    }
}
pub(super) fn run(
    method: CompressionMethod,
    input: &mut Input<'_>,
    output: &mut dyn Write,
    limit: u64,
) -> Result<u64> {
    let count = match method {
        CompressionMethod::None => pump(input, output, limit, poll)?,
        CompressionMethod::Deflate => {
            let mut decoder = flate2::bufread::DeflateDecoder::new(&mut *input);
            pump(&mut decoder, output, limit, |d| poll(d.get_mut()))?
        }
        CompressionMethod::Bzip2 => {
            let mut decoder = Bzip2Reader::new(&mut *input);
            pump(&mut decoder, output, limit, |d| poll(d.input_mut()))?
        }
        CompressionMethod::Lzma => {
            let mut header = [0; 5];
            input.read_exact(&mut header)?;
            let dictionary = u32::from_le_bytes(header[1..].try_into().unwrap());
            if dictionary > 64 * 1024 * 1024 {
                return Err(bad("LZMA dictionary exceeds 64 MiB"));
            }
            let mut decoder = lzma_rust2::LzmaReader::new_with_props(
                &mut *input,
                u64::MAX,
                header[0],
                dictionary,
                None,
            )?;
            pump(&mut decoder, output, limit, |d| poll(d.inner_mut()))?
        }
    };
    if !input.fill_buf()?.is_empty() || input.limit() != 0 {
        return Err(bad("trailing or unconsumed compressed block data"));
    }
    Ok(count)
}

pub(super) fn source_poll(source: &mut BufReader<Box<dyn Source>>) -> Result<()> {
    let _ = source.get_mut().read(&mut [])?;
    Ok(())
}
