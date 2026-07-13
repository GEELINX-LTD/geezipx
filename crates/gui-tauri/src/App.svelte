<script lang="ts">
  import { onMount } from 'svelte';
  import { fly } from 'svelte/transition';
  import { listen, getShellAction, getOpenedArchives, extractArchive, compressArchive } from './bridge';
  import type { TaskProgressPayload, ShellActionPayload } from './bridge';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { appStore } from './stores/appStore.svelte';
  import { taskStore } from './stores/taskStore.svelte';
  import { archiveStore, archiveManager } from './stores/archiveStore.svelte';
  import { toastStore } from './stores/toastStore.svelte.ts';
  import { settingsStore } from './stores/settingsStore.svelte';
  import TabBar from './components/TabBar.svelte';
  import DropOverlay from './components/DropOverlay.svelte';
  import ToastContainer from './components/ToastContainer.svelte';
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

      // ── "解压缩到当前文件夹" — smart extract ──
      case 'extract-here': {
        const { tabId, label } = await archiveManager.openArchive(payload.paths[0]);
        appStore.addArchiveTab(tabId, label, payload.paths[0]);
        appStore.switchTab(tabId);
        // suggestedOutputDir handles the smart logic: single top-level folder →
        // extract to parent; scattered → create folder named after archive.
        const outputDir = archiveStore.suggestedOutputDir;
        const taskId = `extract-${Date.now()}`;
        taskStore.startTask(taskId, 'extract');
        try {
          const res = await extractArchive(payload.paths[0], outputDir, true, undefined, taskId);
          taskStore.finishTask('finished');
          toastStore.show(
            `Extracted ${res.files_extracted} file(s) to ${outputDir}`,
            'success',
            5000,
          );
        } catch (e: unknown) {
          const msg = e instanceof Error ? e.message : String(e);
          taskStore.finishTask('failed', msg);
          toastStore.show(`Extraction failed: ${msg}`, 'error');
        }
        break;
      }

      // ── "压缩为 ZIP" — headless one-click ZIP ──
      case 'compress-zip': {
        const first = payload.paths[0];
        const isSingle = payload.paths.length === 1;
        const sep = first.includes('\\') ? '\\' : '/';
        const parentDir = first.substring(0, first.lastIndexOf(sep) + 1) || './';
        const baseName = isSingle
          ? (first.replace(/\/$/, '').split(/[/\\]/).pop() || 'archive')
          : 'archive';
        const outputPath = parentDir + baseName + '.zip';

        // Honour the user's saved compression defaults.
        const [level, rec] = await Promise.all([
          settingsStore.get('default_level'),
          settingsStore.get('recursive'),
        ]);
        const taskId = `compress-${Date.now()}`;
        taskStore.startTask(taskId, 'compress');
        try {
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
          taskStore.finishTask('finished');
          toastStore.show(
            `Compressed ${res.files_added} file(s) → ${outputPath.split(/[/\\]/).pop()}`,
            'success',
            5000,
          );
        } catch (e: unknown) {
          const msg = e instanceof Error ? e.message : String(e);
          taskStore.finishTask('failed', msg);
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
          archiveManager.setActive(lastId);
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
      getCurrentWindow().setTitle(`${name} — GeeZipX`);
    } else {
      getCurrentWindow().setTitle('GeeZipX');
    }
  });

  onMount(() => {
    const unlisteners: (() => void)[] = [];

    listen<TaskProgressPayload>('task:progress', (event) => {
      taskStore.updateTask(event.payload);
    }).then((un) => unlisteners.push(un));

    // Hot-start: another instance forwarded opened archives
    listen<string[]>('opened-archives', (event) => {
      for (const p of event.payload) {
        archiveManager.openArchive(p).then(({ tabId, label }) => {
          appStore.addArchiveTab(tabId, label, p);
          appStore.switchTab(tabId);
        });
      }
    }).then((un) => unlisteners.push(un));

    listen<ShellActionPayload>('shell-action', (event) => {
      handleShellAction(event.payload);
    }).then((un) => unlisteners.push(un));

    // Check for cold-start shell action (e.g., opened from context menu while GUI was not running).
    getShellAction().then((action) => {
      if (action && action.paths.length > 0) {
        handleShellAction(action);
      }
    });

    // ── COLD-START BUG FIX: pull pending archive paths from direct file-open (Windows double-click, etc.) ──
    getOpenedArchives().then((paths) => {
      for (const p of paths) {
        archiveManager.openArchive(p).then(({ tabId, label }) => {
          appStore.addArchiveTab(tabId, label, p);
          appStore.switchTab(tabId);
        });
      }
    });

    appStore.loadRecent();

    return () => unlisteners.forEach((fn) => fn());
  });
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="app-shell">
  <TabBar />
  <main class="app-content">
    <DropOverlay />
    {#key pageKey}
      <div class="page-wrapper" in:fly={{ x: 20, duration: 120 }} out:fly={{ x: -20, duration: 120 }}>
        <PageComponent />
      </div>
    {/key}
  </main>
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
