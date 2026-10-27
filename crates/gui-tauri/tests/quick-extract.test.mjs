import assert from 'node:assert/strict';
import test from 'node:test';
import { componentScript, deferred, settle } from './helpers/component-script.mjs';
import { runeModule } from './helpers/rune-module.mjs';

const payload = { action: 'extract-here', paths: ['C:\\data\\archive.7z'] };
const entry = { path: 'file.txt', is_dir: false, size: 1 };

async function quickWindow(overrides = {}) {
  const taskStore = (await runeModule('stores/taskStore.svelte.ts')).taskStore;
  const { archiveManager } = await runeModule('stores/archiveStore.svelte.ts', {
    '../archiveFormats': await runeModule('archiveFormats.ts'),
    '../bridge': {
      listArchiveStream: overrides.listArchiveStream ?? (async (_path, chunk) => {
        chunk({ entries: [entry], total_entries: 1 });
      }),
      previewEntry: assert.fail,
    },
  });
  let mount;
  let progress;
  let closeRequested;
  let closed = 0;
  let unlistened = 0;
  const extracted = [];
  const cancelled = [];
  const currentWindow = {
    close: async () => { closed++; },
    onCloseRequested: overrides.onCloseRequested ?? (async callback => {
      closeRequested = callback;
      return () => { unlistened++; };
    }),
  };
  const component = componentScript('QuickExtract.svelte', {
    getCurrentWindow: () => currentWindow,
    taskStore, archiveManager,
    onMount: fn => { mount = fn; },
    localeStore: { t: key => key, switchLocale() {} },
    settingsStore: { get: async () => null },
    document: { documentElement: { removeAttribute() {}, setAttribute() {} } },
    getShellAction: overrides.getShellAction ?? (async () => payload),
    listen: overrides.listen ?? (async (_name, callback) => {
      progress = callback;
      return () => { unlistened++; };
    }),
    extractArchive: overrides.extractArchive ?? (async (...args) => { extracted.push(args); }),
    cancelTask: overrides.cancelTask ?? (async id => { cancelled.push(id); }),
  }, `{
    runExtraction, handleCancel, handleCloseRequested,
    get busy() { return busy; }, get taskId() { return taskId; },
    get tabId() { return tabId; }, get windowError() { return windowError; }
  }`);
  return {
    component, taskStore, archiveManager, extracted, cancelled,
    mount: () => mount(),
    progress: event => progress(event),
    closeRequested: event => closeRequested(event),
    get closed() { return closed; },
    get unlistened() { return unlistened; },
  };
}

test('quick extraction consumes its own action after listeners attach, uses its own task, and closes on success', async () => {
  const worker = deferred();
  let invocation;
  const view = await quickWindow({ extractArchive: async (...args) => { invocation = args; await worker.promise; } });
  const cleanup = view.mount();
  await settle();
  assert.equal(view.component.busy, true);
  assert.equal(invocation[0], payload.paths[0]);
  assert.equal(invocation[1], 'C:\\data\\archive');
  assert.equal(invocation[2], true);
  assert.equal(invocation[4], view.component.taskId);
  const foreign = view.taskStore.startTask('extract');
  view.progress({ payload: { task_id: foreign, kind: 'extract', status: 'finished' } });
  assert.equal(view.taskStore.getTask(foreign).status, 'pending');
  view.progress({ payload: { task_id: view.component.taskId, kind: 'extract', status: 'progress', percent: 45 } });
  assert.equal(view.taskStore.getTask(view.component.taskId).percent, 45);
  assert.equal(view.closed, 0);
  worker.resolve();
  await settle();
  assert.equal(view.closed, 1);
  assert.equal(view.taskStore.getTask(view.component.taskId).status, 'finished');
  assert.equal(view.taskStore.getTask(foreign).status, 'pending');
  assert.equal(view.archiveManager.tabs.size, 0);
  cleanup();
  assert.equal(view.unlistened, 2);
});

test('quick extraction waits for password retries and passes only the unlocked archive password', async () => {
  const view = await quickWindow({
    listArchiveStream: async (_path, chunk, password) => {
      if (password !== 'secret') throw new Error('Failed to read entries: crypto error: invalid password for 7z archive');
      chunk({ entries: [entry], total_entries: 1 });
    },
  });
  const running = view.component.runExtraction(payload);
  await settle();
  const tabId = view.component.tabId;
  assert.ok(view.archiveManager.tabs.get(tabId).passwordChallenge);
  assert.equal(view.extracted.length, 0);
  await view.archiveManager.submitPassword(tabId, 'wrong');
  assert.equal(view.archiveManager.tabs.get(tabId).passwordChallenge.incorrectPassword, true);
  assert.equal(view.extracted.length, 0);
  await view.archiveManager.submitPassword(tabId, 'secret');
  await running;
  assert.equal(view.extracted[0][3], 'secret');
  assert.equal(view.closed, 1);
  assert.equal(view.archiveManager.tabs.size, 0);
});

