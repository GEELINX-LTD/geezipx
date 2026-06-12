<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import { localeStore } from '../../stores/localeStore.svelte';
  import { extractArchive, extractEntries, cancelTask } from '../../bridge';
  import { archiveStore } from '../../stores/archiveStore.svelte';
  import { taskStore } from '../../stores/taskStore.svelte';

  let disabled = $derived(taskStore.isRunning && taskStore.activeTask?.kind === 'extract');
  let outputDir = $state('');
  let overwrite = $state(true);
  let extractPassword = $state('');
  let result: string | null = $state(null);
  let error: string | null = $state(null);

  async function browseOutput() {
    const selected = await open({ directory: true, multiple: false });
    if (selected && typeof selected === 'string') {
      outputDir = selected;
    }
  }

  async function runExtractAll() {
    if (!archiveStore.archivePath || !outputDir) return;
    error = null; result = null;
    const taskId = `extract-${Date.now()}`;
    taskStore.startTask(taskId, 'extract');
    try {
      const res = await extractArchive(archiveStore.archivePath, outputDir, overwrite, extractPassword || undefined, taskId);
      taskStore.finishTask('finished');
      result = localeStore.t('extract.result', { files: res.files_extracted, bytes: res.bytes_extracted, skipped: res.files_skipped > 0 ? localeStore.t('extract.resultSkipped', { count: res.files_skipped }) : '' });
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      taskStore.finishTask('failed', msg);
      error = localeStore.t('extract.errorPrefix', { message: msg });
    }
  }

  async function runExtractSelected() {
    if (!archiveStore.archivePath || !outputDir || archiveStore.selectedCount === 0) return;
    error = null; result = null;
    const taskId = `extract-${Date.now()}`;
    taskStore.startTask(taskId, 'extract');
    try {
      const res = await extractEntries(archiveStore.archivePath, [...archiveStore.selectedPaths], outputDir, overwrite, extractPassword || undefined, taskId);
      taskStore.finishTask('finished');
      result = localeStore.t('extract.result', { files: res.files_extracted, bytes: 0, skipped: '' });
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      taskStore.finishTask('failed', msg);
      error = localeStore.t('extract.errorPrefix', { message: msg });
    }
  }
</script>

{#if archiveStore.hasArchive}
  <div class="controls">
    <div class="controls-top">
      <div class="controls-field">
        <label class="controls-label" for="extract-output-dir">{localeStore.t('extract.output')}</label>
        <div class="input-row">
          <input id="extract-output-dir" type="text" bind:value={outputDir} readonly placeholder={localeStore.t('extract.outputPlaceholder')} />
          <button class="btn-secondary" onclick={browseOutput} disabled={disabled}>{localeStore.t('common.browse')}</button>
        </div>
      </div>
      <div class="controls-options">
        <label class="checkbox-label">
          <input type="checkbox" bind:checked={overwrite} />
          <span>{localeStore.t('extract.overwrite')}</span>
        </label>
        <div class="controls-field-inline">
          <label class="controls-label" for="extract-password">{localeStore.t('extract.password')}</label>
          <input id="extract-password" type="password" bind:value={extractPassword} placeholder={localeStore.t('extract.passwordPlaceholder')} class="pw-input" />
        </div>
      </div>
    </div>

    {#if result}
      <div class="extract-result success">{result}</div>
    {/if}
    {#if error}
      <div class="extract-result error">{error}</div>
    {/if}

    <div class="controls-actions">
      <button class="btn-primary" onclick={runExtractAll} disabled={disabled || !outputDir}>{localeStore.t('extract.extractAll')}</button>
      <button class="btn-secondary" onclick={runExtractSelected} disabled={disabled || !outputDir || archiveStore.selectedCount === 0}>
        {localeStore.t('extract.extractSelected')} ({archiveStore.selectedCount})
      </button>
    </div>
  </div>
{/if}

<style>
  .controls {
    padding: var(--space-3);
    background: var(--color-surface);
    border: 1px solid var(--color-border-light);
    border-radius: var(--radius-md);
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .controls-top { display: flex; flex-direction: column; gap: var(--space-2); }
  .controls-field { display: flex; flex-direction: column; gap: var(--space-1); }
  .controls-label {
    font-size: var(--text-xs);
    font-weight: 600;
    color: var(--color-text-secondary);
    text-transform: uppercase;
  }
  .input-row { display: flex; gap: var(--space-2); }
  .input-row input { flex: 1; }
  .controls-options {
    display: flex;
    align-items: center;
    gap: var(--space-4);
  }
  .checkbox-label {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-sm);
    cursor: pointer;
  }
  .controls-field-inline {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .pw-input { width: 140px; }
  .extract-result { padding: var(--space-2); border-radius: var(--radius-sm); font-size: var(--text-xs); }
  .extract-result.success { background: var(--color-success-bg); color: var(--color-success); }
  .extract-result.error { background: var(--color-error-bg); color: var(--color-error); }
  .controls-actions { display: flex; gap: var(--space-2); }
  .btn-primary {
    padding: var(--space-2) var(--space-4);
    background: var(--color-accent);
    color: #fff;
    border-radius: var(--radius-md);
    font-weight: 500;
    font-size: var(--text-sm);
  }
  .btn-primary:hover { background: var(--color-accent-hover); }
  .btn-primary:disabled { opacity: 0.5; cursor: not-allowed; }
  .btn-secondary {
    padding: var(--space-2) var(--space-3);
    background: var(--color-surface-alt);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    font-size: var(--text-sm);
  }
  .btn-secondary:hover { background: var(--color-border); }
  .btn-secondary:disabled { opacity: 0.4; cursor: not-allowed; }
</style>
