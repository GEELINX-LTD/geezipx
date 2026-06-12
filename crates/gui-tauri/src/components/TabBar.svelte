<script lang="ts">
  import { appStore } from '../stores/appStore.svelte';
  import { localeStore } from '../stores/localeStore.svelte';

  const tabs = [
    { id: 'home', key: 'nav.home' },
    { id: 'compress', key: 'nav.compress' },
    { id: 'extract', key: 'nav.extract' },
  ] as const;
</script>

<nav class="tab-bar">
  {#each tabs as tab (tab.id)}
    <button
      class="tab"
      class:active={appStore.activeTab === tab.id}
      onclick={() => appStore.switchTab(tab.id)}
    >
      {localeStore.t(tab.key)}
    </button>
  {/each}
  <div class="tab-spacer"></div>
  <button
    class="tab"
    class:active={appStore.activeTab === 'settings'}
    onclick={() => appStore.switchTab('settings')}
  >
    {localeStore.t('nav.settings')}
  </button>
</nav>

<style>
  .tab-bar {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    padding: 0 var(--space-4);
    height: 40px;
    background: var(--color-surface);
    border-bottom: 1px solid var(--color-border);
    user-select: none;
    -webkit-app-region: drag;
  }
  .tab {
    padding: var(--space-1) var(--space-3);
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
    border-radius: var(--radius-sm);
    transition: color var(--transition-fast), background var(--transition-fast);
    -webkit-app-region: no-drag;
  }
  .tab:hover {
    background: var(--color-surface-alt);
    color: var(--color-text);
  }
  .tab.active {
    color: var(--color-accent);
    font-weight: 500;
  }
  .tab-spacer {
    flex: 1;
  }
</style>
