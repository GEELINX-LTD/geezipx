<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import { appStore } from '../../stores/appStore.svelte';
  import { archiveManager } from '../../stores/archiveStore.svelte';
  import { localeStore } from '../../stores/localeStore.svelte';

  import { isArchivePath } from '../../archiveFormats';
  import { toastStore } from '../../stores/toastStore.svelte';

  let dialogOpen = false;

  async function handleClick() {
    if (dialogOpen) return;
    dialogOpen = true;
    try {
      const result = await open({ multiple: true });
      if (!result) return;
      const paths = Array.isArray(result) ? result : [result];
      if (paths.length === 0) return;

      const archives = paths.filter(isArchivePath);
      const files = paths.filter(p => !isArchivePath(p));

      if (archives.length === 1 && files.length === 0) {
        appStore.addRecent(archives[0]);
        const { tabId, label } = await archiveManager.openArchive(archives[0]);
        appStore.addArchiveTab(tabId, label, archives[0]);
        appStore.switchTab(tabId);
      } else if (files.length > 0 || archives.length > 1) {
        appStore.setPendingSourcePaths(paths);
        appStore.switchTab('compress');
      }
    } catch (error) {
      toastStore.show(localeStore.t('common.errorDetail', { message: String(error) }), 'error');
    } finally {
      dialogOpen = false;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      handleClick();
    }
  }
</script>

<div
  class="dropzone"
  role="button"
  tabindex="0"
  onclick={handleClick}
  onkeydown={handleKeydown}
>
  <svg width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
    <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
    <polyline points="17 8 12 3 7 8" />
    <line x1="12" y1="3" x2="12" y2="15" />
  </svg>
  <p class="dropzone-title">{localeStore.t('drop.title')}</p>
  <p class="dropzone-hint">{localeStore.t('drop.hint')}</p>
</div>

<style>
  .dropzone {
    width: 100%;
    max-width: 520px;
    padding: var(--space-8) var(--space-6);
    border: 2px dashed var(--color-border);
    border-radius: var(--radius-lg);
    text-align: center;
    color: var(--color-text-secondary);
    transition: border-color var(--transition-normal), background var(--transition-normal), transform var(--transition-fast);
    cursor: pointer;
  }
  .dropzone:hover {
    border-color: var(--color-accent);
    background: var(--color-dropzone-bg);
  }
  .dropzone:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: 2px;
  }
  .dropzone:active {
    transform: scale(0.99);
  }
  .dropzone svg {
    color: var(--color-text-muted);
    margin-bottom: var(--space-3);
  }
  .dropzone-title {
    font-size: var(--text-base);
    font-weight: 500;
    color: var(--color-text);
    margin-bottom: var(--space-1);
  }
  .dropzone-hint {
    font-size: var(--text-xs);
    color: var(--color-text-muted);
  }
</style>
