import assert from 'node:assert/strict';
import test from 'node:test';
import { derived, get } from 'svelte/internal/client';
import { runeModule } from './helpers/rune-module.mjs';
import { deferred, settle } from './helpers/component-script.mjs';

const passwordError = 'Failed to read entries: crypto error: invalid ZIP password';
const entry = (path, is_dir = false) => ({ path, is_dir, size: 1, compressed_size: 1, modified: null, crc32: null });

async function browser(overrides = {}) {
  return runeModule('stores/archiveStore.svelte.ts', {
    '../archiveFormats': await runeModule('archiveFormats.ts'),
    '../bridge': {
      listArchiveStream: async (_path, chunk) => chunk({ entries: [entry('a.txt')], total_entries: 1 }),
      previewEntry: async (_archive, path) => ({ entry_path: path, content: 'preview', kind: 'text', size_hint: '' }),
      ...overrides,
    },
  });
}

async function opened(manager, path = '/tmp/a.zip', targetTabId) {
  const result = await manager.openArchive(path, undefined, targetTabId);
  assert.equal(await manager.waitForOpenResult(result.tabId, path), true);
  return manager.tabs.get(result.tabId);
}

test('new archive returns its tab while listing is pending so loading and closing are available', async () => {
  const pending = deferred();
  const { archiveManager: manager } = await browser({ listArchiveStream: () => pending.promise });
  let result;
  const opening = manager.openArchive('/tmp/a.zip').then(value => { result = value; });
  await settle();
  try {
    assert.ok(result, 'tab metadata must be available before native listing completes');
    assert.equal(manager.tabs.get(result.tabId).isLoading, true);
  } finally {
    pending.resolve();
    await opening;
  }
});

test('tab map size is reactive when tabs are added and removed', async () => {
  const { archiveManager: manager } = await browser();
  const size = derived(() => manager.tabs.size);
  assert.equal(get(size), 0);
  const tab = await opened(manager);
  assert.equal(get(size), 1);
  manager.closeTab(tab.id);
  assert.equal(get(size), 0);
});

test('closing then reopening a pending path creates a live tab and ignores old chunks', async () => {
  const first = deferred();
  let oldChunk;
  let calls = 0;
  const { archiveManager: manager } = await browser({
    listArchiveStream: async (_path, chunk) => {
      if (++calls === 1) { oldChunk = chunk; await first.promise; }
      else chunk({ entries: [entry('new.txt')], total_entries: 1 });
    },
  });
  const oldOpening = manager.openArchive('/tmp/a.zip');
  const oldId = manager.tabOrder[0];
  manager.closeTab(oldId);
  const freshOpening = manager.openArchive('/tmp/a.zip');
  first.resolve();
  const [old, fresh] = await Promise.all([oldOpening, freshOpening]);
  assert.notEqual(fresh.tabId, old.tabId);
  oldChunk({ entries: [entry('stale.txt')], total_entries: 1 });
  await settle();
  assert.equal(manager.tabs.get(fresh.tabId)?.archivePath, '/tmp/a.zip');
  assert.deepEqual(manager.tabs.get(fresh.tabId)?.entries.map(item => item.path), ['new.txt']);
});

test('same-path reload discards an earlier pending preview', async () => {
  const preview = deferred();
  const { archiveManager: manager } = await browser({ previewEntry: () => preview.promise });
  const tab = await opened(manager);
  const pending = tab.showPreview('old.txt');
  await tab.openArchive('/tmp/a.zip');
  preview.resolve({ entry_path: 'old.txt', content: 'stale', kind: 'text', size_hint: '' });
  await pending;
  assert.equal(tab.previewState, null);
});

