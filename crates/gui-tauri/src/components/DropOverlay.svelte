<script lang="ts">
  import { localeStore } from '../stores/localeStore.svelte';

  let depth = $state(0);
  let dropActive = $derived(depth > 0);

  function handleDragEnter(e: DragEvent) {
    e.preventDefault();
    depth++;
  }

  function handleDragLeave(e: DragEvent) {
    e.preventDefault();
    depth--;
    if (depth < 0) depth = 0;
  }

  function handleDragOver(e: DragEvent) {
    e.preventDefault();
  }

  function handleDrop(e: DragEvent) {
    e.preventDefault();
    depth = 0;
    // File handling will be wired later
  }

  function handleGlobalDragEnter(e: DragEvent) {
    e.preventDefault();
    depth++;
  }

  function handleGlobalDragLeave(e: DragEvent) {
    // Only decrement if leaving the actual window (not child elements)
    if (e.target === document.documentElement || e.relatedTarget === null) {
      depth = Math.max(0, depth - 1);
    }
  }
</script>

<svelte:window
  ondragenter={handleGlobalDragEnter}
  ondragleave={handleGlobalDragLeave}
  ondragover={handleDragOver}
  ondrop={handleDrop}
/>

{#if dropActive}
  <div
    class="overlay"
    role="dialog"
    aria-label={localeStore.t('drop.title')}
    tabindex="-1"
    ondragenter={handleDragEnter}
    ondragleave={handleDragLeave}
    ondragover={handleDragOver}
    ondrop={handleDrop}
  >
    <div class="overlay-content">
      <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
        <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
        <polyline points="7 10 12 15 17 10" />
        <line x1="12" y1="15" x2="12" y2="3" />
      </svg>
      <p class="overlay-title">{localeStore.t('drop.title')}</p>
      <p class="overlay-hint">{localeStore.t('drop.hint')}</p>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 1000;
    background: rgba(0, 0, 0, 0.4);
    backdrop-filter: blur(6px);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .overlay-content {
    text-align: center;
    color: #fff;
    padding: var(--space-8);
    border: 2px dashed rgba(255, 255, 255, 0.4);
    border-radius: var(--radius-lg);
    background: rgba(0, 0, 0, 0.2);
  }
  .overlay-title {
    font-size: var(--text-xl);
    font-weight: 600;
    margin-top: var(--space-3);
  }
  .overlay-hint {
    font-size: var(--text-sm);
    margin-top: var(--space-2);
    opacity: 0.7;
  }
</style>
