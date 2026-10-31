use super::{bad, decode, Result, Source, METADATA_BUDGET};
use crate::error::GeeZipError;
use nsis::decompress::{CompressionMethod, CompressionMode};
use std::{
    fs::File,
    io::{self, BufReader, Read, Seek, SeekFrom},
};

pub(super) struct Layout {
    pub(super) first: [u8; 28],
    pub(super) start: u64,
    pub(super) end: u64,
    pub(super) crc: bool,
    pub(super) header_size: usize,
    pub(super) stub_hints: Vec<u8>,
}
fn u16le(bytes: &[u8]) -> u16 {
    u16::from_le_bytes(bytes[..2].try_into().unwrap())
}
fn u32le(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes[..4].try_into().unwrap())
}
fn at<const N: usize>(source: &mut (impl Read + Seek), offset: u64) -> Result<[u8; N]> {
    source.seek(SeekFrom::Start(offset))?;
    let mut data = [0; N];
    source.read_exact(&mut data)?;
    Ok(data)
}
pub(super) fn layout(source: &mut (impl Read + Seek)) -> Result<Layout> {
    let length = source.seek(SeekFrom::End(0))?;
    let dos = at::<64>(source, 0)?;
    if &dos[..2] != b"MZ" {
        return Err(bad("NSIS requires a PE executable"));
    }
    let pe_offset = u64::from(u32le(&dos[60..]));
    if pe_offset > 4 * 1024 * 1024 {
        return Err(bad("PE header offset exceeds probe bound"));
    }
    let pe = at::<24>(source, pe_offset)?;
    if &pe[..4] != b"PE\0\0" {
        return Err(bad("invalid PE signature"));
    }
    let sections = usize::from(u16le(&pe[6..]));
    let optional_len = usize::from(u16le(&pe[20..]));
    if sections == 0 || sections > 96 || !(144..=4096).contains(&optional_len) {
        return Err(bad("unsupported PE header layout"));
    }
    let optional_offset = pe_offset + 24;
    let mut optional = vec![0; optional_len];
    source.read_exact(&mut optional)?;
    let directory = match u16le(&optional) {
        0x10b => 96,
        0x20b => 112,
        _ => return Err(bad("invalid PE optional header")),
    };
    let security = optional
        .get(directory + 32..directory + 40)
        .ok_or_else(|| bad("missing PE security directory"))?;
    let certificate = (u64::from(u32le(security)), u64::from(u32le(&security[4..])));
    let mut overlay = u64::from(u32le(&optional[60..]));
    let table_offset = optional_offset + optional_len as u64;
    for position in 0..sections {
        let section = at::<40>(source, table_offset + position as u64 * 40)?;
        let size = u64::from(u32le(&section[16..]));
        let start = u64::from(u32le(&section[20..]));
        if size != 0 {
            overlay = overlay.max(start + size);
        }
    }
    if overlay < table_offset + sections as u64 * 40 || overlay > length {
        return Err(bad("PE section exceeds executable bounds"));
    }
    let aligned = overlay.div_ceil(512) * 512;
    let mut first = None;
    for start in (aligned..length.min(aligned.saturating_add(65536))).step_by(512) {
        if start + 28 > length {
            break;
        }
        let bytes = at::<28>(source, start)?;
        if nsis::header::FirstHeader::parse(&bytes).is_ok() {
            first = Some((start, bytes));
            break;
        }
    }
    let (start, first) =
        first.ok_or_else(|| bad("NSIS first header was not found in the PE overlay"))?;
    let parsed = nsis::header::FirstHeader::parse(&first).map_err(|e| bad(e.to_string()))?;
    if parsed.is_legacy() || parsed.siginfo() != 0xdead_beef {
        return Err(bad("legacy or custom NSIS signature is not supported"));
    }
    let header_size = u32le(&first[20..]) as usize;
    if !(68..=METADATA_BUDGET).contains(&header_size) {
        return Err(bad("NSIS header exceeds metadata budget or is too short"));
    }
    let archive_end = start + u64::from(u32le(&first[24..]));
    let crc = !parsed.has_no_crc() || parsed.has_force_crc();
    if archive_end > length || archive_end < start + 28 + 4 + if crc { 4 } else { 0 } {
        return Err(bad("invalid declared installer size"));
    }
    if archive_end != length {
        let (offset, size) = certificate;
        if size == 0
            || offset % 8 != 0
            || offset < archive_end
            || offset - archive_end > 7
            || offset.checked_add(size) != Some(length)
        {
            return Err(bad(
                "unsupported trailing data outside the installer and certificate table",
            ));
        }
        let padding = usize::try_from(offset - archive_end).unwrap();
        let mut bytes = [0; 7];
        source.seek(SeekFrom::Start(archive_end))?;
        source.read_exact(&mut bytes[..padding])?;
        if bytes[..padding].iter().any(|&byte| byte != 0) {
            return Err(bad("nonzero PE certificate alignment padding"));
        }
    }
    let mut stub_hints = vec![0; (start.saturating_sub(512) as usize).min(65536)];
    source.seek(SeekFrom::Start(512))?;
    source.read_exact(&mut stub_hints)?;
    Ok(Layout {
        first,
        start,
        end: archive_end - if crc { 4 } else { 0 },
        crc,
        header_size,
        stub_hints,
    })
}
pub(super) fn verify_crc(source: &mut BufReader<Box<dyn Source>>, layout: &Layout) -> Result<()> {
    if !layout.crc {
        return Ok(());
    }
    let expected = u32le(&at::<4>(source, layout.end)?);
    source.seek(SeekFrom::Start(512))?;
    let mut remaining = layout.end - 512;
    let mut bytes = [0; 65536];
    let mut hash = crc32fast::Hasher::new();
    while remaining != 0 {
        decode::source_poll(source)?;
        let count = remaining.min(bytes.len() as u64) as usize;
        source.read_exact(&mut bytes[..count])?;
        hash.update(&bytes[..count]);
        remaining -= count as u64;
    }
    if hash.finalize() != expected {
        return Err(bad("NSIS installer CRC32 mismatch"));
    }
    Ok(())
}
pub(super) fn guess(source: &mut BufReader<Box<dyn Source>>) -> Result<CompressionMethod> {
    let position = source.stream_position()?;
    let bytes = at::<5>(source, position)?;
    source.seek(SeekFrom::Start(position))?;
    Ok(nsis::decompress::detect_compression(&bytes))
}
type Header = (Vec<u8>, CompressionMethod, CompressionMode, u64, u64);
pub(super) fn read_header(
    source: &mut BufReader<Box<dyn Source>>,
    layout: &Layout,
) -> Result<Header> {
    let payload = layout.start + 28;
    let raw = u32le(&at::<4>(source, payload)?);
    let size = u64::from(raw & 0x7fff_ffff);
    let compressed = raw & 0x8000_0000 != 0;
    if !compressed && size == layout.header_size as u64 {
        let base = payload + 4 + size;
        if base > layout.end {
            return Err(bad("header block exceeds installer bounds"));
        }
        let mut header = vec![0; layout.header_size];
        source.read_exact(&mut header)?;
        return Ok((
            header,
            CompressionMethod::None,
            CompressionMode::NonSolid,
            base,
            layout.end,
        ));
    }
    if compressed && size <= METADATA_BUDGET as u64 && payload + 4 + size <= layout.end {
        let method = guess(source)?;
        let mut header = Vec::new();
        let result = decode::run(
            method,
            &mut source.by_ref().take(size),
            &mut header,
            layout.header_size as u64,
        );
        match result {
            Ok(count) if count == layout.header_size as u64 => {
                return Ok((
                    header,
                    method,
                    CompressionMode::NonSolid,
                    payload + 4 + size,
                    layout.end,
                ))
            }
            Err(GeeZipError::Cancelled) => return Err(GeeZipError::Cancelled),
            // A solid LZMA stream may start 5d 00 00 80, which also looks
            // like a non-solid compressed-length prefix. Deflate reports
            // malformed candidate data as InvalidInput on this backend.
            Err(GeeZipError::Io { source, context })
                if !matches!(
                    source.kind(),
                    io::ErrorKind::InvalidData
                        | io::ErrorKind::InvalidInput
                        | io::ErrorKind::UnexpectedEof
                ) =>
            {
                return Err(GeeZipError::Io { source, context })
            }
            _ => {}
        }
    }
    source.seek(SeekFrom::Start(payload))?;
    let method = guess(source)?;
    let mut staged = tempfile::tempfile()?;
    let decoded = decode::run(
        method,
        &mut source.by_ref().take(layout.end - payload),
        &mut staged,
        u64::from(u32::MAX),
    )?;
    staged.rewind()?;
    let mut size = [0; 4];
    staged.read_exact(&mut size)?;
    if u32le(&size) as usize != layout.header_size {
        return Err(bad("solid header length mismatch"));
    }
    let mut header = vec![0; layout.header_size];
    staged.read_exact(&mut header)?;
    let base = 4 + layout.header_size as u64;
    let placeholder: Box<dyn Source> = Box::new(io::Cursor::new(Vec::new()));
    let original = std::mem::replace(source, BufReader::new(placeholder)).into_inner();
    *source = BufReader::with_capacity(
        65536,
        Box::new(Staged {
            file: staged,
            original,
        }),
    );
    Ok((header, method, CompressionMode::Solid, base, decoded))
}
struct Staged {
    file: File,
    original: Box<dyn Source>,
}
impl Read for Staged {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let _ = self.original.read(&mut [])?;
        self.file.read(bytes)
    }
}
impl Seek for Staged {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let _ = self.original.read(&mut [])?;
        self.file.seek(position)
    }
}

/// Header-only probe used by seekable format detection. Genuine I/O and
/// cancellation errors survive; malformed/truncated PE structures are a miss.
pub(crate) fn probe(source: &mut (impl Read + Seek)) -> io::Result<bool> {
    match layout(source) {
        Ok(_) => Ok(true),
        Err(GeeZipError::Io { source, .. })
            if !matches!(
                source.kind(),
                io::ErrorKind::UnexpectedEof | io::ErrorKind::InvalidData
            ) =>
        {
            Err(source)
        }
        Err(GeeZipError::Cancelled) => Err(io::Error::other(GeeZipError::Cancelled)),
        Err(_) => Ok(false),
    }
}
