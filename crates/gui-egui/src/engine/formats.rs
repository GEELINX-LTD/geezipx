//! Format constructors are UI adapters, never codec implementations.
use super::streams;
use crate::job::Control;
use geezipx_core::archive::{self, lzh::LzhCompressionMethod, ArchiveReader, ArchiveWriter};
use geezipx_core::config::{CompressOptions, SevenZipOptions};
use geezipx_core::detect::{self, ArchiveFormat};
use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
};

pub const WRITE_FORMATS: &[(ArchiveFormat, &str)] = &[
    (ArchiveFormat::Zip, "ZIP"),
    (ArchiveFormat::SevenZip, "7z"),
    (ArchiveFormat::TarGz, "TAR.GZ"),
    (ArchiveFormat::TarZst, "TAR.ZST"),
    (ArchiveFormat::TarXz, "TAR.XZ"),
    (ArchiveFormat::TarBz2, "TAR.BZ2"),
    (ArchiveFormat::TarBr, "TAR.BR"),
    (ArchiveFormat::TarLz4, "TAR.LZ4"),
    (ArchiveFormat::Tar, "TAR"),
    (ArchiveFormat::Zipx, "ZIPX (XZ)"),
    (ArchiveFormat::Cab, "CAB"),
    (ArchiveFormat::Asar, "ASAR"),
    (ArchiveFormat::Deb, "DEB"),
    (ArchiveFormat::Lzh, "LZH / LHA"),
    (ArchiveFormat::Cpio, "CPIO (newc)"),
    (ArchiveFormat::Iso, "ISO"),
    (ArchiveFormat::Udf, "UDF"),
    #[cfg(feature = "wim")]
    (ArchiveFormat::Wim, "WIM (无压缩)"),
    #[cfg(feature = "zpaq")]
    (ArchiveFormat::Zpaq, "ZPAQ"),
    (ArchiveFormat::Gzip, "GZIP (.gz)"),
    (ArchiveFormat::Bzip2, "BZIP2 (.bz2)"),
    (ArchiveFormat::Brotli, "Brotli (.br)"),
    (ArchiveFormat::Lz4, "LZ4"),
    (ArchiveFormat::Xz, "XZ"),
    (ArchiveFormat::Lzma, "LZMA"),
    (ArchiveFormat::Lz, "Lzip (.lz)"),
    (ArchiveFormat::Zstd, "Zstandard (.zst)"),
    (ArchiveFormat::Uu, "UU / UUE"),
    (ArchiveFormat::Xxe, "XXE"),
    (ArchiveFormat::Aes, "AES 加密容器 (.enc)"),
    (ArchiveFormat::Img, "IMG / IMA (原样复制)"),
    (ArchiveFormat::Bin, "BIN (原样复制)"),
    (ArchiveFormat::Isz, "ISZ (压缩镜像)"),
];

pub fn extension(format: ArchiveFormat) -> String {
    match format {
        ArchiveFormat::Gzip => "gz".into(),
        ArchiveFormat::Bzip2 => "bz2".into(),
        ArchiveFormat::Brotli => "br".into(),
        ArchiveFormat::Zstd => "zst".into(),
        ArchiveFormat::Aes => "enc".into(),
        _ => format.to_string(),
    }
}
pub fn write_password(format: ArchiveFormat) -> bool {
    matches!(
        format,
        ArchiveFormat::Zip | ArchiveFormat::Zipx | ArchiveFormat::SevenZip | ArchiveFormat::Aes
    )
}
pub fn adjustable(format: ArchiveFormat) -> bool {
    matches!(
        format,
        ArchiveFormat::Zipx
            | ArchiveFormat::SevenZip
            | ArchiveFormat::Gzip
            | ArchiveFormat::Bzip2
            | ArchiveFormat::Brotli
            | ArchiveFormat::Xz
            | ArchiveFormat::Lzma
            | ArchiveFormat::Lz
            | ArchiveFormat::Zstd
            | ArchiveFormat::TarGz
            | ArchiveFormat::TarBz2
            | ArchiveFormat::TarBr
            | ArchiveFormat::TarXz
            | ArchiveFormat::TarZst
            | ArchiveFormat::Lzh
            | ArchiveFormat::Zpaq
    )
}
pub fn preset_level(format: ArchiveFormat, preset: usize) -> Option<u32> {
    if !adjustable(format) || preset == 1 {
        return None;
    }
    Some(match (format, preset) {
        (ArchiveFormat::Zpaq | ArchiveFormat::Lzh, 0) => 1,
        (ArchiveFormat::Zpaq, _) => 5,
        (ArchiveFormat::Lzh, _) => 4,
        (ArchiveFormat::Zstd | ArchiveFormat::TarZst, 0) => 1,
        (ArchiveFormat::Zstd | ArchiveFormat::TarZst, _) => 19,
        (ArchiveFormat::Brotli | ArchiveFormat::TarBr, 0) => 1,
        (ArchiveFormat::Brotli | ArchiveFormat::TarBr, _) => 11,
        (_, 0) => 1,
        _ => 9,
    })
}

