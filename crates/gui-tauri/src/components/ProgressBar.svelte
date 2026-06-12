<script lang="ts">
  import { taskStore } from '../stores/taskStore.svelte';

  let { kind }: { kind: 'compress' | 'extract' } = $props();
  let task = $derived(taskStore.activeTask && taskStore.activeTask.kind === kind ? taskStore.activeTask : null);

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  }
</script>

{#if task}
  <div class="progress">
    <div class="progress-header">
      <span class="progress-stage">{task.message || task.stage}</span>
      <span class="progress-pct">{task.percent != null ? `${Math.round(task.percent)}%` : '--'}</span>
    </div>
    <div class="progress-bar">
      <div class="progress-fill" style="width: {task.percent ?? 0}%"></div>
    </div>
    <div class="progress-details">
      {#if task.currentEntry}
        <span class="progress-entry truncate">{task.currentEntry}</span>
      {/if}
      <span class="progress-stats">
        {formatBytes(task.current)}
        {#if task.total != null} / {formatBytes(task.total)}{/if}
        {#if task.bytesPerSecond != null && task.bytesPerSecond > 0} · {formatBytes(task.bytesPerSecond)}/s{/if}
      </span>
    </div>
  </div>
{/if}

<style>
  .progress { flex: 1; }
  .progress-header {
    display: flex;
    justify-content: space-between;
    font-size: var(--text-sm);
    margin-bottom: var(--space-1);
  }
  .progress-stage { color: var(--color-text); }
  .progress-pct { color: var(--color-text-secondary); font-weight: 500; }
  .progress-bar {
    height: 4px;
    background: var(--color-progress-bg);
    border-radius: 2px;
    overflow: hidden;
    margin-bottom: var(--space-1);
  }
  .progress-fill {
    height: 100%;
    background: var(--color-progress-fill);
    border-radius: 2px;
    transition: width 200ms ease;
  }
  .progress-details {
    display: flex;
    justify-content: space-between;
    font-size: var(--text-xs);
    color: var(--color-text-muted);
  }
  .progress-entry { max-width: 50%; }
  .progress-stats { flex-shrink: 0; }
</style>
