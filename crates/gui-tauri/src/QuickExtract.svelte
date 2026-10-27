<script lang="ts">
  import { onMount } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { cancelTask, extractArchive, getShellAction, listen } from './bridge';
  import type { ShellActionPayload, TaskProgressPayload } from './bridge';
  import { archiveManager } from './stores/archiveStore.svelte';
  import { taskStore } from './stores/taskStore.svelte';
  import { settingsStore } from './stores/settingsStore.svelte';
  import { localeStore } from './stores/localeStore.svelte';
  import ArchivePasswordDialog from './components/ArchivePasswordDialog.svelte';

  const currentWindow = getCurrentWindow();
  const taskId = taskStore.startTask('extract');
  let tabId = $state<string | null>(null);
  let archivePath = $state('');
  let busy = $state(true);
  let extracting = $state(false);
  let cancellationPending = $state(false);
  let windowError = $state('');
  let disposed = false;
  let started = false;
  let cancellationRequested = false;
  let closing = false;

  let task = $derived(taskStore.getTask(taskId));
  let challenge = $derived(tabId ? archiveManager.tabs.get(tabId)?.passwordChallenge : null);
  let archiveName = $derived(archivePath.split(/[/\\]/).pop() || archivePath);

  function errorMessage(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
  }

  function releaseArchive(): void {
    if (!tabId) return;
    archiveManager.closeTab(tabId);
    tabId = null;
  }

  async function closeWhenIdle(): Promise<void> {
    if (busy || disposed || closing) return;
    closing = true;
    try {
      await currentWindow.close();
    } catch (error) {
      closing = false;
      windowError = errorMessage(error);
    }
  }

  async function handleCancel(): Promise<void> {
    if (!busy) {
      await closeWhenIdle();
      return;
    }
    if (cancellationPending) return;
    cancellationRequested = true;
    windowError = '';
    if (extracting) {
      cancellationPending = true;
      try {
        // The worker's terminal event/result remains authoritative. Even a
        // successful cancellation request does not mean the worker has stopped.
        await cancelTask(taskId);
      } catch (error) {
        cancellationRequested = false;
        cancellationPending = false;
        if (!disposed) windowError = errorMessage(error);
      }
      return;
    }

    // Closing the tab invalidates listing callbacks and resolves open waiters,
    // including a pending password challenge, without starting extraction.
    releaseArchive();
    taskStore.finishTask(taskId, 'cancelled');
    busy = false;
    await closeWhenIdle();
  }

  function handleCloseRequested(event: { preventDefault(): void }): void {
    if (!busy) return;
    event.preventDefault();
    void handleCancel();
  }

  async function runExtraction(payload: ShellActionPayload): Promise<void> {
    if (started || disposed || cancellationRequested) return;
    started = true;
    try {
      if (payload.action !== 'extract-here' || !payload.paths[0]) {
        throw new Error(localeStore.t('common.unavailable'));
      }
      archivePath = payload.paths[0];
      const openedTab = await archiveManager.openArchive(archivePath);
      if (disposed || cancellationRequested) {
        archiveManager.closeTab(openedTab.tabId);
        return;
      }
      tabId = openedTab.tabId;
      const opened = await archiveManager.waitForOpenResult(tabId, archivePath);
      if (disposed || cancellationRequested) return;
      const tab = archiveManager.tabs.get(tabId);
      if (!opened || !tab || tab.archivePath !== archivePath) {
        // ArchivePasswordDialog cancellation may have removed this tab.
        if (tab?.error) throw new Error(tab.error);
        if (!tab || (!tab.pendingArchivePath && !tab.archivePath)) {
          taskStore.finishTask(taskId, 'cancelled');
          return;
        }
        throw new Error(localeStore.t('common.unavailable'));
      }
      if (tab.error) throw new Error(tab.error);
      if (!tab.suggestedOutputDir) throw new Error(localeStore.t('common.unavailable'));

      extracting = true;
      await extractArchive(
        tab.archivePath,
        tab.suggestedOutputDir,
        true,
        tab.archivePassword || undefined,
        taskId,
      );
      taskStore.finishTask(taskId, 'finished');
    } catch (error) {
      taskStore.finishTask(taskId, 'failed', errorMessage(error));
    } finally {
      extracting = false;
      busy = false;
      cancellationPending = false;
      releaseArchive();
      if (disposed) {
        taskStore.dismissTask(taskId);
      } else {
        const status = taskStore.getTask(taskId)?.status;
        if (status === 'finished' || status === 'cancelled') await closeWhenIdle();
      }
    }
  }

  async function initialize(): Promise<void> {
    // Settings are read once in this webview, before consuming its own action.
    // A settings failure can use the existing locale/theme defaults.
    try {
      const results = await Promise.allSettled([
        settingsStore.get('theme'),
        settingsStore.get('locale'),
      ]);
      if (disposed || cancellationRequested) return;
      const [theme, locale] = results;
      if (theme.status === 'fulfilled') {
        if (theme.value === 'light' || theme.value === 'dark') {
          document.documentElement.setAttribute('data-theme', theme.value);
        } else {
          document.documentElement.removeAttribute('data-theme');
        }
      } else console.error('Quick extraction theme loading failed', theme.reason);
      if (locale.status === 'fulfilled') localeStore.switchLocale(locale.value);
      else console.error('Quick extraction locale loading failed', locale.reason);
    } catch (error) {
      console.error('Quick extraction settings loading failed', error);
    }
    if (disposed || cancellationRequested) return;
    const payload = await getShellAction();
    if (disposed || cancellationRequested) return;
    if (!payload) throw new Error(localeStore.t('common.unavailable'));
    await runExtraction(payload);
  }

  onMount(() => {
    const unlisteners: (() => void)[] = [];
    const keepUnlistener = (unlisten: () => void) => {
      if (disposed) unlisten();
      else unlisteners.push(unlisten);
    };
    // Both listeners must be attached before any archive work is invoked.
    void Promise.all([
      listen<TaskProgressPayload>('task:progress', (event) => {
        if (!disposed && event.payload.task_id === taskId) taskStore.updateTask(event.payload);
      }).then(keepUnlistener),
      currentWindow.onCloseRequested(handleCloseRequested).then(keepUnlistener),
    ]).then(() => initialize()).catch((error) => {
      if (disposed || cancellationRequested) return;
      taskStore.finishTask(taskId, 'failed', errorMessage(error));
      busy = false;
      releaseArchive();
    });

    return () => {
      disposed = true;
      cancellationRequested = true;
      unlisteners.forEach((unlisten) => unlisten());
      releaseArchive();
      if (extracting) {
        void cancelTask(taskId).catch((error) => console.error('Quick extraction cancellation failed', error));
      } else {
        taskStore.finishTask(taskId, 'cancelled');
        taskStore.dismissTask(taskId);
      }
    };
  });
