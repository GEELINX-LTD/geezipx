//! Cached directory index and file-manager selection. Redraws only visit visible rows.
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

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum SortColumn {
    #[default]
    Name,
    Size,
    Packed,
    Type,
}

#[derive(Clone, Debug)]
pub struct Row {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub packed: u64,
    pub kind: String,
    pub entry_index: Option<usize>,
    search: String,
}

#[derive(Default)]
struct Directory {
    by_name: Vec<Arc<Row>>,
    by_size: Vec<Arc<Row>>,
    by_packed: Vec<Arc<Row>>,
    by_type: Vec<Arc<Row>>,
}

#[derive(Default)]
pub struct Browser {
    directories: BTreeMap<String, Directory>,
    rows: BTreeMap<String, Arc<Row>>,
    pub directory: String,
    pub query: String,
    pub visible: Vec<Arc<Row>>,
    pub selection: BTreeSet<String>,
    pub sort: SortColumn,
    pub descending: bool,
    pub focus: Option<String>,
    pub reveal: Option<usize>,
    anchor: Option<String>,
    back: Vec<String>,
    forward: Vec<String>,
    pub selected_bytes: u64,
    pub hidden_selected: usize,
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
                        packed: 0,
                        kind: if is_dir {
                            "文件夹".into()
                        } else {
                            name.rsplit_once('.')
                                .filter(|(stem, suffix)| !stem.is_empty() && !suffix.is_empty())
                                .map_or_else(
                                    || "文件".into(),
                                    |(_, extension)| extension.to_uppercase(),
                                )
                        },
                        entry_index: None,
                        search: name.to_lowercase(),
                    });
                row.is_dir |= is_dir;
                if !entry.is_dir {
                    row.size = row.size.saturating_add(entry.size);
                    row.packed = row.packed.saturating_add(entry.compressed_size);
                }
                if leaf && !entry.is_dir {
                    row.entry_index = Some(entry_index);
                }
                if is_dir {
                    index.entry(path.clone()).or_default();
                }
                parent = path;
            }
        }
        let mut rows = BTreeMap::new();
        let directories = index
            .into_iter()
            .map(|(path, items)| {
                let mut by_name = items.into_values().map(Arc::new).collect::<Vec<_>>();
                for row in &by_name {
                    rows.insert(row.path.clone(), row.clone());
                }
                by_name
                    .sort_unstable_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.search.cmp(&b.search)));
                let mut by_size = by_name.clone();
                by_size.sort_unstable_by(|a, b| {
                    b.is_dir
                        .cmp(&a.is_dir)
                        .then(a.size.cmp(&b.size))
                        .then(a.search.cmp(&b.search))
                });
                let mut by_packed = by_name.clone();
                by_packed.sort_unstable_by(|a, b| {
                    b.is_dir
                        .cmp(&a.is_dir)
                        .then(a.packed.cmp(&b.packed))
                        .then(a.search.cmp(&b.search))
                });
                let mut by_type = by_name.clone();
                by_type.sort_unstable_by(|a, b| {
                    b.is_dir
                        .cmp(&a.is_dir)
                        .then(a.kind.cmp(&b.kind))
                        .then(a.search.cmp(&b.search))
                });
                (
                    path,
                    Directory {
                        by_name,
                        by_size,
                        by_packed,
                        by_type,
                    },
                )
            })
            .collect();
        let mut browser = Self {
            directories,
            rows,
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
            .map(|directory| match self.sort {
                SortColumn::Name => &directory.by_name,
                SortColumn::Size => &directory.by_size,
                SortColumn::Packed => &directory.by_packed,
                SortColumn::Type => &directory.by_type,
            })
            .into_iter()
            .flatten()
            .filter(|row| row.search.contains(&query))
            .cloned()
            .collect();
        if self.descending {
            // Keep folders first in either direction without sorting in a paint pass.
            let split = self.visible.partition_point(|row| row.is_dir);
            self.visible[..split].reverse();
            self.visible[split..].reverse();
        }
        if !self
            .visible
            .iter()
            .any(|row| Some(&row.path) == self.focus.as_ref())
        {
            self.focus = None;
        }
        self.reveal = Some(0);
        self.selection_changed();
    }
    pub fn sort_by(&mut self, column: SortColumn) {
        self.descending = if self.sort == column {
            !self.descending
        } else {
            matches!(column, SortColumn::Size | SortColumn::Packed)
        };
        self.sort = column;
        self.refresh();
    }
    fn enter(&mut self, path: String) {
        self.directory = path;
        self.query.clear();
        self.clear_selection();
        self.focus = None;
        self.refresh();
    }
    pub fn navigate(&mut self, path: String) {
        if path != self.directory && self.directories.contains_key(&path) {
            self.back.push(self.directory.clone());
            self.forward.clear();
            self.enter(path);
        }
    }
    pub fn can_back(&self) -> bool {
        !self.back.is_empty()
    }
    pub fn can_forward(&self) -> bool {
        !self.forward.is_empty()
    }
    pub fn back(&mut self) {
        if let Some(path) = self.back.pop() {
            self.forward.push(self.directory.clone());
            self.enter(path);
        }
    }
    pub fn forward(&mut self) {
        if let Some(path) = self.forward.pop() {
            self.back.push(self.directory.clone());
            self.enter(path);
        }
    }
    pub fn parent(&mut self) {
        let previous = self.directory.clone();
        let parent = previous
            .rsplit_once('/')
            .map_or("", |(parent, _)| parent)
            .to_owned();
        self.navigate(parent);
        if let Some(index) = self.visible.iter().position(|row| row.path == previous) {
            self.focus = Some(previous);
            self.reveal = Some(index);
        }
    }
    pub fn clear_selection(&mut self) {
        self.selection.clear();
        self.anchor = None;
        self.selection_changed();
    }
    pub fn toggle(&mut self, path: &str) {
        if !self.selection.remove(path) {
            self.selection.insert(path.into());
        }
        self.selection_changed();
    }
    pub fn select_all(&mut self) {
        self.selection
            .extend(self.visible.iter().map(|row| row.path.clone()));
        self.selection_changed();
    }
    pub fn select(&mut self, index: usize, extend: bool, toggle: bool) {
        let Some(row) = self.visible.get(index) else {
            return;
        };
        let path = row.path.clone();
        if extend {
            let anchor = self
                .visible
                .iter()
                .position(|row| Some(&row.path) == self.anchor.as_ref())
                .unwrap_or(index);
            if !toggle {
                self.selection.clear();
            }
            for row in &self.visible[index.min(anchor)..=index.max(anchor)] {
                self.selection.insert(row.path.clone());
            }
            if self.anchor.is_none() {
                self.anchor = Some(path.clone());
            }
        } else if toggle {
            self.toggle(&path);
            self.anchor = Some(path.clone());
        } else {
            self.selection.clear();
            self.selection.insert(path.clone());
            self.anchor = Some(path.clone());
        }
        self.focus = Some(path);
        self.selection_changed();
    }
    pub fn move_focus(&mut self, offset: isize, extend: bool, focus_only: bool) {
        if self.visible.is_empty() {
            return;
        }
        let current = self
            .visible
            .iter()
            .position(|row| Some(&row.path) == self.focus.as_ref());
        let index = current.map_or_else(
            || {
                if offset < 0 {
                    self.visible.len() - 1
                } else {
                    0
                }
            },
            |index| {
                index
                    .saturating_add_signed(offset)
                    .min(self.visible.len() - 1)
            },
        );
        if focus_only {
            self.focus = Some(self.visible[index].path.clone());
        } else {
            self.select(index, extend, false);
        }
        self.reveal = Some(index);
    }
    pub fn jump_to(&mut self, prefix: &str) {
        let prefix = prefix.to_lowercase();
        let start = self
            .visible
            .iter()
            .position(|row| Some(&row.path) == self.focus.as_ref())
            .map_or(0, |index| index + 1);
        let found = (start..self.visible.len())
            .chain(0..start.min(self.visible.len()))
            .find(|&index| self.visible[index].search.starts_with(&prefix));
        if let Some(index) = found {
            self.select(index, false, false);
            self.reveal = Some(index);
        }
    }
    fn selection_changed(&mut self) {
        self.selected_bytes = self
            .selection
            .iter()
            .filter_map(|path| self.rows.get(path))
            .fold(0u64, |sum, row| sum.saturating_add(row.size));
        let query = self.query.to_lowercase();
        self.hidden_selected = if query.is_empty() {
            0
        } else {
            self.selection
                .iter()
                .filter(|path| {
                    self.rows
                        .get(*path)
                        .is_some_and(|row| !row.search.contains(&query))
                })
                .count()
        };
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
        browser.toggle("资料");
        assert!(selected(&entries[0].path, &browser.selection));
        assert!(!selected(&entries[1].path, &browser.selection));
        assert_eq!(browser.selected_bytes, 3);
        browser.navigate("资料".into());
        assert!(browser.selection.is_empty());
        assert_eq!(browser.visible[0].name, "子目录");
        browser.parent();
        assert_eq!(browser.directory, "");
        assert_eq!(browser.focus.as_deref(), Some("资料"));
    }
    #[test]
    fn filtering_does_not_drop_hidden_selection() {
        let mut browser = Browser::new(&[file("a.txt"), file("b.txt")]);
        browser.toggle("a.txt");
        browser.query = "B.TXT".into();
        browser.refresh();
        assert_eq!(browser.visible.len(), 1);
        assert!(browser.selection.contains("a.txt"));
        assert_eq!(browser.hidden_selected, 1);
    }
    #[test]
    fn selection_anchor_survives_sort_and_keyboard_range() {
        let mut browser = Browser::new(&[file("a.txt"), file("b.txt"), file("c.txt")]);
        browser.select(0, false, false);
        browser.move_focus(1, true, false);
        assert_eq!(browser.selection.len(), 2);
        browser.sort_by(SortColumn::Name);
        browser.select(0, true, false);
        assert_eq!(browser.selection.len(), 3);
        browser.select(1, false, true);
        assert_eq!(browser.selection.len(), 2);
        browser.clear_selection();
        browser.select_all();
        assert_eq!(browser.selected_bytes, 9);
    }
    #[test]
    fn history_and_sort_keep_folders_first() {
        let mut browser = Browser::new(&[file("dir/a"), file("x.txt"), file("y.txt")]);
        browser.sort_by(SortColumn::Name);
        assert_eq!(browser.visible[0].name, "dir");
        assert_eq!(browser.visible[1].name, "y.txt");
        browser.navigate("dir".into());
        browser.back();
        assert_eq!(browser.directory, "");
        assert!(browser.can_forward());
        browser.forward();
        assert_eq!(browser.directory, "dir");
        browser.parent();
        browser.jump_to("x");
        assert!(browser.selection.contains("x.txt"));
    }
}
