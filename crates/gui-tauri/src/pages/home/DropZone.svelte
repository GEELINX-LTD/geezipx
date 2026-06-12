<script lang="ts">
  import { localeStore } from '../../stores/localeStore.svelte';
</script>

<div class="dropzone">
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
    transition: border-color var(--transition-normal), background var(--transition-normal);
  }
  .dropzone:hover {
    border-color: var(--color-accent);
    background: var(--color-dropzone-bg);
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