test('directory results are cached across selection and invalidated on navigation or replacement', async () => {
  const entries = Array.from({ length: 50000 }, (_, i) => entry(`folder/${i}.txt`));
  const { archiveManager: manager } = await browser({
    listArchiveStream: async (_path, chunk) => chunk({ entries, total_entries: entries.length }),
  });
  const tab = await opened(manager);
  const children = tab.currentChildren;
  assert.strictEqual(tab.currentChildren, children);
  tab.toggleSelection('folder/');
  assert.strictEqual(tab.currentChildren, children);
  tab.navigateTo('folder/');
  assert.equal(tab.currentChildren.length, 50000);
  assert.strictEqual(tab.currentChildren, tab.currentChildren);
  tab.entries = [entry('folder/replaced.txt')];
  assert.deepEqual(tab.currentChildren.map(item => item.name), ['replaced.txt']);
});

test('directory entries without a trailing slash merge with their implicit children', async () => {
  const { archiveManager: manager } = await browser({
    listArchiveStream: async (_path, chunk) => chunk({ entries: [entry('folder', true), entry('folder/file.txt')], total_entries: 2 }),
  });
  const tab = await opened(manager);
  assert.deepEqual(tab.currentChildren.map(item => item.name), ['folder/']);
  tab.navigateTo(tab.currentChildren[0].name);
  assert.deepEqual(tab.currentChildren.map(item => item.name), ['file.txt']);
});

for (const [path, expected] of [['/a.zip', '/a'], ['/tmp/disc.iso', '/tmp/disc'], ['C:\\disc.zpaq', 'C:\\disc']]) {
  test(`suggested extraction folder for ${path} is safe`, async () => {
    const { archiveManager: manager } = await browser();
    const tab = await opened(manager, path);
    assert.equal(tab.suggestedOutputDir, expected);
  });
}

for (const [name, entries] of [
  ['explicit 7z directory before its children', [entry('folder', true), entry('folder/file.txt')]],
  ['explicit 7z directory after its children', [entry('folder/file.txt'), entry('folder', true)]],
  ['ZIP-style trailing directory slash', [entry('folder/', true), entry('folder/file.txt')]],
  ['Windows archive entry separators', [entry('folder', true), entry('folder\\nested\\file.txt')]],
  ['an empty explicit directory', [entry('folder', true)]],
]) {
  test(`single-root extraction avoids another folder for ${name}`, async () => {
    const { archiveManager: manager } = await browser({
      listArchiveStream: async (_path, chunk) => chunk({ entries, total_entries: entries.length }),
    });
    const tab = await opened(manager, 'C:\\data\\folder.7z');
    assert.equal(tab.suggestedOutputDir, 'C:\\data\\');
  });
}

for (const [name, entries] of [
  ['multiple roots sharing a partial prefix', [entry('folder', true), entry('folder/file.txt'), entry('folder-other/file.txt')]],
  ['a loose file beside the root directory', [entry('folder', true), entry('folder/file.txt'), entry('readme.txt')]],
  ['a bare file cannot stand in for a root directory', [entry('folder'), entry('folder/file.txt')]],
]) {
  test(`extraction creates a containing folder for ${name}`, async () => {
    const { archiveManager: manager } = await browser({
      listArchiveStream: async (_path, chunk) => chunk({ entries, total_entries: entries.length }),
    });
    const tab = await opened(manager, 'C:\\data\\archive.7z');
    assert.equal(tab.suggestedOutputDir, 'C:\\data\\archive');
  });
}

for (const [name, entries] of [
  ['implicit Windows directories', [entry('资料\\子目录\\说明.txt'), entry('资料\\根文件.txt'), entry('readme.txt')]],
  ['explicit Windows directories', [entry('资料\\', true), entry('资料\\子目录\\', true), entry('资料\\子目录\\说明.txt'), entry('资料\\根文件.txt'), entry('readme.txt')]],
  ['mixed separators and directory records after their files', [entry('资料\\子目录/说明.txt'), entry('资料/根文件.txt'), entry('资料/子目录', true), entry('资料\\', true), entry('readme.txt')]],
]) {
  test(`ZIP browsing shows one level at a time for ${name}`, async () => {
    const { archiveManager: manager } = await browser({
      listArchiveStream: async (_path, chunk) => chunk({ entries, total_entries: entries.length }),
    });
    const tab = await opened(manager);
    assert.deepEqual(tab.currentChildren.map(item => [item.name, item.isDir]), [['资料/', true], ['readme.txt', false]]);
    tab.navigateTo('资料/');
    assert.deepEqual(tab.currentChildren.map(item => [item.name, item.isDir]), [['子目录/', true], ['根文件.txt', false]]);
    tab.navigateTo('资料/子目录/');
    assert.deepEqual(tab.currentChildren.map(item => [item.name, item.isDir]), [['说明.txt', false]]);
    const leaf = entries.find(item => item.path.endsWith('说明.txt'));
    assert.strictEqual(tab.currentChildren[0].entry, leaf);
    tab.navigateTo('资料/');
    assert.equal(tab.currentDir, '资料/');
    assert.deepEqual(tab.currentChildren.map(item => item.name), ['子目录/', '根文件.txt']);
  });
}

