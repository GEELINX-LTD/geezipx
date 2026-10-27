import assert from 'node:assert/strict';
import test from 'node:test';
import { componentScript, deferred, settle } from './helpers/component-script.mjs';

function drag(overrides = {}) {
  const cleaned = [];
  const instance = componentScript('pages/extract/BrowserTable.svelte', {
    archiveStore: { archivePath: '/a.zip', archivePassword: '', selectedPaths: new Set(['a.txt']), currentChildren: [] },
    prepareDragEntries: async () => '/tmp/drag-id',
    startDrag: async () => {},
    cleanupDragTempDir: async path => { cleaned.push(path); },
    ...overrides,
  }, '{ initiateDrag }');
  return { instance, cleaned };
}

test('drag temp directory is cleaned even when the native drag fails', async () => {
  const { instance, cleaned } = drag({ startDrag: async () => { throw new Error('drag failed'); } });
  await instance.initiateDrag();
  assert.deepEqual(cleaned, ['/tmp/drag-id']);
});

test('drag success cleans its temp directory and preparation failure has nothing to clean', async () => {
  const success = drag();
  await success.instance.initiateDrag();
  assert.deepEqual(success.cleaned, ['/tmp/drag-id']);
  const failure = drag({ prepareDragEntries: async () => { throw new Error('extract failed'); } });
  await failure.instance.initiateDrag();
  assert.deepEqual(failure.cleaned, []);
});

function settingsPage() {
  const saved = [];
  const defaults = { locale: 'en', theme: 'system', default_level: null, default_format: 'zip', shell_menu_enabled: false, shell_menu_verbs: [] };
  const instance = componentScript('pages/SettingsPage.svelte', {
    onMount() {},
    settingsStore: { DEFAULTS: defaults, saveAll: async value => saved.push(value) },
    settingsGuard: { dirty: false },
    localeStore: { locale: 'en', t: key => key },
    toastStore: { show() {} },
    document: { documentElement: { removeAttribute() {}, setAttribute() {} } },
  }, `{
    handleCancel, handleReset, handleSave,
    editLevel(value) { levelInput = value; },
    seedLevel(value) { original = { ...original, default_level: value }; formData = { ...original }; levelInput = String(value); },
    get input() { return levelInput; },
    get level() { return formData.default_level; }
  }`);
  return { instance, saved };
}

test('settings save commits the current level input even before blur', async () => {
  const { instance, saved } = settingsPage();
  instance.editLevel('12');
  await instance.handleSave();
  assert.equal(saved[0].default_level, 12);
});

test('settings cancel and reset synchronize the visible level with the saved value', () => {
  const { instance } = settingsPage();
  instance.seedLevel(9);
  instance.editLevel('12');
  instance.handleCancel();
  assert.equal(instance.input, '9');
  instance.handleReset();
  assert.equal(instance.input, '');
  assert.equal(instance.level, null);
});

test('closing an inactive archive keeps the currently visible home page', async () => {
  const switches = [];
  const instance = componentScript('components/TabBar.svelte', {
    appStore: { activeTab: 'home', switchTab: value => switches.push(value), removeArchiveTab() {} },
    archiveManager: { closeTab: () => 'other-archive' },
    taskStore: { activeTask: null, hasRunning: () => false },
  }, '{ maybeCloseTab }');
  await instance.maybeCloseTab('inactive-archive');
  assert.deepEqual(switches, []);
});

test('app unmount cleans listeners whose registration resolves after unmount', async () => {
  let mounted;
  const registration = deferred();
  const removed = [];
  componentScript('App.svelte', {
    onMount: fn => { mounted = fn; }, HomePage() {},
    appStore: { activeTab: 'home', loadRecent() {} },
    listen: () => registration.promise,
    getShellAction: async () => null,
    getOpenedArchives: async () => [],
    settingsStore: { get: async () => null },
    localeStore: { switchLocale() {} },
    document: { documentElement: { setAttribute() {}, removeAttribute() {} } },
  }, '{}');
  const cleanup = mounted();
  cleanup();
  registration.resolve(() => removed.push('listener'));
  await settle();
  assert.equal(removed.length, 3);
});

test('app restores saved theme and locale during startup', async () => {
  let mounted;
  const attributes = [];
  const locales = [];
  componentScript('App.svelte', {
    onMount: fn => { mounted = fn; }, HomePage() {},
    appStore: { activeTab: 'home', loadRecent() {} },
    listen: async () => () => {},
    getShellAction: async () => null, getOpenedArchives: async () => [],
    settingsStore: { get: async key => ({ theme: 'dark', locale: 'en' })[key] },
    localeStore: { switchLocale: locale => locales.push(locale) },
    document: { documentElement: { setAttribute: (...args) => attributes.push(args), removeAttribute() {} } },
  }, '{}');
  const cleanup = mounted();
  await settle();
  cleanup();
  assert.deepEqual(attributes, [['data-theme', 'dark']]);
  assert.deepEqual(locales, ['en']);
});
