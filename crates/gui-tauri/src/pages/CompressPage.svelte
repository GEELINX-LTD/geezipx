<script lang="ts">
  import { onMount } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import { save } from '@tauri-apps/plugin-dialog';
  import { compressArchive, getFormats, type FormatInfo } from '../bridge';
  import { localeStore } from '../stores/localeStore.svelte';
  import type { CompressArchiveResult } from '../bridge';
  import { taskStore } from '../stores/taskStore.svelte';
  import { appStore } from '../stores/appStore.svelte';
  import ProgressDialog from '../components/ProgressDialog.svelte';

  let sourcePaths = $state<string[]>([]);
  let outputPath = $state('');
  let format = $state('zip');
  let level: number | undefined = $state(undefined);
  let password = $state('');
  let formats = $state<FormatInfo[]>([]);
  let result: CompressArchiveResult | null = $state(null);
  let error = $state<string | null>(null);

  let isRunning = $derived(taskStore.isVisible && taskStore.activeTask?.kind === 'compress');

  let levelPlaceholder = $derived(
    formats.find(f => f.name === format)?.level_hint || localeStore.t('compress.levelPlaceholder')
  );

  let showPassword = $derived(
    formats.find(f => f.name === format)?.supports_encryption ?? false
  );

  getFormats().then((f) => (formats = f));

  onMount(() => {
    const pending = appStore.consumePendingSourcePaths();
    if (pending.length > 0) {
      sourcePaths = pending;
      inferOutput();
    }
  });

  function inferOutput() {
    if (sourcePaths.length === 0) return;
    const first = sourcePaths[0].replace(/\/$/, '');
    const base = first.split('/').pop() || 'archive';
    const ext = format === 'tar.gz' ? '.tar.gz' : `.${format}`;
    // Use the source file's parent directory so the output is created next to the source
    const parent = first.substring(0, first.lastIndexOf('/') + 1);
    outputPath = parent + base + ext;
  }

  async function browseFiles() {
    const selected = await open({ multiple: true, directory: true });
    if (selected) {
      const paths = Array.isArray(selected) ? selected : [selected];
      for (const p of paths) {
        if (!sourcePaths.includes(p)) sourcePaths = [...sourcePaths, p];
      }
      inferOutput();
    }
  }

  function handleSourceKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      browseFiles();
    }
  }

  function removeFile(idx: number) {
    sourcePaths = sourcePaths.filter((_, i) => i !== idx);
    inferOutput();
  }

  async function browseOutput() {
    const ext = format === 'tar.gz' ? 'tar.gz' : format;
    const selected = await save({ filters: [{ name: 'Archive', extensions: [ext] }] });
    if (selected) outputPath = selected;
  }

  async function runCompress() {
    if (sourcePaths.length === 0 || !outputPath) return;
    error = null;
    result = null;

    const taskId = `compress-${Date.now()}`;
    taskStore.startTask(taskId, 'compress');

    try {
      const res = await compressArchive(
        sourcePaths,
        outputPath,
        format,
        level,
        undefined,
        password || undefined,
        taskId,
      );
      taskStore.finishTask('finished');
      result = res;
      appStore.addRecent(outputPath);
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      taskStore.finishTask('failed', msg);
      error = msg;
    }
  }


</script>

