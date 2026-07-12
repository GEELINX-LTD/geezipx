<script lang="ts">
  import { onMount } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { localeStore } from '../stores/localeStore.svelte';
  import { appStore } from '../stores/appStore.svelte';
  import { archiveManager } from '../stores/archiveStore.svelte';

  let dropActive = $state(false);

  const ARCHIVE_EXTS = new Set([
    'zip', '7z', 'rar', 'tar', 'gz', 'tgz', 'bz2', 'tbz', 'tbz2',
    'br', 'lz4', 'xz', 'txz', 'zst', 'tzst', 'cab', 'asar', 'deb',
    'iso', 'cpio', 'zpaq', 'lzh', 'lha', 'jar', 'war', 'apk', 'ipa',
    'xpi', 'zstd', 'lzma',
  ]);

  function isArchivePath(path: string): boolean {
    const lower = path.toLowerCase();
    if (
      lower.endsWith('.tar.gz') ||
      lower.endsWith('.tar.bz2') ||
      lower.endsWith('.tar.br') ||
      lower.endsWith('.tar.lz4') ||
      lower.endsWith('.tar.xz') ||
      lower.endsWith('.tar.zst')
    ) {
      return true;
    }
    const ext = lower.split('.').pop() || '';
    return ARCHIVE_EXTS.has(ext);
  }

  onMount(() => {
    const unlistenPromise = getCurrentWindow().onDragDropEvent((event) => {
      if (event.payload.type === 'enter') {
        dropActive = true;
      } else if (event.payload.type === 'over') {
        // keep overlay visible while hovering
      } else if (event.payload.type === 'leave') {
        dropActive = false;
      } else if (event.payload.type === 'drop') {
        dropActive = false;
        const paths = event.payload.paths;
        if (paths.length === 0) return;

        if (paths.length === 1 && isArchivePath(paths[0])) {
          archiveManager.openArchive(paths[0]).then(({ tabId, label }) => {
            appStore.addArchiveTab(tabId, label, paths[0]);
            appStore.switchTab(tabId);
          });
        } else {
          appStore.setPendingSourcePaths(paths);
          appStore.switchTab('compress');
        }
      }
    });

    return () => {
      unlistenPromise.then((fn) => fn());
    };
  });
</script>

{#if dropActive}
  <div
    class="overlay"
    role="dialog"
    aria-label={localeStore.t('drop.title')}
    tabindex="-1"
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