pub fn detect(path: &Path) -> Result<ArchiveFormat, String> {
    let mut file = File::open(path).map_err(|e| format!("无法打开 {}：{e}", path.display()))?;
    let magic = detect::detect_seekable_format(&mut file).map_err(|e| e.to_string())?;
    let extension = detect::detect_from_extension(path);
    Ok(match (magic, extension) {
        (Some(ArchiveFormat::Zip), Some(ArchiveFormat::Zipx)) => ArchiveFormat::Zipx,
        (Some(ArchiveFormat::Gzip), Some(ArchiveFormat::TarGz)) => ArchiveFormat::TarGz,
        (Some(ArchiveFormat::Bzip2), Some(ArchiveFormat::TarBz2)) => ArchiveFormat::TarBz2,
        (Some(ArchiveFormat::Xz), Some(ArchiveFormat::TarXz)) => ArchiveFormat::TarXz,
        (Some(ArchiveFormat::Zstd), Some(ArchiveFormat::TarZst)) => ArchiveFormat::TarZst,
        (Some(ArchiveFormat::Lz4), Some(ArchiveFormat::TarLz4)) => ArchiveFormat::TarLz4,
        (Some(ArchiveFormat::Lz | ArchiveFormat::Lzma), Some(ArchiveFormat::TarLz)) => {
            ArchiveFormat::TarLz
        }
        (Some(format), _) | (None, Some(format)) => format,
        _ => return Err("无法识别此文件的归档格式。".into()),
    })
}

pub fn open_reader(
    path: &Path,
    password: Option<&str>,
    control: &Control,
) -> Result<Box<dyn ArchiveReader>, String> {
    let result = build_reader(path, password, control);
    control.check()?;
    result
}

