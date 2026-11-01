//! Blocking filesystem orchestration. Called exclusively by background jobs.
pub mod formats;
mod index;
pub use index::ArchiveIndex;
pub mod streams;
use crate::{browser, i18n::Language, job::Control, tr};
use geezipx_core::{
    archive::{extraction::ExtractionDir, is_entry_path_dangerous, Entry},
    config::CompressOptions,
    detect::ArchiveFormat,
    GeeZipError, ProgressReader, ProgressWriter,
};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fs::{self, File},
    io::{self, BufReader, Write},
    path::{Path, PathBuf},
};

pub struct Archive {
    pub index: Option<ArchiveIndex>,
    pub path: PathBuf,
    pub format: ArchiveFormat,
    pub entries: Vec<Entry>,
    pub browser: browser::Browser,
    pub bytes: u64,
}

pub fn list(path: PathBuf, password: Option<String>, control: &Control) -> Result<Archive, String> {
    let language = control.language;
    control.update(|p| p.stage = tr!(language, ReadingDirectory).into());
    control.check()?;
    let mut index = ArchiveIndex::capture(&path)?;
    let format = formats::detect_in(&path, language)?;
    if streams::is_stream(format) && format != ArchiveFormat::Aes {
        index.stream = Some(Default::default());
    }
    let mut reader =
        formats::open_reader_cached(&path, password.as_deref(), control, index.stream.clone())?;
    let entries = reader.entries_with_cancel(&|| control.is_cancelled());
    control.check()?;
    let entries = entries.map_err(|e| e.to_string())?;
    let bytes = entries
        .iter()
        .fold(0u64, |total, entry| total.saturating_add(entry.size));
    let browser = browser::Browser::new(&entries);
    control.check()?;
    if !index.matches(&path) {
        return Err(tr!(language, ArchiveChanged).into());
    }
    index.entries = entries.clone().into();
    Ok(Archive {
        index: Some(index),
        path,
        format,
        entries,
        browser,
        bytes,
    })
}

pub struct CompressRequest {
    pub sources: Vec<PathBuf>,
    pub output: PathBuf,
    pub format: ArchiveFormat,
    pub options: CompressOptions,
    pub replace: bool,
    pub sfx_target: Option<geezipx_core::sfx::SfxTarget>,
}

#[derive(Debug)]
pub struct Summary {
    pub message: String,
    pub output: Option<PathBuf>,
}

struct Input {
    real: PathBuf,
    name: PathBuf,
    is_dir: bool,
    size: u64,
}

fn is_link(metadata: &fs::Metadata) -> bool {
    let link = metadata.file_type().is_symlink();
    #[cfg(windows)]
    let link = {
        use std::os::windows::fs::MetadataExt;
        link || metadata.file_attributes() & 0x400 != 0
    };
    link
}

fn parent(path: &Path) -> &Path {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}

/// Resolve existing ancestors without requiring the final directory to exist.
fn resolved(path: &Path, language: Language) -> Result<PathBuf, String> {
    if path.exists() {
        return fs::canonicalize(path).map_err(|e| e.to_string());
    }
    let name = path
        .file_name()
        .ok_or_else(|| tr!(language, InvalidPath, path.display()))?;
    Ok(resolved(parent(path), language)?.join(name))
}

