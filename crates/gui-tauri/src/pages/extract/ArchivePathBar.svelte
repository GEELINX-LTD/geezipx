<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import { ARCHIVE_EXTENSIONS } from '../../archiveFormats';
  import { toastStore } from '../../stores/toastStore.svelte';
  import { appStore } from '../../stores/appStore.svelte';
  import { localeStore } from '../../stores/localeStore.svelte';
  import { archiveStore, archiveManager } from '../../stores/archiveStore.svelte';

  async function changeArchive() {
    try {
      await selectArchive();
    } catch (error) {
      toastStore.show(localeStore.t('common.errorDetail', { message: String(error) }), 'error');
    }
  }

  async function selectArchive() {
    const selected = await open({
      multiple: false,
      filters: [{ name: 'Archives', extensions: [...ARCHIVE_EXTENSIONS] }]
    });
    if (selected && typeof selected === 'string') {
      const targetTabId = archiveManager.activeTabId ?? undefined;
      const replacesExistingTab = targetTabId
        ? appStore.archiveTabs.some((tab) => tab.id === targetTabId)
        : false;
      const { tabId, label } = await archiveManager.openArchive(
        selected,
        undefined,
        targetTabId,
      );
      if (!archiveManager.tabs.has(tabId)) return;

      if (!replacesExistingTab) {
        appStore.addArchiveTab(tabId, label, selected);
      }
      appStore.switchTab(tabId);

      if (!replacesExistingTab) return;

      const opened = await archiveManager.waitForOpenResult(tabId, selected);
      const tab = archiveManager.tabs.get(tabId);
      if (
        !opened ||
        tabId !== targetTabId ||
        !tab ||
        tab.archivePath !== selected ||
        tab.pendingArchivePath === selected
      ) return;

      appStore.updateArchiveTab(tabId, label, selected);
    }
  }
</script>

<div class="path-bar">
  {#if archiveStore.hasArchive}
    <div class="path-display">
      <span class="path-label">{localeStore.t('extract.archiveLabel')}</span>
      <span class="path-value text-mono truncate" title={archiveStore.archivePath}>
        {archiveStore.archivePath}
      </span>
    </div>
  {/if}
  <button class="change-btn" onclick={changeArchive}>
    {archiveStore.hasArchive ? localeStore.t('extract.changeBtn') : localeStore.t('extract.openArchiveBtn')}
  </button>
</div>

<style>
  .path-bar {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-3);
    background: var(--color-surface);
    border: 1px solid var(--color-border-light);
    border-radius: var(--radius-md);
  }
  .path-display {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .path-label {
    font-size: var(--text-xs);
    font-weight: 600;
    color: var(--color-text-secondary);
    text-transform: uppercase;
  }
  .path-value {
    font-size: var(--text-sm);
    color: var(--color-text);
    flex: 1;
    min-width: 0;
  }
  .change-btn {
    flex-shrink: 0;
    padding: var(--space-1) var(--space-3);
    background: var(--color-accent);
    color: #fff;
    border-radius: var(--radius-sm);
    font-size: var(--text-xs);
    font-weight: 500;
  }
  .change-btn:hover { background: var(--color-accent-hover); }
</style>