fn build_reader(
    path: &Path,
    password: Option<&str>,
    control: &Control,
) -> Result<Box<dyn ArchiveReader>, String> {
    let format = detect(path)?;
    let file = || {
        File::open(path)
            .map(|f| TaskFile::new(f, control))
            .map_err(|e| e.to_string())
    };
    let mut reader: Box<dyn ArchiveReader> = match format {
        ArchiveFormat::Zip | ArchiveFormat::Zipx => {
            Box::new(archive::zip::ZipReader::new(file()?).map_err(|e| e.to_string())?)
        }
        ArchiveFormat::SevenZip => Box::new(archive::seven_zip::SevenZipReader::new(path)),
        ArchiveFormat::Tar => Box::new(archive::tar::TarReader::new(file()?)),
        ArchiveFormat::TarGz => Box::new(archive::targz::TarGzReader::new(file()?)),
        ArchiveFormat::TarBz2 => Box::new(archive::tarbz2::TarBz2Reader::new(file()?)),
        ArchiveFormat::TarBr => Box::new(archive::tarbr::TarBrReader::new(file()?)),
        ArchiveFormat::TarLz4 => Box::new(archive::tarlz4::TarLz4Reader::new(file()?)),
        ArchiveFormat::TarZst => Box::new(archive::tarzst::TarZstReader::new(file()?)),
        ArchiveFormat::TarXz => Box::new(archive::tarxz::TarXzReader::new(file()?)),
        ArchiveFormat::TarLz => Box::new(archive::tarlz::TarLzReader::new(file()?)),
        ArchiveFormat::Cab => Box::new(archive::cab::CabReader::new(path)),
        ArchiveFormat::Asar => Box::new(archive::asar::AsarReader::new(path)),
        ArchiveFormat::Cpio => Box::new(archive::cpio::CpioReader::new(path)),
        ArchiveFormat::Deb => Box::new(archive::deb::DebReader::new(file()?)),
        ArchiveFormat::Lzh => Box::new(archive::lzh::LzhReader::new(file()?)),
        ArchiveFormat::Iso => Box::new(archive::iso::IsoReader::new(file()?)),
        ArchiveFormat::Udf => {
            Box::new(archive::udf::UdfReader::new(path).map_err(|e| e.to_string())?)
        }
        ArchiveFormat::Img | ArchiveFormat::Bin => Box::new(
            archive::disk_image::DiskImageReader::open(path, format).map_err(|e| e.to_string())?,
        ),
        ArchiveFormat::Isz => Box::new(archive::iso::IsoReader::new(
            archive::isz::IszImage::new(io::BufReader::new(file()?)).map_err(|e| e.to_string())?,
        )),
        ArchiveFormat::Arj => Box::new(archive::arj::ArjReader::new(path)),
        ArchiveFormat::Ace => Box::new(archive::ace::AceReader::new(path)),
        ArchiveFormat::Arc => Box::new(archive::arc::ArcReader::new(path)),
        ArchiveFormat::Alz => Box::new(
            archive::alz::AlzReader::open(path, password.map(str::to_owned))
                .map_err(|e| e.to_string())?,
        ),
        ArchiveFormat::Compound => {
            Box::new(archive::compound::CompoundReader::open(path).map_err(|e| e.to_string())?)
        }
        ArchiveFormat::Egg => {
            Box::new(archive::egg::EggReader::open(path).map_err(|e| e.to_string())?)
        }
        ArchiveFormat::Pea => {
            Box::new(archive::pea::PeaReader::new(file()?).map_err(|e| e.to_string())?)
        }
        ArchiveFormat::Nsis => {
            Box::new(archive::nsis::NsisReader::new(file()?).map_err(|e| e.to_string())?)
        }
        #[cfg(feature = "rar")]
        ArchiveFormat::Rar => Box::new(archive::rar::RarReader::new(path)),
        #[cfg(feature = "zpaq")]
        ArchiveFormat::Zpaq => Box::new(archive::zpaq::ZpaqReader::new(path)),
        #[cfg(feature = "wim")]
        ArchiveFormat::Wim => {
            Box::new(archive::wim::WimReader::open(path).map_err(|e| e.to_string())?)
        }
        stream if streams::is_stream(stream) => {
            Box::new(streams::StreamReader::new(path, stream, control))
        }
        _ => return Err(format!("此构建未启用 {format} 支持。")),
    };
    if let Some(password) = password.filter(|p| !p.is_empty()) {
        if !matches!(
            format,
            ArchiveFormat::Zip
                | ArchiveFormat::Zipx
                | ArchiveFormat::SevenZip
                | ArchiveFormat::Rar
                | ArchiveFormat::Alz
                | ArchiveFormat::Aes
                | ArchiveFormat::Arj
                | ArchiveFormat::Ace
                | ArchiveFormat::Arc
        ) {
            return Err(format!("{format} 不支持密码，请清空密码后重试。"));
        }
        if format != ArchiveFormat::Alz {
            reader.set_password(password).map_err(|e| e.to_string())?;
        }
    }
    Ok(reader)
}

