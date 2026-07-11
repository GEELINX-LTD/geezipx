<script lang="ts">
  import { fly } from 'svelte/transition';
  import { toastStore } from '../stores/toastStore.svelte.ts';
</script>

{#if toastStore.toasts.length > 0}
  <div class="toast-container">
    {#each toastStore.toasts as toast (toast.id)}
      <div
        class="toast toast-{toast.type}"
        role="alert"
        transition:fly={{ x: 320, duration: 300, opacity: 0 }}
      >
        <span class="toast-message">{toast.message}</span>
        <button
          class="toast-close"
          onclick={() => toastStore.dismiss(toast.id)}
          aria-label="Close notification"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>
      </div>
    {/each}
  </div>
{/if}

<style>
  .toast-container {
    position: fixed;
    bottom: var(--space-5);
    right: var(--space-5);
    z-index: 2000;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    max-width: 400px;
  }

  .toast {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border-radius: var(--radius-md);
    font-size: var(--text-sm);
    color: var(--color-text);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
    min-width: 280px;
    border-left: 3px solid;
  }

  .toast-success {
    background: var(--color-success-bg);
    border-color: var(--color-success);
  }

  .toast-error {
    background: var(--color-error-bg);
    border-color: var(--color-error);
  }

  .toast-info {
    background: var(--color-accent-light);
    border-color: var(--color-accent);
  }

  .toast-message {
    flex: 1;
    line-height: 1.4;
    overflow: hidden;
    text-overflow: ellipsis;
    display: -webkit-box;
    -webkit-line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow-wrap: break-word;
  }

  .toast-close {
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    color: inherit;
    opacity: 0.5;
    transition: opacity var(--transition-fast), background var(--transition-fast);
  }

  .toast-close:hover {
    opacity: 1;
    background: rgba(0, 0, 0, 0.08);
  }
</style>
