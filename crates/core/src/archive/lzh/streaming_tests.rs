use super::*;
use std::io::Cursor;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

struct ShortInput<'a> {
    inner: Cursor<&'a [u8]>,
    calls: usize,
}
impl Read for ShortInput<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        assert!(buf.len() <= LZH_READ_CHUNK_SIZE);
        self.calls += 1;
        if self.calls % 37 == 2 {
            return Err(std::io::ErrorKind::Interrupted.into());
        }
        let len = buf.len().min(113);
        self.inner.read(&mut buf[..len])
    }
}
fn mixed_data() -> Vec<u8> {
    let mut data = Vec::new();
    let mut state = 0x1984a5f1u32;
    for _ in 0..256 * 1024 {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        data.push(state as u8);
    }
    let history = data[data.len() - 2048..].to_vec();
    for _ in 0..256 {
        data.extend_from_slice(&history);
    }
    data.extend_from_slice(b"tail after multiple non-byte-aligned Huffman blocks");
    data
}

#[test]
fn lzh_reader_encoder_preserves_continuous_bits_across_short_reads() {
    let data = mixed_data();
    let mut short = ShortInput {
        inner: Cursor::new(data.as_slice()),
        calls: 0,
    };
    let mut packed = Vec::new();
    let mut encoder = LzhEncoder::new(LzhMethod::Lh7);
    assert_eq!(
        encoder.encode_reader(&mut short, &mut packed).unwrap(),
        data.len() as u64
    );
    assert!(encoder
        .encode_reader(&mut std::io::empty(), &mut packed)
        .is_err());
    let mut archive = Vec::new();
    write_lzh_entry(
        &mut archive,
        Path::new("data"),
        b"data",
        *b"-lh7-",
        packed.len() as u32,
        data.len() as u32,
        lzh_crc16(&data),
        false,
        &mut Cursor::new(packed),
    )
    .unwrap();
    archive.push(0);
    let mut reader = LzhReader::from_buf(archive);
    let entry = reader.entries().unwrap().remove(0);
    let mut result = Vec::new();
    reader.extract(&entry, &mut result).unwrap();
    assert_eq!(result, data);
}

#[test]
fn lzh_reader_blocks_keep_long_run_compression_efficiency() {
    let data = vec![0u8; 4 * 1024 * 1024];
    let baseline = oxiarc_lzhuf::encode_lzh(&data, LzhMethod::Lh7).unwrap();
    let mut encoded = Vec::new();
    LzhEncoder::new(LzhMethod::Lh7)
        .encode_reader(&mut Cursor::new(&data), &mut encoded)
        .unwrap();
    // Bounded input reads must not introduce a Huffman header per input block.
    // The allowance covers match boundaries introduced by bounded LZSS input.
    assert!(
        encoded.len() <= baseline.len() + 64,
        "{} vs {} bytes",
        encoded.len(),
        baseline.len()
    );
}

#[test]
fn lzh_streaming_methods_keep_history_crc_and_store_fallback() {
    let data = mixed_data();
    for method in [
        LzhCompressionMethod::Lh4,
        LzhCompressionMethod::Lh5,
        LzhCompressionMethod::Lh6,
        LzhCompressionMethod::Lh7,
    ] {
        let mut writer = LzhWriter::new(Cursor::new(Vec::new()), method);
        writer
            .add_entry_from_reader(Path::new("data"), &mut Cursor::new(&data))
            .unwrap();
        writer
            .add_entry_from_reader(Path::new("empty"), &mut std::io::empty())
            .unwrap();
        writer.add_directory(Path::new("empty-dir")).unwrap();
        let (_, cursor) = writer.finalize().unwrap();
        let mut bytes = cursor.into_inner();
        assert_eq!(&bytes[2..7], &method.method_bytes());
        let mut reader = LzhReader::from_buf(bytes.clone());
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 3);
        let mut result = Vec::new();
        reader.extract(&entries[0], &mut result).unwrap();
        assert_eq!(result, data);
        assert_eq!(reader.extract(&entries[1], &mut Vec::new()).unwrap(), 0);
        let end = bytes[0] as usize + 2;
        bytes[end - 2] ^= 1;
        bytes[1] = lzh_header_checksum(&bytes[2..end]);
        let mut damaged = LzhReader::from_buf(bytes);
        let entry = damaged.entries().unwrap().remove(0);
        assert!(damaged
            .extract(&entry, &mut std::io::sink())
            .unwrap_err()
            .to_string()
            .contains("CRC"));
    }
    let mut writer = LzhWriter::new(Cursor::new(Vec::new()), LzhCompressionMethod::Lh7);
    writer
        .add_entry_from_reader(Path::new("tiny"), &mut Cursor::new(b"no repetition"))
        .unwrap();
    let (_, cursor) = writer.finalize().unwrap();
    assert_eq!(&cursor.get_ref()[2..7], b"-lh0-");
}

#[test]
fn lzh_failed_source_leaves_output_unchanged() {
    struct Broken(bool);
    impl Read for Broken {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.0 {
                return Err(std::io::Error::other("source failure"));
            }
            self.0 = true;
            buf[0] = 42;
            Ok(1)
        }
    }
    let mut writer = LzhWriter::new(Cursor::new(b"existing".to_vec()), LzhCompressionMethod::Lh7);
    assert!(writer
        .add_entry_from_reader(Path::new("data"), &mut Broken(false))
        .is_err());
    assert_eq!(writer.inner.as_ref().unwrap().get_ref(), b"existing");
    writer
        .add_entry_from_reader(Path::new("valid"), &mut Cursor::new(b"ok"))
        .unwrap();
}

#[test]
fn lzh_cancels_after_source_eof_before_compressed_payload() {
    struct Source {
        inner: Cursor<Vec<u8>>,
        flag: Arc<AtomicBool>,
    }
    impl Read for Source {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = self.inner.read(buf)?;
            if n == 0 {
                self.flag.store(true, Ordering::Relaxed);
            }
            Ok(n)
        }
    }
    struct Output {
        inner: Cursor<Vec<u8>>,
        flag: Arc<AtomicBool>,
    }
    impl Write for Output {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if self.flag.load(Ordering::Relaxed) {
                return Err(std::io::Error::other(GeeZipError::Cancelled));
            }
            self.inner.write(buf)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.inner.flush()
        }
    }
    impl Seek for Output {
        fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
            self.inner.seek(pos)
        }
    }
    let flag = Arc::new(AtomicBool::new(false));
    let mut source = Source {
        inner: Cursor::new(vec![3; 512 * 1024]),
        flag: flag.clone(),
    };
    let mut writer = LzhWriter::new(
        Output {
            inner: Cursor::new(Vec::new()),
            flag,
        },
        LzhCompressionMethod::Lh7,
    );
    assert!(writer
        .add_entry_from_reader(Path::new("data"), &mut source)
        .unwrap_err()
        .to_string()
        .contains("cancelled"));
    assert!(writer.inner.as_ref().unwrap().inner.get_ref().is_empty());
}
