<script lang="ts">
  import { taskStore } from '../stores/taskStore.svelte';
  import { localeStore } from '../stores/localeStore.svelte';
  import { cancelTask } from '../bridge';
  import ProgressBar from './ProgressBar.svelte';

  let { kind, oncancel }: { kind: 'compress' | 'extract'; oncancel?: () => void } = $props();

  let visible = $derived(taskStore.isVisible && taskStore.activeTask?.kind === kind);
  let title = $derived(taskStore.activeTask?.message || taskStore.activeTask?.stage || '');
  let isRunning = $derived(taskStore.activeTask?.status === 'pending' || taskStore.activeTask?.status === 'running');
  let isFinished = $derived(taskStore.activeTask?.status === 'finished');
  let isFailed = $derived(taskStore.activeTask?.status === 'failed' || taskStore.activeTask?.status === 'cancelled');

  async function handleCancel() {
    const taskId = taskStore.activeTask?.taskId;
    if (taskId) {
      try {
        await cancelTask(taskId);
      } catch {
        // ignore cancel errors
      }
    }
    oncancel?.();
  }

  function handleDismiss() {
    taskStore.dismissTask();
    oncancel?.();
  }

  function handleBackdropClick() {
    if (!isRunning) {
      handleDismiss();
    } else {
      oncancel?.();
    }
  }
</script>

{#if visible}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="backdrop" onclick={handleBackdropClick} onkeydown={(e) => { if (e.key === 'Escape') handleBackdropClick(); }} role="presentation">
    <div class="dialog-card" onclick={(e) => e.stopPropagation()} onkeydown={(e) => { if (e.key === 'Escape') handleBackdropClick(); }} role="dialog" aria-modal="true" tabindex="-1">
      <h3 class="dialog-title">{title}</h3>
      <ProgressBar kind={kind} />
      <div class="dialog-actions">
        {#if isRunning}
          <button class="btn-cancel" onclick={handleCancel}>
            {localeStore.t('common.cancel')}
          </button>
        {:else}
          <button class="btn-dismiss" onclick={handleDismiss}>
            {localeStore.t('common.close')}
          </button>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 100;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.5);
    backdrop-filter: blur(2px);
  }

  .dialog-card {
    background: var(--color-bg);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    padding: var(--space-5);
    min-width: 420px;
    max-width: 520px;
    width: 90vw;
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.3);
  }

  .dialog-title {
    margin: 0;
    font-size: var(--text-base);
    font-weight: 600;
    color: var(--color-text);
  }

  .dialog-actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    padding-top: var(--space-2);
  }

  .btn-cancel {
    padding: var(--space-2) var(--space-4);
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    color: var(--color-text-secondary);
    font-size: var(--text-sm);
    font-weight: 500;
    cursor: pointer;
    transition: background 150ms ease, color 150ms ease;
  }

  .btn-cancel:hover {
    background: var(--color-border);
    color: var(--color-text);
  }

  .btn-dismiss {
    padding: var(--space-2) var(--space-4);
    background: var(--color-primary);
    border: 1px solid var(--color-primary);
    border-radius: var(--radius-md);
    color: #fff;
    font-size: var(--text-sm);
    font-weight: 500;
    cursor: pointer;
    transition: background 150ms ease, opacity 150ms ease;
  }

  .btn-dismiss:hover {
    opacity: 0.85;
  }
</style>
