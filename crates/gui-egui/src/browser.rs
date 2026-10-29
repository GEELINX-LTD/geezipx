//! Cached directory index; rendering a frame only visits visible rows.
use geezipx_core::archive::Entry;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub fn normalized(path: &str) -> String {
    path.replace('\\', "/").trim_end_matches('/').to_owned()
}

pub fn selected(path: &str, selection: &BTreeSet<String>) -> bool {
    let path = normalized(path);
    selection.iter().any(|root| {
        path == *root
            || path
                .strip_prefix(root)
                .is_some_and(|tail| tail.starts_with('/'))
    })
}

#[derive(Clone, Debug)]
pub struct Row {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub entry_index: Option<usize>,
    search: String,
}

#[derive(Default)]
struct Directory {
    by_name: Vec<Arc<Row>>,
    by_size: Vec<Arc<Row>>,
}

#[derive(Default)]
pub struct Browser {
    directories: BTreeMap<String, Directory>,
    pub directory: String,
    pub query: String,
    pub visible: Vec<Arc<Row>>,
    pub selection: BTreeSet<String>,
    pub size_descending: bool,
}

impl Browser {
    pub fn new(entries: &[Entry]) -> Self {
        let mut index: BTreeMap<String, BTreeMap<String, Row>> = BTreeMap::new();
        index.entry(String::new()).or_default();
        for (entry_index, entry) in entries.iter().enumerate() {
            let path = normalized(&entry.path);
            let parts: Vec<_> = path.split('/').filter(|part| !part.is_empty()).collect();
            let mut parent = String::new();
            for (position, name) in parts.iter().enumerate() {
                let path = if parent.is_empty() {
                    (*name).to_owned()
                } else {
                    format!("{parent}/{name}")
                };
                let leaf = position + 1 == parts.len();
                let is_dir = !leaf || entry.is_dir;
                let row = index
                    .entry(parent.clone())
                    .or_default()
                    .entry(path.clone())
                    .or_insert_with(|| Row {
                        path: path.clone(),
                        name: (*name).to_owned(),
                        is_dir,
                        size: 0,
                        entry_index: None,
                        search: name.to_lowercase(),
                    });
                row.is_dir |= is_dir;
                if leaf && !entry.is_dir {
                    row.size = entry.size;
                    row.entry_index = Some(entry_index);
                }
                if is_dir {
                    index.entry(path.clone()).or_default();
                }
                parent = path;
            }
        }
        let mut browser = Self {
            directories: index
                .into_iter()
                .map(|(path, rows)| {
                    let mut by_name = rows.into_values().map(Arc::new).collect::<Vec<_>>();
                    by_name.sort_unstable_by(|a, b| {
                        b.is_dir.cmp(&a.is_dir).then(a.search.cmp(&b.search))
                    });
                    let mut by_size = by_name.clone();
                    by_size.sort_unstable_by(|a, b| {
                        b.is_dir
                            .cmp(&a.is_dir)
                            .then(b.size.cmp(&a.size))
                            .then(a.search.cmp(&b.search))
                    });
                    (path, Directory { by_name, by_size })
                })
                .collect(),
            ..Self::default()
        };
        browser.refresh();
        browser
    }
    pub fn refresh(&mut self) {
        let query = self.query.to_lowercase();
        self.visible = self
            .directories
            .get(&self.directory)
            .map(|directory| {
                if self.size_descending {
                    &directory.by_size
                } else {
                    &directory.by_name
                }
            })
            .into_iter()
            .flatten()
            .filter(|row| row.search.contains(&query))
            .cloned()
            .collect();
    }
    pub fn navigate(&mut self, path: String) {
        if self.directories.contains_key(&path) {
            self.directory = path;
            self.query.clear();
            self.refresh();
        }
    }
    pub fn parent(&mut self) {
        let parent = self
            .directory
            .rsplit_once('/')
            .map_or("", |(parent, _)| parent)
            .to_owned();
        self.navigate(parent);
    }
    pub fn toggle(&mut self, path: &str) {
        if !self.selection.remove(path) {
            self.selection.insert(path.into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn file(path: &str) -> Entry {
        Entry {
            path: path.into(),
            size: 3,
            compressed_size: 0,
            crc32: None,
            modified: None,
            is_dir: false,
        }
    }
    #[test]
    fn implicit_windows_folders_and_selection_boundaries() {
        let entries = [
            file("资料\\子目录\\a.txt"),
            file("资料2/b.txt"),
            file("root.txt"),
        ];
        let mut browser = Browser::new(&entries);
        assert_eq!(browser.visible.len(), 3);
        assert!(browser.visible[0].is_dir);
        browser.navigate("资料".into());
        assert_eq!(browser.visible[0].name, "子目录");
        browser.toggle("资料");
        assert!(selected(&entries[0].path, &browser.selection));
        assert!(!selected(&entries[1].path, &browser.selection));
        browser.parent();
        assert_eq!(browser.directory, "");
    }
    #[test]
    fn filtering_does_not_drop_hidden_selection() {
        let mut browser = Browser::new(&[file("a.txt"), file("b.txt")]);
        browser.toggle("a.txt");
        browser.query = "B.TXT".into();
        browser.refresh();
        assert_eq!(browser.visible.len(), 1);
        assert!(browser.selection.contains("a.txt"));
    }
}
