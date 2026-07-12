// Archive browser state store using Svelte 5 class-based state runes.
//
// Refactored: TabArchive manages per-archive state; archiveManager owns the tab set;
// archiveStore remains a proxy for the active tab so that all consuming components
// (ExtractPage, Breadcrumb, BrowserTable, etc.) need zero changes.

import { listArchiveStream, previewEntry } from '../bridge';
import type { EntryInfo, PreviewResult } from '../bridge';

// --- Types ---

export interface DirChild {
  name: string;
  isDir: boolean;
  entry: EntryInfo;
}

// --- Helper functions (pure, top-level) ---

function stripArchiveExtension(fileName: string): string {
  const compound = ['.tar.gz', '.tar.bz2', '.tar.br', '.tar.lz4', '.tar.xz', '.tar.zst', '.tgz', '.tbz', '.tbz2', '.txz', '.tzst'];
  const simple = ['.zip', '.zipx', '.jar', '.war', '.apk', '.ipa', '.xpi', '.tar', '.gz', '.gzip', '.bz2', '.br', '.lz4', '.xz', '.zst', '.zstd', '.lzma', '.7z', '.rar', '.cab', '.asar', '.deb', '.lzh', '.lha', '.cpio'];
  const lower = fileName.toLowerCase();
  for (const ext of compound) { if (lower.endsWith(ext)) return fileName.slice(0, -ext.length); }
  for (const ext of simple) { if (lower.endsWith(ext)) return fileName.slice(0, -ext.length); }
  return fileName;
}

function parentDir(filePath: string): string {
  const sep = filePath.includes('\\') ? '\\' : '/';
  const idx = filePath.lastIndexOf(sep);
  return idx > 0 ? filePath.slice(0, idx + 1) : '';
}

function computeSuggestedDir(archivePath: string, allEntries: EntryInfo[]): string {
  if (!archivePath || allEntries.length === 0) return '';

  const parent = parentDir(archivePath);
  const archiveFileName = archivePath.split(/[/\\]/).pop() || 'archive';
  const archiveName = stripArchiveExtension(archiveFileName);

  // Check if all entries share a single top-level directory prefix
  const firstSlash = allEntries[0]?.path.indexOf('/') ?? -1;
  if (firstSlash > 0) {
    const commonPrefix = allEntries[0].path.slice(0, firstSlash + 1);
    const allNested = allEntries.every(
      (e) => e.path === commonPrefix || e.path.startsWith(commonPrefix)
    );
    if (allNested) {
      // All entries inside a single top-level folder -> extract to parent directory
      return parent;
    }
  }

  // Multiple top-level items -> create a named folder
  return parent + archiveName;
}

function getCurrentDirChildren(
  allEntries: EntryInfo[],
  prefix: string
): DirChild[] {
  const seen = new Set<string>();
  const result: DirChild[] = [];

  for (const entry of allEntries) {
    if (!entry.path.startsWith(prefix)) continue;

    const relative = entry.path.slice(prefix.length);
    if (relative === '') continue; // skip exact prefix match (the dir itself)

    const slashIdx = relative.indexOf('/');

    if (slashIdx === -1) {
      // Direct child — file or empty-named dir entry
      if (!seen.has(relative)) {
        seen.add(relative);
        result.push({
          name: relative,
          isDir: entry.is_dir,
          entry,
        });
      }
    } else {
      // Nested under a subdirectory
      const dirName = relative.slice(0, slashIdx + 1); // e.g. "photos/"
      if (!seen.has(dirName)) {
        seen.add(dirName);
        result.push({
          name: dirName,
          isDir: true,
          entry: entry, // use first entry for the dir, but isDir reflects the group
        });
      }
    }
  }

  // Sort: directories first, then alphabetically
  result.sort((a, b) => {
    if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
    return a.name.localeCompare(b.name);
  });

  return result;
}

function tabName(path: string): string {
  return path.split(/[/\\]/).pop() || path;
}

// --- TabArchive: per-archive reactive state ---

class TabArchive {
  id: string;

