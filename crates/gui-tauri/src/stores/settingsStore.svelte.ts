import { Store } from '@tauri-apps/plugin-store';
import type { GeeZipXSettings, ShellMenuVerb } from '../bridge';

const STORE_PATH = 'settings.json';
const SHELL_MENU_VERBS: ShellMenuVerb[] = [
  'extract',
  'extract_here',
  'compress_zip',
  'compress',
];

const DEFAULTS: GeeZipXSettings = {
  locale: 'zh-CN',
  default_output_dir: null,
  overwrite_strategy: 'prompt',
  default_format: 'zip',
  default_level: null,
  recursive: true,
  theme: 'system',
  on_complete: 'nothing',
  shell_menu_enabled: true,
  shell_menu_verbs: [...SHELL_MENU_VERBS],
};

function isShellMenuVerb(value: unknown): value is ShellMenuVerb {
  return typeof value === 'string' && SHELL_MENU_VERBS.includes(value as ShellMenuVerb);
}

function normalizeShellMenuVerbs(value: unknown): ShellMenuVerb[] {
  if (!Array.isArray(value)) return [...SHELL_MENU_VERBS];
  const selected = new Set(value.filter(isShellMenuVerb));
  return SHELL_MENU_VERBS.filter((verb) => selected.has(verb));
}

function cloneDefaults(): GeeZipXSettings {
  return { ...DEFAULTS, shell_menu_verbs: [...DEFAULTS.shell_menu_verbs] };
}

function normalizeSettings(value: Partial<Record<keyof GeeZipXSettings, unknown>>): GeeZipXSettings {
  const result = cloneDefaults();
  if (value.locale === 'en' || value.locale === 'zh-CN') result.locale = value.locale;
  if (typeof value.default_output_dir === 'string' && value.default_output_dir.trim()) {
    result.default_output_dir = value.default_output_dir;
  }
  if (value.overwrite_strategy === 'prompt' || value.overwrite_strategy === 'skip' || value.overwrite_strategy === 'overwrite') {
    result.overwrite_strategy = value.overwrite_strategy;
  }
  if (typeof value.default_format === 'string' && /^[a-z0-9]+(?:\.[a-z0-9]+)*$/.test(value.default_format)) {
    result.default_format = value.default_format;
  }
  if (typeof value.default_level === 'number' && Number.isInteger(value.default_level) && value.default_level >= 0 && value.default_level <= 22) {
    result.default_level = value.default_level;
  }
  if (typeof value.recursive === 'boolean') result.recursive = value.recursive;
  if (value.theme === 'system' || value.theme === 'light' || value.theme === 'dark') result.theme = value.theme;
  if (value.on_complete === 'nothing' || value.on_complete === 'open_output') result.on_complete = value.on_complete;
  if (typeof value.shell_menu_enabled === 'boolean') result.shell_menu_enabled = value.shell_menu_enabled;
  result.shell_menu_verbs = normalizeShellMenuVerbs(value.shell_menu_verbs);
  return result;
}

async function loadStore(): Promise<Store | null> {
  try {
    const loadedStore = await Store.load(STORE_PATH);

    // Remove legacy password settings without reading their values. Cleanup is
    // best-effort so a store write failure cannot block application startup.
    try {
      const hasDefaultPassword = await loadedStore.has('default_password');
      const hasRememberPassword = await loadedStore.has('remember_password');
      if (hasDefaultPassword || hasRememberPassword) {
        if (hasDefaultPassword) await loadedStore.delete('default_password');
        if (hasRememberPassword) await loadedStore.delete('remember_password');
        await loadedStore.save();
      }
    } catch {
      // Cleanup is retried on the next launch.
    }

    return loadedStore;
  } catch {
    return null;
  }
}

// Start loading and cleanup when this module is imported by the app.
const storePromise = loadStore();

async function getStore(): Promise<Store> {
  const loadedStore = await storePromise;
  if (!loadedStore) throw new Error('Settings store is unavailable');
  return loadedStore;
}

/** Load all settings, filling unset keys with defaults. */
async function loadAll(): Promise<GeeZipXSettings> {
  try {
    const s = await getStore();
    const values: Partial<Record<keyof GeeZipXSettings, unknown>> = {};
    for (const key of Object.keys(DEFAULTS) as (keyof GeeZipXSettings)[]) {
      values[key] = await s.get<unknown>(key);
    }
    return normalizeSettings(values);
  } catch {
    return cloneDefaults();
  }
}

let saveQueue: Promise<void> = Promise.resolve();

/** Persist a validated snapshot, keeping concurrent saves in call order. */
function saveAll(settings: GeeZipXSettings): Promise<void> {
  const snapshot = normalizeSettings(settings);
  const saving = saveQueue.then(async () => {
    const s = await getStore();
    for (const key of Object.keys(DEFAULTS) as (keyof GeeZipXSettings)[]) {
      await s.set(key, snapshot[key]);
    }
    await s.save();
  });
  saveQueue = saving.catch(() => {});
  return saving;
}

/** Get a single setting. */
async function get<K extends keyof GeeZipXSettings>(key: K): Promise<GeeZipXSettings[K]> {
  try {
    const s = await getStore();
    const val = await s.get<unknown>(key);
    return normalizeSettings({ [key]: val })[key];
  } catch {
    return cloneDefaults()[key];
  }
}

export const settingsStore = {
  loadAll,
  saveAll,
  get,
  DEFAULTS,
};
