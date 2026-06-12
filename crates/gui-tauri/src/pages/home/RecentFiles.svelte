<script lang="ts">
  import { appStore } from '../../stores/appStore.svelte';
  import { localeStore } from '../../stores/localeStore.svelte';
</script>

{#if appStore.recentFiles.length > 0}
  <div class="recent-section">
    <span class="recent-label">{localeStore.t('recent.label')}</span>
    <div class="recent-chips">
      {#each appStore.recentFiles as file (file.path)}
        <div class="recent-chip">
          <span class="chip-name" title={file.path}>{file.name}</span>
          <button class="chip-remove" onclick={() => appStore.removeRecent(file.path)} aria-label="Remove">
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </div>
      {/each}
    </div>
  </div>
{/if}

<style>
  .recent-section {
    width: 100%;
    max-width: 520px;
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
  }
  .recent-label {
    font-size: var(--text-xs);
    font-weight: 500;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    flex-shrink: 0;
    padding-top: var(--space-2);
  }
  .recent-chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
  .recent-chip {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    padding: var(--space-1) var(--space-2);
    background: var(--color-surface);
    border: 1px solid var(--color-border-light);
    border-radius: var(--radius-md);
    font-size: var(--text-xs);
    color: var(--color-text);
  }
  .chip-name {
    max-width: 200px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chip-remove {
    display: flex;
    align-items: center;
    padding: 2px;
    border-radius: 50%;
    color: var(--color-text-muted);
  }
  .chip-remove:hover {
    color: var(--color-error);
    background: var(--color-error-bg);
  }
</style>