test('legacy Windows ZIP selection and preview retain the original entry name', async () => {
  const entries = [entry('资料\\子目录\\说明.txt'), entry('资料\\根文件.txt'), entry('readme.txt')];
  const previews = [];
  const { archiveManager: manager } = await browser({
    listArchiveStream: async (_path, chunk) => chunk({ entries, total_entries: entries.length }),
    previewEntry: async (archive, path) => {
      previews.push([archive, path]);
      return { entry_path: path, content: 'original file', kind: 'text', size_hint: '' };
    },
  });
  const tab = await opened(manager);
  tab.navigateTo('资料/');
  tab.selectAll();
  assert.deepEqual([...tab.selectedPaths], ['资料/子目录/', '资料\\根文件.txt']);
  tab.navigateTo('资料/子目录/');
  tab.selectAll();
  assert.deepEqual([...tab.selectedPaths], ['资料\\子目录\\说明.txt']);
  await tab.showPreview(tab.currentChildren[0].entry.path);
  assert.deepEqual(previews, [['/tmp/a.zip', '资料\\子目录\\说明.txt']]);
  assert.equal(tab.previewState.content, 'original file');
  assert.deepEqual(tab.entries.map(item => item.path), entries.map(item => item.path));
});

test('password retries preserve the committed archive and cancellation resolves waiters', async () => {
  const { archiveManager: manager } = await browser({
    listArchiveStream: async (path, chunk, password) => {
      if (path.endsWith('locked.zip') && password !== 'right') throw new Error(passwordError);
      chunk({ entries: [entry('a.txt')], total_entries: 1 });
    },
  });
  const tab = await opened(manager);
  await tab.openArchive('/tmp/locked.zip');
  assert.equal(tab.passwordChallenge.closeOnCancel, false);
  assert.equal(tab.archivePath, '/tmp/a.zip');
  const cancelled = tab.waitForOpenResult('/tmp/locked.zip');
  await tab.submitPassword('wrong');
  assert.equal(tab.passwordChallenge.incorrectPassword, true);
  tab.cancelPasswordChallenge();
  assert.equal(await cancelled, false);
  assert.equal(tab.archivePath, '/tmp/a.zip');
  await tab.openArchive('/tmp/locked.zip');
  const unlocked = tab.waitForOpenResult('/tmp/locked.zip');
  await tab.submitPassword('right');
  assert.equal(await unlocked, true);
  assert.equal(tab.archivePassword, 'right');
  assert.equal(tab.passwordChallenge, null);
});

test('superseded listing cannot overwrite a newer archive or its counters', async () => {
  const old = deferred();
  let emitOld;
  const { archiveManager: manager } = await browser({
    listArchiveStream: async (path, chunk) => {
      if (path.endsWith('old.zip')) { emitOld = chunk; await old.promise; }
      else chunk({ entries: [entry('latest.txt')], total_entries: 1 });
    },
  });
  const tab = await opened(manager);
  const pending = tab.openArchive('/tmp/old.zip');
  await tab.openArchive('/tmp/new.zip');
  emitOld({ entries: [entry('old.txt')], total_entries: 100 });
  old.resolve();
  await pending;
  assert.equal(tab.archivePath, '/tmp/new.zip');
  assert.equal(tab.loadedCount, 1);
  assert.equal(tab.totalCount, 1);
});
