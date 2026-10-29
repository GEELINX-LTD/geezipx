<script lang="ts">
  import { onMount } from 'svelte';
  import { listen, getLaunchRequests, acknowledgeLaunchRequest, compressArchive } from './bridge';
  import type { TaskProgressPayload, ShellActionPayload, LaunchRequest } from './bridge';
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
  async function handleShellAction(payload: ShellActionPayload, isDisposed: () => boolean = () => false) {
    if (isDisposed() || payload.paths.length === 0) return;

    switch (payload.action) {
      // ── "用 GeeZipX 打开" — browse archive ──
      case 'open':
      // ── "解压缩到..." — jump to extract page ──
      case 'extract': {
        const { tabId, label } = await archiveManager.openArchive(payload.paths[0]);
        if (isDisposed()) return;
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
          if (isDisposed()) return;
          const baseName = payload.paths.length === 1 ? (await basename(first) || 'archive') : 'archive';
          const parentDir = defaultOutputDir || await dirname(first);
          const outputName = `${baseName}.zip`;
          const outputPath = await join(parentDir, outputName);
          if (isDisposed()) return;
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
          if (isDisposed()) return;
          const status = taskStore.finishTask(taskId, 'finished');
          if (status === 'cancelled' || status === 'failed') return;
          toastStore.show(
            `Compressed ${res.files_added} file(s) → ${outputName}`,
            'success',
            5000,
          );
        } catch (e: unknown) {
          if (isDisposed()) return;
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
          if (!isDisposed()) appStore.switchTab('compress');
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

    // Keep IDs for this mount, including acknowledged requests: a stale snapshot
    // must never execute the same launch again.
    const deliveries: Record<string, 'running' | 'processed' | 'acknowledging' | 'acknowledged' | undefined> = Object.create(null);
    const acknowledge = async (id: string) => {
      if (disposed || deliveries[id] !== 'processed') return;
      deliveries[id] = 'acknowledging';
      try {
        await acknowledgeLaunchRequest(id);
        deliveries[id] = 'acknowledged';
      } catch (error) {
        deliveries[id] = 'processed';
        reportFailure(error);
      }
    };
    const deliver = async (request: LaunchRequest) => {
      deliveries[request.id] = 'running';
      try {
        if (request.action.kind === 'open-archives') {
          await Promise.all(request.action.payload.map(path => openArchive(path).catch(reportFailure)));
        } else {
          await handleShellAction(request.action.payload, () => disposed);
        }
      } catch (error) {
        reportFailure(error);
      }
      // A surfaced handler failure is delivered too; replaying it could repeat work.
      if (disposed) return;
      deliveries[request.id] = 'processed';
      await acknowledge(request.id);
    };
    let ready = false;
    let draining = false;
    let drainRequested = false;
    const drain = async () => {
      draining = true;
      try {
        while (!disposed && drainRequested) {
          drainRequested = false;
          try {
            const requests = await getLaunchRequests();
            if (disposed) return;
            for (const request of requests) {
              if (deliveries[request.id] === undefined) void deliver(request);
              else if (deliveries[request.id] === 'processed') void acknowledge(request.id);
            }
          } catch (error) {
            // Honor queued wakeups after failure; without one, stop instead of spinning.
            reportFailure(error);
          }
        }
      } finally {
        draining = false;
      }
    };
    const requestDrain = () => {
      if (disposed) return;
      drainRequested = true;
      if (ready && !draining) void drain();
    };
    const registered = async (registration: Promise<() => void>) => {
      try {
        keepUnlistener(await registration);
        return true;
      } catch (error) {
        reportFailure(error);
        return false;
      }
    };
    // Subscribe before the first snapshot. Setup wakeups only set the drain flag;
    // launch handlers cannot run before progress and wakeup listeners are ready.
    Promise.all([
      registered(listen<TaskProgressPayload>('task:progress', (event) => {
        if (!disposed) taskStore.updateTask(event.payload);
      })),
      registered(listen<void>('launch-requests-pending', requestDrain)),
    ]).then((registrations) => {
      if (disposed || !registrations.every(Boolean)) return;
      ready = true;
      requestDrain();
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
