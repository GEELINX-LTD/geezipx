// Archive browser state store using Svelte 5 class-based state runes.
//
// Refactored: TabArchive manages per-archive state; archiveManager owns the tab set;
// archiveStore remains a proxy for the active tab so that all consuming components
// (ExtractPage, Breadcrumb, BrowserTable, etc.) need zero changes.

import { SvelteMap, SvelteSet } from 'svelte/reactivity';
import { listArchiveStream, previewEntry } from '../bridge';
import { stripArchiveExtension } from '../archiveFormats';
import type { EntryInfo, PreviewResult } from '../bridge';

// --- Types ---

export interface DirChild {
  name: string;
  isDir: boolean;
  entry: EntryInfo;
}

export interface ArchivePasswordChallenge {
  id: number;
  path: string;
  isSubmitting: boolean;
  incorrectPassword: boolean;
  closeOnCancel: boolean;
}

type OpenArchiveStatus =
  | 'opened'
  | 'password-required'
  | 'failed'
  | 'cancelled';

interface OpenSnapshot {
  error: string | null;
  loadedCount: number;
  totalCount: number;
}

// --- Helper functions (pure, top-level) ---

function parentDir(filePath: string): string {
  const sep = filePath.includes('\\') ? '\\' : '/';
  const idx = filePath.lastIndexOf(sep);
  return idx >= 0 ? filePath.slice(0, idx + 1) : '';
}

