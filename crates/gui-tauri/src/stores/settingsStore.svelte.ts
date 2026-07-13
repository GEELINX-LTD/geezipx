import { Store } from '@tauri-apps/plugin-store';
import type { GeeZipXSettings } from '../bridge';

const STORE_PATH = 'settings.json';

const DEFAULTS: GeeZipXSettings = {
  locale: 'zh-CN',
  default_output_dir: null,
  overwrite_strategy: 'prompt',
  default_format: 'zip',
  default_level: null,
  recursive: true,
  theme: 'system',
  on_complete: 'nothing',
};

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
    const result = { ...DEFAULTS };
    for (const key of Object.keys(DEFAULTS) as (keyof GeeZipXSettings)[]) {
      const val = await s.get<unknown>(key);
      if (val !== null && val !== undefined) {
        (result as Record<string, unknown>)[key] = val;
      }
    }
    return result;
  } catch {
    return { ...DEFAULTS };
  }
}

/** Persist all settings. */
async function saveAll(settings: GeeZipXSettings): Promise<void> {
  const s = await getStore();
  for (const key of Object.keys(DEFAULTS) as (keyof GeeZipXSettings)[]) {
    await s.set(key, settings[key]);
  }
  await s.save();
}

/** Get a single setting. */
async function get<K extends keyof GeeZipXSettings>(key: K): Promise<GeeZipXSettings[K]> {
  try {
    const s = await getStore();
    const val = await s.get<GeeZipXSettings[K]>(key);
    return val !== null && val !== undefined ? val : DEFAULTS[key];
  } catch {
    return DEFAULTS[key];
  }
}

export const settingsStore = {
  loadAll,
  saveAll,
  get,
  DEFAULTS,
};