fn validate_output(request: &CompressRequest, language: Language) -> Result<(), String> {
    if request.sources.is_empty() {
        return Err(tr!(language, AddFilesFirst).into());
    }
    if let Some(target) = request.sfx_target {
        if !target.is_available() {
            return Err(tr!(language, MissingSfx, target));
        }
        if request.format != ArchiveFormat::Zip {
            return Err(tr!(language, SfxZipOnly).into());
        }
        if target == geezipx_core::sfx::SfxTarget::Windows
            && request
                .output
                .extension()
                .is_none_or(|e| !e.eq_ignore_ascii_case("exe"))
        {
            return Err(tr!(language, SfxExe).into());
        }
    } else if geezipx_core::detect::detect_from_extension(&request.output) != Some(request.format) {
        return Err(tr!(
            language,
            OutputExtension,
            formats::extension(request.format)
        ));
    }
    if request.options.password.as_deref() == Some("") {
        return Err(tr!(language, EmptyPassword).into());
    }
    if request.options.password.is_some() && !formats::write_password(request.format) {
        return Err(tr!(language, NoEncryption, request.format));
    }
    if request.format == ArchiveFormat::Aes && request.options.password.is_none() {
        return Err(tr!(language, AesNeedsPassword).into());
    }
    if streams::single_input(request.format) {
        if request.sources.len() != 1 {
            return Err(tr!(language, OneFileFormat, request.format));
        }
        let metadata = fs::symlink_metadata(&request.sources[0]).map_err(|e| e.to_string())?;
        if !metadata.is_file() || is_link(&metadata) {
            return Err(tr!(language, RegularFileFormat, request.format));
        }
    }
    let output_parent = fs::canonicalize(parent(&request.output))
        .map_err(|e| tr!(language, OutputFolderError, e))?;
    let output = output_parent.join(
        request
            .output
            .file_name()
            .ok_or(tr!(language, EnterOutputName))?,
    );
    if let Ok(metadata) = fs::symlink_metadata(&output) {
        if is_link(&metadata) || !metadata.is_file() {
            return Err(tr!(language, OutputRegularFile).into());
        }
        if !request.replace {
            return Err(tr!(language, OutputExists).into());
        }
    }
    for source in &request.sources {
        let canonical = fs::canonicalize(source)
            .map_err(|e| tr!(language, CannotAccess, source.display(), e))?;
        if (canonical.is_dir() && output_parent.starts_with(&canonical))
            || canonical == output
            || same_file::is_same_file(&canonical, &output).unwrap_or(false)
        {
            return Err(tr!(language, OutputInsideInput).into());
        }
    }
    Ok(())
}

fn collect(sources: &[PathBuf], control: &Control) -> Result<(Vec<Input>, usize), String> {
    let language = control.language;
    let mut roots = BTreeMap::new();
    let mut directories = HashSet::new();
    let mut skipped = 0;
    for path in sources {
        control.check()?;
        let metadata =
            fs::symlink_metadata(path).map_err(|e| tr!(language, CannotRead, path.display(), e))?;
        if is_link(&metadata) || (!metadata.is_dir() && !metadata.is_file()) {
            skipped += 1;
            continue;
        }
        let real = fs::canonicalize(path).map_err(|e| e.to_string())?;
        let name = real.file_name().ok_or(tr!(language, RootInput))?.to_owned();
        if metadata.is_dir() {
            directories.insert(real.clone());
        }
        roots.insert(real, PathBuf::from(name));
    }
    let mut stack = roots
        .into_iter()
        .rev()
        .filter(|(real, _)| {
            !real
                .ancestors()
                .skip(1)
                .any(|ancestor| directories.contains(ancestor))
        })
        .collect::<Vec<_>>();
    let mut inputs = Vec::new();
    let mut visited = HashSet::new();
    let mut names = HashSet::new();
    while let Some((real, name)) = stack.pop() {
        control.check()?;
        let metadata = fs::symlink_metadata(&real)
            .map_err(|e| tr!(language, CannotRead, real.display(), e))?;
        if is_link(&metadata) || (!metadata.is_dir() && !metadata.is_file()) {
            skipped += 1;
            continue;
        }
        let real = fs::canonicalize(real).map_err(|e| e.to_string())?;
        if !visited.insert(real.clone()) {
            continue;
        }
        let archive_name = name
            .to_str()
            .ok_or(tr!(language, NonUnicodeName))?
            .replace('\\', "/");
        if is_entry_path_dangerous(Path::new(&archive_name)) || !names.insert(archive_name.clone())
        {
            return Err(tr!(language, DuplicateName, archive_name));
        }
        if metadata.is_dir() {
            let mut children = fs::read_dir(&real)
                .map_err(|e| tr!(language, CannotRead, real.display(), e))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            children.sort_by_key(|child| child.file_name());
            for child in children.into_iter().rev() {
                stack.push((child.path(), name.join(child.file_name())));
            }
        }
        inputs.push(Input {
            real,
            name,
            is_dir: metadata.is_dir(),
            size: if metadata.is_file() {
                metadata.len()
            } else {
                0
            },
        });
    }
    if inputs.is_empty() {
        return Err(tr!(language, NoRegularInputs).into());
    }
    Ok((inputs, skipped))
}

