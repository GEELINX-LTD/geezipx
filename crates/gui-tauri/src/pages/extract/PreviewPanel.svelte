<script lang="ts">
  import { localeStore } from '../../stores/localeStore.svelte';
  import { archiveStore } from '../../stores/archiveStore.svelte';
</script>

{#if archiveStore.previewState}
  <div class="preview-panel">
    <div class="preview-header">
      <span class="preview-title truncate">{archiveStore.previewState.path.split('/').pop()}</span>
      <span class="preview-meta">{archiveStore.previewState.sizeHint}</span>
      <button class="preview-close" onclick={() => archiveStore.closePreview()} aria-label={localeStore.t('preview.close')}>
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
        </svg>
      </button>
    </div>
    <div class="preview-body">
      {#if archiveStore.previewState.kind === 'dir'}
        <p class="preview-placeholder">{localeStore.t('preview.dirUnavailable')}</p>
      {:else}
        <pre class="preview-content text-mono">{archiveStore.previewState.content}</pre>
      {/if}
    </div>
  </div>
{/if}

<style>
  .preview-panel {
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    overflow: hidden;
    max-height: 200px;
    display: flex;
    flex-direction: column;
  }
  .preview-header {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-3);
    background: var(--color-surface);
    border-bottom: 1px solid var(--color-border-light);
  }
  .preview-title { font-size: var(--text-sm); font-weight: 500; flex: 1; min-width: 0; }
  .preview-meta { font-size: var(--text-xs); color: var(--color-text-muted); flex-shrink: 0; }
  .preview-close {
    display: flex;
    padding: 2px;
    border-radius: var(--radius-sm);
    color: var(--color-text-muted);
  }
  .preview-close:hover { background: var(--color-surface-alt); color: var(--color-text); }
  .preview-body { flex: 1; overflow: auto; padding: var(--space-3); }
  .preview-content {
    font-size: var(--text-xs);
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-all;
  }
  .preview-placeholder {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    text-align: center;
    padding: var(--space-4);
  }
</style>
