import assert from 'node:assert/strict';
import test from 'node:test';
import { componentScript, deferred, settle } from './helpers/component-script.mjs';
import { runeModule } from './helpers/rune-module.mjs';

const formats = await runeModule('archiveFormats.ts');
const failure = async () => { throw new Error('bridge unavailable'); };
const localeStore = { locale: 'en', t: (key, values) => values ? `${key}: ${values.message}` : key };
const quietConsole = { error() {} };

test('native drop, home selection and archive picker share all known suffix aliases', async () => {
  const opened = [];
  const switches = [];
  const dialogs = [];
  let selection;
  let nativeDrop;
  let mount;
  const appStore = {
    archiveTabs: [], addRecent() {}, addArchiveTab() {},
    switchTab: tab => switches.push(tab), setPendingSourcePaths: paths => switches.push(paths),
  };
  const archiveManager = {
    tabs: new Map([['archive-tab', {}]]),
    openArchive: async path => { opened.push(path); return { tabId: 'archive-tab', label: 'archive' }; },
  };
  const bindings = {
    ...formats, appStore, archiveManager, localeStore, toastStore: { show: assert.fail },
    getCurrentWindow: () => ({ onDragDropEvent: async callback => { nativeDrop = callback; return () => {}; } }),
    onMount: fn => { mount = fn; },
    open: async options => { dialogs.push(options); return selection; },
  };
  componentScript('components/DropOverlay.svelte', bindings, '{}');
  const cleanup = mount();
  const home = componentScript('pages/home/DropZone.svelte', bindings, '{ handleClick }');
  const picker = componentScript('pages/extract/ArchivePathBar.svelte', bindings, '{ changeArchive }');
  for (const ext of ['ZIPX', 'JAR', 'WAR', 'APK', 'IPA', 'XPI', 'TAR.GZ', 'TBZ2', 'TXZ', 'TZST', 'GZIP', 'ZSTD', 'IMA', 'ENC', 'LZ']) {
    selection = `/data/archive.${ext}`;
    nativeDrop({ payload: { type: 'drop', paths: [selection] } });
    await settle();
    await home.handleClick();
    await picker.changeArchive();
    assert.deepEqual(opened.slice(-3), [selection, selection, selection]);
    assert.ok(dialogs.at(-1).filters[0].extensions.includes(ext.toLowerCase()));
  }
  assert.equal(switches.every(tab => tab === 'archive-tab'), true);
  assert.equal(formats.isArchivePath('/directory.zip/file.txt'), false);
  assert.equal(formats.isArchivePath('/archive.zip/'), false);
  assert.equal(formats.isArchivePath('/file.txt'), false);
  assert.equal(formats.isArchivePath('/file.aes'), false);
  assert.equal(formats.stripArchiveExtension('Archive.TAR.BZ2'), 'Archive');
  cleanup();
});

function settings(overrides = {}) {
  let mount;
  const messages = [];
  const defaults = { locale: 'en', theme: 'system', default_level: 7, default_format: 'zipx', shell_menu_enabled: false, shell_menu_verbs: [] };
  const instance = componentScript('pages/SettingsPage.svelte', {
    onMount: fn => { mount = fn; },
    settingsStore: { DEFAULTS: defaults, loadAll: async () => defaults },
    settingsGuard: { dirty: false }, localeStore,
    toastStore: { show: message => messages.push(message) }, console: quietConsole,
    getFormats: async () => [], getFileAssociations: async () => null,
    getShellMenuState: async () => null, getVersion: async () => '9.8.7',
    ...overrides,
  }, `{
    get loaded() { return loaded; }, get loadError() { return loadError; },
    get formatsFailed() { return formatsFailed; }, get formData() { return formData; },
    get version() { return version; }, get versionFailed() { return versionFailed; }
  }`);
  const cleanup = mount();
  return { instance, cleanup, messages };
}

test('settings capability and optional bridge failures preserve loaded user settings and terminate version loading', async () => {
  const { instance, cleanup, messages } = settings({
    getFormats: failure, getFileAssociations: failure, getShellMenuState: failure, getVersion: failure,
  });
  await settle();
  assert.equal(instance.loaded, true);
  assert.equal(instance.formData.default_level, 7);
  assert.equal(instance.formData.default_format, 'zipx');
  assert.equal(instance.formatsFailed, true);
  assert.equal(instance.versionFailed, true);
  assert.equal(messages.length, 3);
  cleanup();
});

test('settings persistence failure produces a terminal error instead of an infinite spinner or editable defaults', async () => {
  const { instance, cleanup } = settings({ settingsStore: { DEFAULTS: {}, loadAll: failure } });
  await settle();
  assert.match(instance.loadError, /bridge unavailable/);
  assert.equal(instance.loaded, false);
  cleanup();
});

test('settings initialization resolving after unmount cannot apply values or synchronize the shell menu', async () => {
  const pending = deferred();
  const { instance, cleanup } = settings({
    getFormats: () => pending.promise,
    getVersion: () => pending.promise,
    getShellMenuState: async () => ({ supported: true }),
    settingsStore: { DEFAULTS: {}, loadAll: async () => ({ default_level: 9, shell_menu_enabled: true, shell_menu_verbs: [] }) },
    setShellMenu: assert.fail,
  });
  cleanup();
  pending.resolve([]);
  await settle();
  assert.equal(instance.loaded, false);
  assert.equal(instance.version, '');
});