  archivePath = $state('');
  archivePassword = $state('');
  entries = $state.raw<EntryInfo[]>([]);
  currentDir = $state(''); // "" = root, "photos/" = subdir
  selectedPaths = $state.raw<Set<string>>(new Set());
  isLoading = $state(false);
  loadedCount = $state(0);
  totalCount = $state(0);
  error = $state<string | null>(null);
  previewState = $state<{
    path: string;
    content: string;
    kind: string;
    sizeHint: string;
  } | null>(null);

  constructor(id: string) {
    this.id = id;
  }

  // --- Derived (getters; reactive because they read $state fields) ---

  get currentChildren(): DirChild[] {
    return getCurrentDirChildren(this.entries, this.currentDir);
  }

  get selectedCount(): number {
    return this.selectedPaths.size;
  }

  get entryCount(): number {
    return this.entries.length;
  }

  get hasArchive(): boolean {
    return this.archivePath.length > 0;
  }

  get suggestedOutputDir(): string {
    return computeSuggestedDir(this.archivePath, this.entries);
  }

  // --- Methods ---

  async openArchive(path: string, password?: string): Promise<void> {
    this.isLoading = true;
    this.error = null;
    this.entries = [];
    this.loadedCount = 0;
    this.totalCount = 0;
    this.currentDir = '';
    this.selectedPaths = new Set();
    this.previewState = null;
    this.archivePassword = password ?? '';

    try {
      await listArchiveStream(
        path,
        (chunk) => {
          this.entries = [...this.entries, ...chunk.entries];
          this.loadedCount = this.entries.length;
          this.totalCount = chunk.total_entries;
        },
        password,
      );
      this.archivePath = path;
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      this.error = msg;
      this.archivePath = '';
    } finally {
      this.isLoading = false;
    }
  }

  navigateTo(dir: string): void {
    this.currentDir = dir;
  }

  toggleSelection(path: string): void {
    const next = new Set(this.selectedPaths);
    if (next.has(path)) {
      next.delete(path);
    } else {
      next.add(path);
    }
    this.selectedPaths = next;
  }

  selectAll(): void {
    const paths = this.currentChildren.map((c) =>
      this.currentDir ? this.currentDir + c.name : c.name
    );
    this.selectedPaths = new Set(paths);
  }

  clearSelection(): void {
    this.selectedPaths = new Set();
  }

  async showPreview(path: string): Promise<void> {
    try {
      const result: PreviewResult = await previewEntry(
        this.archivePath,
        path,
        this.archivePassword || undefined
      );
      this.previewState = {
        path: result.entry_path,
        content: result.content,
        kind: result.kind,
        sizeHint: result.size_hint,
      };
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      this.previewState = {
        path,
        content: msg,
        kind: 'error',
        sizeHint: '',
      };
    }
  }

  closePreview(): void {
    this.previewState = null;
  }

  resetArchive(): void {
    this.archivePath = '';
    this.archivePassword = '';
    this.entries = [];
    this.loadedCount = 0;
    this.totalCount = 0;
    this.currentDir = '';
    this.selectedPaths = new Set();
    this.isLoading = false;
    this.error = null;
    this.previewState = null;
  }
}

// --- archiveManager: multi-tab management ---

class ArchiveManager {
  tabs = $state.raw(new Map<string, TabArchive>());
  tabOrder = $state<string[]>([]);
  activeTabId = $state<string | null>(null);
  /** Tracks paths currently being opened to prevent duplicate tabs from concurrent calls. */
  _openingPaths = new Set<string>();

  get activeTab(): TabArchive | undefined {
    return this.activeTabId ? this.tabs.get(this.activeTabId) : undefined;
  }

