<script lang="ts">
  import { onMount } from 'svelte';
  import { fly } from 'svelte/transition';
  import { listen, getShellAction, extractArchive, compressArchive } from './bridge';
  import type { TaskProgressPayload, ShellActionPayload } from './bridge';
  import { appStore } from './stores/appStore.svelte';
  import { taskStore } from './stores/taskStore.svelte';
  import { archiveStore } from './stores/archiveStore.svelte';
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
    switch (appStore.activeTab) {
      case 'compress': return CompressPage;
      case 'extract': return ExtractPage;
      case 'settings': return SettingsPage;
      case 'about': return AboutPage;
      default: return HomePage;
    }
  });

  let compressDebounceTimer: ReturnType<typeof setTimeout> | null = null;

  /** Handle a shell context menu action. */
  async function handleShellAction(payload: ShellActionPayload) {
    if (payload.paths.length === 0) return;

    switch (payload.action) {
      // ── "用 GeeZipX 打开" — browse archive ──
      case 'open':
        appStore.switchTab('extract');
        await archiveStore.openArchive(payload.paths[0]);
        break;

      // ── "解压缩到..." — jump to extract page ──
      case 'extract':
        appStore.switchTab('extract');
        await archiveStore.openArchive(payload.paths[0]);
        break;

      // ── "解压缩到当前文件夹" — smart extract ──
      case 'extract-here': {
        appStore.switchTab('extract');
        await archiveStore.openArchive(payload.paths[0]);
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

        // Honour user's saved defaults for level, password, and recursion.
        const [level, pwd, remember, rec] = await Promise.all([
          settingsStore.get('default_level'),
          settingsStore.get('default_password'),
          settingsStore.get('remember_password'),
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
            remember ? (pwd || undefined) : undefined,
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
      case '3': e.preventDefault(); appStore.switchTab('extract'); break;
      case ',': e.preventDefault(); appStore.switchTab('settings'); break;
    }
  }

  onMount(() => {
    const unlisteners: (() => void)[] = [];

    listen<TaskProgressPayload>('task:progress', (event) => {
      taskStore.updateTask(event.payload);
    }).then((un) => unlisteners.push(un));

    listen<string[]>('opened-archives', (event) => {
      if (event.payload.length > 0) {
        appStore.switchTab('extract');
        archiveStore.openArchive(event.payload[0]);
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

    appStore.loadRecent();

    return () => unlisteners.forEach((fn) => fn());
  });
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="app-shell">
  <TabBar />
  <main class="app-content">
    <DropOverlay />
    {#key appStore.activeTab}
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
