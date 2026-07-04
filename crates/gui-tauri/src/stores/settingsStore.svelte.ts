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
};

let store: Store | null = null;

async function getStore(): Promise<Store> {
  if (!store) store = await Store.load(STORE_PATH);
  return store;
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
  for (const [key, value] of Object.entries(settings)) {
    await s.set(key, value);
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
