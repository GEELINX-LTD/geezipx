import assert from 'node:assert/strict';
import path from 'node:path';
import test from 'node:test';
import { componentScript, deferred, settle } from './helpers/component-script.mjs';

function page(platform = path.win32, settings = {}, overrides = {}) {
  const dialogs = [];
  const folders = [];
  let nextSelection = [];
  const instance = componentScript('pages/CompressPage.svelte', {
    basename: async value => platform.basename(value) || 'archive',
    dirname: async value => platform.dirname(value),
    join: async (...values) => platform.join(...values),
    open: async options => { dialogs.push(options); return nextSelection; },
    save: async () => platform.join(platform.parse('C:\\chosen\\out.zip').root || '/', 'chosen', 'out.zip'),
    getFormats: async () => [],
    settingsStore: { get: async key => settings[key] ?? null },
    localeStore: { t: key => key },
    taskStore: { isVisible: false, startTask() {}, finishTask() {} },
    appStore: { addRecent() {}, pendingSourcePaths: [] },
    toastStore: { show() {} },
    compressArchive: async () => ({ files_added: 1, bytes_written: 8 }),
    openFolder: async folder => folders.push(folder),
    ...overrides,
  }, `{
    addSources, inferOutput, browseFiles, browseFolders, browseOutput, runCompress,
    setFormat(value) { format = value; changeFormat(); },
    get output() { return outputPath; },
    get sources() { return sourcePaths; }
  }`);
  return { instance, dialogs, folders, select(value) { nextSelection = value; } };
}

for (const [name, platform, input, expected] of [
  ['Windows', path.win32, 'C:\\Users\\demo\\file.txt', 'C:\\Users\\demo\\file.txt.zip'],
  ['UNC', path.win32, '\\\\server\\share\\资料\\file.txt', '\\\\server\\share\\资料\\file.txt.zip'],
  ['POSIX', path.posix, '/home/demo/file.txt', '/home/demo/file.txt.zip'],
]) {
  test(`${name} source produces a valid sibling output`, async () => {
    const { instance } = page(platform);
    await settle();
    instance.addSources([input]);
    await settle();
    assert.equal(instance.output, expected);
  });
}

test('saved output directory and compound extension are respected', async () => {
  const { instance } = page(path.win32, { default_output_dir: 'D:\\archives' });
  await settle();
  instance.addSources(['C:\\data\\文件.txt']);
  instance.setFormat('tar.gz');
  await settle();
  assert.equal(instance.output, 'D:\\archives\\文件.txt.tar.gz');
});

test('file and folder dialogs retain multiple deduplicated sources', async () => {
  const { instance, dialogs, select } = page();
  await settle();
  select(['C:\\data\\a.txt', 'C:\\data\\b.txt']);
  await instance.browseFiles();
  select(['C:\\data\\folder', 'C:\\data\\a.txt']);
  await instance.browseFolders();
  assert.deepEqual(dialogs.map(dialog => [dialog.directory, dialog.multiple]), [[false, true], [true, true]]);
  assert.equal(instance.sources.length, 3);
});

test('slow path inference cannot overwrite a later format choice', async () => {
  const first = deferred();
  let requests = 0;
  const { instance } = page(path.win32, {}, {
    join: (...parts) => ++requests === 1 ? first.promise : Promise.resolve(path.win32.join(...parts)),
  });
  await settle();
  instance.addSources(['C:\\data\\a.txt']);
  await settle();
  instance.setFormat('7z');
  await settle();
  first.resolve('C:\\data\\a.txt.zip');
  await settle();
  assert.equal(instance.output, 'C:\\data\\a.txt.7z');
});

test('Save As remains selected after defaults and source updates', async () => {
  const settings = deferred();
  const { instance } = page(path.win32, {}, { settingsStore: { get: () => settings.promise } });
  await instance.browseOutput();
  const chosen = instance.output;
  settings.resolve(null);
  await settle();
  instance.addSources(['C:\\data\\a.txt']);
  instance.setFormat('7z');
  await settle();
  assert.equal(instance.output, chosen);
});

test('open on completion uses the output parent on Windows', async () => {
  const { instance, folders } = page(path.win32, { on_complete: 'open_output' });
  await settle();
  instance.addSources(['C:\\data\\a.txt']);
  await settle();
  await instance.runCompress();
  assert.deepEqual(folders, ['C:\\data']);
});
