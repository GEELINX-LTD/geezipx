//! Blocking filesystem orchestration. Called exclusively by background jobs.
pub mod formats;
use crate::{browser, job::Control};
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
    pub path: PathBuf,
    pub format: ArchiveFormat,
    pub entries: Vec<Entry>,
    pub browser: browser::Browser,
    pub bytes: u64,
}

pub fn list(path: PathBuf, password: Option<String>, control: &Control) -> Result<Archive, String> {
    control.update(|p| p.stage = "正在读取归档目录…".into());
    control.check()?;
    let mut reader = formats::open_reader(&path, password.as_deref())?;
    let format = reader.format();
    let entries = reader.entries().map_err(|e| e.to_string())?;
    control.check()?;
    let bytes = entries
        .iter()
        .fold(0u64, |total, entry| total.saturating_add(entry.size));
    let browser = browser::Browser::new(&entries);
    control.check()?;
    Ok(Archive {
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
fn resolved(path: &Path) -> Result<PathBuf, String> {
    if path.exists() {
        return fs::canonicalize(path).map_err(|e| e.to_string());
    }
    let name = path
        .file_name()
        .ok_or_else(|| format!("无效路径：{}", path.display()))?;
    Ok(resolved(parent(path))?.join(name))
}

fn validate_output(request: &CompressRequest) -> Result<(), String> {
    if request.sources.is_empty() {
        return Err("请先添加文件或文件夹。".into());
    }
    if geezipx_core::detect::detect_from_extension(&request.output) != Some(request.format) {
        return Err(format!(
            "输出文件名应使用 .{} 扩展名，请检查格式和保存位置。",
            request.format
        ));
    }
    let output_parent = fs::canonicalize(parent(&request.output))
        .map_err(|e| format!("输出文件夹不存在或无法访问：{e}"))?;
    let output = output_parent.join(request.output.file_name().ok_or("请填写输出文件名。")?);
    if let Ok(metadata) = fs::symlink_metadata(&output) {
        if is_link(&metadata) || !metadata.is_file() {
            return Err("输出路径必须是普通文件，不能是链接或文件夹。".into());
        }
        if !request.replace {
            return Err("输出文件已存在，请更改文件名，或明确勾选替换。".into());
        }
    }
    for source in &request.sources {
        let canonical =
            fs::canonicalize(source).map_err(|e| format!("无法访问 {}：{e}", source.display()))?;
        if (canonical.is_dir() && output_parent.starts_with(&canonical))
            || canonical == output
            || same_file::is_same_file(&canonical, &output).unwrap_or(false)
        {
            return Err("输出文件不能是输入文件，也不能放在正在压缩的文件夹内。".into());
        }
    }
    Ok(())
}

fn collect(sources: &[PathBuf], control: &Control) -> Result<(Vec<Input>, usize), String> {
    let mut roots = BTreeMap::new();
    let mut directories = HashSet::new();
    let mut skipped = 0;
    for path in sources {
        control.check()?;
        let metadata =
            fs::symlink_metadata(path).map_err(|e| format!("无法读取 {}：{e}", path.display()))?;
        if is_link(&metadata) || (!metadata.is_dir() && !metadata.is_file()) {
            skipped += 1;
            continue;
        }
        let real = fs::canonicalize(path).map_err(|e| e.to_string())?;
        let name = real
            .file_name()
            .ok_or("不能直接压缩文件系统根目录，请选择其中的文件夹。")?
            .to_owned();
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
        let metadata =
            fs::symlink_metadata(&real).map_err(|e| format!("无法读取 {}：{e}", real.display()))?;
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
            .ok_or("输入包含不能表示为 Unicode 的文件名。")?
            .replace('\\', "/");
        if is_entry_path_dangerous(Path::new(&archive_name)) || !names.insert(archive_name.clone())
        {
            return Err(format!(
                "归档中出现重复或无效名称：{archive_name}。请移除同名来源。"
            ));
        }
        if metadata.is_dir() {
            let mut children = fs::read_dir(&real)
                .map_err(|e| format!("无法读取 {}：{e}", real.display()))?
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
        return Err("没有可压缩的普通文件或文件夹；符号链接和特殊文件会被跳过。".into());
    }
    Ok((inputs, skipped))
}

pub fn compress(request: CompressRequest, control: &Control) -> Result<Summary, String> {
    control.update(|p| p.stage = "正在扫描输入文件…".into());
    validate_output(&request)?;
    let (inputs, skipped) = collect(&request.sources, control)?;
    control.update(|p| {
        p.stage = "正在压缩".into();
        p.entries = inputs.len();
        p.total = inputs
            .iter()
            .fold(0u64, |sum, entry| sum.saturating_add(entry.size));
    });
    let temp =
        tempfile::NamedTempFile::new_in(parent(&request.output)).map_err(|e| e.to_string())?;
    let file = temp.reopen().map_err(|e| e.to_string())?;
    let mut writer = formats::create_writer(file, request.format, request.options)?;
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
                .map_err(|e| format!("无法读取 {}：{e}", entry.real.display()))?;
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
    control.update(|p| p.stage = "正在完成归档，请稍候…".into());
    writer.finish().map_err(|e| e.to_string())?;
    control.check()?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    if request.replace {
        temp.persist(&request.output)
    } else {
        temp.persist_noclobber(&request.output)
    }
    .map_err(|e| format!("无法保存压缩包：{}", e.error))?;
    Ok(Summary {
        message: format!(
            "已创建归档，共 {} 项，跳过 {skipped} 个链接或特殊文件。",
            inputs.len()
        ),
        output: Some(request.output),
    })
}

pub struct ReadRequest {
    pub archive: PathBuf,
    pub password: Option<String>,
    /// Empty means extract everything; folder selections include descendants.
    pub selection: BTreeSet<String>,
    pub destination: PathBuf,
    pub overwrite: bool,
}

pub fn extract(request: ReadRequest, control: &Control) -> Result<Summary, String> {
    read_entries(request, control, false)
}
pub fn verify(request: ReadRequest, control: &Control) -> Result<Summary, String> {
    read_entries(request, control, true)
}

fn read_entries(request: ReadRequest, control: &Control, verify: bool) -> Result<Summary, String> {
    control.update(|p| p.stage = "正在读取归档…".into());
    control.check()?;
    let mut reader = formats::open_reader(&request.archive, request.password.as_deref())?;
    let entries: Vec<_> = reader
        .entries()
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|entry| {
            verify
                || request.selection.is_empty()
                || browser::selected(&entry.path, &request.selection)
        })
        .collect();
    control.check()?;
    if !request.selection.is_empty() && entries.is_empty() {
        return Err("所选条目已不存在，请重新打开归档。".into());
    }
    // Preflight the entire selection before creating files, protecting the input archive too.
    if !verify {
        let root = resolved(&request.destination)?;
        let archive = fs::canonicalize(&request.archive).map_err(|e| e.to_string())?;
        for entry in &entries {
            let entry_path = browser::normalized(&entry.path);
            if is_entry_path_dangerous(Path::new(&entry_path)) {
                return Err(format!("归档包含不安全路径：{}", entry.path));
            }
            let target = root.join(browser::normalized(&entry.path));
            if target == archive || same_file::is_same_file(&archive, &target).unwrap_or(false) {
                return Err("解压路径会覆盖正在读取的归档，请选择其他文件夹。".into());
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
            "正在验证完整性"
        } else {
            "正在解压"
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
    let names: HashSet<_> = entries.iter().map(|entry| entry.path.as_str()).collect();
    let source = fs::canonicalize(&request.archive).map_err(|e| e.to_string())?;
    let root = if verify {
        None
    } else {
        Some(resolved(&request.destination)?)
    };
    let mut completed = 0;
    let result = reader.visit_entries(
        &mut |entry, input| {
            if control.is_cancelled() {
                return Err(GeeZipError::Cancelled);
            }
            if !names.contains(entry.path.as_str()) {
                return Ok(true);
            }
            control.update(|p| {
                p.entry = entry.path.clone();
                p.completed = completed;
            });
            let entry_path = browser::normalized(&entry.path);
            if let Some(root) = &root {
                let target = root.join(browser::normalized(&entry.path));
                if target == source || same_file::is_same_file(&source, &target).unwrap_or(false) {
                    return Err(GeeZipError::io(
                        io::Error::other("解压会覆盖输入归档"),
                        "检查输出路径",
                    ));
                }
            }
            let mut write = |output: &mut dyn Write| {
                let mut output = ProgressWriter::new(output)
                    .with_total(entry.size)
                    .with_callback(control.callback(bytes));
                let result = io::copy(input, &mut output)
                    .map_err(|e| GeeZipError::io(e, format!("读取 {}", entry.path)));
                if control.is_cancelled() {
                    Err(GeeZipError::Cancelled)
                } else {
                    result
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
        format!(
            "{}：{error}\n已完成 {files} 个文件，跳过 {skipped} 个。",
            control.snapshot().entry
        )
    })?;
    Ok(Summary {
        message: if verify {
            format!("完整性验证通过，已读取 {files} 个文件。")
        } else {
            format!("已解压 {files} 个文件，跳过 {skipped} 个已有文件。")
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
    control.check()?;
    let mut reader = formats::open_reader(&archive, password.as_deref())?;
    let mut buffer = PreviewBuffer {
        bytes: Vec::new(),
        truncated: false,
    };
    let mut found = false;
    let result = reader.visit_entries(
        &mut |candidate, input| {
            if candidate.path != entry.path {
                return Ok(true);
            }
            found = true;
            let result = {
                let mut writer =
                    ProgressWriter::new(&mut buffer).with_callback(control.callback(0));
                io::copy(input, &mut writer).map_err(|e| GeeZipError::io(e, "读取预览"))
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
        return Err("归档内容已变化，请重新打开后再预览。".into());
    }
    if buffer.bytes.contains(&0) {
        return Err("此文件包含二进制内容，请提取后用相应应用打开。".into());
    }
    let text = match std::str::from_utf8(&buffer.bytes) {
        Ok(text) => text.to_owned(),
        Err(error) if buffer.truncated && error.error_len().is_none() => {
            String::from_utf8_lossy(&buffer.bytes[..error.valid_up_to()]).into_owned()
        }
        Err(_) => return Err("目前仅支持 UTF-8 文本预览，请提取后打开此文件。".into()),
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
        ".tar.gz", ".tar.bz2", ".tar.xz", ".tar.zst", ".tar.br", ".tar.lz4",
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
    parent(archive).join(if stem.is_empty() { "extracted" } else { &stem })
}
