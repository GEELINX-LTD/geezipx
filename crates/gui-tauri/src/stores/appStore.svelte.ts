// Global application state store using Svelte 5 module-level $state runes.

const STORAGE_KEY = 'geezipx-recent';
const MAX_RECENT = 20;

// --- Reactive State ---
let activeTab = $state<string>('home');
let dropActive = $state(false);
let pendingSourcePaths = $state<string[]>([]);
let recentFiles = $state<{ path: string; name: string; timestamp: number }[]>([]);

// --- Derived ---
let isExtractTab = $derived(activeTab === 'extract');
let isCompressTab = $derived(activeTab === 'compress');

// --- Archive Tabs ---

let archiveTabs = $state.raw<{ id: string; label: string; tooltip: string }[]>([]);

// --- Functions ---

function switchTab(tab: string): void {
  activeTab = tab;
}

function addArchiveTab(id: string, label: string, tooltip: string): void {
  const exists = archiveTabs.find((t) => t.id === id);
  if (!exists) {
    archiveTabs = [...archiveTabs, { id, label, tooltip }];
  }
}

function removeArchiveTab(id: string): void {
  archiveTabs = archiveTabs.filter((t) => t.id !== id);
}

function setDropActive(active: boolean): void {
  dropActive = active;
}

function setPendingSourcePaths(paths: string[]): void {
  pendingSourcePaths = [...paths];
}

function consumePendingSourcePaths(): string[] {
  const paths = pendingSourcePaths;
  pendingSourcePaths = [];
  return paths;
}

function addShellCompressPaths(paths: string[]): void {
  const merged = [...pendingSourcePaths, ...paths.filter(p => !pendingSourcePaths.includes(p))];
  pendingSourcePaths = merged;
}

function addRecent(path: string): void {
  const name = path.split(/[/\\]/).pop() || path;
  const exists = recentFiles.findIndex((f) => f.path === path);

  if (exists !== -1) {
    // Update timestamp and move to front
    recentFiles.splice(exists, 1);
  }

  recentFiles.unshift({ path, name, timestamp: Date.now() });

  // Trim to max
  if (recentFiles.length > MAX_RECENT) {
    recentFiles.length = MAX_RECENT;
  }

  persistRecent();
}

function removeRecent(path: string): void {
  recentFiles = recentFiles.filter((f) => f.path !== path);
  persistRecent();
}

function loadRecent(): void {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored) {
      const parsed = JSON.parse(stored);
      if (Array.isArray(parsed)) {
        recentFiles = parsed.filter(
          (f: unknown): f is { path: string; name: string; timestamp: number } =>
            typeof f === 'object' &&
            f !== null &&
            'path' in f &&
            'name' in f &&
            'timestamp' in f
        );
      }
    }
  } catch {
    // Ignore corrupt localStorage data
    recentFiles = [];
  }
}

function persistRecent(): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(recentFiles));
  } catch {
    // Ignore storage failures
  }
}

// --- Export ---
export const appStore = {
  get activeTab() {
    return activeTab;
  },
  get dropActive() {
    return dropActive;
  },
  get pendingSourcePaths() {
    return pendingSourcePaths;
  },
  get recentFiles() {
    return recentFiles;
  },
  get isExtractTab() {
    return isExtractTab;
  },
  get isCompressTab() {
    return isCompressTab;
  },
  get archiveTabs() {
    return archiveTabs;
  },
  switchTab,
  addArchiveTab,
  removeArchiveTab,
  setDropActive,
  setPendingSourcePaths,
  consumePendingSourcePaths,
  addShellCompressPaths,
  addRecent,
  removeRecent,
  loadRecent,
};
