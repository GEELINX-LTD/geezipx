<script lang="ts">
  import { localeStore } from '../stores/localeStore.svelte';
  import { appStore } from '../stores/appStore.svelte';
  import { archiveStore, archiveManager } from '../stores/archiveStore.svelte';
  import ArchivePasswordDialog from '../components/ArchivePasswordDialog.svelte';
  import ArchivePathBar from './extract/ArchivePathBar.svelte';
  import Breadcrumb from './extract/Breadcrumb.svelte';
  import BrowserTable from './extract/BrowserTable.svelte';
  import PreviewPanel from './extract/PreviewPanel.svelte';
  import ExtractControls from './extract/ExtractControls.svelte';

  let activeChallenge = $derived.by(() => {
    const tab = archiveManager.activeTab;
    if (!tab || appStore.activeTab !== tab.id || !tab.passwordChallenge) return null;
    return { tabId: tab.id, challengeId: tab.passwordChallenge.id };
  });
  let controlsKey = $derived(`${archiveManager.activeTabId ?? ''}:${archiveStore.archivePath}`);
</script>

<div class="extract-page">
  <ArchivePathBar />

  {#if archiveStore.isLoading}
    <div class="loading">{localeStore.t('extract.loading')}</div>
  {:else}
    {#if archiveStore.error}
      <div class="error-msg">{localeStore.t('extract.errorPrefix', { message: archiveStore.error })}</div>
    {/if}

    {#if archiveStore.hasArchive}
      <Breadcrumb />
      <div class="browser-area">
        <div class="browser-main">
          <BrowserTable />
          <PreviewPanel />
        </div>
      </div>
      {#key controlsKey}
        <ExtractControls />
      {/key}
    {:else if !archiveStore.error}
      <div class="empty-state">
        <p>{localeStore.t('extract.emptyState')}</p>
      </div>
    {/if}
  {/if}

  {#if activeChallenge}
    {#key `${activeChallenge.tabId}:${activeChallenge.challengeId}`}
      <ArchivePasswordDialog tabId={activeChallenge.tabId} />
    {/key}
  {/if}
</div>

<style>
  .extract-page {
    display: flex;
    flex-direction: column;
    flex: 1;
    overflow: hidden;
    padding: var(--space-4);
    gap: var(--space-3);
  }
  .browser-area { flex: 1; overflow: hidden; }
  .browser-main { display: flex; flex-direction: column; height: 100%; gap: var(--space-3); }
  .loading {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }
  .error-msg {
    padding: var(--space-3);
    background: var(--color-error-bg);
    color: var(--color-error);
    border-radius: var(--radius-md);
    font-size: var(--text-sm);
  }
  .empty-state {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }
</style>
