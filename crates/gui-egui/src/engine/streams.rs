//! Single-file UI adapters; all encoding and authentication stays in core.
use super::formats::TaskFile;
use crate::job::Control;
use geezipx_core::{
    archive::{self, ArchiveReader, Entry, EntryVisitor},
    config::CompressOptions,
    detect::ArchiveFormat,
    GeeZipError, GeeZipResult, ProgressWriter,
};
use std::{
    fs::File,
    io::{self, BufReader, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

/// An anonymous file shared only by operations on one unchanged, unencrypted source.
pub type DecodedStream = Arc<Mutex<Option<File>>>;

pub fn is_stream(format: ArchiveFormat) -> bool {
    matches!(
        format,
        ArchiveFormat::Gzip
            | ArchiveFormat::Bzip2
            | ArchiveFormat::Brotli
            | ArchiveFormat::Lz4
            | ArchiveFormat::Xz
            | ArchiveFormat::Lzma
            | ArchiveFormat::Lz
            | ArchiveFormat::Zstd
            | ArchiveFormat::Z
            | ArchiveFormat::Uu
            | ArchiveFormat::Xxe
            | ArchiveFormat::Aes
    )
}

pub fn single_input(format: ArchiveFormat) -> bool {
    is_stream(format)
        || matches!(
            format,
            ArchiveFormat::Img | ArchiveFormat::Bin | ArchiveFormat::Isz
        )
}

pub fn encode<R: Read, W: Write>(
    input: &mut R,
    mut output: W,
    format: ArchiveFormat,
    options: CompressOptions,
    name: &str,
) -> GeeZipResult<u64> {
    match format {
        ArchiveFormat::Gzip => archive::gzip::gzip_compress_with_options(input, output, options),
        ArchiveFormat::Bzip2 => archive::bzip2::bzip2_compress_with_options(input, output, options),
        ArchiveFormat::Brotli => {
            archive::brotli::brotli_compress_with_options(input, output, options)
        }
        ArchiveFormat::Lz4 => archive::lz4::lz4_compress_with_options(input, output, options),
        ArchiveFormat::Xz => archive::xz::xz_compress_with_options(input, output, options),
        ArchiveFormat::Lzma => archive::xz::lzma_compress_with_options(input, output, options),
        ArchiveFormat::Lz => archive::lz::lz_compress_with_options(input, output, &options),
        ArchiveFormat::Zstd => archive::zstd::zstd_compress_with_options(input, output, options),
        ArchiveFormat::Aes => {
            archive::aes::aes_encrypt(input, output, options.password.as_deref().unwrap_or(""))
        }
        ArchiveFormat::Uu => archive::uu::uu_encode_to_writer(input, name, &mut output),
        ArchiveFormat::Xxe => archive::xxe::xxe_encode_to_writer(input, name, &mut output),
        ArchiveFormat::Img => archive::img::img_compress(input, output),
        ArchiveFormat::Bin => archive::bin::bin_compress(input, output),
        ArchiveFormat::Isz => archive::isz::isz_compress(input, output, &options),
        _ => Err(GeeZipError::format(
            "unsupported single-file output",
            format,
        )),
    }
}

pub struct StreamReader {
    path: PathBuf,
    format: ArchiveFormat,
    password: Option<String>,
    control: Control,
    decoded: DecodedStream,
}
impl StreamReader {
    pub fn new(path: &Path, format: ArchiveFormat, control: &Control) -> Self {
        Self {
            path: path.to_owned(),
            format,
            password: None,
            control: control.clone(),
            decoded: Arc::default(),
        }
    }
    pub fn with_cache(mut self, cache: DecodedStream) -> Self {
        self.decoded = cache;
        self
    }
    fn prepare(&mut self) -> GeeZipResult<()> {
        let mut decoded = self.decoded.lock().unwrap_or_else(|e| e.into_inner());
        if decoded.is_none() {
            let mut staged = tempfile::tempfile()?;
            {
                let mut output =
                    ProgressWriter::new(&mut staged).with_callback(self.control.callback(0));
                self.decode(&mut output)?;
            }
            staged.rewind()?;
            *decoded = Some(staged);
        }
        Ok(())
    }
    fn decode(&self, mut output: &mut dyn Write) -> GeeZipResult<u64> {
        let mut input = BufReader::with_capacity(
            64 * 1024,
            TaskFile::new(File::open(&self.path)?, &self.control),
        );
        match self.format {
            ArchiveFormat::Gzip => archive::gzip::gzip_decompress(&mut input, &mut output),
            ArchiveFormat::Bzip2 => archive::bzip2::bzip2_decompress(&mut input, &mut output),
            ArchiveFormat::Brotli => archive::brotli::brotli_decompress(&mut input, &mut output),
            ArchiveFormat::Lz4 => archive::lz4::lz4_decompress(&mut input, &mut output),
            ArchiveFormat::Xz => archive::xz::xz_decompress(&mut input, &mut output),
            ArchiveFormat::Lzma => archive::xz::lzma_decompress(&mut input, &mut output),
            ArchiveFormat::Lz => archive::lz::lz_decompress(&mut input, &mut output),
            ArchiveFormat::Zstd => archive::zstd::zstd_decompress(&mut input, &mut output),
            ArchiveFormat::Z => archive::unix_compress::z_decompress(&mut input, &mut output),
            ArchiveFormat::Uu => archive::uu::uu_decode_stream(input, &mut output),
            ArchiveFormat::Xxe => archive::xxe::xxe_decode_stream(input, &mut output),
            ArchiveFormat::Aes => archive::aes::aes_decrypt(
                &mut input,
                output,
                self.password.as_deref().unwrap_or(""),
            ),
            _ => Err(GeeZipError::format("unsupported stream", self.format)),
        }
    }
}
impl ArchiveReader for StreamReader {
    fn format(&self) -> ArchiveFormat {
        self.format
    }
    fn entries(&mut self) -> GeeZipResult<Vec<Entry>> {
        // Decode once to disk: streams may carry neither a name nor a length.
        // Real sizes are required by the browser and native drag descriptors.
        self.prepare()?;
        let name = self
            .path
            .file_stem()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty() && *name != "." && *name != "..")
            .unwrap_or("data");
        Ok(vec![Entry {
            path: name.to_owned(),
            size: self
                .decoded
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
                .unwrap()
                .metadata()?
                .len(),
            compressed_size: self.path.metadata()?.len(),
            crc32: None,
            modified: None,
            is_dir: false,
        }])
    }
    fn set_password(&mut self, password: &str) -> GeeZipResult<()> {
        self.password = Some(password.to_owned());
        // Password changes start a new authentication scope, without shared plaintext.
        self.decoded = Arc::default();
        Ok(())
    }
    fn extract(&mut self, entry: &Entry, output: &mut dyn Write) -> GeeZipResult<u64> {
        if self.entries()?[0].path != entry.path {
            return Err(GeeZipError::format(
                "stream entry no longer exists",
                self.format,
            ));
        }
        let mut decoded = self.decoded.lock().unwrap_or_else(|e| e.into_inner());
        let file = decoded.as_mut().unwrap();
        file.rewind()?;
        io::copy(file, output).map_err(|e| GeeZipError::io(e, "reading decoded stream"))
    }
    fn visit_entries(
        &mut self,
        visitor: &mut EntryVisitor<'_>,
        is_cancelled: &dyn Fn() -> bool,
    ) -> GeeZipResult<()> {
        if is_cancelled() {
            return Err(GeeZipError::Cancelled);
        }
        let entry = self.entries()?.remove(0);
        if is_cancelled() {
            return Err(GeeZipError::Cancelled);
        }
        let mut decoded = self.decoded.lock().unwrap_or_else(|e| e.into_inner());
        let file = decoded.as_mut().unwrap();
        file.seek(SeekFrom::Start(0))?;
        visitor(&entry, file)?;
        Ok(())
    }
}
