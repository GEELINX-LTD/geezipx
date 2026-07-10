<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import { localeStore } from '../../stores/localeStore.svelte';
  import { settingsStore } from '../../stores/settingsStore.svelte';
  import { extractArchive, extractEntries, openFolder, cancelTask } from '../../bridge';
  import { archiveStore } from '../../stores/archiveStore.svelte';
  import { taskStore } from '../../stores/taskStore.svelte';

  let disabled = $derived(taskStore.isVisible && taskStore.activeTask?.kind === 'extract');
  let lastArchivePath = '';
  let outputDir = $state('');

  // Settings-backed behavior.
  let defaultOutputDir = $state<string | null>(null);
  let strategy = $state<'prompt' | 'skip' | 'overwrite'>('prompt');
  let onComplete = $state<'nothing' | 'open_output'>('nothing');
  let showPrompt = $state(false);
  let pendingKind = $state<'all' | 'selected'>('all');

  $effect(() => {
    const current = archiveStore.archivePath;
    if (current && current !== lastArchivePath) {
      lastArchivePath = current;
      // Prefer the configured default output directory; fall back to the suggestion.
      outputDir = defaultOutputDir ?? archiveStore.suggestedOutputDir;
    }
  });
  let overwrite = $state(true);

  // Apply saved defaults
  Promise.all([
    settingsStore.get('overwrite_strategy'),
    settingsStore.get('default_output_dir'),
    settingsStore.get('default_password'),
    settingsStore.get('remember_password'),
    settingsStore.get('on_complete'),
  ]).then(([strat, outDir, pwd, remember, onComp]) => {
    strategy = strat ?? 'prompt';
    defaultOutputDir = outDir ?? null;
    // If the output dir hasn't been set yet (e.g. the archive was already open at
    // mount, before settings resolved), seed it from the configured default.
    if (outDir && !outputDir) outputDir = outDir;
    onComplete = onComp ?? 'nothing';
    if (remember && pwd) extractPassword = pwd;
    // Initialize the per-run overwrite checkbox from the strategy.
    overwrite = strategy !== 'skip';
  });
  let extractPassword = $state('');
  let result: string | null = $state(null);
  let error: string | null = $state(null);

  async function browseOutput() {
    const selected = await open({ directory: true, multiple: false });
    if (selected && typeof selected === 'string') {
      outputDir = selected;
    }
  }

  // Entry point for the action buttons. When the strategy is "prompt", ask the
  // user how to handle existing files before running.
  function startExtract(kind: 'all' | 'selected') {
    if (kind === 'selected' && archiveStore.selectedCount === 0) return;
    if (strategy === 'prompt') {
      pendingKind = kind;
      showPrompt = true;
      return;
    }
    runExtract(kind);
  }

  function confirmPrompt(choice: 'overwrite' | 'skip') {
    overwrite = choice === 'overwrite';
    showPrompt = false;
    runExtract(pendingKind);
  }

  function cancelPrompt() {
    showPrompt = false;
  }

  async function runExtract(kind: 'all' | 'selected') {
    if (!archiveStore.archivePath || !outputDir) return;
    error = null; result = null;
    const taskId = `extract-${Date.now()}`;
    taskStore.startTask(taskId, 'extract');
    try {
      const res = kind === 'all'
        ? await extractArchive(archiveStore.archivePath, outputDir, overwrite, extractPassword || undefined, taskId)
        : await extractEntries(archiveStore.archivePath, [...archiveStore.selectedPaths], outputDir, overwrite, extractPassword || undefined, taskId);
      taskStore.finishTask('finished');
      result = localeStore.t('extract.result', { files: res.files_extracted, bytes: res.bytes_extracted, skipped: res.files_skipped > 0 ? localeStore.t('extract.resultSkipped', { count: res.files_skipped }) : '' });
      if (onComplete === 'open_output') {
        await openFolder(outputDir);
      }
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
      <button class="btn-primary" onclick={() => startExtract('all')} disabled={disabled || !outputDir}>{localeStore.t('extract.extractAll')}</button>
      <button class="btn-secondary" onclick={() => startExtract('selected')} disabled={disabled || !outputDir || archiveStore.selectedCount === 0}>
        {localeStore.t('extract.extractSelected')} ({archiveStore.selectedCount})
      </button>
    </div>
  </div>

  {#if showPrompt}
    <div class="prompt-overlay" role="dialog" aria-modal="true">
      <div class="prompt-box">
        <p class="prompt-title">{localeStore.t('extract.promptTitle')}</p>
        <p class="prompt-text">{localeStore.t('extract.promptText', { dir: outputDir })}</p>
        <div class="prompt-actions">
          <button class="btn-tertiary" onclick={cancelPrompt}>{localeStore.t('extract.promptCancel')}</button>
          <button class="btn-secondary" onclick={() => confirmPrompt('skip')}>{localeStore.t('extract.promptSkip')}</button>
          <button class="btn-primary" onclick={() => confirmPrompt('overwrite')}>{localeStore.t('extract.promptOverwrite')}</button>
        </div>
      </div>
    </div>
  {/if}
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
  .btn-tertiary {
    padding: var(--space-2) var(--space-3);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    border-radius: var(--radius-sm);
  }
  .btn-tertiary:hover { color: var(--color-text); background: var(--color-surface-alt); }

  /* --- Prompt overlay --- */
  .prompt-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }
  .prompt-box {
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    padding: var(--space-4);
    max-width: 360px;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.3);
  }
  .prompt-title {
    font-size: var(--text-base);
    font-weight: 600;
    color: var(--color-text);
  }
  .prompt-text {
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
    word-break: break-all;
    white-space: pre-line;
  }
  .prompt-actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
  }
</style>
