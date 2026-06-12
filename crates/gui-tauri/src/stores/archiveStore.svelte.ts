// Archive browser state store using Svelte 5 module-level $state runes.

import { listArchive, previewEntry } from '../bridge';
import type { EntryInfo, PreviewResult } from '../bridge';

// --- Types ---

interface DirChild {
  name: string;
  isDir: boolean;
  entry: EntryInfo;
}

// --- Reactive State ---

let archivePath = $state('');
let archivePassword = $state('');
let entries = $state<EntryInfo[]>([]);
let currentDir = $state(''); // "" = root, "photos/" = subdir
let selectedPaths = $state<Set<string>>(new Set());
let isLoading = $state(false);
let error = $state<string | null>(null);
let previewState = $state<{
  path: string;
  content: string;
  kind: string;
  sizeHint: string;
} | null>(null);

// --- Derived ---

let currentChildren = $derived(getCurrentDirChildren(entries, currentDir));
let selectedCount = $derived(selectedPaths.size);
let entryCount = $derived(entries.length);
let hasArchive = $derived(archivePath.length > 0);

// --- Helper: get children of current directory ---

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

// --- Functions ---

async function openArchive(path: string, password?: string): Promise<void> {
  isLoading = true;
  error = null;
  entries = [];
  currentDir = '';
  selectedPaths = new Set();
  previewState = null;
  archivePassword = password ?? '';

  try {
    const result = await listArchive(path, password);
    entries = result;
    archivePath = path;
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    error = msg;
    archivePath = '';
  } finally {
    isLoading = false;
  }
}

function navigateTo(dir: string): void {
  currentDir = dir;
}

function toggleSelection(path: string): void {
  const next = new Set(selectedPaths);
  if (next.has(path)) {
    next.delete(path);
  } else {
    next.add(path);
  }
  selectedPaths = next;
}

function selectAll(): void {
  const paths = currentChildren.map((c) =>
    currentDir ? currentDir + c.name : c.name
  );
  selectedPaths = new Set(paths);
}

function clearSelection(): void {
  selectedPaths = new Set();
}

async function showPreview(path: string): Promise<void> {
  try {
    const result: PreviewResult = await previewEntry(
      archivePath,
      path,
      archivePassword || undefined
    );
    previewState = {
      path: result.entry_path,
      content: result.content,
      kind: result.kind,
      sizeHint: result.size_hint,
    };
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    previewState = {
      path,
      content: msg,
      kind: 'error',
      sizeHint: '',
    };
  }
}

function closePreview(): void {
  previewState = null;
}

function resetArchive(): void {
  archivePath = '';
  archivePassword = '';
  entries = [];
  currentDir = '';
  selectedPaths = new Set();
  isLoading = false;
  error = null;
  previewState = null;
}

// --- Export ---

export const archiveStore = {
  get archivePath() {
    return archivePath;
  },
  get archivePassword() {
    return archivePassword;
  },
  get entries() {
    return entries;
  },
  get currentDir() {
    return currentDir;
  },
  get selectedPaths() {
    return selectedPaths;
  },
  get isLoading() {
    return isLoading;
  },
  get error() {
    return error;
  },
  get previewState() {
    return previewState;
  },
  get currentChildren() {
    return currentChildren;
  },
  get selectedCount() {
    return selectedCount;
  },
  get entryCount() {
    return entryCount;
  },
  get hasArchive() {
    return hasArchive;
  },
  openArchive,
  navigateTo,
  toggleSelection,
  selectAll,
  clearSelection,
  showPreview,
  closePreview,
  resetArchive,
};
