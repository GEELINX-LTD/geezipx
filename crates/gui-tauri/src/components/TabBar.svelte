<script lang="ts">
  import { appStore } from '../stores/appStore.svelte';
  import { archiveManager } from '../stores/archiveStore.svelte';
  import { taskStore } from '../stores/taskStore.svelte';
  import { localeStore } from '../stores/localeStore.svelte';
  import { settingsGuard } from '../stores/settingsGuard.svelte';
  import { ask } from '@tauri-apps/plugin-dialog';

  const tabs = [
    { id: 'home', key: 'nav.home' },
    { id: 'compress', key: 'nav.compress' },
    { id: 'extract', key: 'nav.extract' },
  ] as const;

  // Guard navigation away from the settings page when there are unsaved edits.
  async function navTo(tab: string): Promise<void> {
    if (appStore.activeTab === 'settings' && tab !== 'settings' && settingsGuard.dirty) {
      const confirmed = await ask(localeStore.t('settings.unsavedConfirm'), {
        title: localeStore.t('settings.unsavedTitle'),
        kind: 'warning',
      });
      if (!confirmed) return;
      settingsGuard.dirty = false;
    }
    appStore.switchTab(tab);
  }

  function isTaskRunning(): boolean {
    const task = taskStore.activeTask;
    if (!task) return false;
    return task.status === 'pending' || task.status === 'running';
  }

  async function maybeCloseTab(tabId: string): Promise<void> {
    if (isTaskRunning()) {
      const confirmed = await ask(
        localeStore.t('tab.closeConfirmRunning') ||
          'An operation is in progress. Closing this tab will hide the progress but the operation will continue.',
        {
          title: localeStore.t('tab.closeConfirmTitle') || 'Close Tab',
          kind: 'warning',
        }
      );
      if (!confirmed) return;
    }

    const nextId = archiveManager.closeTab(tabId);
    appStore.removeArchiveTab(tabId);

    if (nextId) {
      appStore.switchTab(nextId);
    } else {
      // No more archive tabs; if we were on this tab, go to extract empty page
      if (appStore.activeTab === tabId) {
        appStore.switchTab('extract');
      }
    }
  }

  function activateArchiveTab(tabId: string): void {
    appStore.switchTab(tabId);
    archiveManager.setActive(tabId);
  }

  /** Handle middle-click close on an archive tab. */
  async function handleAuxClick(e: MouseEvent, tabId: string): Promise<void> {
    if (e.button === 1) {
      e.preventDefault();
      await maybeCloseTab(tabId);
    }
  }

  /** Handle close-button click (stop propagation to avoid activating the tab). */
  async function handleCloseClick(e: MouseEvent, tabId: string): Promise<void> {
    e.stopPropagation();
    await maybeCloseTab(tabId);
  }
</script>

<nav class="tab-bar">
  {#each tabs as tab (tab.id)}
    <button
      class="tab"
      class:active={appStore.activeTab === tab.id}
      onclick={() => navTo(tab.id)}
    >
      {localeStore.t(tab.key)}
    </button>
  {/each}

  {#if appStore.archiveTabs.length > 0}
    <span class="tab-separator" aria-hidden="true"></span>

    {#each appStore.archiveTabs as tab (tab.id)}
      <div
        role="tab"
        tabindex="0"
        class="tab archive-tab"
        class:active={appStore.activeTab === tab.id}
        onclick={() => activateArchiveTab(tab.id)}
        onauxclick={(e) => handleAuxClick(e, tab.id)}
        onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); activateArchiveTab(tab.id); } }}
        title={tab.tooltip}
      >
        <span class="archive-label">{tab.label}</span>
        <button
          class="archive-close"
          onclick={(e) => handleCloseClick(e, tab.id)}
          title={localeStore.t('tab.close') || 'Close'}
          aria-label={localeStore.t('tab.close') || 'Close tab'}
        >&times;</button>
      </div>
    {/each}
  {/if}

  <div class="tab-spacer"></div>
  <button
    class="tab"
    class:active={appStore.activeTab === 'settings'}
    onclick={() => navTo('settings')}
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

  /* ── Separator between static and archive tabs ── */
  .tab-separator {
    display: inline-block;
    width: 1px;
    height: 20px;
    background: var(--color-border);
    margin: 0 var(--space-1);
    -webkit-app-region: no-drag;
  }

  /* ── Archive tab styling ── */
  .archive-tab {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    max-width: 180px;
    padding-right: var(--space-2);
  }

  .archive-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .archive-close {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    font-size: 11px;
    line-height: 1;
    color: var(--color-text-secondary);
    background: transparent;
    flex-shrink: 0;
    -webkit-app-region: no-drag;
  }

  .archive-close:hover {
    background: rgba(220, 38, 38, 0.15);
    color: var(--color-error, #dc2626);
  }
</style>
