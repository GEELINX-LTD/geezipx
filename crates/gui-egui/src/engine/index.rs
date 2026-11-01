//! Metadata and decoded-stream state belonging to one unchanged source file.
use super::streams::DecodedStream;
use geezipx_core::archive::Entry;
use std::{fs, path::Path, sync::Arc, time::SystemTime};

#[derive(Clone)]
pub struct ArchiveIndex {
    pub entries: Arc<[Entry]>,
    pub(super) stream: Option<DecodedStream>,
    source: Arc<same_file::Handle>,
    length: u64,
    modified: Option<SystemTime>,
}
impl ArchiveIndex {
    pub(super) fn capture(path: &Path) -> Result<Self, String> {
        let metadata = fs::metadata(path).map_err(|e| e.to_string())?;
        Ok(Self {
            entries: Arc::from([]),
            stream: None,
            source: Arc::new(same_file::Handle::from_path(path).map_err(|e| e.to_string())?),
            length: metadata.len(),
            modified: metadata.modified().ok(),
        })
    }
    pub fn matches(&self, path: &Path) -> bool {
        let Ok(metadata) = fs::metadata(path) else {
            return false;
        };
        metadata.len() == self.length
            && metadata.modified().ok() == self.modified
            && same_file::Handle::from_path(path).is_ok_and(|handle| handle == *self.source)
    }
}
