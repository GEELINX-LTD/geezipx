<script lang="ts">
  import { onMount } from 'svelte';
  import { listen } from './bridge';
  import type { TaskProgressPayload } from './bridge';
  import { appStore } from './stores/appStore.svelte';
  import { taskStore } from './stores/taskStore.svelte';
  import { archiveStore } from './stores/archiveStore.svelte';
  import TabBar from './components/TabBar.svelte';
  import DropOverlay from './components/DropOverlay.svelte';
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

<div class="app-shell">
  <TabBar />
  <main class="app-content">
    <DropOverlay />
    <PageComponent />
  </main>
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
</style>