for (const fails of [false, true]) {
  test(`about displays backend version or terminal failure (fails=${fails})`, async () => {
    let mount;
    const instance = componentScript('pages/AboutPage.svelte', {
      onMount: fn => { mount = fn; }, localeStore,
      getVersion: fails ? failure : async () => '9.8.7', console: quietConsole,
    }, '{ get version() { return version; }, get failed() { return versionFailed; } }');
    const cleanup = mount();
    await settle();
    assert.equal(instance.version, fails ? '' : '9.8.7');
    assert.equal(instance.failed, fails);
    cleanup();
  });
}

test('app consumes rejected startup queries and listener registrations with visible errors', async () => {
  let mount;
  const messages = [];
  componentScript('App.svelte', {
    HomePage() {}, onMount: fn => { mount = fn; },
    appStore: { activeTab: 'home', loadRecent() {} },
    settingsStore: { get: failure }, listen: failure, getShellAction: failure, getOpenedArchives: failure,
    toastStore: { show: message => messages.push(message) }, localeStore, console: quietConsole,
  }, '{}');
  const cleanup = mount();
  await settle();
  assert.equal(messages.length, 6);
  assert.equal(messages.every(message => message.includes('bridge unavailable')), true);
  cleanup();
});

test('app consumes rejected cold and hot archive opens and shell handlers', async () => {
  let mount;
  const callbacks = new Map();
  const messages = [];
  componentScript('App.svelte', {
    HomePage() {}, onMount: fn => { mount = fn; },
    appStore: { activeTab: 'home', loadRecent() {} },
    settingsStore: { get: async () => null },
    listen: async (name, callback) => { callbacks.set(name, callback); return () => {}; },
    getShellAction: async () => ({ action: 'open', paths: ['/cold-shell.zip'] }),
    getOpenedArchives: async () => ['/cold-open.zip'],
    archiveManager: { openArchive: failure },
    toastStore: { show: message => messages.push(message) }, localeStore, console: quietConsole,
    document: { documentElement: { removeAttribute() {} } },
  }, '{}');
  const cleanup = mount();
  await settle();
  callbacks.get('opened-archives')({ payload: ['/hot-open.zip'] });
  callbacks.get('shell-action')({ payload: { action: 'open', paths: ['/hot-shell.zip'] } });
  await settle();
  assert.equal(messages.length, 4);
  cleanup();
});

test('native drop registration failure is handled even after unmount', async () => {
  let mount;
  let reject;
  const registration = new Promise((_resolve, fail) => { reject = fail; });
  const messages = [];
  componentScript('components/DropOverlay.svelte', {
    onMount: fn => { mount = fn; },
    getCurrentWindow: () => ({ onDragDropEvent: () => registration }),
    toastStore: { show: message => messages.push(message) }, localeStore, console: quietConsole,
  }, '{}');
  const cleanup = mount();
  cleanup();
  reject(new Error('bridge unavailable'));
  await settle();
  assert.deepEqual(messages, []);
});

test('native drop registration failure while mounted displays an error', async () => {
  let mount;
  const messages = [];
  componentScript('components/DropOverlay.svelte', {
    onMount: fn => { mount = fn; },
    getCurrentWindow: () => ({ onDragDropEvent: failure }),
    toastStore: { show: message => messages.push(message) }, localeStore, console: quietConsole,
  }, '{}');
  const cleanup = mount();
  await settle();
  assert.equal(messages.length, 1);
  cleanup();
});

for (const failedDependency of ['capabilities', 'settings']) {
  test(`compression cannot start with manually selected output after ${failedDependency} initialization fails`, async () => {
    let calls = 0;
    const instance = componentScript('pages/CompressPage.svelte', {
      getFormats: failedDependency === 'capabilities' ? failure : async () => [],
      settingsStore: { get: failedDependency === 'settings' ? failure : async () => null },
      localeStore, taskStore: { hasRunning: () => false, startTask: () => { calls++; } },
      appStore: { pendingSourcePaths: [] },
      save: async () => '/manual.zip',
    }, '{ addSources, browseOutput, runCompress, get error() { return error; } }');
    await settle();
    await instance.browseOutput();
    instance.addSources(['/source.txt']);
    await instance.runCompress();
    assert.equal(calls, 0);
    assert.match(instance.error, /bridge unavailable/);
  });
}

test('native drop removes listeners that finish registering after unmount', async () => {
  let mount;
  const registration = deferred();
  let removed = 0;
  componentScript('components/DropOverlay.svelte', {
    onMount: fn => { mount = fn; },
    getCurrentWindow: () => ({ onDragDropEvent: () => registration.promise }),
  }, '{}');
  const cleanup = mount();
  cleanup();
  registration.resolve(() => { removed++; });
  await settle();
  assert.equal(removed, 1);
});

test('extraction default failure leaves actions disabled and exposes the bridge error', async () => {
  const instance = componentScript('pages/extract/ExtractControls.svelte', {
    archiveStore: { suggestedOutputDir: '/output' }, taskStore: { hasRunning: () => false },
    settingsStore: { get: failure }, localeStore,
  }, '{ get ready() { return settingsReady; }, get error() { return error; } }');
  await settle();
  assert.equal(instance.ready, false);
  assert.match(instance.error, /bridge unavailable/);
});
