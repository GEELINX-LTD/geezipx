import assert from 'node:assert/strict';
import path from 'node:path';
import test from 'node:test';
import { runeModule } from './helpers/rune-module.mjs';
import { componentScript, deferred } from './helpers/component-script.mjs';

async function store() {
  return (await runeModule('stores/taskStore.svelte.ts')).taskStore;
}

function event(task_id, kind, fields = {}) {
  return {
    task_id, kind, status: 'progress', stage: kind === 'compress' ? 'compressing' : 'extracting',
    phase: null, message: '', current: 10, total: 100, percent: 10,
    bytes_per_second: 12, current_entry: '资料.txt', completed_entries: 1, total_entries: 10,
    ...fields,
  };
}

test('interleaved compression and extraction retain independent progress and selection', async () => {
  const tasks = await store();
  const a = tasks.startTask('compress');
  const b = tasks.startTask('extract');
  tasks.updateTask(event(a, 'compress', { percent: 25, current: 25 }));
  tasks.updateTask(event(b, 'extract', { percent: 70, current: 70 }));
  tasks.updateTask(event(a, 'compress', { percent: 35, current: 35 }));
  assert.equal(tasks.activeTask.taskId, b);
  assert.equal(tasks.activeTask.percent, 70);
  assert.equal(tasks.getTask(a).percent, 35);
  assert.equal(tasks.getTask(a).kind, 'compress');
  assert.equal(tasks.hasRunning('compress'), true);
  assert.equal(tasks.hasRunning('extract'), true);
  tasks.selectTask(a);
  assert.equal(tasks.activeTask.current, 35);
});

test('unknown IDs and mismatched kinds cannot adopt or update registered tasks', async () => {
  const tasks = await store();
  const a = tasks.startTask('compress');
  const initial = tasks.activeTask;
  tasks.updateTask(event('foreign', 'extract'));
  tasks.updateTask(event(a, 'extract', { status: 'cancelled' }));
  tasks.selectTask('foreign');
  assert.equal(tasks.finishTask('foreign', 'failed'), undefined);
  assert.strictEqual(tasks.activeTask, initial);
  assert.equal(tasks.tasks.length, 1);
  assert.equal(tasks.getTask('foreign'), undefined);
});

test('completion of A cannot finish B and terminal visibility does not disable a kind', async () => {
  const tasks = await store();
  const a = tasks.startTask('compress');
  const b = tasks.startTask('extract');
  tasks.finishTask(a, 'finished');
  assert.equal(tasks.getTask(a).status, 'finished');
  assert.equal(tasks.activeTask.taskId, b);
  assert.equal(tasks.activeTask.status, 'pending');
  assert.equal(tasks.hasRunning('compress'), false);
  assert.equal(tasks.hasRunning('extract'), true);
  assert.equal(tasks.isVisible, true);
  assert.equal(tasks.tasks.length, 2);
});

test('all terminal states ignore late progress, starts, and duplicate completions', async () => {
  for (const terminal of ['finished', 'cancelled', 'failed']) {
    const tasks = await store();
    const a = tasks.startTask('compress');
    tasks.updateTask(event(a, 'compress', { status: terminal, message: 'authoritative result' }));
    const result = tasks.getTask(a);
    for (const status of ['started', 'progress', 'finished', 'cancelled', 'failed']) {
      tasks.updateTask(event(a, 'compress', { status, percent: 99, message: 'stale' }));
    }
    tasks.finishTask(a, 'failed', 'late rejection');
    tasks.finishTask(a, 'finished');
    assert.strictEqual(tasks.getTask(a), result);
    assert.equal(tasks.activeTask.status, terminal);
    assert.equal(tasks.hasRunning(), false);
  }
});

test('late started event cannot regress running status', async () => {
  const tasks = await store();
  const a = tasks.startTask('extract');
  tasks.updateTask(event(a, 'extract'));
  tasks.updateTask(event(a, 'extract', { status: 'started' }));
  assert.equal(tasks.activeTask.status, 'running');
});

test('cancellation rejection is cancelled and a terminal cancellation wins over rejection', async () => {
  const tasks = await store();
  const a = tasks.startTask('compress');
  assert.equal(tasks.finishTask(a, 'failed', 'Operation cancelled by user'), 'cancelled');
  const b = tasks.startTask('extract');
  tasks.updateTask(event(b, 'extract', { status: 'cancelled', message: 'cancelled event' }));
  assert.equal(tasks.finishTask(b, 'failed', 'different rejection'), 'cancelled');
  assert.equal(tasks.getTask(b).message, 'cancelled event');
  assert.equal(tasks.hasRunning(), false);
});

test('dismiss only removes the requested terminal task and selects another remaining task', async () => {
  const tasks = await store();
  const a = tasks.startTask('compress');
  const b = tasks.startTask('extract');
  tasks.dismissTask(b);
  assert.equal(tasks.tasks.length, 2);
  tasks.finishTask(a, 'finished');
  tasks.selectTask(a);
  tasks.dismissTask(a);
  assert.equal(tasks.activeTask.taskId, b);
  assert.equal(tasks.getTask(a), undefined);
  tasks.updateTask(event(a, 'compress'));
  assert.equal(tasks.tasks.length, 1);
  tasks.finishTask(b, 'cancelled');
  tasks.dismissTask(b);
  assert.equal(tasks.activeTask, null);
  assert.equal(tasks.isVisible, false);
});

