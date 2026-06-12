<script lang="ts">
  import { localeStore } from '../../stores/localeStore.svelte';
  import { archiveStore } from '../../stores/archiveStore.svelte';

  let segments = $derived.by(() => {
    if (!archiveStore.currentDir) return [{ label: localeStore.t('browser.root'), dir: '' }];
    const parts = archiveStore.currentDir.replace(/\/$/, '').split('/');
    const result: { label: string; dir: string }[] = [{ label: localeStore.t('browser.root'), dir: '' }];
    let built = '';
    for (const part of parts) {
      built += part + '/';
      result.push({ label: part, dir: built });
    }
    return result;
  });
</script>

{#if archiveStore.hasArchive}
  <nav class="breadcrumb" aria-label="Directory navigation">
    {#each segments as seg, i (seg.dir)}
      {#if i > 0}
        <span class="bc-sep">/</span>
      {/if}
      <button
        class="bc-item"
        class:active={i === segments.length - 1}
        onclick={() => archiveStore.navigateTo(seg.dir)}
      >
        {seg.label}
      </button>
    {/each}
  </nav>
{/if}

<style>
  .breadcrumb {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    padding: var(--space-1) 0;
    font-size: var(--text-sm);
    overflow-x: auto;
  }
  .bc-item {
    padding: 2px var(--space-1);
    border-radius: var(--radius-sm);
    color: var(--color-accent);
    font-size: var(--text-sm);
    white-space: nowrap;
  }
  .bc-item:hover { background: var(--color-accent-light); }
  .bc-item.active { color: var(--color-text); font-weight: 500; cursor: default; }
  .bc-item.active:hover { background: transparent; }
  .bc-sep { color: var(--color-text-muted); font-size: var(--text-sm); }
</style>
