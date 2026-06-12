<script lang="ts">
  import { appStore } from '../../stores/appStore.svelte';
  import { localeStore } from '../../stores/localeStore.svelte';
  function openCompress() {
    appStore.switchTab('compress');
  }

  function openExtract() {
    appStore.switchTab('extract');
  }
</script>

<div class="action-cards">
  <button class="action-card" onclick={openCompress}>
    <span class="card-icon">
      <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
        <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
        <polyline points="17 8 12 3 7 8" />
        <line x1="12" y1="3" x2="12" y2="15" />
      </svg>
    </span>
    <span class="card-title">{localeStore.t('home.createArchive')}</span>
    <span class="card-desc">{localeStore.t('home.createArchiveDesc')}</span>
  </button>

  <button class="action-card" onclick={openExtract}>
    <span class="card-icon">
      <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
        <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
        <polyline points="7 10 12 15 17 10" />
        <line x1="12" y1="15" x2="12" y2="3" />
      </svg>
    </span>
    <span class="card-title">{localeStore.t('home.openArchive')}</span>
    <span class="card-desc">{localeStore.t('home.openArchiveDesc')}</span>
  </button>
</div>

<style>
  .action-cards {
    display: flex;
    gap: var(--space-4);
    width: 100%;
    max-width: 520px;
  }
  .action-card {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-5) var(--space-4);
    background: var(--color-surface);
    border: 1px solid var(--color-border-light);
    border-radius: var(--radius-lg);
    text-align: center;
    transition: background var(--transition-fast), border-color var(--transition-fast), box-shadow var(--transition-fast);
  }
  .action-card:hover {
    background: var(--color-surface-alt);
    border-color: var(--color-border);
    box-shadow: var(--shadow-md);
  }
  .card-icon {
    color: var(--color-accent);
    display: flex;
  }
  .card-title {
    font-size: var(--text-base);
    font-weight: 600;
    color: var(--color-text);
  }
  .card-desc {
    font-size: var(--text-xs);
    color: var(--color-text-muted);
    line-height: 1.4;
  }
</style>
