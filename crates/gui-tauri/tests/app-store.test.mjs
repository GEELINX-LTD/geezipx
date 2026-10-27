import assert from 'node:assert/strict';
import test from 'node:test';
import { derived, get } from 'svelte/internal/client';
import { runeModule } from './helpers/rune-module.mjs';

async function app() {
  return (await runeModule('stores/appStore.svelte.ts', {
    './archiveStore.svelte': { archiveManager: { tabs: new Map(), setActive() {} } },
  })).appStore;
}

function storage(t, value) {
  const previous = Object.getOwnPropertyDescriptor(globalThis, 'localStorage');
  Object.defineProperty(globalThis, 'localStorage', { configurable: true, value });
  t.after(() => {
    if (previous) Object.defineProperty(globalThis, 'localStorage', previous);
    else delete globalThis.localStorage;
  });
}

test('recent files reject invalid fields, duplicate keys, and excessive history', async t => {
  const valid = Array.from({ length: 30 }, (_, i) => ({ path: `/data/${i}.zip`, name: `${i}.zip`, timestamp: i }));
  const history = [
    { path: 123, name: 'invalid', timestamp: 1 },
    { path: '/invalid.zip', name: {}, timestamp: 2 },
    { path: '/other.zip', name: 'other.zip', timestamp: 'not a timestamp' },
    { path: '', name: '', timestamp: 0 },
    valid[0], valid[0], ...valid.slice(1),
  ];
  storage(t, { getItem: () => JSON.stringify(history), setItem() {} });
  const store = await app();
  store.loadRecent();
  assert.equal(store.recentFiles.length, 20);
  assert.equal(new Set(store.recentFiles.map(file => file.path)).size, 20);
  assert.ok(store.recentFiles.every(file => typeof file.path === 'string' && typeof file.name === 'string' && Number.isFinite(file.timestamp)));
});

test('recent file loading tolerates malformed JSON and storage failures', async t => {
  storage(t, { getItem: () => '{broken', setItem() {} });
  const store = await app();
  store.loadRecent();
  assert.deepEqual([...store.recentFiles], []);
  store.addRecent('/data/资料.zip');
  assert.equal(store.recentFiles[0].name, '资料.zip');
});

test('locale getter invalidates reactive consumers after switching language', async () => {
  let locale = 'en';
  const { localeStore } = await runeModule('stores/localeStore.svelte.ts', {
    '../i18n': { getLocale: () => locale, setLocale: value => { locale = value; }, t: key => key },
  });
  const current = derived(() => localeStore.locale);
  assert.equal(get(current), 'en');
  localeStore.switchLocale('zh-CN');
  assert.equal(get(current), 'zh-CN');
});