<div class="compress-page">
  <!-- Source Files -->
  <section class="section">
    <span class="section-label">{localeStore.t('compress.sourceFiles')}</span>
    <div class="source-area">
      {#if sourcePaths.length === 0}
        <div class="source-empty" role="button" tabindex="0" onclick={browseFiles} onkeydown={handleSourceKeydown}>
          <p>{localeStore.t('compress.sourcePlaceholder')}</p>
        </div>
      {:else}
        <div class="source-chips">
          {#each sourcePaths as path, i (path)}
            <div class="source-chip">
              <span class="chip-icon">{path.endsWith('/') ? '📁' : '📄'}</span>
              <span class="chip-path" title={path}>{path.split('/').pop()}</span>
              <button class="chip-remove" onclick={() => removeFile(i)} aria-label="Remove">
                <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
                </svg>
              </button>
            </div>
          {/each}
        </div>
        <button class="add-btn" onclick={browseFiles}>{localeStore.t('common.addFiles')}</button>
      {/if}
    </div>
  </section>

  <!-- Format + Level -->
  <section class="section">
    <div class="row">
      <div class="field">
        <label class="section-label" for="compress-format">{localeStore.t('compress.format')}</label>
        <select id="compress-format" bind:value={format} onchange={inferOutput}>
          {#each formats.filter(f => f.can_compress) as fmt (fmt.name)}
            <option value={fmt.name}>{fmt.name.toUpperCase()}</option>
          {/each}
        </select>
      </div>
      <div class="field">
        <label class="section-label" for="compress-level">{localeStore.t('compress.level')}</label>
        <input type="number" id="compress-level" bind:value={level} placeholder={levelPlaceholder} />
      </div>
    </div>
  </section>

  <!-- Password -->
  {#if showPassword}
    <section class="section">
      <div class="field">
        <label class="section-label" for="compress-password">{localeStore.t('compress.password')}</label>
        <input type="password" id="compress-password" bind:value={password} placeholder={localeStore.t('compress.passwordPlaceholder')} />
      </div>
    </section>
  {/if}

  <!-- Output -->
  <section class="section">
    <div class="field">
      <label class="section-label" for="compress-output">{localeStore.t('compress.output')}</label>
      <div class="input-row">
        <input type="text" id="compress-output" value={outputPath} readonly placeholder={localeStore.t('compress.outputPlaceholder')} />
        <button class="btn-secondary" onclick={browseOutput}>{localeStore.t('common.saveAs')}</button>
      </div>
    </div>
  </section>

  <!-- Result / Error -->
  {#if result}
    <div class="result success">{localeStore.t('compress.result', { files: result.files_added, bytes: result.bytes_written })}</div>
  {/if}
  {#if error}
    <div class="result error">{localeStore.t('compress.errorPrefix', { message: error })}</div>
  {/if}

  <!-- Action Bar -->
  <div class="action-bar">
    <button class="btn-primary" onclick={runCompress} disabled={isRunning || sourcePaths.length === 0 || !outputPath}>
      {localeStore.t('compress.run')}
    </button>
  </div>
  <ProgressDialog kind="compress" />
</div>

<style>
  .compress-page {
    flex: 1;
    overflow-y: auto;
    padding: var(--space-5);
    display: flex;
    flex-direction: column;
    gap: var(--space-5);
  }
  .section { display: flex; flex-direction: column; gap: var(--space-2); }
  .section-label {
    font-size: var(--text-xs);
    font-weight: 600;
    color: var(--color-text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .row { display: flex; gap: var(--space-4); }
  .row .field { flex: 1; display: flex; flex-direction: column; gap: var(--space-2); }
  .field { display: flex; flex-direction: column; gap: var(--space-2); }
  .input-row { display: flex; gap: var(--space-2); }
  .input-row input { flex: 1; }

  .source-area {
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    padding: var(--space-3);
    min-height: 80px;
  }
  .source-empty {
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 60px;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    cursor: pointer;
    border: 1px dashed var(--color-border);
    border-radius: var(--radius-sm);
  }
  .source-empty:hover { border-color: var(--color-accent); color: var(--color-accent); }
  .source-chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin-bottom: var(--space-2);
  }
  .source-chip {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    padding: 2px var(--space-2);
    background: var(--color-surface);
    border: 1px solid var(--color-border-light);
    border-radius: var(--radius-sm);
    font-size: var(--text-sm);
  }
  .chip-icon { font-size: 14px; }
  .chip-path { max-width: 180px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .chip-remove { display: flex; padding: 2px; border-radius: 50%; color: var(--color-text-muted); }
  .chip-remove:hover { color: var(--color-error); background: var(--color-error-bg); }

  select, input { padding: var(--space-2) var(--space-3); }

  .add-btn {
    font-size: var(--text-xs);
    color: var(--color-accent);
    padding: var(--space-1) var(--space-2);
  }
  .add-btn:hover { text-decoration: underline; }

  .result { padding: var(--space-3); border-radius: var(--radius-md); font-size: var(--text-sm); }
  .result.success { background: var(--color-success-bg); color: var(--color-success); }
  .result.error { background: var(--color-error-bg); color: var(--color-error); }

  .action-bar {
    display: flex;
    gap: var(--space-3);
    align-items: center;
    padding-top: var(--space-4);
    border-top: 1px solid var(--color-border-light);
  }
  .btn-primary {
    padding: var(--space-2) var(--space-5);
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
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    font-size: var(--text-sm);
  }
  .btn-secondary:hover { background: var(--color-surface-alt); }

</style>