pub fn compress(request: CompressRequest, control: &Control) -> Result<Summary, String> {
    let language = control.language;
    control.update(|p| p.stage = tr!(language, Scanning).into());
    validate_output(&request, language)?;
    let (inputs, skipped) = collect(&request.sources, control)?;
    control.update(|p| {
        p.stage = tr!(language, Compressing).into();
        p.entries = inputs.len();
        p.total = inputs
            .iter()
            .fold(0u64, |sum, entry| sum.saturating_add(entry.size));
    });
    let temp =
        tempfile::NamedTempFile::new_in(parent(&request.output)).map_err(|e| e.to_string())?;
    let mut file = temp.reopen().map_err(|e| e.to_string())?;
    if streams::single_input(request.format) {
        let entry = &inputs[0];
        control.update(|p| p.entry = entry.name.display().to_string());
        let mut input = ProgressReader::new(BufReader::new(
            File::open(&entry.real).map_err(|e| e.to_string())?,
        ))
        .with_total(entry.size)
        .with_callback(control.callback(0));
        let result = streams::encode(
            &mut input,
            formats::TaskFile::new(file, control),
            request.format,
            request.options,
            &entry.name.to_string_lossy(),
        );
        control.check()?;
        result.map_err(|e| e.to_string())?;
        control.update(|p| {
            p.bytes = entry.size;
            p.completed = 1;
        });
    } else {
        let mut writer = formats::create_writer(file, request.format, request.options, control)?;
        let mut bytes = 0u64;
        for (index, entry) in inputs.iter().enumerate() {
            control.check()?;
            control.update(|p| {
                p.entry = entry.name.display().to_string();
                p.completed = index;
            });
            let result = if entry.is_dir {
                writer.add_directory(&entry.name)
            } else {
                let file = File::open(&entry.real)
                    .map_err(|e| tr!(language, CannotRead, entry.real.display(), e))?;
                let mut input = ProgressReader::new(BufReader::new(file))
                    .with_total(entry.size)
                    .with_callback(control.callback(bytes));
                writer.add_entry_with_size(&entry.name, &mut input, entry.size)
            };
            control.check()?;
            result.map_err(|e| format!("{}：{e}", entry.name.display()))?;
            bytes = bytes.saturating_add(entry.size);
            control.update(|p| {
                p.bytes = bytes;
                p.completed = index + 1;
            });
        }
        control.update(|p| p.stage = tr!(language, FinishingArchive).into());
        let result = writer.finish();
        control.check()?;
        result.map_err(|e| e.to_string())?;
    }
    let temp = if let Some(target) = request.sfx_target {
        control.update(|p| p.stage = tr!(language, CreatingSfx).into());
        let sfx =
            tempfile::NamedTempFile::new_in(parent(&request.output)).map_err(|e| e.to_string())?;
        file = temp.reopen().map_err(|e| e.to_string())?;
        geezipx_core::sfx::write_zip_sfx(
            formats::TaskFile::new(file, control),
            formats::TaskFile::new(sfx.reopen().map_err(|e| e.to_string())?, control),
            target,
        )
        .map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            sfx.as_file()
                .set_permissions(fs::Permissions::from_mode(0o755))
                .map_err(|e| e.to_string())?;
        }
        sfx
    } else {
        temp
    };
    control.check()?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    if request.replace {
        temp.persist(&request.output)
    } else {
        temp.persist_noclobber(&request.output)
    }
    .map_err(|e| tr!(language, SaveArchiveError, e.error))?;
    Ok(Summary {
        message: tr!(language, CreatedSummary, inputs.len(), skipped),
        output: Some(request.output),
    })
}

pub struct ReadRequest {
    /// Metadata from the opened source; checked for replacement and modification.
    pub index: Option<ArchiveIndex>,
    pub archive: PathBuf,
    pub password: Option<String>,
    /// Empty means extract everything; folder selections include descendants.
    pub selection: BTreeSet<String>,
    pub destination: PathBuf,
    pub overwrite: bool,
}

pub fn extract(request: ReadRequest, control: &Control) -> Result<Summary, String> {
    read_entries(request, control, false, None)
}

/// Writes selected archive paths under independently validated output names.
pub fn extract_to_paths(
    request: ReadRequest,
    paths: &BTreeMap<String, String>,
    control: &Control,
) -> Result<Summary, String> {
    read_entries(request, control, false, Some(paths))
}
pub fn verify(request: ReadRequest, control: &Control) -> Result<Summary, String> {
    control.check()?;
    if formats::detect_in(&request.archive, control.language)? == ArchiveFormat::Isz {
        // Traversing ISO files can leave padding/unreferenced ISZ chunks unread.
        // Validate the entire outer container before checking its filesystem.
        control.update(|p| p.stage = tr!(control.language, VerifyingIsz).into());
        let file = File::open(&request.archive).map_err(|e| e.to_string())?;
        let mut file = formats::TaskFile::new(file, control);
        let result = geezipx_core::archive::isz::isz_verify(&mut file);
        control.check()?;
        result.map_err(|e| e.to_string())?;
    }
    read_entries(request, control, true, None)
}

