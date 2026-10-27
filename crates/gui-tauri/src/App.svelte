<script lang="ts">
  import { onMount } from 'svelte';
  import { listen, getShellAction, getOpenedArchives, compressArchive } from './bridge';
  import type { TaskProgressPayload, ShellActionPayload } from './bridge';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { basename, dirname, join } from '@tauri-apps/api/path';
  import { appStore } from './stores/appStore.svelte';
  import { taskStore } from './stores/taskStore.svelte';
  import { archiveManager } from './stores/archiveStore.svelte';
  import { toastStore } from './stores/toastStore.svelte.ts';
  import { settingsStore } from './stores/settingsStore.svelte';
  import { localeStore } from './stores/localeStore.svelte';
  import TabBar from './components/TabBar.svelte';
  import DropOverlay from './components/DropOverlay.svelte';
  import ToastContainer from './components/ToastContainer.svelte';
  import ProgressDialog from './components/ProgressDialog.svelte';
  import HomePage from './pages/HomePage.svelte';
  import CompressPage from './pages/CompressPage.svelte';
  import ExtractPage from './pages/ExtractPage.svelte';
  import SettingsPage from './pages/SettingsPage.svelte';
  import AboutPage from './pages/AboutPage.svelte';

  let PageComponent = $derived.by(() => {
    const tab = appStore.activeTab;
    switch (tab) {
      case 'compress': return CompressPage;
      case 'settings': return SettingsPage;
      case 'about': return AboutPage;
      case 'home': return HomePage;
      default:
        // Any other value (UUID or 'extract') -> ExtractPage
        return ExtractPage;
    }
  });

  let pageKey = $derived.by(() => {
    switch (appStore.activeTab) {
      case 'home':
      case 'compress':
      case 'settings':
      case 'about':
        return appStore.activeTab;    // unique key, remount on switch
      default:
        return 'extract-page';       // all archive tabs + extract share key
    }
  });

  let compressDebounceTimer: ReturnType<typeof setTimeout> | null = null;

  /** Handle a shell context menu action. */
  async function handleShellAction(payload: ShellActionPayload) {
    if (payload.paths.length === 0) return;

    switch (payload.action) {
      // ── "用 GeeZipX 打开" — browse archive ──
      case 'open': {
        const { tabId, label } = await archiveManager.openArchive(payload.paths[0]);
        appStore.addArchiveTab(tabId, label, payload.paths[0]);
        appStore.switchTab(tabId);
        break;
      }

      // ── "解压缩到..." — jump to extract page ──
      case 'extract': {
        const { tabId, label } = await archiveManager.openArchive(payload.paths[0]);
        appStore.addArchiveTab(tabId, label, payload.paths[0]);
        appStore.switchTab(tabId);
        break;
      }

      // ── "压缩为 ZIP" — headless one-click ZIP ──
      case 'compress-zip': {
        const first = payload.paths[0];
        const taskId = taskStore.startTask('compress');
        try {
          // Resolve native paths and saved defaults inside the task's error boundary.
          const [level, rec, defaultOutputDir] = await Promise.all([
            settingsStore.get('default_level'),
            settingsStore.get('recursive'),
            settingsStore.get('default_output_dir'),
          ]);
          const baseName = payload.paths.length === 1 ? (await basename(first) || 'archive') : 'archive';
          const parentDir = defaultOutputDir || await dirname(first);
          const outputName = `${baseName}.zip`;
          const outputPath = await join(parentDir, outputName);
          const res = await compressArchive(
            payload.paths,
            outputPath,
            'zip',
            level ?? undefined,
            undefined,
            undefined,
            rec ?? true,
            taskId,
          );
          const status = taskStore.finishTask(taskId, 'finished');
          if (status === 'cancelled' || status === 'failed') return;
          toastStore.show(
            `Compressed ${res.files_added} file(s) → ${outputName}`,
            'success',
            5000,
          );
        } catch (e: unknown) {
          const msg = e instanceof Error ? e.message : String(e);
          const status = taskStore.finishTask(taskId, 'failed', msg);
          if (status === 'cancelled') return;
          toastStore.show(`Compression failed: ${msg}`, 'error');
        }
        break;
      }

      // ── "压缩为..." — jump to compress page ──
      case 'compress':
        appStore.addShellCompressPaths(payload.paths);
        // Debounce: accumulate paths from rapid multi-file invocations.
        if (compressDebounceTimer) clearTimeout(compressDebounceTimer);
        compressDebounceTimer = setTimeout(() => {
          appStore.switchTab('compress');
        }, 300);
        break;
    }
  }

  // Global keyboard shortcuts: Ctrl+N → tab numbers
  function handleKeydown(e: KeyboardEvent) {
    // Skip when typing in inputs
    const tag = (e.target as HTMLElement)?.tagName;
    if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || (e.target as HTMLElement)?.isContentEditable) {
      return;
    }

    const mod = e.metaKey || e.ctrlKey;
    if (!mod) return;

    switch (e.key) {
      case '1': e.preventDefault(); appStore.switchTab('home'); break;
      case '2': e.preventDefault(); appStore.switchTab('compress'); break;
      case '3': {
        e.preventDefault();
        // Switch to the most-recent archive tab, or fall back to extract
        if (archiveManager.tabOrder.length > 0) {
          const lastId = archiveManager.tabOrder[archiveManager.tabOrder.length - 1];
          appStore.switchTab(lastId);
        } else {
          appStore.switchTab('extract');
        }
        break;
      }
      case ',': e.preventDefault(); appStore.switchTab('settings'); break;
    }
  }

  // ── Window title reactive effect ──
  $effect(() => {
    const tab = archiveManager.activeTab;
    const isViewingArchive = tab && appStore.activeTab === tab.id;
    if (isViewingArchive) {
      const name = tab.archivePath.split(/[/\\]/).pop() || 'Archive';
      getCurrentWindow().setTitle(`${name} — GeeZipX`).catch((error) => console.error('Window title update failed', error));
    } else {
      getCurrentWindow().setTitle('GeeZipX').catch((error) => console.error('Window title update failed', error));
    }
  });

  onMount(() => {
    let disposed = false;
    const reportFailure = (error: unknown) => {
      console.error('Application initialization or native event failed', error);
      if (!disposed) toastStore.show(localeStore.t('common.errorDetail', { message: String(error) }), 'error');
    };
    const openArchive = async (path: string) => {
      const { tabId, label } = await archiveManager.openArchive(path);
      if (disposed) return;
      appStore.addArchiveTab(tabId, label, path);
      appStore.switchTab(tabId);
    };
    const unlisteners: (() => void)[] = [];
    const keepUnlistener = (unlisten: () => void) => {
      if (disposed) unlisten();
      else unlisteners.push(unlisten);
    };

    Promise.all([settingsStore.get('theme'), settingsStore.get('locale')]).then(([theme, locale]) => {
      if (disposed) return;
      if (theme === 'light' || theme === 'dark') document.documentElement.setAttribute('data-theme', theme);
      else document.documentElement.removeAttribute('data-theme');
      if (locale) localeStore.switchLocale(locale);
    }).catch(reportFailure);

    listen<TaskProgressPayload>('task:progress', (event) => {
      taskStore.updateTask(event.payload);
    }).then(keepUnlistener).catch(reportFailure);
    // Hot-start: another instance forwarded opened archives
    listen<string[]>('opened-archives', (event) => {
      if (disposed) return;
      for (const p of event.payload) {
        void openArchive(p).catch(reportFailure);
      }
    }).then(keepUnlistener).catch(reportFailure);

    listen<ShellActionPayload>('shell-action', (event) => {
      if (!disposed) void handleShellAction(event.payload).catch(reportFailure);
    }).then(keepUnlistener).catch(reportFailure);

    // Check for cold-start shell action (e.g., opened from context menu while GUI was not running).
    getShellAction().then((action) => {
      if (!disposed && action && action.paths.length > 0) {
        return handleShellAction(action);
      }
    }).catch(reportFailure);

    // ── COLD-START BUG FIX: pull pending archive paths from direct file-open (Windows double-click, etc.) ──
    getOpenedArchives().then((paths) => {
      if (disposed) return;
      for (const p of paths) {
        void openArchive(p).catch(reportFailure);
      }
    }).catch(reportFailure);

    appStore.loadRecent();

    return () => {
      disposed = true;
      if (compressDebounceTimer) clearTimeout(compressDebounceTimer);
      unlisteners.forEach((fn) => fn());
    };
  });
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="app-shell">
  <TabBar />
  <main class="app-content">
    <DropOverlay />
    {#key pageKey}
      <div class="page-wrapper">
        <PageComponent />
      </div>
    {/key}
  </main>
  <ProgressDialog />
  <ToastContainer />
</div>

<style>
  .app-shell {
    display: flex;
    flex-direction: column;
    height: 100vh;
    overflow: hidden;
  }
  .app-content {
    flex: 1;
    overflow: hidden;
    position: relative;
    display: flex;
    flex-direction: column;
  }
  .page-wrapper {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
</style>