function computeSuggestedDir(archivePath: string, allEntries: EntryInfo[]): string {
  if (!archivePath || allEntries.length === 0) return '';

  const parent = parentDir(archivePath);
  const archiveFileName = archivePath.split(/[/\\]/).pop() || 'archive';
  const archiveName = stripArchiveExtension(archiveFileName);

  // Normalize only comparison paths; preserve the original archive entry paths.
  const normalizeEntryPath = (path: string): string =>
    path.replace(/\\/g, '/').replace(/\/+$/, '');
  const first = allEntries[0];
  const firstPath = normalizeEntryPath(first.path);
  const firstSlash = firstPath.indexOf('/');
  const topLevelDir = firstSlash > 0
    ? firstPath.slice(0, firstSlash)
    : first.is_dir ? firstPath : '';

  if (topLevelDir) {
    const commonPrefix = `${topLevelDir}/`;
    const allNested = allEntries.every((entry) => {
      const path = normalizeEntryPath(entry.path);
      return path.startsWith(commonPrefix) || (path === topLevelDir && entry.is_dir);
    });
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
  const seen = new SvelteSet<string>();
  const result: DirChild[] = [];

  for (const entry of allEntries) {
    // Normalize only the virtual hierarchy; keep entry.path for archive lookups.
    const path = entry.path.replace(/\\/g, '/');
    if (!path.startsWith(prefix)) continue;

    const entryPath = entry.is_dir && !path.endsWith('/') ? `${path}/` : path;
    const relative = entryPath.slice(prefix.length);
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

const READ_ENTRIES_CRYPTO_PREFIX = 'Failed to read entries: crypto error: ';
const PASSWORD_ERROR_DETAILS = new Set([
  'ZIP password required: Password required to decrypt file',
  'invalid ZIP password',
  'encrypted 7z archive requires a password (use --password)',
  'invalid password for 7z archive',
  'RAR error: Password for encrypted archive not specified',
  'RAR error: Wrong password was specified',
]);

function isPasswordError(message: string): boolean {
  if (!message.startsWith(READ_ENTRIES_CRYPTO_PREFIX)) return false;
  const detail = message.slice(READ_ENTRIES_CRYPTO_PREFIX.length).trim();
  return PASSWORD_ERROR_DETAILS.has(detail);
}

// --- TabArchive: per-archive reactive state ---

class TabArchive {
  id: string;

  archivePath = $state('');
  archivePassword = $state('');
  entries = $state.raw<EntryInfo[]>([]);
  currentDir = $state(''); // "" = root, "photos/" = subdir
  selectedPaths = $state.raw<Set<string>>(new SvelteSet());
  isLoading = $state(false);
  loadedCount = $state(0);
  totalCount = $state(0);
  error = $state<string | null>(null);
  pendingArchivePath = $state('');
  passwordChallenge = $state<ArchivePasswordChallenge | null>(null);
  previewState = $state<{
    path: string;
    content: string;
    kind: string;
    sizeHint: string;
  } | null>(null);

  private requestGeneration = 0;
  private previewGeneration = 0;
  private challengeId = 0;
  private openSnapshot: OpenSnapshot | null = null;
  private openWaiters = new Set<(opened: boolean) => void>();

  constructor(id: string) {
    this.id = id;
  }

  // --- Derived (getters; reactive because they read $state fields) ---

  private children = $derived(getCurrentDirChildren(this.entries, this.currentDir));
  private outputDir = $derived(computeSuggestedDir(this.archivePath, this.entries));

  get currentChildren(): DirChild[] {
    return this.children;
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
    return this.outputDir;
  }

  // --- Methods ---

  private resolveOpenWaiters(opened: boolean): void {
    for (const resolve of this.openWaiters) resolve(opened);
    this.openWaiters.clear();
  }

  private restoreOpenSnapshot(): void {
    if (!this.openSnapshot) return;
    this.error = this.openSnapshot.error;
    this.loadedCount = this.openSnapshot.loadedCount;
    this.totalCount = this.openSnapshot.totalCount;
  }

  private abortPendingOpen(): void {
    if (!this.pendingArchivePath && !this.passwordChallenge && !this.isLoading) return;
    this.requestGeneration++;
    this.restoreOpenSnapshot();
    this.pendingArchivePath = '';
    this.passwordChallenge = null;
    this.isLoading = false;
    this.openSnapshot = null;
    this.resolveOpenWaiters(false);
  }

  private async tryOpenArchive(
    path: string,
    password: string | undefined,
    generation: number,
  ): Promise<OpenArchiveStatus> {
    const nextEntries: EntryInfo[] = [];
    this.isLoading = true;
    this.error = null;
    this.loadedCount = 0;
    this.totalCount = 0;

    try {
      await listArchiveStream(
        path,
        (chunk) => {
          if (generation !== this.requestGeneration) return;
          nextEntries.push(...chunk.entries);
          this.loadedCount = nextEntries.length;
          this.totalCount = chunk.total_entries;
        },
        password,
      );

      if (generation !== this.requestGeneration) return 'cancelled';

      this.archivePath = path;
      this.archivePassword = password ?? '';
      this.entries = nextEntries;
      this.currentDir = '';
      this.selectedPaths = new SvelteSet();
      this.closePreview();
      this.pendingArchivePath = '';
      this.passwordChallenge = null;
      this.openSnapshot = null;
      this.isLoading = false;
      this.resolveOpenWaiters(true);
      return 'opened';
    } catch (e) {
      if (generation !== this.requestGeneration) return 'cancelled';

      const msg = e instanceof Error ? e.message : String(e);
      this.isLoading = false;

      if (isPasswordError(msg)) {
        this.restoreOpenSnapshot();
        const existingChallenge = this.passwordChallenge;
        this.error = null;
        this.passwordChallenge = {
          id: existingChallenge?.id ?? ++this.challengeId,
          path,
          isSubmitting: false,
          incorrectPassword: Boolean(password),
          closeOnCancel: existingChallenge?.closeOnCancel ?? !this.hasArchive,
        };
        return 'password-required';
      }

      this.restoreOpenSnapshot();
      this.error = msg;
      this.pendingArchivePath = '';
      this.passwordChallenge = null;
      this.openSnapshot = null;
      this.resolveOpenWaiters(false);
      return 'failed';
    }
  }

  async openArchive(path: string, password?: string): Promise<OpenArchiveStatus> {
    this.abortPendingOpen();
    this.closePreview();
    this.openSnapshot = {
      error: this.error,
      loadedCount: this.loadedCount,
      totalCount: this.totalCount,
    };
    this.pendingArchivePath = path;
    const generation = ++this.requestGeneration;
    return this.tryOpenArchive(path, password, generation);
  }

  async submitPassword(password: string): Promise<OpenArchiveStatus> {
    const challenge = this.passwordChallenge;
    if (!challenge || !password || challenge.isSubmitting) return 'cancelled';

    this.passwordChallenge = {
      ...challenge,
      isSubmitting: true,
      incorrectPassword: false,
    };
    const generation = ++this.requestGeneration;
    return this.tryOpenArchive(challenge.path, password, generation);
  }

  cancelPasswordChallenge(): void {
    if (!this.passwordChallenge && !this.pendingArchivePath) return;
    this.requestGeneration++;
    this.restoreOpenSnapshot();
    this.pendingArchivePath = '';
    this.passwordChallenge = null;
    this.isLoading = false;
    this.openSnapshot = null;
    this.resolveOpenWaiters(false);
  }

  waitForOpenResult(path: string): Promise<boolean> {
    if (this.archivePath === path && this.pendingArchivePath !== path) {
      return Promise.resolve(true);
    }
    if (this.pendingArchivePath !== path) return Promise.resolve(false);
    return new Promise((resolve) => this.openWaiters.add(resolve));
  }

  navigateTo(dir: string): void {
    this.currentDir = dir;
  }

  toggleSelection(path: string): void {
    const next = new SvelteSet(this.selectedPaths);
    if (next.has(path)) {
      next.delete(path);
    } else {
      next.add(path);
    }
    this.selectedPaths = next;
  }

  selectAll(): void {
    const paths = this.currentChildren.map((c) =>
      c.isDir ? this.currentDir + c.name : c.entry.path
    );
    this.selectedPaths = new SvelteSet(paths);
  }

  clearSelection(): void {
    this.selectedPaths = new SvelteSet();
  }

  async showPreview(path: string): Promise<void> {
    const generation = ++this.previewGeneration;
    const archivePath = this.archivePath;
    try {
      const result: PreviewResult = await previewEntry(
        archivePath,
        path,
        this.archivePassword || undefined
      );
      if (generation !== this.previewGeneration || archivePath !== this.archivePath) return;
      this.previewState = {
        path: result.entry_path,
        content: result.content,
        kind: result.kind,
        sizeHint: result.size_hint,
      };
    } catch (e) {
      if (generation !== this.previewGeneration || archivePath !== this.archivePath) return;
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
    this.previewGeneration++;
    this.previewState = null;
  }

  resetArchive(): void {
    this.requestGeneration++;
    this.previewGeneration++;
    this.resolveOpenWaiters(false);
    this.archivePath = '';
    this.archivePassword = '';
    this.entries = [];
    this.loadedCount = 0;
    this.totalCount = 0;
    this.currentDir = '';
    this.selectedPaths = new SvelteSet();
    this.isLoading = false;
    this.error = null;
    this.pendingArchivePath = '';
    this.passwordChallenge = null;
    this.previewState = null;
    this.openSnapshot = null;
  }
}

// --- archiveManager: multi-tab management ---

class ArchiveManager {
  tabs = new SvelteMap<string, TabArchive>();
  tabOrder = $state<string[]>([]);
  activeTabId = $state<string | null>(null);

  get activeTab(): TabArchive | undefined {
    return this.activeTabId ? this.tabs.get(this.activeTabId) : undefined;
  }

  /** Open an archive in a new or existing tab. Returns the tab id and display label. */
  async openArchive(
    path: string,
    password?: string,
    targetTabId?: string,
  ): Promise<{ tabId: string; label: string }> {
    // Pending paths are registered synchronously before returning the tab, so
    // concurrent callers share it without retaining a promise for a closed tab.
    for (const [id, tab] of this.tabs) {
      if (tab.pendingArchivePath === path || (!tab.pendingArchivePath && tab.archivePath === path)) {
        return { tabId: id, label: tabName(path) };
      }
    }

    const existingTarget = targetTabId ? this.tabs.get(targetTabId) : undefined;
    const tabId = existingTarget?.id ?? crypto.randomUUID();
    const tab = existingTarget ?? new TabArchive(tabId);
    if (!existingTarget) {
      this.tabs.set(tabId, tab);
      this.tabOrder = [...this.tabOrder, tabId];
    }
    // Let callers display loading state immediately. Actions that need entries
    // must waitForOpenResult, which also covers password retries and tab closure.
    void tab.openArchive(path, password);
    return { tabId, label: tabName(path) };
  }

  async submitPassword(tabId: string, password: string): Promise<boolean> {
    const tab = this.tabs.get(tabId);
    if (!tab) return false;
    return (await tab.submitPassword(password)) === 'opened';
  }

  cancelPasswordChallenge(tabId: string): { closed: boolean; nextTabId: string | null } {
    const tab = this.tabs.get(tabId);
    if (!tab) return { closed: false, nextTabId: this.activeTabId };

    const closeOnCancel = tab.passwordChallenge?.closeOnCancel ?? false;
    tab.cancelPasswordChallenge();
    if (!closeOnCancel) return { closed: false, nextTabId: this.activeTabId };

    return { closed: true, nextTabId: this.closeTab(tabId) };
  }

  waitForOpenResult(tabId: string, path: string): Promise<boolean> {
    const tab = this.tabs.get(tabId);
    return tab ? tab.waitForOpenResult(path) : Promise.resolve(false);
  }

  /** Close a tab. Returns the id of the next tab that should become active, or null. */
  closeTab(tabId: string): string | null {
    this.tabs.get(tabId)?.resetArchive();
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
    return archiveManager.activeTab?.selectedPaths ?? new SvelteSet<string>();
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
  get passwordChallenge() {
    return archiveManager.activeTab?.passwordChallenge ?? null;
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
    await archiveManager.openArchive(path, password, archiveManager.activeTabId ?? undefined);
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
