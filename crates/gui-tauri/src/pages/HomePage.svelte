<script lang="ts">
  import DropZone from './home/DropZone.svelte';
  import ActionCards from './home/ActionCards.svelte';
  import RecentFiles from './home/RecentFiles.svelte';
  import { localeStore } from '../stores/localeStore.svelte';
</script>

<div class="home-page">
  <div class="welcome">
    <h1 class="welcome-title">{localeStore.t('home.title')}</h1>
    <p class="welcome-sub">{localeStore.t('home.subtitle')}</p>
  </div>

  <DropZone />
  <ActionCards />
  <RecentFiles />
</div>

<style>
  .home-page {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-6);
    padding: var(--space-8) var(--space-6);
    overflow-y: auto;
    flex: 1;
  }
  .welcome {
    text-align: center;
    margin-bottom: var(--space-2);
  }
  .welcome-title {
    font-size: var(--text-xl);
    font-weight: 600;
    color: var(--color-text);
    margin-bottom: var(--space-1);
  }
  .welcome-sub {
    font-size: var(--text-sm);
    color: var(--color-text-muted);
  }
</style>