test('dismissing an inactive result keeps the selected task and final results remain selectable', async () => {
  const tasks = await store();
  const a = tasks.startTask('compress');
  const b = tasks.startTask('extract');
  tasks.finishTask(a, 'finished');
  tasks.finishTask(b, 'failed');
  tasks.dismissTask(a);
  assert.equal(tasks.activeTask.taskId, b);
  const c = tasks.startTask('compress');
  tasks.finishTask(c, 'finished');
  tasks.dismissTask(c);
  assert.equal(tasks.activeTask.taskId, b);
  assert.equal(tasks.activeTask.status, 'failed');
});

test('same-millisecond starts generate unique UUIDs including concurrent tasks of one kind', async t => {
  t.mock.method(Date, 'now', () => 123456);
  const tasks = await store();
  const ids = Array.from({ length: 100 }, () => tasks.startTask('compress'));
  assert.equal(new Set(ids).size, 100);
  assert.equal(tasks.tasks.length, 100);
  for (const id of ids) assert.match(id, /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i);
  tasks.finishTask(ids[0], 'finished');
  assert.equal(tasks.hasRunning('compress'), true);
});

test('dialog cancellation captures A while selection switches to B during the native call', async () => {
  const tasks = await store();
  const a = tasks.startTask('compress');
  const b = tasks.startTask('extract');
  tasks.selectTask(a);
  const pending = deferred();
  const cancelled = [];
  const dialog = componentScript('components/ProgressDialog.svelte', {
    taskStore: tasks,
    cancelTask: id => { cancelled.push(id); return pending.promise; },
  }, '{ handleCancel, handleBackdropClick }');
  const request = dialog.handleCancel(tasks.activeTask.taskId);
  tasks.selectTask(b);
  pending.resolve();
  await request;
  assert.deepEqual(cancelled, [a]);
  assert.equal(tasks.activeTask.taskId, b);
  tasks.finishTask(a, 'cancelled');
  dialog.handleBackdropClick(a);
  assert.equal(tasks.activeTask.taskId, b);
  assert.equal(tasks.getTask(b).status, 'pending');
});

function shell(tasks, platform, settings = {}, overrides = {}) {
  return componentScript('App.svelte', {
    onMount() {},
    HomePage() {},
    appStore: { activeTab: 'home' },
    taskStore: tasks,
    settingsStore: { get: async key => settings[key] ?? null },
    basename: async value => platform.basename(value),
    dirname: async value => platform.dirname(value),
    join: async (...values) => platform.join(...values),
    toastStore: { show() {} },
    compressArchive: async () => ({ files_added: 1 }),
    ...overrides,
  }, '{ handleShellAction }');
}

for (const [name, platform, input, expected] of [
  ['Windows', path.win32, 'C:\\data\\folder\\', 'C:\\data\\folder.zip'],
  ['UNC', path.win32, '\\\\server\\share\\资料\\file.txt', '\\\\server\\share\\资料\\file.txt.zip'],
  ['POSIX', path.posix, '/data/folder/', '/data/folder.zip'],
]) {
  test(`quick ZIP resolves ${name} native paths and completes its own registered task`, async () => {
    const tasks = await store();
    const a = tasks.startTask('extract');
    let invocation;
    const app = shell(tasks, platform, {}, {
      compressArchive: async (...args) => { invocation = args; return { files_added: 1 }; },
    });
    await app.handleShellAction({ action: 'compress-zip', paths: [input] });
    assert.equal(invocation[1], expected);
    assert.equal(tasks.getTask(invocation[7]).status, 'finished');
    assert.equal(tasks.getTask(a).status, 'pending');
  });
}

test('quick ZIP honours output defaults and records native path errors inside its task', async () => {
  const tasks = await store();
  let output;
  const app = shell(tasks, path.win32, { default_output_dir: 'D:\\archives' }, {
    compressArchive: async (_paths, value) => { output = value; return { files_added: 1 }; },
  });
  await app.handleShellAction({ action: 'compress-zip', paths: ['C:\\data\\a.txt'] });
  assert.equal(output, 'D:\\archives\\a.txt.zip');
  const broken = shell(tasks, path.win32, {}, { join: async () => { throw new Error('native path failed'); } });
  await broken.handleShellAction({ action: 'compress-zip', paths: ['C:\\data\\a.txt'] });
  assert.equal(tasks.activeTask.status, 'failed');
  assert.equal(tasks.activeTask.message, 'native path failed');
  assert.equal(tasks.tasks.length, 2);
});

test('shell cancellation rejection keeps a cancelled result and does not show a failure toast', async () => {
  const tasks = await store();
  const toasts = [];
  const app = shell(tasks, path.posix, {}, {
    compressArchive: async () => { throw new Error('Operation cancelled by user'); },
    toastStore: { show: (...args) => toasts.push(args) },
  });
  await app.handleShellAction({ action: 'compress-zip', paths: ['/data/a.txt'] });
  assert.equal(tasks.activeTask.status, 'cancelled');
  assert.deepEqual(toasts, []);
});
