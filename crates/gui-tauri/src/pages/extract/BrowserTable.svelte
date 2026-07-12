<script lang="ts">
  import { localeStore } from '../../stores/localeStore.svelte';
  import { archiveStore, type DirChild } from '../../stores/archiveStore.svelte';
  import { prepareDragEntries, cleanupDragTempDir } from '../../bridge';
  import { startDrag } from '@crabnebula/tauri-plugin-drag';
  import { VList } from 'virtua/svelte';

  let dragTracking = $state(false);
  let dragStartX = $state(0);
  let dragStartY = $state(0);

  function formatBytes(bytes: number): string {
    if (bytes === 0) return '—';
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  }

  function formatType(child: { name: string; isDir: boolean }): string {
    if (child.isDir) return localeStore.t('browser.folder');
    const ext = child.name.split('.').pop()?.toLowerCase() || '';
    return ext ? ext.toUpperCase() : localeStore.t('browser.file');
  }

  function formatDate(ts: number | null): string {
    if (!ts) return '—';
    const d = new Date(ts * 1000);
    return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' });
  }

  function handleRowClick(path: string, isDir: boolean) {
    if (isDir) {
      archiveStore.navigateTo(path);
    } else {
      archiveStore.toggleSelection(path);
    }
  }

  function handleRowDblClick(path: string, isDir: boolean) {
    if (isDir) {
      archiveStore.navigateTo(path);
    } else {
      archiveStore.showPreview(path);
    }
  }

  function onDragHandleDown(e: MouseEvent, entryPath: string) {
    e.stopPropagation();
    e.preventDefault();
    if (!archiveStore.selectedPaths.has(entryPath)) {
      archiveStore.clearSelection();
      archiveStore.toggleSelection(entryPath);
    }
    dragTracking = true;
    dragStartX = e.clientX;
    dragStartY = e.clientY;
  }

  function onWindowMouseMove(e: MouseEvent) {
    if (!dragTracking) return;
    const dx = Math.abs(e.clientX - dragStartX);
    const dy = Math.abs(e.clientY - dragStartY);
    if (dx < 5 && dy < 5) return;
    dragTracking = false;
    initiateDrag();
  }

  function onWindowMouseUp() {
    dragTracking = false;
  }

  async function initiateDrag() {
    if (!archiveStore.archivePath) return;
    const paths = [...archiveStore.selectedPaths];
    if (paths.length === 0) return;
    try {
      const tempDir = await prepareDragEntries(archiveStore.archivePath, paths, archiveStore.archivePassword || undefined);
      const filePaths = paths.map(p => `${tempDir}/${p}`);
      await startDrag({ item: filePaths, icon: '' });
      cleanupDragTempDir(tempDir).catch(() => {});
    } catch { /* silent */ }
  }

  function childPath(c: { name: string; isDir: boolean; entry: { path: string } }): string {
    return c.isDir ? archiveStore.currentDir + c.name : c.entry.path;
  }

  let allSelected = $derived(
    archiveStore.currentChildren.length > 0 &&
    archiveStore.currentChildren.every(c => archiveStore.selectedPaths.has(childPath(c)))
  );

  function toggleSelectAll() {
    if (allSelected) {
      archiveStore.clearSelection();
    } else {
      archiveStore.selectAll();
    }
  }
</script>