fn read_entries(
    request: ReadRequest,
    control: &Control,
    verify: bool,
    paths: Option<&BTreeMap<String, String>>,
) -> Result<Summary, String> {
    let language = control.language;
    control.update(|p| p.stage = tr!(language, ReadingArchive).into());
    control.check()?;
    if request
        .index
        .as_ref()
        .is_some_and(|index| !index.matches(&request.archive))
    {
        return Err(tr!(language, ArchiveChanged).into());
    }
    let mut reader = formats::open_reader_cached(
        &request.archive,
        request.password.as_deref(),
        control,
        if verify {
            None
        } else {
            request
                .index
                .as_ref()
                .and_then(|index| index.stream.clone())
        },
    )?;
    let entries = match &request.index {
        Some(index) => index.entries.to_vec(),
        None => reader
            .entries_with_cancel(&|| control.is_cancelled())
            .map_err(|e| e.to_string())?,
    };
    control.check()?;
    let entries: Vec<_> = entries
        .into_iter()
        .filter(|entry| {
            verify
                || request.selection.is_empty()
                || browser::selected(&entry.path, &request.selection)
        })
        .collect();
    control.check()?;
    if !request.selection.is_empty() && entries.is_empty() {
        return Err(tr!(language, SelectionMissing).into());
    }
    // Preflight the entire selection before creating files, protecting the input archive too.
    if !verify {
        let root = resolved(&request.destination, language)?;
        let archive = fs::canonicalize(&request.archive).map_err(|e| e.to_string())?;
        for entry in &entries {
            control.check()?;
            let source_path = browser::normalized(&entry.path);
            let entry_path = match paths {
                Some(paths) => paths
                    .get(&source_path)
                    .cloned()
                    .ok_or(tr!(language, ArchiveChanged))?,
                None => source_path,
            };
            if is_entry_path_dangerous(Path::new(&entry_path)) {
                return Err(tr!(language, UnsafePath, entry.path));
            }
            let target = root.join(&entry_path);
            if target == archive || same_file::is_same_file(&archive, &target).unwrap_or(false) {
                return Err(tr!(language, ExtractionOverwritesArchive).into());
            }
        }
    }
    let destination = if verify {
        None
    } else {
        Some(ExtractionDir::new(&request.destination).map_err(|e| e.to_string())?)
    };
    control.update(|p| {
        p.stage = if verify {
            tr!(language, Verifying)
        } else {
            tr!(language, Extracting)
        }
        .into();
        p.entries = entries.len();
        p.total = entries
            .iter()
            .fold(0u64, |sum, entry| sum.saturating_add(entry.size));
    });
    let mut bytes = 0u64;
    let mut skipped = 0;
    let mut files = 0;
    let source = fs::canonicalize(&request.archive).map_err(|e| e.to_string())?;
    let root = if verify {
        None
    } else {
        Some(resolved(&request.destination, language)?)
    };
    let reader_format = reader.format();
    let mut completed = 0;
    let result = reader.extract_entries(
        &entries,
        &mut |entry, extract| {
            if control.is_cancelled() {
                return Err(GeeZipError::Cancelled);
            }
            control.update(|p| {
                p.entry = entry.path.clone();
                p.completed = completed;
            });
            let source_path = browser::normalized(&entry.path);
            let entry_path = match paths {
                Some(paths) => paths
                    .get(&source_path)
                    .cloned()
                    .ok_or(GeeZipError::EntryNotFound { name: source_path })?,
                None => source_path,
            };
            if let Some(root) = &root {
                let target = root.join(&entry_path);
                if target == source || same_file::is_same_file(&source, &target).unwrap_or(false) {
                    return Err(GeeZipError::io(
                        io::Error::other(tr!(language, OverwritesInput)),
                        tr!(language, CheckOutputPath),
                    ));
                }
            }
            let mut write = |output: &mut dyn Write| {
                let mut output = ProgressWriter::new(output)
                    .with_total(entry.size)
                    .with_callback(control.callback(bytes));
                let result = extract(&mut output);
                if control.is_cancelled() {
                    Err(GeeZipError::Cancelled)
                } else {
                    let bytes = result?;
                    if bytes != entry.size {
                        return Err(GeeZipError::format("entry size mismatch", reader_format));
                    }
                    Ok(bytes)
                }
            };
            let result = match (&destination, entry.is_dir) {
                (None, true) => Ok(0),
                (None, false) => write(&mut io::sink()),
                (Some(output), true) => output.create_dir(&entry_path).map(|()| 0),
                (Some(output), false) => output.write_file(&entry_path, request.overwrite, write),
            };
            if control.is_cancelled() {
                return Err(GeeZipError::Cancelled);
            }
            match result {
                Ok(_) => {
                    if !entry.is_dir {
                        files += 1;
                    }
                }
                Err(GeeZipError::ClobberDenied { .. }) if !verify => skipped += 1,
                Err(error) => return Err(error),
            }
            bytes = bytes.saturating_add(entry.size);
            completed += 1;
            control.update(|p| {
                p.bytes = bytes;
                p.completed = completed;
            });
            Ok(true)
        },
        &|| control.is_cancelled(),
    );
    control.check()?;
    result.map_err(|error| {
        tr!(
            language,
            PartialRead,
            control.snapshot().entry,
            error,
            files,
            skipped
        )
    })?;
    Ok(Summary {
        message: if verify {
            tr!(language, VerifiedSummary, files)
        } else {
            tr!(language, ExtractedSummary, files, skipped)
        },
        output: if verify {
            None
        } else {
            Some(request.destination)
        },
    })
}

