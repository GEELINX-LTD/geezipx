import assert from 'node:assert/strict';
import test from 'node:test';
import { runeModule } from './helpers/rune-module.mjs';
import { deferred, settle } from './helpers/component-script.mjs';

async function settings(values = {}, overrides = {}) {
  const data = new Map(Object.entries(values));
  const writes = [];
  const native = {
    has: async key => data.has(key),
    get: async key => data.get(key),
    delete: async key => data.delete(key),
    set: async (key, value) => { writes.push([key, value]); data.set(key, value); },
    save: async () => {},
    ...overrides,
  };
  const { settingsStore } = await runeModule('stores/settingsStore.svelte.ts', {
    '@tauri-apps/plugin-store': { Store: { load: async () => native } },
  });
  return { settingsStore, data, writes };
}

test('malformed persisted settings never reach UI or command parameters through either read API', async () => {
  const invalid = {
    locale: { unexpected: true }, default_output_dir: 123, overwrite_strategy: 'invalid',
    default_format: [], default_level: '9', recursive: 'false', theme: false,
    on_complete: ['open_output'], shell_menu_enabled: 0, shell_menu_verbs: 'extract',
  };
  const { settingsStore: store } = await settings(invalid);
  assert.deepEqual(await store.loadAll(), store.DEFAULTS);
  for (const key of Object.keys(invalid)) assert.deepEqual(await store.get(key), store.DEFAULTS[key], key);
});

test('levels must be finite integers within range and output paths must be nonempty strings', async () => {
  for (const level of [NaN, Infinity, -1, 23, 1.5]) {
    const { settingsStore: store } = await settings({ default_level: level, default_output_dir: '   ' });
    assert.equal(await store.get('default_level'), null);
    assert.equal(await store.get('default_output_dir'), null);
  }
});

test('valid settings and selected shell verbs survive and callers receive independent arrays', async () => {
  const { settingsStore: store } = await settings({ locale: 'en', default_level: 0, recursive: false, shell_menu_verbs: ['compress', 'bad', 'compress', 'extract'] });
  const loaded = await store.loadAll();
  assert.equal(loaded.locale, 'en');
  assert.equal(loaded.default_level, 0);
  assert.equal(loaded.recursive, false);
  assert.deepEqual(loaded.shell_menu_verbs, ['extract', 'compress']);
  assert.deepEqual(await store.get('shell_menu_verbs'), ['extract', 'compress']);
  loaded.shell_menu_verbs.push('compress_zip');
  assert.deepEqual(await store.get('shell_menu_verbs'), ['extract', 'compress']);
});

test('legacy secrets are deleted without reading their values', async () => {
  const readKeys = [];
  const { settingsStore: store, data } = await settings({ default_password: 'secret', remember_password: true }, {
    get: async key => { readKeys.push(key); return null; },
  });
  await store.loadAll();
  assert.equal(data.has('default_password'), false);
  assert.equal(data.has('remember_password'), false);
  assert.ok(!readKeys.includes('default_password'));
  assert.ok(!readKeys.includes('remember_password'));
});

test('save captures settings before awaiting the native store', async () => {
  const { settingsStore: store, data } = await settings();
  const snapshot = { ...store.DEFAULTS, shell_menu_verbs: ['extract'] };
  const saved = store.saveAll(snapshot);
  snapshot.default_format = '7z';
  snapshot.shell_menu_verbs.push('compress');
  await saved;
  assert.equal(data.get('default_format'), 'zip');
  assert.deepEqual(data.get('shell_menu_verbs'), ['extract']);
});

test('concurrent saves stay ordered and a failed save does not poison later saves', async () => {
  const gate = deferred();
  const writes = [];
  let first = true;
  let fail = false;
  const { settingsStore: store } = await settings({}, {
    set: async (key, value) => {
      if (first) { first = false; await gate.promise; }
      writes.push([key, value]);
    },
    save: async () => { if (fail) { fail = false; throw new Error('disk failure'); } },
  });
  const a = store.saveAll({ ...store.DEFAULTS, locale: 'en' });
  const b = store.saveAll({ ...store.DEFAULTS, locale: 'zh-CN' });
  await settle();
  assert.deepEqual(writes, []);
  gate.resolve();
  await Promise.all([a, b]);
  assert.deepEqual(writes.filter(([key]) => key === 'locale').map(([, value]) => value), ['en', 'zh-CN']);
  fail = true;
  await assert.rejects(store.saveAll(store.DEFAULTS), /disk failure/);
  await store.saveAll(store.DEFAULTS);
});
