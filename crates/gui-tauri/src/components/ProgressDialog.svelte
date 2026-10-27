<script lang="ts">
  import { taskStore } from '../stores/taskStore.svelte';
  import { localeStore } from '../stores/localeStore.svelte';
  import { cancelTask } from '../bridge';
  import ProgressBar from './ProgressBar.svelte';

  async function handleCancel(taskId: string) {
    try {
      await cancelTask(taskId);
    } catch {
      // The terminal event or invoke result determines whether cancellation won.
    }
  }

  function handleBackdropClick(taskId: string) {
    taskStore.dismissTask(taskId); // Running tasks cannot be dismissed.
  }
</script>

{#if taskStore.activeTask}
  {@const task = taskStore.activeTask}
  {@const isRunning = task.status === 'pending' || task.status === 'running'}
  <div class="backdrop" onclick={() => handleBackdropClick(task.taskId)} onkeydown={(e) => { if (e.key === 'Escape') handleBackdropClick(task.taskId); }} role="presentation">
    <div class="dialog-card" onclick={(e) => e.stopPropagation()} onkeydown={(e) => { if (e.key === 'Escape') handleBackdropClick(task.taskId); }} role="dialog" aria-modal="true" aria-labelledby="task-dialog-title" tabindex="-1">
      <h3 id="task-dialog-title" class="dialog-title">{localeStore.t(`tasks.${task.kind}`)} · {localeStore.t(`tasks.status.${task.status}`)}</h3>
      {#if taskStore.tasks.length > 1}
        <div class="task-choices" role="group" aria-label={localeStore.t('tasks.choose')}>
          {#each taskStore.tasks as choice (choice.taskId)}
            <button
              class="task-choice"
              aria-pressed={choice.taskId === task.taskId}
              onclick={() => taskStore.selectTask(choice.taskId)}
            >
              {localeStore.t('tasks.choice', { kind: localeStore.t(`tasks.${choice.kind}`), number: choice.ordinal, status: localeStore.t(`tasks.status.${choice.status}`) })}
            </button>
          {/each}
        </div>
      {/if}
      <ProgressBar kind={task.kind} />
      <div class="dialog-actions">
        {#if isRunning}
          <button class="btn-cancel" onclick={() => handleCancel(task.taskId)}>
            {localeStore.t('common.cancel')}
          </button>
        {:else}
          <button class="btn-dismiss" onclick={() => taskStore.dismissTask(task.taskId)}>
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

  .task-choices {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    max-height: 180px;
    overflow-y: auto;
  }

  .task-choice {
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    color: var(--color-text);
    text-align: left;
  }

  .task-choice[aria-pressed='true'] {
    border-color: var(--color-accent);
    background: var(--color-surface);
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
