//! Prepare real files for the OS drag source using the existing safe extractor.
use crate::{
    browser,
    engine::{self, ReadRequest},
    job::Control,
};
use std::{collections::BTreeSet, path::PathBuf};

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
    if key.selection.is_empty() {
        return Err("请先选择要拖出的文件或文件夹。".into());
    }
    let directory = tempfile::Builder::new()
        .prefix("geezipx-drag-")
        .tempdir()
        .map_err(|e| format!("无法准备拖出文件：{e}"))?;
    engine::extract(
        ReadRequest {
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
            return Err(format!("无法准备所选条目：{name}"));
        }
        paths.push(path);
    }
    Ok(PreparedExport {
        key,
        paths,
        _directory: directory,
    })
}