pub const PREVIEW_LIMIT: usize = 512 * 1024;
pub struct Preview {
    pub name: String,
    pub text: String,
    pub truncated: bool,
}

pub fn preview(
    archive: PathBuf,
    password: Option<String>,
    entry: Entry,
    control: &Control,
) -> Result<Preview, String> {
    preview_cached(archive, password, entry, None, control)
}

pub fn preview_cached(
    archive: PathBuf,
    password: Option<String>,
    entry: Entry,
    index: Option<ArchiveIndex>,
    control: &Control,
) -> Result<Preview, String> {
    let language = control.language;
    control.check()?;
    if index.as_ref().is_some_and(|index| !index.matches(&archive)) {
        return Err(tr!(language, ArchiveChanged).into());
    }
    let mut reader = formats::open_reader_cached(
        &archive,
        password.as_deref(),
        control,
        index.as_ref().and_then(|index| index.stream.clone()),
    )?;
    let mut buffer = PreviewBuffer {
        bytes: Vec::new(),
        truncated: false,
    };
    let mut found = false;
    let result = reader.extract_entries(
        std::slice::from_ref(&entry),
        &mut |_, extract| {
            found = true;
            let result = {
                let mut writer =
                    ProgressWriter::new(&mut buffer).with_callback(control.callback(0));
                extract(&mut writer)
            };
            if !buffer.truncated {
                result?;
            }
            Ok(false)
        },
        &|| control.is_cancelled(),
    );
    control.check()?;
    result.map_err(|e| e.to_string())?;
    if !found {
        return Err(tr!(language, ArchiveChanged).into());
    }
    if buffer.bytes.contains(&0) {
        return Err(tr!(language, BinaryPreview).into());
    }
    let text = match std::str::from_utf8(&buffer.bytes) {
        Ok(text) => text.to_owned(),
        Err(error) if buffer.truncated && error.error_len().is_none() => {
            String::from_utf8_lossy(&buffer.bytes[..error.valid_up_to()]).into_owned()
        }
        Err(_) => return Err(tr!(language, Utf8Preview).into()),
    };
    Ok(Preview {
        name: entry.path,
        text,
        truncated: buffer.truncated,
    })
}
struct PreviewBuffer {
    bytes: Vec<u8>,
    truncated: bool,
}
impl Write for PreviewBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let available = PREVIEW_LIMIT.saturating_sub(self.bytes.len());
        self.bytes
            .extend_from_slice(&bytes[..available.min(bytes.len())]);
        if bytes.len() > available {
            self.truncated = true;
            return Err(io::Error::other("preview limit"));
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub fn default_destination(archive: &Path) -> PathBuf {
    let name = archive.file_name().unwrap_or_default().to_string_lossy();
    let lower = name.to_ascii_lowercase();
    let suffix = [
        ".tar.gz",
        ".tar.bz2",
        ".tar.xz",
        ".tar.zst",
        ".tar.br",
        ".tar.lz4",
        ".tar.lzma",
        ".tar.lz",
    ]
    .iter()
    .find(|suffix| lower.ends_with(**suffix));
    let stem = suffix.map_or_else(
        || {
            archive
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        },
        |suffix| name[..name.len() - suffix.len()].to_owned(),
    );
    let directory =
        if geezipx_core::detect::detect_from_extension(archive).is_some_and(streams::is_stream) {
            // Removing .gz/.enc commonly lands on the original input file.
            format!("{stem}-extracted")
        } else if stem.is_empty() {
            "extracted".to_owned()
        } else {
            stem
        };
    parent(archive).join(directory)
}
