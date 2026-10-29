//! Format constructors are UI adapters, never codec implementations.
use geezipx_core::archive::{self, ArchiveReader, ArchiveWriter};
use geezipx_core::config::{CompressOptions, SevenZipOptions};
use geezipx_core::detect::{self, ArchiveFormat};
use std::fs::File;
use std::path::Path;

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
    #[cfg(feature = "zpaq")]
    (ArchiveFormat::Zpaq, "ZPAQ"),
];

pub fn detect(path: &Path) -> Result<ArchiveFormat, String> {
    let mut file = File::open(path).map_err(|e| format!("无法打开 {}：{e}", path.display()))?;
    let magic = detect::read_magic_bytes(&mut file).map_err(|e| e.to_string())?;
    let extension = detect::detect_from_extension(path);
    Ok(match (detect::detect_format(&magic), extension) {
        (Some(ArchiveFormat::Gzip), Some(ArchiveFormat::TarGz)) => ArchiveFormat::TarGz,
        (Some(ArchiveFormat::Bzip2), Some(ArchiveFormat::TarBz2)) => ArchiveFormat::TarBz2,
        (Some(ArchiveFormat::Xz), Some(ArchiveFormat::TarXz)) => ArchiveFormat::TarXz,
        (Some(ArchiveFormat::Zstd), Some(ArchiveFormat::TarZst)) => ArchiveFormat::TarZst,
        (Some(ArchiveFormat::Lz4), Some(ArchiveFormat::TarLz4)) => ArchiveFormat::TarLz4,
        (Some(format), _) | (None, Some(format)) => format,
        _ => return Err("无法识别此文件的归档格式。".into()),
    })
}

pub fn open_reader(path: &Path, password: Option<&str>) -> Result<Box<dyn ArchiveReader>, String> {
    let format = detect(path)?;
    let file = || File::open(path).map_err(|e| e.to_string());
    let mut reader: Box<dyn ArchiveReader> = match format {
        ArchiveFormat::Zip => {
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
        ArchiveFormat::Cab => Box::new(archive::cab::CabReader::new(path)),
        ArchiveFormat::Asar => Box::new(archive::asar::AsarReader::new(path)),
        ArchiveFormat::Cpio => Box::new(archive::cpio::CpioReader::new(path)),
        ArchiveFormat::Deb => Box::new(archive::deb::DebReader::new(file()?)),
        ArchiveFormat::Lzh => Box::new(archive::lzh::LzhReader::new(file()?)),
        ArchiveFormat::Iso => Box::new(archive::iso::IsoReader::new(file()?)),
        ArchiveFormat::Udf => {
            Box::new(archive::udf::UdfReader::new(path).map_err(|e| e.to_string())?)
        }
        ArchiveFormat::Arj => Box::new(archive::arj::ArjReader::new(path)),
        ArchiveFormat::Ace => Box::new(archive::ace::AceReader::new(path)),
        ArchiveFormat::Arc => Box::new(archive::arc::ArcReader::new(path)),
        #[cfg(feature = "rar")]
        ArchiveFormat::Rar => Box::new(archive::rar::RarReader::new(path)),
        #[cfg(feature = "zpaq")]
        ArchiveFormat::Zpaq => Box::new(archive::zpaq::ZpaqReader::new(path)),
        _ => {
            return Err(format!(
                "原生界面暂不支持 {format}。可使用 GeeZipX CLI 处理此格式。"
            ))
        }
    };
    if let Some(password) = password.filter(|p| !p.is_empty()) {
        if !matches!(
            format,
            ArchiveFormat::Zip | ArchiveFormat::SevenZip | ArchiveFormat::Rar
        ) {
            return Err(format!("{format} 不支持密码，请清空密码后重试。"));
        }
        reader.set_password(password).map_err(|e| e.to_string())?;
    }
    Ok(reader)
}

pub fn create_writer(
    file: File,
    format: ArchiveFormat,
    options: CompressOptions,
) -> Result<Box<dyn ArchiveWriter>, String> {
    if options.password.is_some() && !matches!(format, ArchiveFormat::Zip | ArchiveFormat::SevenZip)
    {
        return Err("只有 ZIP 和 7z 支持密码加密。".into());
    }
    Ok(match format {
        ArchiveFormat::Zip => {
            let mut writer = archive::zip::ZipWriter::new(file);
            if let Some(password) = &options.password {
                writer.set_password(password);
            }
            Box::new(writer)
        }
        ArchiveFormat::SevenZip => {
            let mut writer = archive::seven_zip::SevenZipWriter::new(
                file,
                &options,
                &SevenZipOptions::default(),
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
        ArchiveFormat::TarXz => {
            Box::new(archive::tarxz::TarXzWriter::new_with_options(file, options))
        }
        #[cfg(feature = "zpaq")]
        ArchiveFormat::Zpaq => Box::new(archive::zpaq::ZpaqWriter::new(file, options.level)),
        _ => return Err(format!("此界面不能创建 {format}。")),
    })
}