{#if archiveStore.hasArchive}
  <div class="browser-table-wrapper">
    <!-- Sticky header (outside VList, so it doesn't scroll with rows) -->
    <div class="browser-header" role="rowgroup">
      <div class="header-row">
        <div class="col-check">
          <input type="checkbox" checked={allSelected} onchange={toggleSelectAll} />
        </div>
        <div class="col-drag"></div>
        <div class="col-name">{localeStore.t('browser.table.name')}</div>
        <div class="col-size">{localeStore.t('browser.table.size')}</div>
        <div class="col-type">{localeStore.t('browser.table.type')}</div>
        <div class="col-date">{localeStore.t('browser.table.modified')}</div>
      </div>
    </div>

    <!-- Loading skeleton -->
    {#if archiveStore.isLoading}
      <div class="loading-skeleton" aria-busy="true">
        {#each Array(10) as _, i (i)}
          <div class="skeleton-row">
            <div class="sk-col sk-check"></div>
            <div class="sk-col sk-drag"></div>
            <div class="sk-col sk-name"></div>
            <div class="sk-col sk-size"></div>
            <div class="sk-col sk-type"></div>
            <div class="sk-col sk-date"></div>
          </div>
        {/each}
      </div>
    {:else}
      <!-- Virtual scroll container -->
      <div class="vlist-container">
        <VList
          data={archiveStore.currentChildren}
          getKey={(item: DirChild) => childPath(item)}
        >
          {#snippet children(item: DirChild, index: number)}
            <div
              class="browser-row"
              class:selected={archiveStore.selectedPaths.has(childPath(item))}
              role="row"
            >
              <div class="col-check" role="cell" tabindex="-1" onclick={(e: MouseEvent) => e.stopPropagation()} onkeydown={(e: KeyboardEvent) => e.stopPropagation()}>
                <input
                  type="checkbox"
                  checked={archiveStore.selectedPaths.has(childPath(item))}
                  onchange={() => archiveStore.toggleSelection(childPath(item))}
                />
              </div>
              <div class="col-drag" role="cell" tabindex="-1" onmousedown={(e: MouseEvent) => onDragHandleDown(e, childPath(item))}>
                <span class="drag-handle" title={localeStore.t('browser.table.dragHandle')} aria-label={localeStore.t('browser.table.dragHandle')}>⋮⋮</span>
              </div>
              <div class="col-name">
                <button class="row-name-btn" onclick={() => handleRowClick(childPath(item), item.isDir)} ondblclick={() => handleRowDblClick(childPath(item), item.isDir)}>
                  <span class="row-icon">{item.isDir ? '📁' : '📄'}</span>
                  <span class="row-name-text truncate">{item.name}</span>
                </button>
              </div>
              <div class="col-size text-mono">
                {item.isDir ? '—' : formatBytes(item.entry.size)}
              </div>
              <div class="col-type">{formatType(item)}</div>
              <div class="col-date">{formatDate(item.entry.modified)}</div>
            </div>
          {/snippet}
        </VList>
      </div>

      {#if archiveStore.currentChildren.length === 0}
        <div class="empty-dir">{localeStore.t('browser.emptyDir')}</div>
      {/if}
    {/if}
  </div>
  <div class="browser-info">
    {localeStore.t('browser.info', { current: archiveStore.currentChildren.length, total: archiveStore.entryCount })}
  </div>
{/if}
<svelte:window onmousemove={onWindowMouseMove} onmouseup={onWindowMouseUp} />

<style>
  .browser-table-wrapper {
    flex: 1;
    display: flex;
    flex-direction: column;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    overflow: hidden;
    min-height: 0;
  }

  /* --- Header (sticky) --- */
  .browser-header {
    flex-shrink: 0;
    background: var(--color-surface);
    border-bottom: 1px solid var(--color-border);
  }
  .header-row {
    display: grid;
    grid-template-columns: 32px 24px 1fr 90px 80px 120px;
    align-items: center;
    padding: var(--space-2) var(--space-3);
    font-size: var(--text-xs);
    font-weight: 600;
    color: var(--color-text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  /* --- VList container --- */
  .vlist-container {
    flex: 1;
    min-height: 0;
  }

  /* --- Row --- */
  .browser-row {
    display: grid;
    grid-template-columns: 32px 24px 1fr 90px 80px 120px;
    align-items: center;
    padding: var(--space-1) var(--space-3);
    border-bottom: 1px solid var(--color-border-light);
    font-size: var(--text-sm);
  }
  .browser-row:hover { background: var(--color-surface); }
  .browser-row.selected { background: var(--color-accent-light); }

  /* --- Columns --- */
  .col-check { display: flex; justify-content: center; }
  .col-drag { display: flex; justify-content: center; cursor: grab; }
  .col-drag:active { cursor: grabbing; }
  .drag-handle { color: var(--color-text-muted); font-size: 14px; user-select: none; letter-spacing: -2px; }
  .browser-row:hover .drag-handle { color: var(--color-text-secondary); }
  .col-size { text-align: right; white-space: nowrap; font-size: var(--text-xs); color: var(--color-text-secondary); }
  .col-type { font-size: var(--text-xs); color: var(--color-text-secondary); white-space: nowrap; }
  .col-date { font-size: var(--text-xs); color: var(--color-text-muted); white-space: nowrap; }

  .row-name-btn {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    padding: 2px 0;
    cursor: pointer;
    background: none;
    border: none;
    color: inherit;
    font: inherit;
    text-align: left;
  }
  .row-icon { font-size: 14px; flex-shrink: 0; }
  .row-name-text { min-width: 0; }

  input[type="checkbox"] { cursor: pointer; }

  /* --- Empty state --- */
  .empty-dir {
    padding: var(--space-6);
    text-align: center;
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }
  .browser-info {
    padding: var(--space-1) var(--space-2);
    font-size: var(--text-xs);
    color: var(--color-text-muted);
  }

  /* --- Loading skeleton --- */
  .loading-skeleton {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 0;
  }
  .skeleton-row {
    display: grid;
    grid-template-columns: 32px 24px 1fr 90px 80px 120px;
    align-items: center;
    height: 32px;
    padding: var(--space-1) var(--space-3);
    border-bottom: 1px solid var(--color-border-light);
    animation: shimmer 1.5s ease-in-out infinite;
  }
  .sk-col {
    height: 12px;
    background: var(--color-border);
    border-radius: var(--radius-sm);
  }
  .sk-check { width: 12px; justify-self: center; }
  .sk-drag { width: 12px; justify-self: center; }
  .sk-name { width: 70%; }
  .sk-size { width: 50px; justify-self: end; }
  .sk-type { width: 40px; }
  .sk-date { width: 70px; }

  @keyframes shimmer {
    0% { opacity: 0.4; }
    50% { opacity: 0.8; }
    100% { opacity: 0.4; }
  }
</style>