  /** Open an archive in a new or existing tab. Returns the tab id and display label. */
  async openArchive(path: string, password?: string): Promise<{ tabId: string; label: string }> {
    // Deduplicate: reuse an existing tab for the same archive path
    for (const [id, tab] of this.tabs) {
      if (tab.archivePath === path) {
        this.activeTabId = id;
        return { tabId: id, label: tabName(path) };
      }
    }
    // Guard against concurrent opens of the same path
    // (e.g. cold-start + hot-start forwarding the same file simultaneously).
    if (this._openingPaths.has(path)) {
      // Wait briefly for the in-flight open to complete, then reuse the tab
      for (const [id, tab] of this.tabs) {
        if (tab.archivePath === path) {
          this.activeTabId = id;
          return { tabId: id, label: tabName(path) };
        }
      }
    }
    this._openingPaths.add(path);

    const tabId = crypto.randomUUID();
    const tab = new TabArchive(tabId);
    this.tabs.set(tabId, tab);
    this.tabOrder = [...this.tabOrder, tabId];
    this.activeTabId = tabId;

    try {
      await tab.openArchive(path, password);
      return { tabId, label: tabName(path) };
    } finally {
      this._openingPaths.delete(path);
    }
  }

  /** Close a tab. Returns the id of the next tab that should become active, or null. */
  closeTab(tabId: string): string | null {
    this.tabs.delete(tabId);
    this.tabOrder = this.tabOrder.filter((id) => id !== tabId);

    if (this.activeTabId === tabId) {
      // Pick the last remaining tab, or null
      const next = this.tabOrder.length > 0
        ? this.tabOrder[this.tabOrder.length - 1]
        : null;
      this.activeTabId = next;
      return next;
    }
    return this.activeTabId;
  }

  /** Switch the active tab. */
  setActive(tabId: string): void {
    if (this.tabs.has(tabId)) {
      this.activeTabId = tabId;
    }
  }
}

export const archiveManager = new ArchiveManager();

// --- archiveStore: proxy that delegates to the active tab ---

export const archiveStore = {
  get archivePath() {
    return archiveManager.activeTab?.archivePath ?? '';
  },
  get archivePassword() {
    return archiveManager.activeTab?.archivePassword ?? '';
  },
  get entries() {
    return archiveManager.activeTab?.entries ?? [];
  },
  get currentDir() {
    return archiveManager.activeTab?.currentDir ?? '';
  },
  get selectedPaths() {
    return archiveManager.activeTab?.selectedPaths ?? new Set<string>();
  },
  get isLoading() {
    return archiveManager.activeTab?.isLoading ?? false;
  },
  get loadedCount() {
    return archiveManager.activeTab?.loadedCount ?? 0;
  },
  get totalCount() {
    return archiveManager.activeTab?.totalCount ?? 0;
  },
  get error() {
    return archiveManager.activeTab?.error ?? null;
  },
  get previewState() {
    return archiveManager.activeTab?.previewState ?? null;
  },
  get currentChildren() {
    return archiveManager.activeTab?.currentChildren ?? [];
  },
  get selectedCount() {
    return archiveManager.activeTab?.selectedCount ?? 0;
  },
  get entryCount() {
    return archiveManager.activeTab?.entryCount ?? 0;
  },
  get hasArchive() {
    return archiveManager.activeTab?.hasArchive ?? false;
  },
  get suggestedOutputDir() {
    return archiveManager.activeTab?.suggestedOutputDir ?? '';
  },

  // Replace current tab content (called by ArchivePathBar "Change" button).
  // The caller should check archiveManager.activeTab first and use
  // archiveManager.openArchive() when no tab exists.
  async openArchive(path: string, password?: string) {
    await archiveManager.activeTab?.openArchive(path, password);
  },

  navigateTo(dir: string) {
    archiveManager.activeTab?.navigateTo(dir);
  },

  toggleSelection(path: string) {
    archiveManager.activeTab?.toggleSelection(path);
  },

  selectAll() {
    archiveManager.activeTab?.selectAll();
  },

  clearSelection() {
    archiveManager.activeTab?.clearSelection();
  },

  async showPreview(path: string) {
    await archiveManager.activeTab?.showPreview(path);
  },

  closePreview() {
    archiveManager.activeTab?.closePreview();
  },

  resetArchive() {
    archiveManager.activeTab?.resetArchive();
  },
};