pub fn create_writer(
    file: File,
    format: ArchiveFormat,
    options: CompressOptions,
    control: &Control,
) -> Result<Box<dyn ArchiveWriter>, String> {
    if options.password.is_some() && !write_password(format) {
        return Err(format!("{format} 不支持密码加密。"));
    }
    let file = TaskFile::new(file, control);
    Ok(match format {
        ArchiveFormat::Zip | ArchiveFormat::Zipx => {
            let mut writer = if format == ArchiveFormat::Zipx {
                archive::zip::ZipWriter::new_xz(file)
            } else {
                archive::zip::ZipWriter::new(file)
            };
            if let Some(level) = options.level {
                writer.set_level(level).map_err(|e| e.to_string())?;
            }
            if let Some(password) = &options.password {
                writer.set_password(password);
            }
            Box::new(writer)
        }
        ArchiveFormat::SevenZip => {
            let mut writer = archive::seven_zip::SevenZipWriter::new(
                file,
                &options,
                options
                    .seven_zip
                    .as_ref()
                    .unwrap_or(&SevenZipOptions::default()),
            )
            .map_err(|e| e.to_string())?;
            if let Some(password) = &options.password {
                writer.set_password(password).map_err(|e| e.to_string())?;
            }
            Box::new(writer)
        }
        ArchiveFormat::Tar => Box::new(archive::tar::TarWriter::new(file)),
        ArchiveFormat::TarGz => {
            Box::new(archive::targz::TarGzWriter::new_with_options(file, options))
        }
        ArchiveFormat::TarBz2 => Box::new(archive::tarbz2::TarBz2Writer::new_with_options(
            file, options,
        )),
        ArchiveFormat::TarBr => Box::new(
            archive::tarbr::TarBrWriter::new_with_options(file, options)
                .map_err(|e| e.to_string())?,
        ),
        ArchiveFormat::TarLz4 => Box::new(
            archive::tarlz4::TarLz4Writer::new_with_options(file, options)
                .map_err(|e| e.to_string())?,
        ),
        ArchiveFormat::TarZst => Box::new(archive::tarzst::TarZstWriter::new_with_options(
            file, options,
        )),
        ArchiveFormat::TarXz => Box::new(
            archive::tarxz::TarXzWriter::try_new_with_options(file, options)
                .map_err(|e| e.to_string())?,
        ),
        ArchiveFormat::Cab => Box::new(archive::cab::CabWriter::new(file)),
        ArchiveFormat::Asar => Box::new(archive::asar::AsarWriter::new(file)),
        ArchiveFormat::Deb => Box::new(archive::deb::DebWriter::new(file)),
        ArchiveFormat::Lzh => Box::new(archive::lzh::LzhWriter::new(
            file,
            match options.level {
                Some(0) => LzhCompressionMethod::Store,
                Some(1) => LzhCompressionMethod::Lh4,
                Some(2) => LzhCompressionMethod::Lh5,
                Some(3) => LzhCompressionMethod::Lh6,
                Some(_) => LzhCompressionMethod::Lh7,
                None => LzhCompressionMethod::default(),
            },
        )),
        ArchiveFormat::Cpio => Box::new(archive::cpio::CpioWriter::new(file)),
        ArchiveFormat::Iso => Box::new(archive::iso::IsoWriter::new(file)),
        ArchiveFormat::Udf => Box::new(archive::udf::UdfWriter::new(file)),
        #[cfg(feature = "wim")]
        ArchiveFormat::Wim => Box::new(archive::wim::WimWriter::new(file)),
        #[cfg(feature = "zpaq")]
        ArchiveFormat::Zpaq => Box::new(archive::zpaq::ZpaqWriter::new(file, options.level)),
        _ => return Err(format!("此界面不能创建 {format}。")),
    })
}

/// Check cancellation even while a core adapter is staging or finalizing.
pub(super) struct TaskFile {
    file: File,
    control: Control,
}
impl TaskFile {
    pub fn new(file: File, control: &Control) -> Self {
        Self {
            file,
            control: control.clone(),
        }
    }
    fn check(&self) -> io::Result<()> {
        if self.control.is_cancelled() {
            Err(io::Error::other(geezipx_core::GeeZipError::Cancelled))
        } else {
            Ok(())
        }
    }
}
impl Read for TaskFile {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.check()?;
        self.file.read(bytes)
    }
}
impl Write for TaskFile {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.check()?;
        self.file.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.check()?;
        self.file.flush()
    }
}
impl Seek for TaskFile {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.check()?;
        self.file.seek(position)
    }
}