</script>

<main class="quick-extract" aria-labelledby="quick-extract-title">
  <h1 id="quick-extract-title">{localeStore.t('tasks.extract')}</h1>
  {#if archiveName}<p class="archive-name">{archiveName}</p>{/if}
  <p class="status" role="status">
    {localeStore.t(`tasks.status.${task?.status ?? 'pending'}`)}
  </p>
  {#if busy && !extracting}
    <p class="detail">{localeStore.t('extract.loading')}</p>
  {/if}
  <progress
    max="100"
    value={task?.percent == null ? undefined : Math.min(100, Math.max(0, task.percent))}
    aria-label={localeStore.t('tasks.extract')}
  ></progress>
  {#if task?.percent != null}<p class="detail">{Math.round(task.percent)}%</p>{/if}
  {#if task?.currentEntry}<p class="detail">{task.currentEntry}</p>{/if}
  {#if task?.status === 'failed'}
    <p class="error" role="alert">
      {localeStore.t('common.errorDetail', { message: task.message || localeStore.t('common.unavailable') })}
    </p>
  {/if}
  {#if windowError}
    <p class="error" role="alert">{localeStore.t('common.errorDetail', { message: windowError })}</p>
  {/if}
  <div class="actions">
    <button onclick={handleCancel} disabled={busy && cancellationPending}>
      {localeStore.t(busy ? 'common.cancel' : 'common.close')}
    </button>
  </div>
</main>

{#if tabId && challenge}
  <ArchivePasswordDialog {tabId} />
{/if}

<style>
  .quick-extract {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    width: 100%;
    min-width: 0;
    height: 100%;
    min-height: 0;
    padding: var(--space-5);
    overflow-y: auto;
    overflow-wrap: anywhere;
    background: var(--color-bg);
    color: var(--color-text);
  }

  h1 {
    font-size: var(--text-lg);
    font-weight: 600;
  }

  p { margin: 0; }
  .archive-name, .status { font-size: var(--text-base); }
  .detail { font-size: var(--text-sm); color: var(--color-text-secondary); }
  progress { flex-shrink: 0; width: 100%; height: var(--space-2); accent-color: var(--color-progress-fill); }

  .error {
    padding: var(--space-3);
    border-radius: var(--radius-sm);
    font-size: var(--text-sm);
    background: var(--color-error-bg);
    color: var(--color-error);
    white-space: pre-wrap;
  }

  .actions { display: flex; justify-content: flex-end; margin-top: auto; }
  button {
    padding: var(--space-2) var(--space-4);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
  }
  button:hover:not(:disabled) { background: var(--color-surface-alt); }
</style>
