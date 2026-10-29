import assert from 'node:assert/strict';
import test from 'node:test';
import { componentScript, deferred, settle } from './helpers/component-script.mjs';

function launchApp(overrides = {}) {
  let mount;
  const callbacks = new Map();
  const opened = [];
  const added = [];
  const switched = [];
  const acknowledged = [];
  const messages = [];
  const progress = [];
  const instance = componentScript('App.svelte', {
    HomePage() {}, onMount: fn => { mount = fn; },
    appStore: {
      activeTab: 'home', loadRecent() {}, addShellCompressPaths() {},
      addArchiveTab: (...args) => added.push(args), switchTab: tab => switched.push(tab),
    },
    archiveManager: { openArchive: async path => { opened.push(path); return { tabId: path, label: path }; } },
    settingsStore: { get: async () => null },
    listen: async (name, callback) => { callbacks.set(name, callback); return () => {}; },
    // Keep baseline execution valid so the regression fails on lost delivery.
    getShellAction: async () => null, getOpenedArchives: async () => [],
    getLaunchRequests: async () => [],
    acknowledgeLaunchRequest: async id => { acknowledged.push(id); },
    taskStore: { updateTask: value => progress.push(value) },
    toastStore: { show: message => messages.push(message) },
    localeStore: { t: (_key, values) => values.message }, console: { error() {} },
    document: { documentElement: { removeAttribute() {} } },
    ...overrides,
  }, '{}');
  const cleanup = mount();
  return { instance, cleanup, callbacks, opened, added, switched, acknowledged, messages, progress,
    wake: () => callbacks.get('launch-requests-pending')({ payload: undefined }) };
}

const openRequest = (id, path = '/queued.zip') => ({ id, action: { kind: 'open-archives', payload: [path] } });
const shellRequest = (id, action = 'open') => ({ id, action: { kind: 'shell-action', payload: { action, paths: ['/shell.zip'] } } });

test('cold queued launch is delivered even when its wakeup was missed', async () => {
  const app = launchApp({ getLaunchRequests: async () => [openRequest('cold')] });
  await settle();
  assert.deepEqual(app.opened, ['/queued.zip'], 'cold queued request must be delivered without a wakeup');
  assert.deepEqual(app.acknowledged, ['cold']);
  app.cleanup();
});

test('setup wakeups wait for both subscriptions and progress is ready before work starts', async () => {
  const progressReady = deferred();
  const wakeReady = deferred();
  const callbacks = new Map();
  let fetches = 0;
  let opened = 0;
  const app = launchApp({
    listen: (name, callback) => {
      callbacks.set(name, callback);
      return name === 'task:progress' ? progressReady.promise : wakeReady.promise;
    },
    getLaunchRequests: async () => { fetches++; return [openRequest('setup')]; },
    archiveManager: { openArchive: async path => {
      callbacks.get('task:progress')({ payload: 'early progress' });
      opened++;
      return { tabId: path, label: path };
    } },
  });
  callbacks.get('launch-requests-pending')();
  wakeReady.resolve(() => {});
  await settle();
  assert.equal(fetches, 0);
  assert.equal(opened, 0);
  progressReady.resolve(() => {});
  await settle();
  assert.equal(fetches, 1);
  assert.equal(opened, 1);
  assert.deepEqual(app.progress, ['early progress']);
  app.cleanup();
});

test('wakeups during snapshot fetch coalesce a followup fetch without overlapping snapshots', async () => {
  const first = deferred();
  const second = deferred();
  let fetches = 0;
  const app = launchApp({ getLaunchRequests: () => ++fetches === 1 ? first.promise : second.promise });
  await settle();
  app.wake();
  app.wake();
  assert.equal(fetches, 1);
  first.resolve([openRequest('first', '/first.zip')]);
  await settle();
  assert.equal(fetches, 2);
  second.resolve([openRequest('first', '/first.zip'), openRequest('second', '/second.zip')]);
  await settle();
  assert.deepEqual(app.opened, ['/first.zip', '/second.zip']);
  assert.deepEqual(app.acknowledged, ['first', 'second']);
  assert.equal(fetches, 2);
  app.cleanup();
});

test('IDs dedupe in-flight and stale deliveries but identical payloads with different IDs run', async () => {
  const pending = deferred();
  let calls = 0;
  const requests = [openRequest('a'), openRequest('a'), openRequest('b')];
  const app = launchApp({
    getLaunchRequests: async () => requests,
    archiveManager: { openArchive: () => { calls++; return pending.promise; } },
  });
  await settle();
  app.wake();
  await settle();
  assert.equal(calls, 2);
  assert.deepEqual(app.acknowledged, [], 'an in-flight duplicate must not be acknowledged early');
  pending.resolve({ tabId: 'tab', label: 'archive' });
  await settle();
  assert.deepEqual(app.acknowledged, ['a', 'b']);
  app.wake();
  await settle();
  assert.equal(calls, 2);
  assert.deepEqual(app.acknowledged, ['a', 'b']);
  app.cleanup();
});

