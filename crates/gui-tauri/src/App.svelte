<script lang="ts">
  import { onMount } from 'svelte';
  import { fly } from 'svelte/transition';
  import { listen } from './bridge';
  import type { TaskProgressPayload } from './bridge';
  import { appStore } from './stores/appStore.svelte';
  import { taskStore } from './stores/taskStore.svelte';
  import { archiveStore } from './stores/archiveStore.svelte';
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