test('cancelling the password prompt closes without starting extraction', async () => {
  const view = await quickWindow({
    listArchiveStream: async () => { throw new Error('Failed to read entries: crypto error: invalid password for 7z archive'); },
  });
  const running = view.component.runExtraction(payload);
  await settle();
  view.archiveManager.cancelPasswordChallenge(view.component.tabId);
  await running;
  assert.equal(view.extracted.length, 0);
  assert.equal(view.closed, 1);
  assert.equal(view.taskStore.getTask(view.component.taskId).status, 'cancelled');
});

test('titlebar close during extraction cancels its worker and waits for the native result before closing', async () => {
  const worker = deferred();
  const view = await quickWindow({ extractArchive: async () => worker.promise });
  const running = view.component.runExtraction(payload);
  await settle();
  let prevented = false;
  view.component.handleCloseRequested({ preventDefault: () => { prevented = true; } });
  await settle();
  assert.equal(prevented, true);
  assert.deepEqual(view.cancelled, [view.component.taskId]);
  assert.equal(view.closed, 0);
  assert.equal(view.component.busy, true);
  worker.resolve();
  await running;
  assert.equal(view.closed, 1);
});

test('cancellation while listing invalidates pending work and cannot extract after the list resolves', async () => {
  const listing = deferred();
  const view = await quickWindow({ listArchiveStream: async () => listing.promise });
  const running = view.component.runExtraction(payload);
  await settle();
  await view.component.handleCancel();
  assert.equal(view.closed, 1);
  listing.resolve();
  await running;
  assert.equal(view.extracted.length, 0);
  assert.equal(view.archiveManager.tabs.size, 0);
  assert.equal(view.taskStore.getTask(view.component.taskId).status, 'cancelled');
});

test('failed cancellation keeps the window open until the worker terminates', async () => {
  const worker = deferred();
  const view = await quickWindow({
    extractArchive: async () => worker.promise,
    cancelTask: async () => { throw new Error('cancel unavailable'); },
  });
  const running = view.component.runExtraction(payload);
  await settle();
  await view.component.handleCancel();
  assert.equal(view.closed, 0);
  assert.equal(view.component.busy, true);
  assert.equal(view.component.windowError, 'cancel unavailable');
  worker.resolve();
  await running;
  assert.equal(view.closed, 1);
});

test('archive listing and extraction errors retain a closeable result instead of closing or remaining pending', async () => {
  for (const stage of ['list', 'extract']) {
    const fail = async () => { throw new Error(`${stage} failed`); };
    const view = await quickWindow(stage === 'list' ? { listArchiveStream: fail } : { extractArchive: fail });
    await view.component.runExtraction(payload);
    assert.equal(view.component.busy, false);
    assert.equal(view.closed, 0);
    const task = view.taskStore.getTask(view.component.taskId);
    assert.equal(task.status, 'failed');
    assert.equal(task.message, `${stage} failed`);
    await view.component.handleCancel();
    assert.equal(view.closed, 1);
  }
});

test('a failed terminal event remains authoritative when the invoke promise resolves', async () => {
  const worker = deferred();
  const view = await quickWindow({ extractArchive: async () => worker.promise });
  const cleanup = view.mount();
  await settle();
  view.progress({ payload: { task_id: view.component.taskId, kind: 'extract', status: 'failed', message: 'damaged archive' } });
  worker.resolve();
  await settle();
  assert.equal(view.closed, 0);
  assert.equal(view.taskStore.getTask(view.component.taskId).status, 'failed');
  cleanup();
});

test('listener registration resolving after unmount is removed and cannot start extraction', async () => {
  const listener = deferred();
  let removed = 0;
  let consumed = 0;
  const view = await quickWindow({
    listen: () => listener.promise,
    getShellAction: async () => { consumed++; return payload; },
  });
  const cleanup = view.mount();
  cleanup();
  listener.resolve(() => { removed++; });
  await settle();
  assert.equal(removed, 1);
  assert.equal(view.unlistened, 1);
  assert.equal(consumed, 0);
  assert.equal(view.extracted.length, 0);
});