test('failed acknowledgement retries on wakeup without replaying processed work or spinning', async () => {
  let attempts = 0;
  const app = launchApp({
    getLaunchRequests: async () => [openRequest('ack')],
    acknowledgeLaunchRequest: async () => { if (++attempts === 1) throw new Error('ack unavailable'); },
  });
  await settle();
  assert.equal(attempts, 1);
  assert.equal(app.messages.length, 1);
  assert.match(app.messages[0], /ack unavailable/);
  app.wake();
  await settle();
  assert.equal(attempts, 2);
  app.wake();
  await settle();
  assert.equal(attempts, 2);
  assert.deepEqual(app.opened, ['/queued.zip']);
  app.cleanup();
});

test('an in-flight acknowledgement is not repeated by stale snapshots', async () => {
  const ack = deferred();
  let attempts = 0;
  const app = launchApp({
    getLaunchRequests: async () => [openRequest('ack')],
    acknowledgeLaunchRequest: () => { attempts++; return ack.promise; },
  });
  await settle();
  app.wake();
  await settle();
  assert.equal(attempts, 1);
  assert.deepEqual(app.opened, ['/queued.zip']);
  ack.resolve();
  await settle();
  app.cleanup();
});

test('snapshot IPC failure is visible and a later wakeup recovers pending launches', async () => {
  let fetches = 0;
  const app = launchApp({ getLaunchRequests: async () => {
    if (++fetches === 1) throw new Error('snapshot unavailable');
    return [openRequest('recovered')];
  } });
  await settle();
  assert.equal(fetches, 1);
  assert.match(app.messages[0], /snapshot unavailable/);
  app.wake();
  await settle();
  assert.deepEqual(app.acknowledged, ['recovered']);
  app.cleanup();
});

test('a wakeup during a failed snapshot triggers a followup read', async () => {
  const snapshot = deferred();
  let fetches = 0;
  const app = launchApp({ getLaunchRequests: () => {
    if (++fetches === 1) return snapshot.promise;
    return Promise.resolve([openRequest('pending')]);
  } });
  await settle();
  app.wake();
  app.wake();
  assert.equal(fetches, 1, 'wakeups must not overlap the pending snapshot');
  snapshot.resolve(Promise.reject(new Error('snapshot unavailable')));
  await settle();
  assert.deepEqual(app.acknowledged, ['pending'], 'a wakeup already received during a failed query must trigger a followup read');
  assert.deepEqual(app.opened, ['/queued.zip']);
  assert.equal(fetches, 2, 'queued wakeups coalesce without spinning after recovery');
  assert.equal(app.messages.length, 1);
  assert.match(app.messages[0], /snapshot unavailable/);
  app.cleanup();
});

test('disposal before a snapshot failure drops its queued followup wakeup', async () => {
  const snapshot = deferred();
  let fetches = 0;
  const app = launchApp({ getLaunchRequests: () => {
    if (++fetches === 1) return snapshot.promise;
    return Promise.resolve([openRequest('late')]);
  } });
  await settle();
  app.wake();
  app.cleanup();
  snapshot.resolve(Promise.reject(new Error('late snapshot failure')));
  await settle();
  assert.equal(fetches, 1);
  assert.deepEqual(app.opened, []);
  assert.deepEqual(app.acknowledged, []);
  assert.deepEqual(app.messages, []);
});

test('one rejected archive does not skip siblings, shell launches or acknowledgement', async () => {
  let calls = 0;
  const app = launchApp({
    getLaunchRequests: async () => [
      { id: 'archives', action: { kind: 'open-archives', payload: ['/bad.zip', '/good.zip'] } },
      shellRequest('shell'),
    ],
    archiveManager: { openArchive: async path => {
      calls++;
      if (path !== '/good.zip') throw new Error(`cannot open ${path}`);
      return { tabId: path, label: path };
    } },
  });
  await settle();
  assert.equal(calls, 3);
  assert.equal(app.messages.length, 2);
  assert.deepEqual(app.acknowledged.sort(), ['archives', 'shell']);
  app.wake();
  await settle();
  assert.equal(calls, 3);
  app.cleanup();
});

test('disposal during a pending snapshot prevents handlers and acknowledgements', async () => {
  const snapshot = deferred();
  const app = launchApp({ getLaunchRequests: () => snapshot.promise });
  await settle();
  app.cleanup();
  snapshot.resolve([openRequest('late'), shellRequest('late-shell')]);
  await settle();
  assert.deepEqual(app.opened, []);
  assert.deepEqual(app.acknowledged, []);
  assert.deepEqual(app.messages, []);
});

