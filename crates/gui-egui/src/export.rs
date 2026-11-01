//! Deferred OS drag exports. Descriptors use cached metadata; extraction needs a confirmed drop.
use crate::{
    browser,
    engine::{self, ReadRequest},
    i18n::Language,
    job::Control,
    tr,
};
use geezipx_core::archive::{is_entry_path_dangerous, Entry};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

#[derive(Clone, Debug)]
pub struct ExportItem {
    pub archive_path: String,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

/// One immutable drag snapshot. No archive reads or temporary files before drop.
pub struct DeferredExport {
    pub key: ExportKey,
    pub items: Vec<ExportItem>,
    pub control: Control,
    dropped: AtomicBool,
    state: Mutex<DeferredState>,
}
struct DeferredState {
    password: Option<String>,
    index: Option<engine::ArchiveIndex>,
    prepared: Option<Result<PreparedExport, String>>,
}
impl DeferredExport {
    pub fn new(
        key: ExportKey,
        entries: &[Entry],
        password: Option<String>,
        control: Control,
    ) -> Result<Self, String> {
        let language = control.language;
        let roots: BTreeSet<_> = export_roots(&key.selection, language)?
            .into_iter()
            .collect();
        let mut root_names = BTreeSet::new();
        for root in &roots {
            let leaf = root.rsplit('/').next().unwrap_or(root);
            if !root_names.insert(leaf) {
                return Err(tr!(language, DuplicateExport, leaf));
            }
        }
        let mut items = BTreeMap::new();
        // One metadata pass, including large multi-selections; never roots × entries.
        for entry in entries {
            let path = browser::normalized(&entry.path);
            let Some(root) = roots.get(&path).or_else(|| {
                path.rmatch_indices('/')
                    .find_map(|(separator, _)| roots.get(&path[..separator]))
            }) else {
                continue;
            };
            let prefix = root.rsplit_once('/').map_or("", |(parent, _)| parent);
            if is_entry_path_dangerous(Path::new(&path)) {
                return Err(tr!(language, UnsafePath, path));
            }
            let name = if prefix.is_empty() {
                path.as_str()
            } else {
                &path[prefix.len() + 1..]
            };
            // Include implicit parents and empty directories in virtual-file descriptors.
            for (separator, _) in name.match_indices('/') {
                let parent = &name[..separator];
                items
                    .entry(parent.to_owned())
                    .or_insert_with(|| ExportItem {
                        archive_path: if prefix.is_empty() {
                            parent.into()
                        } else {
                            format!("{prefix}/{parent}")
                        },
                        name: parent.into(),
                        is_dir: true,
                        size: 0,
                    });
            }
            if items
                .get(name)
                .is_some_and(|item: &ExportItem| item.archive_path != path)
            {
                return Err(tr!(language, DuplicateExport, name));
            }
            let name = name.to_owned();
            items.insert(
                name.clone(),
                ExportItem {
                    archive_path: path,
                    name,
                    is_dir: entry.is_dir,
                    size: entry.size,
                },
            );
        }
        if items.is_empty() {
            return Err(tr!(language, SelectionMissing).into());
        }
        Ok(Self {
            key,
            items: items.into_values().collect(),
            control,
            dropped: AtomicBool::new(false),
            state: Mutex::new(DeferredState {
                password,
                index: None,
                prepared: None,
            }),
        })
    }
    pub fn with_index(mut self, index: Option<engine::ArchiveIndex>) -> Self {
        self.state
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .index = index;
        self
    }
    /// Write the selected roots directly, without a drag staging directory.
    pub fn extract_to(
        &self,
        destination: PathBuf,
        control: &Control,
    ) -> Result<engine::Summary, String> {
        if !self.is_dropped() {
            return Err(tr!(control.language, DropNotConfirmed).into());
        }
        control.check()?;
        let (password, index) = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| tr!(control.language, ExportStateDamaged))?;
            (state.password.take(), state.index.clone())
        };
        let paths = self
            .items
            .iter()
            .map(|item| (item.archive_path.clone(), item.name.clone()))
            .collect();
        engine::extract_to_paths(
            ReadRequest {
                archive: self.key.archive.clone(),
                index,
                password,
                selection: self.key.selection.clone(),
                destination,
                overwrite: false,
            },
            &paths,
            control,
        )
    }
    /// Called by the native drop protocol after releasing over an accepting target.
    pub fn confirm_drop(&self) {
        self.dropped.store(true, Ordering::Release);
    }
    pub fn is_dropped(&self) -> bool {
        self.dropped.load(Ordering::Acquire)
    }
    fn with_prepared<T>(&self, read: impl FnOnce(&PreparedExport) -> T) -> Result<T, String> {
        if !self.is_dropped() {
            return Err(tr!(self.control.language, DropNotConfirmed).into());
        }
        self.control.check()?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| tr!(self.control.language, ExportStateDamaged))?;
        if state.prepared.is_none() {
            let password = state.password.take();
            state.prepared = Some(prepare(self.key.clone(), password, &self.control));
        }
        match state.prepared.as_ref().expect("preparation stored") {
            Ok(prepared) => Ok(read(prepared)),
            Err(error) => Err(error.clone()),
        }
    }
    pub fn paths(&self) -> Result<Vec<PathBuf>, String> {
        self.with_prepared(|p| p.paths.clone())
    }
    pub fn write_promised_root(&self, name: &str, destination: &Path) -> Result<(), String> {
        let source = self
            .paths()?
            .into_iter()
            .find(|path| path.file_name().is_some_and(|file| file == name))
            .ok_or(tr!(self.control.language, PromisedItemMissing))?;
        let parent = destination
            .parent()
            .ok_or(tr!(self.control.language, DestinationParentMissing))?;
        let leaf = destination
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(tr!(self.control.language, InvalidDestinationName))?;
        let output = geezipx_core::archive::extraction::ExtractionDir::new(parent)
            .map_err(|e| e.to_string())?;
        let mut pending = vec![(source, leaf.to_owned())];
        while let Some((source, relative)) = pending.pop() {
            self.control.check()?;
            if source.is_dir() {
                output.create_dir(&relative).map_err(|e| e.to_string())?;
                for item in std::fs::read_dir(source).map_err(|e| e.to_string())? {
                    let item = item.map_err(|e| e.to_string())?;
                    pending.push((
                        item.path(),
                        format!("{relative}/{}", item.file_name().to_string_lossy()),
                    ));
                }
            } else {
                output
                    .write_file(&relative, false, |writer| {
                        let mut input = std::fs::File::open(&source).map_err(|e| {
                            geezipx_core::error::GeeZipError::io(
                                e,
                                tr!(self.control.language, ReadExport),
                            )
                        })?;
                        std::io::copy(&mut input, writer).map_err(|e| {
                            geezipx_core::error::GeeZipError::io(
                                e,
                                tr!(self.control.language, WriteExport),
                            )
                        })
                    })
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }
    pub fn file_path(&self, index: usize) -> Result<PathBuf, String> {
        let item = self
            .items
            .get(index)
            .ok_or(tr!(self.control.language, InvalidExportIndex))?;
        if item.is_dir {
            return Err(tr!(self.control.language, FolderHasNoStream).into());
        }
        self.with_prepared(|p| p._directory.path().join(&item.archive_path))
    }
}
fn export_roots(selection: &BTreeSet<String>, language: Language) -> Result<Vec<String>, String> {
    if selection.is_empty() {
        return Err(tr!(language, SelectExport).into());
    }
    let normalized: BTreeSet<_> = selection
        .iter()
        .map(|path| browser::normalized(path))
        .collect();
    for path in &normalized {
        if path.is_empty() || is_entry_path_dangerous(Path::new(path)) {
            return Err(tr!(language, UnsafePath, path));
        }
    }
    Ok(normalized
        .iter()
        .filter(|name| {
            !name
                .match_indices('/')
                .any(|(separator, _)| normalized.contains(&name[..separator]))
        })
        .cloned()
        .collect())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportKey {
    pub archive: PathBuf,
    pub selection: BTreeSet<String>,
}

pub struct PreparedExport {
    pub key: ExportKey,
    pub paths: Vec<PathBuf>,
    // Keep files alive while the receiver reads/copies them asynchronously.
    _directory: tempfile::TempDir,
}

pub fn prepare(
    key: ExportKey,
    password: Option<String>,
    control: &Control,
) -> Result<PreparedExport, String> {
    let language = control.language;
    if key.selection.is_empty() {
        return Err(tr!(language, SelectExport).into());
    }
    let directory = tempfile::Builder::new()
        .prefix("geezipx-drag-")
        .tempdir()
        .map_err(|e| tr!(language, PrepareExportError, e))?;
    engine::extract(
        ReadRequest {
            index: None,
            archive: key.archive.clone(),
            password,
            selection: key.selection.clone(),
            destination: directory.path().to_owned(),
            overwrite: false,
        },
        control,
    )?;
    control.check()?;
    let normalized: BTreeSet<_> = key
        .selection
        .iter()
        .map(|path| browser::normalized(path))
        .collect();
    let mut paths = Vec::new();
    for name in &normalized {
        // A selected parent already includes this item; do not offer it twice.
        if name
            .match_indices('/')
            .any(|(separator, _)| normalized.contains(&name[..separator]))
        {
            continue;
        }
        let path = directory.path().join(name);
        if !path.exists() {
            return Err(tr!(language, PrepareItemError, name));
        }
        paths.push(path);
    }
    Ok(PreparedExport {
        key,
        paths,
        _directory: directory,
    })
}
