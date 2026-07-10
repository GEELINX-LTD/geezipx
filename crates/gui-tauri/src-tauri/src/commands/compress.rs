//! `compress_archive` command — create an archive from local files.
//!
//! Supported archive container formats: zip, zipx, tar, 7z, tar.gz, tar.bz2, tar.br,
//! tar.lz4, tar.zst, tar.xz, and lzh/lha (store-only writer MVP). Read-only
//! formats such as cpio are rejected for creation, and single-stream formats
//! are intentionally rejected for the current GUI MVP.
//!
//! All heavy work runs on `tokio::task::spawn_blocking` so the Tauri event loop
//! is never blocked. Progress is emitted as `task:progress` events.
use crate::commands::progress::{is_cancelled_error, TaskKind, TaskProgressEmitter, TaskStage};
use crate::state::AppState;
use geezipx_core::archive::iso::IsoWriter;
use geezipx_core::archive::lzh::{LzhCompressionMethod, LzhWriter};
use geezipx_core::archive::seven_zip::SevenZipWriter;
use geezipx_core::archive::tar::TarWriter;
use geezipx_core::archive::tarbr::TarBrWriter;
use geezipx_core::archive::tarbz2::TarBz2Writer;
use geezipx_core::archive::targz::TarGzWriter;
use geezipx_core::archive::tarlz4::TarLz4Writer;
use geezipx_core::archive::tarxz::TarXzWriter;
use geezipx_core::archive::tarzst::TarZstWriter;
use geezipx_core::archive::zip::ZipWriter;
use geezipx_core::archive::ArchiveWriter;
use geezipx_core::config::{CompressOptions, SevenZipOptions};
use geezipx_core::detect::ArchiveFormat;
use geezipx_core::ProgressReader;
use serde::Serialize;
use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::AppHandle;
use tokio::task::spawn_blocking;
const CANCELLED_MESSAGE: &str = "Operation cancelled by user";
// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------
/// Serializable result of a compression operation.
#[derive(Debug, Serialize)]
pub struct CompressArchiveResult {
    /// Number of regular files added to the archive.
    pub files_added: u64,
    /// Number of directory entries added.
    pub directories_added: u64,
    /// Total compressed bytes written to the output file.
    pub bytes_written: u64,
    /// Absolute path to the created output file.
    pub output_path: String,
    /// The format used (e.g. "zip", "tar.gz").
    pub format: String,
    /// Number of items skipped (e.g. symlinks, special files).
    pub skipped: u64,
}
/// A collected file entry ready to be added to an archive.
struct FileEntry {
    /// Real path on the filesystem.
    real_path: PathBuf,
    /// Relative path inside the archive.
    archive_path: PathBuf,
    /// Whether this entry is a directory (not a regular file).
    is_dir: bool,
    /// Input size in bytes for progress accounting (0 for directories).
    size: u64,
}
// ---------------------------------------------------------------------------
// Tauri command
// ---------------------------------------------------------------------------
/// Create an archive from a list of source paths.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn compress_archive(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    source_paths: Vec<String>,
    output_path: String,
    format: String,
    level: Option<u32>,
    jobs: Option<u32>,
    password: Option<String>,
    recursive: bool,
    task_id: Option<String>,
) -> Result<CompressArchiveResult, String> {
    if source_paths.is_empty() {
        return Err("At least one source path is required".to_string());
    }
    let af = parse_gui_compress_format(&format)?;
    if password.is_some() && af != ArchiveFormat::Zip && af != ArchiveFormat::SevenZip {
        return Err(format!(
            "Password is only supported for ZIP and 7z formats; '{af}' does not support encryption when writing"
        ));
    }
    let tid = task_id.unwrap_or_else(|| {
        format!(
            "compress-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        )
    });
    let output = PathBuf::from(&output_path);
    let sources: Vec<PathBuf> = source_paths.iter().map(PathBuf::from).collect();
    let pwd = password;
    let cancel_token = {
        let mut tokens = state
            .cancel_tokens
            .lock()
            .map_err(|e| format!("Internal error: {e}"))?;
        let token = Arc::new(AtomicBool::new(false));
        tokens.insert(tid.clone(), token.clone());
        token
    };
    let emitter = TaskProgressEmitter::new(app, tid.clone(), TaskKind::Compress);
    emitter.emit_started("Scanning input files...");
    let result = {
        let emitter = emitter.clone();
        let cancel_token = cancel_token.clone();
        let out = output.clone();
        let recursive = recursive;
        spawn_blocking(move || {
            let task_result = (|| -> Result<CompressArchiveResult, String> {
                let out_dir = match out.parent() {
                    Some(path) if !path.as_os_str().is_empty() => path,
                    _ => Path::new("."),
                };
                if let Ok(canonical_out) = fs::canonicalize(out_dir) {
                    for src in &sources {
                        if let Ok(real_src) = fs::canonicalize(src) {
                            if real_src.is_dir() && canonical_out.starts_with(&real_src) {
                                return Err(
                                    "Output file cannot be located inside a source directory"
                                        .to_string(),
                                );
                            }
                        }
                    }
                }
                let mut skipped = 0u64;
                let entries = collect_gui_inputs(&sources, &cancel_token, &mut skipped, recursive)?;
                let total_input_bytes = entries
                    .iter()
                    .filter(|entry| !entry.is_dir)
                    .map(|entry| entry.size)
                    .sum::<u64>();
                let total_entries = entries.len() as u64;
                emitter.set_totals(Some(total_input_bytes), Some(total_entries));
                emitter.emit_progress(TaskStage::Compressing, None, 0, None, 0, true);
                let output_file = fs::File::create(&out)
                    .map_err(|e| format!("Cannot create output '{}': {}", out.display(), e))?;
                let options = CompressOptions {
                    level,
                    jobs,
                    password: pwd,
                };
                let mut writer: Box<dyn ArchiveWriter> =
                    create_gui_writer(output_file, af, options)?;
                let mut added_files = 0u64;
                let mut added_dirs = 0u64;
                let mut progress_bytes = 0u64;
                let mut completed_entries = 0u64;
                for entry in &entries {
                    if cancel_token.load(Ordering::SeqCst) {
                        return Err(CANCELLED_MESSAGE.to_string());
                    }
                    let entry_name = entry.archive_path.to_string_lossy().to_string();
                    if entry.is_dir {
                        writer.add_directory(&entry.archive_path).map_err(|e| {
                            format!(
                                "Failed to add directory '{}': {e}",
                                entry.archive_path.display()
                            )
                        })?;
                        added_dirs += 1;
                        completed_entries += 1;
                        emitter.emit_progress(
                            TaskStage::Compressing,
                            None,
                            progress_bytes,
                            Some(&entry_name),
                            completed_entries,
                            true,
                        );
                        continue;
                    }
                    let file = match fs::File::open(&entry.real_path) {
                        Ok(file) => file,
                        Err(_) => {
                            skipped += 1;
                            progress_bytes = progress_bytes.saturating_add(entry.size);
                            completed_entries += 1;
                            emitter.emit_progress(
                                TaskStage::Compressing,
                                None,
                                progress_bytes,
                                Some(&entry_name),
                                completed_entries,
                                true,
                            );
                            continue;
                        }
                    };
                    let callback = emitter.reader_callback(
                        cancel_token.clone(),
                        TaskStage::Compressing,
                        progress_bytes,
                        entry_name.clone(),
                        completed_entries,
                    );
                    let mut reader = ProgressReader::new(BufReader::new(file))
                        .with_total(entry.size)
                        .with_callback(Box::new(callback));
                    match writer.add_entry_from_reader(&entry.archive_path, &mut reader) {
                        Ok(()) => {
                            added_files += 1;
                            progress_bytes = progress_bytes.saturating_add(entry.size);
                            completed_entries += 1;
                            emitter.emit_progress(
                                TaskStage::Compressing,
                                None,
                                progress_bytes,
                                Some(&entry_name),
                                completed_entries,
                                true,
                            );
                        }
                        Err(err) => {
                            if cancel_token.load(Ordering::SeqCst) || is_cancelled_error(&err) {
                                return Err(CANCELLED_MESSAGE.to_string());
                            }
                            return Err(format!(
                                "Failed to add '{}': {err}",
                                entry.archive_path.display()
                            ));
                        }
                    }
                }
                emitter.emit_finalizing(progress_bytes, completed_entries);
                let bytes_written = writer
                    .finish()
                    .map_err(|e| format!("Failed to finalize archive: {e}"))?;
                emitter.emit_finished(progress_bytes, completed_entries);
                Ok(CompressArchiveResult {
                    files_added: added_files,
                    directories_added: added_dirs,
                    bytes_written,
                    output_path: out.to_string_lossy().to_string(),
                    format: af.to_string(),
                    skipped,
                })
            })();
            match task_result {
                Ok(result) => Ok(result),
                Err(message) => {
                    let _ = fs::remove_file(&out);
                    let (current, completed_entries) = emitter.latest_snapshot();
                    if cancel_token.load(Ordering::SeqCst)
                        || message.to_ascii_lowercase().contains("cancelled")
                    {
                        emitter.emit_cancelled(current, completed_entries);
                    } else {
                        emitter.emit_failed(current, completed_entries, message.clone());
                    }
                    Err(message)
                }
            }
        })
        .await
    };
    let mut tokens = state
        .cancel_tokens
        .lock()
        .map_err(|e| format!("Internal error: {e}"))?;
    tokens.remove(&tid);
    drop(tokens);
    result.map_err(|e| format!("Internal error: {e}"))?
}
// ---------------------------------------------------------------------------
// Helpers

// ---------------------------------------------------------------------------
/// Parse a format string accepted by the GUI compress command.
fn parse_gui_compress_format(s: &str) -> Result<ArchiveFormat, String> {
    match s.to_ascii_lowercase().as_str() {
        "zip" | "zipx" => Ok(ArchiveFormat::Zip),
        "tar" => Ok(ArchiveFormat::Tar),
        "tar.gz" | "tgz" => Ok(ArchiveFormat::TarGz),
        "tar.bz2" | "tbz" | "tbz2" => Ok(ArchiveFormat::TarBz2),
        "tar.br" => Ok(ArchiveFormat::TarBr),
        "tar.lz4" => Ok(ArchiveFormat::TarLz4),
        "tar.zst" | "tzst" => Ok(ArchiveFormat::TarZst),
        "tar.xz" | "txz" => Ok(ArchiveFormat::TarXz),
        "gz" | "gzip" => Err(
            "'gzip' is a single-stream compression format; \
             single-stream compression is not yet supported in the GUI \
             (will be added in a later update)"
                .to_string(),
        ),
        "bz2" | "bzip2" => Err(
            "'bzip2' is a single-stream compression format; \
             use tar.bz2/tbz/tbz2 in the GUI, or use the CLI for standalone .bz2 compression"
                .to_string(),
        ),
        "br" | "brotli" => Err(
            "'brotli' is a single-stream compression format; \
             use tar.br in the GUI, or use the CLI for standalone .br compression"
                .to_string(),
        ),
        "lz4" => Err(
            "'lz4' is a single-stream compression format; \
             use tar.lz4 in the GUI, or use the CLI for standalone .lz4 compression"
                .to_string(),
        ),
        "zst" | "zstd" => Err(
            "'zstd' is a single-stream compression format; \
             single-stream compression is not yet supported in the GUI \
             (will be added in a later update)"
                .to_string(),
        ),
        "xz" => Err(
            "'xz' is a single-stream compression format; \
             single-stream compression is not yet supported in the GUI \
             (will be added in a later update)"
                .to_string(),
        ),
        "lzma" => Err(
            "'lzma' is a single-stream compression format; \
             single-stream compression is not yet supported in the GUI \
             (will be added in a later update)"
                .to_string(),
        ),
        "7z" => Ok(ArchiveFormat::SevenZip),
        "lzh" | "lha" => Ok(ArchiveFormat::Lzh),
        "iso" => Ok(ArchiveFormat::Iso),
        "zpaq" => Ok(ArchiveFormat::Zpaq),
        "wim" | "swm" => Err(
            "wim writing is not supported; use list, test, or decompress for read-only wim support"
                .to_string(),
        ),
        "rar" => Err(
            "rar writing is not supported; use list, test, or decompress for read-only rar support"
                .to_string(),
        ),
        "cab" => Err(
            "cab writing is not supported; use list, test, or decompress for read-only cab support"
                .to_string()
        ),
        "cpio" => Err(
            "cpio writing is not supported; use list, test, or decompress for read-only cpio support"
                .to_string()
        ),
        other => Err(format!(
            "Unsupported format '{other}'; expected: zip, zipx, tar, tar.gz, tgz, tar.bz2, tbz, tbz2, tar.br, tar.lz4, tar.zst, tzst, tar.xz, txz, 7z, lzh, lha, iso, zpaq, wim, swm, rar, cab, cpio"
        )),
    }
}
/// Collect source paths into `FileEntry` items, recursing into directories
/// when `recursive` is true. When `recursive` is false, a directory source only
/// contributes its immediate files; subdirectories are skipped entirely.
fn collect_gui_inputs(
    sources: &[PathBuf],
    cancel_token: &Arc<AtomicBool>,
    skipped: &mut u64,
    recursive: bool,
) -> Result<Vec<FileEntry>, String> {
    let mut entries = Vec::new();
    for source in sources {
        if cancel_token.load(Ordering::SeqCst) {
            return Err(CANCELLED_MESSAGE.to_string());
        }
        let meta = fs::symlink_metadata(source)
            .map_err(|e| format!("Cannot inspect path '{}': {}", source.display(), e))?;
        if meta.file_type().is_symlink() {
            *skipped += 1;
            continue;
        }
        let canonical = fs::canonicalize(source)
            .map_err(|e| format!("Cannot resolve path '{}': {}", source.display(), e))?;
        if meta.is_dir() {
            let dir_name = canonical
                .file_name()
                .unwrap_or(canonical.as_os_str())
                .to_os_string();
            let prefix = PathBuf::from(&dir_name);
            entries.push(FileEntry {
                real_path: canonical.clone(),
                archive_path: prefix.clone(),
                is_dir: true,
                size: 0,
            });
            collect_dir_contents(
                &canonical,
                &prefix,
                &mut entries,
                cancel_token,
                skipped,
                recursive,
            )?;
        } else if meta.is_file() {
            let name = canonical
                .file_name()
                .unwrap_or(canonical.as_os_str())
                .to_os_string();
            entries.push(FileEntry {
                real_path: canonical,
                archive_path: PathBuf::from(name),
                is_dir: false,
                size: meta.len(),
            });
        } else {
            *skipped += 1;
        }
    }
    Ok(entries)
}
/// Walk `dir` and append entries, prepending `prefix`.
///
/// When `recursive` is false, subdirectories are not descended into (and are
/// not emitted as empty directory entries either); only immediate files are
/// collected. When `recursive` is true, the walk descends fully.
fn collect_dir_contents(
    dir: &Path,
    prefix: &Path,
    entries: &mut Vec<FileEntry>,
    cancel_token: &Arc<AtomicBool>,
    skipped: &mut u64,
    recursive: bool,
) -> Result<(), String> {
    let read_dir = fs::read_dir(dir)
        .map_err(|e| format!("Cannot read directory '{}': {}", dir.display(), e))?;
    for entry in read_dir {
        if cancel_token.load(Ordering::SeqCst) {
            return Err(CANCELLED_MESSAGE.to_string());
        }
        let entry = entry.map_err(|e| format!("Error reading directory entry: {e}"))?;
        let path = entry.path();
        let relative = prefix.join(entry.file_name());
        let meta = match fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(_) => {
                *skipped += 1;
                continue;
            }
        };
        if meta.file_type().is_symlink() {
            *skipped += 1;
            continue;
        }
        if meta.is_dir() {
            if recursive {
                entries.push(FileEntry {
                    real_path: path.clone(),
                    archive_path: relative.clone(),
                    is_dir: true,
                    size: 0,
                });
                collect_dir_contents(&path, &relative, entries, cancel_token, skipped, recursive)?;
            }
            // When `recursive` is false, skip subdirectories entirely.
        } else if meta.is_file() {
            entries.push(FileEntry {
                real_path: path,
                archive_path: relative,
                is_dir: false,
                size: meta.len(),
            });
        } else {
            *skipped += 1;
        }
    }
    Ok(())
}
/// Create an `ArchiveWriter` for the given format and output file.
fn create_gui_writer(
    file: fs::File,
    format: ArchiveFormat,
    options: CompressOptions,
) -> Result<Box<dyn ArchiveWriter>, String> {
    if options.password.is_some()
        && format != ArchiveFormat::Zip
        && format != ArchiveFormat::SevenZip
    {
        return Err(format!(
            "Password is only supported for ZIP and 7z formats; '{format}' does not support encryption when writing"
        ));
    }
    match format {
        ArchiveFormat::Zip => {
            let mut writer = ZipWriter::new(file);
            if let Some(ref pwd) = options.password {
                writer.set_password(pwd);
            }
            Ok(Box::new(writer))
        }
        ArchiveFormat::Tar => Ok(Box::new(TarWriter::new(file))),
        ArchiveFormat::SevenZip => {
            let sz_opts = options
                .seven_zip
                .as_ref()
                .unwrap_or(&SevenZipOptions::default());
            let mut writer =
                SevenZipWriter::new(file, &options, sz_opts).map_err(|e| e.to_string())?;
            if let Some(ref pwd) = options.password {
                writer.set_password(pwd).map_err(|e| e.to_string())?;
            }
            Ok(Box::new(writer))
        },
        ArchiveFormat::TarGz => Ok(Box::new(TarGzWriter::new_with_options(file, options))),
        ArchiveFormat::TarBz2 => Ok(Box::new(TarBz2Writer::new_with_options(file, options))),
        ArchiveFormat::TarBr => Ok(Box::new(
            TarBrWriter::new_with_options(file, options).map_err(|e| e.to_string())?,
        )),
        ArchiveFormat::TarLz4 => Ok(Box::new(
            TarLz4Writer::new_with_options(file, options).map_err(|e| e.to_string())?,
        )),
        ArchiveFormat::TarZst => Ok(Box::new(TarZstWriter::new_with_options(file, options))),
        ArchiveFormat::TarXz => Ok(Box::new(TarXzWriter::new_with_options(file, options))),
        ArchiveFormat::Gzip
        | ArchiveFormat::Bzip2
        | ArchiveFormat::Brotli
        | ArchiveFormat::Lz4
        | ArchiveFormat::Zstd
        | ArchiveFormat::Xz
        | ArchiveFormat::Lzma => Err(format!(
            "'{format}' is a single-stream compression format; \
                 single-stream compression is not yet supported in the GUI \
                 (will be added in a later update)"
        )),
        ArchiveFormat::Lzh => {
            let method = match options.level {
                Some(0) => LzhCompressionMethod::Store,
                Some(1) => LzhCompressionMethod::Lh4,
                Some(2) => LzhCompressionMethod::Lh5,
                Some(3) => LzhCompressionMethod::Lh6,
                Some(_) => LzhCompressionMethod::Lh7,
                None => LzhCompressionMethod::Lh5,
            };
            Ok(Box::new(LzhWriter::new(file, method)))
        },
        ArchiveFormat::Iso => Ok(Box::new(IsoWriter::new(file))),
        ArchiveFormat::Zpaq => {
            #[cfg(feature = "zpaq")]
            {
                Ok(Box::new(geezipx_core::archive::zpaq::ZpaqWriter::new(
                    file,
                    options.level,
                )))
            }
            #[cfg(not(feature = "zpaq"))]
            {
                Err("'zpaq' support is disabled in this build; rebuild with --features zpaq".to_string())
            }
        }
        ArchiveFormat::Cab => Err(
            "cab writing is not supported; use list, test, or decompress for read-only cab support"
                .to_string(),
        ),
        ArchiveFormat::Cpio => Err(
            "cpio writing is not supported; use list, test, or decompress for read-only cpio support"
                .to_string(),
        ),
        _ => Err(format!("Unsupported format for writing in GUI: {format}")),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use geezipx_core::archive::lzh::LzhReader;
    use geezipx_core::archive::seven_zip::SevenZipReader;
    use geezipx_core::archive::ArchiveReader;
    #[test]
    fn parse_gui_compress_format_accepts_tarbz2_variants() {
        assert_eq!(
            parse_gui_compress_format("tar.bz2").unwrap(),
            ArchiveFormat::TarBz2
        );
        assert_eq!(
            parse_gui_compress_format("tbz").unwrap(),
            ArchiveFormat::TarBz2
        );
        assert_eq!(
            parse_gui_compress_format("tbz2").unwrap(),
            ArchiveFormat::TarBz2
        );
    }
    #[test]
    fn parse_gui_compress_format_accepts_zipx_alias() {
        assert_eq!(
            parse_gui_compress_format("zipx").unwrap(),
            ArchiveFormat::Zip
        );
    }
    #[test]
    fn parse_gui_compress_format_rejects_single_stream_bzip2() {
        let err = parse_gui_compress_format("bz2").unwrap_err();
        assert!(err.contains("single-stream"));
        assert!(err.contains("tar.bz2"));
        let err2 = parse_gui_compress_format("bzip2").unwrap_err();
        assert!(err2.contains("single-stream"));
        assert!(err2.contains("tar.bz2"));
    }
    #[test]
    fn parse_gui_compress_format_accepts_tarbr_and_tarlz4() {
        assert_eq!(
            parse_gui_compress_format("tar.br").unwrap(),
            ArchiveFormat::TarBr
        );
        assert_eq!(
            parse_gui_compress_format("tar.lz4").unwrap(),
            ArchiveFormat::TarLz4
        );
    }
    #[test]
    fn parse_gui_compress_format_accepts_7z() {
        assert_eq!(
            parse_gui_compress_format("7z").unwrap(),
            ArchiveFormat::SevenZip
        );
    }
    #[test]
    fn parse_gui_compress_format_invalid_error_lists_7z_and_zipx() {
        let err = parse_gui_compress_format("bogus").unwrap_err();
        assert!(err.contains("7z"));
        assert!(err.contains("zipx"));
    }

    #[test]
    fn parse_gui_compress_format_rejects_cab_as_read_only() {
        let err = parse_gui_compress_format("cab").unwrap_err();
        assert!(err.contains("read-only cab support"));
    }

    #[test]
    fn parse_gui_compress_format_rejects_cpio_as_read_only() {
        let err = parse_gui_compress_format("cpio").unwrap_err();
        assert!(err.contains("read-only cpio support"));
    }

    #[test]
    fn parse_gui_compress_format_accepts_lzh_lha() {
        assert_eq!(
            parse_gui_compress_format("lzh").unwrap(),
            ArchiveFormat::Lzh
        );
        assert_eq!(
            parse_gui_compress_format("lha").unwrap(),
            ArchiveFormat::Lzh
        );
    }

    #[test]
    fn parse_gui_compress_format_rejects_single_stream_brotli_and_lz4() {
        let br_err = parse_gui_compress_format("brotli").unwrap_err();
        assert!(br_err.contains("single-stream"));
        assert!(br_err.contains("tar.br"));
        let lz4_err = parse_gui_compress_format("lz4").unwrap_err();
        assert!(lz4_err.contains("single-stream"));
        assert!(lz4_err.contains("tar.lz4"));
    }
    #[test]
    fn create_gui_writer_rejects_cpio() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("gui-output.cpio");
        let file = fs::File::create(&archive_path).unwrap();
        match create_gui_writer(file, ArchiveFormat::Cpio, CompressOptions::default()) {
            Ok(_) => panic!("cpio writer should be rejected"),
            Err(err) => assert!(err.contains("read-only cpio support")),
        }
    }

    #[test]
    fn create_gui_writer_lzh_roundtrip() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("gui-output.lzh");
        let file = fs::File::create(&archive_path).unwrap();
        let mut writer = create_gui_writer(file, ArchiveFormat::Lzh, CompressOptions::default())
            .expect("lzh writer should be created");
        writer
            .add_directory(std::path::Path::new("empty"))
            .expect("directory should be added");
        writer
            .add_entry_from_reader(
                std::path::Path::new("docs/readme.txt"),
                &mut std::io::Cursor::new(b"gui lzh".to_vec()),
            )
            .expect("file should be added");
        let bytes_written = writer.finish().expect("writer should finish");
        assert!(bytes_written > 0);

        let mut reader = LzhReader::new(fs::File::open(&archive_path).unwrap());
        let entries = reader.entries().expect("entries should load");
        assert!(entries
            .iter()
            .any(|entry| entry.path == "empty" && entry.is_dir));
        assert!(entries
            .iter()
            .any(|entry| entry.path == "docs/readme.txt" && !entry.is_dir));
        let dest = tempfile::tempdir().unwrap();
        let report = reader
            .extract_all(dest.path(), true)
            .expect("archive should extract");
        assert!(report.errors.is_empty(), "errors: {:?}", report.errors);
        assert!(dest.path().join("empty").is_dir());
        assert_eq!(
            std::fs::read_to_string(dest.path().join("docs/readme.txt")).unwrap(),
            "gui lzh"
        );
    }

    #[test]
    fn create_gui_writer_sevenzip_roundtrip() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("gui-output.7z");
        let file = fs::File::create(&archive_path).unwrap();
        let mut writer =
            create_gui_writer(file, ArchiveFormat::SevenZip, CompressOptions::default())
                .expect("7z writer should be created");
        writer
            .add_directory(std::path::Path::new("empty"))
            .expect("directory should be added");
        writer
            .add_entry_from_reader(
                std::path::Path::new("docs/readme.txt"),
                &mut std::io::Cursor::new(b"gui seven zip".to_vec()),
            )
            .expect("file should be added");
        let bytes_written = writer.finish().expect("writer should finish");
        assert!(bytes_written > 0);
        let mut reader = SevenZipReader::new(&archive_path);
        let entries = reader.entries().expect("entries should load");
        assert!(entries
            .iter()
            .any(|entry| entry.path == "empty" && entry.is_dir));
        assert!(entries
            .iter()
            .any(|entry| entry.path == "docs/readme.txt" && !entry.is_dir));
        let dest = tempfile::tempdir().unwrap();
        let report = reader
            .extract_all(dest.path(), true)
            .expect("archive should extract");
        assert!(report.errors.is_empty(), "errors: {:?}", report.errors);
        assert!(dest.path().join("empty").is_dir());
        assert_eq!(
            std::fs::read_to_string(dest.path().join("docs/readme.txt")).unwrap(),
            "gui seven zip"
        );
    }

    #[test]
    fn create_gui_writer_sevenzip_encrypted_roundtrip() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("gui-encrypted.7z");
        let file = fs::File::create(&archive_path).unwrap();
        let mut writer = create_gui_writer(
            file,
            ArchiveFormat::SevenZip,
            CompressOptions {
                password: Some("guipw".into()),
                ..CompressOptions::default()
            },
        )
        .expect("encrypted 7z writer should be created");
        writer
            .add_entry_from_reader(
                std::path::Path::new("docs/secret.txt"),
                &mut std::io::Cursor::new(b"gui encrypted seven zip".to_vec()),
            )
            .expect("file should be added");
        let bytes_written = writer.finish().expect("writer should finish");
        assert!(bytes_written > 0);

        let mut reader = SevenZipReader::new(&archive_path);
        assert!(
            reader.entries().is_err(),
            "encrypted archive should require a password"
        );
        reader.set_password("guipw");

        let entries = reader.entries().expect("entries should load with password");
        assert!(entries
            .iter()
            .any(|entry| entry.path == "docs/secret.txt" && !entry.is_dir));

        let dest = tempfile::tempdir().unwrap();
        let report = reader
            .extract_all(dest.path(), true)
            .expect("archive should extract with password");
        assert!(report.errors.is_empty(), "errors: {:?}", report.errors);
        assert_eq!(
            std::fs::read_to_string(dest.path().join("docs/secret.txt")).unwrap(),
            "gui encrypted seven zip"
        );
    }
}