for (const request of [openRequest('archive'), shellRequest('open'), shellRequest('extract', 'extract')]) {
  test(`disposal during ${request.id} handler prevents late UI updates and acknowledgement`, async () => {
    const opening = deferred();
    const app = launchApp({
      getLaunchRequests: async () => [request],
      archiveManager: { openArchive: () => opening.promise },
    });
    await settle();
    app.cleanup();
    opening.resolve({ tabId: 'late', label: 'late' });
    await settle();
    assert.deepEqual(app.added, []);
    assert.deepEqual(app.switched, []);
    assert.deepEqual(app.acknowledged, []);
  });
}

function compressionBindings(pending, finished, calls) {
  return {
    basename: async () => 'source', dirname: async () => '/data', join: async () => '/data/source.zip',
    compressArchive: async (...args) => { calls.push(args); return pending.promise; },
    taskStore: {
      updateTask() {}, startTask: () => 'compression-task',
      finishTask: (...args) => { finished.push(args); return args[1]; },
    },
  };
}

test('async compression does not block later launches or acknowledge before completion', async () => {
  const compression = deferred();
  const finished = [];
  const calls = [];
  let requests = [shellRequest('compress', 'compress-zip')];
  const app = launchApp({
    ...compressionBindings(compression, finished, calls),
    getLaunchRequests: async () => requests,
  });
  await settle();
  assert.equal(calls.length, 1);
  requests = [...requests, openRequest('later')];
  app.wake();
  await settle();
  assert.deepEqual(app.opened, ['/queued.zip']);
  assert.deepEqual(app.acknowledged, ['later']);
  assert.equal(calls.length, 1);
  compression.resolve({ files_added: 1 });
  await settle();
  assert.deepEqual(app.acknowledged, ['later', 'compress']);
  assert.deepEqual(finished, [['compression-task', 'finished']]);
  assert.equal(app.messages.length, 1);
  app.cleanup();
});

for (const fails of [false, true]) {
  test(`compression completion after disposal leaves UI and acknowledgement untouched (fails=${fails})`, async () => {
    const compression = deferred();
    const finished = [];
    const calls = [];
    const app = launchApp({
      ...compressionBindings(compression, finished, calls),
      getLaunchRequests: async () => [shellRequest('compress', 'compress-zip')],
    });
    await settle();
    assert.equal(calls.length, 1);
    app.cleanup();
    compression.resolve(fails ? Promise.reject(new Error('late compression failure')) : { files_added: 1 });
    await settle();
    assert.deepEqual(finished, []);
    assert.deepEqual(app.messages, []);
    assert.deepEqual(app.acknowledged, []);
  });
}

test('multi-archive launch waits for every archive before acknowledgement', async () => {
  const first = deferred();
  const last = deferred();
  const app = launchApp({
    getLaunchRequests: async () => [{ id: 'many', action: { kind: 'open-archives', payload: ['/first.zip', '/last.zip'] } }],
    archiveManager: { openArchive: path => path === '/first.zip' ? first.promise : last.promise },
  });
  await settle();
  first.resolve({ tabId: 'first', label: 'first' });
  await settle();
  assert.deepEqual(app.acknowledged, []);
  last.resolve({ tabId: 'last', label: 'last' });
  await settle();
  assert.deepEqual(app.acknowledged, ['many']);
  app.cleanup();
});

test('failed compression is surfaced and acknowledged without replaying the task', async () => {
  const compression = deferred();
  const finished = [];
  const calls = [];
  const app = launchApp({
    ...compressionBindings(compression, finished, calls),
    getLaunchRequests: async () => [shellRequest('compress', 'compress-zip')],
  });
  await settle();
  compression.resolve(Promise.reject(new Error('compression unavailable')));
  await settle();
  assert.deepEqual(finished, [['compression-task', 'failed', 'compression unavailable']]);
  assert.match(app.messages[0], /compression unavailable/);
  assert.deepEqual(app.acknowledged, ['compress']);
  app.wake();
  await settle();
  assert.equal(calls.length, 1);
  app.cleanup();
});

test('unmount clears the shell compression debounce timer', async () => {
  const timers = new Map();
  let timerId = 0;
  const paths = [];
  const switches = [];
  const app = launchApp({
    appStore: {
      activeTab: 'home', loadRecent() {}, addShellCompressPaths: value => paths.push(value),
      switchTab: value => switches.push(value),
    },
    setTimeout: callback => { timers.set(++timerId, callback); return timerId; },
    clearTimeout: id => timers.delete(id),
    getLaunchRequests: async () => [shellRequest('compress', 'compress')],
  });
  await settle();
  assert.deepEqual(paths, [['/shell.zip']]);
  assert.equal(timers.size, 1);
  app.cleanup();
  assert.equal(timers.size, 0);
  assert.deepEqual(switches, []);
});
