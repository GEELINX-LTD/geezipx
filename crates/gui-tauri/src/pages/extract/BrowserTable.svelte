<script lang="ts">
  import { localeStore } from '../../stores/localeStore.svelte';
  import { archiveStore } from '../../stores/archiveStore.svelte';
  import { prepareDragEntries, cleanupDragTempDir } from '../../bridge';
  import { startDrag } from '@crabnebula/tauri-plugin-drag';

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
      const tempId = await prepareDragEntries(archiveStore.archivePath, paths, archiveStore.archivePassword || undefined);
      await startDrag({ item: [tempId], icon: '' });
      cleanupDragTempDir(tempId).catch(() => {});
    } catch { /* silent */ }
  }

  let allSelected = $derived(
    archiveStore.currentChildren.length > 0 &&
    archiveStore.currentChildren.every(c => archiveStore.selectedPaths.has(c.entry.path))
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
    <table class="browser-table">
      <thead>
        <tr>
          <th class="col-check">
            <input type="checkbox" checked={allSelected} onchange={toggleSelectAll} />
          </th>
          <th class="col-drag"></th>
          <th class="col-name">{localeStore.t('browser.table.name')}</th>
          <th class="col-size">{localeStore.t('browser.table.size')}</th>
          <th class="col-type">{localeStore.t('browser.table.type')}</th>
          <th class="col-date">{localeStore.t('browser.table.modified')}</th>
        </tr>
      </thead>
      <tbody>
        {#each archiveStore.currentChildren as child (child.entry.path)}
          <tr
            class="browser-row"
            class:selected={archiveStore.selectedPaths.has(child.entry.path)}
          >
            <td class="col-check" onclick={(e) => e.stopPropagation()}>
              <input
                type="checkbox"
                checked={archiveStore.selectedPaths.has(child.entry.path)}
                onchange={() => archiveStore.toggleSelection(child.entry.path)}
              />
            </td>
            <td class="col-drag" onmousedown={(e) => onDragHandleDown(e, child.entry.path)}>
              <span class="drag-handle" title={localeStore.t('browser.table.dragHandle')} aria-label={localeStore.t('browser.table.dragHandle')}>⋮⋮</span>
            </td>
            <td class="col-name">
              <button class="row-name-btn" onclick={() => handleRowClick(child.entry.path, child.isDir)} ondblclick={() => handleRowDblClick(child.entry.path, child.isDir)}>
                <span class="row-icon">{child.isDir ? '📁' : '📄'}</span>
                <span class="row-name-text truncate">{child.name}</span>
              </button>
            </td>
            <td class="col-size text-mono">
              {child.isDir ? '—' : formatBytes(child.entry.size)}
            </td>
            <td class="col-type">{formatType(child)}</td>
            <td class="col-date">{formatDate(child.entry.modified)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
    {#if archiveStore.currentChildren.length === 0 && !archiveStore.isLoading}
      <div class="empty-dir">{localeStore.t('browser.emptyDir')}</div>
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
    overflow: auto;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
  }
  .browser-table {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--text-sm);
  }
  thead {
    position: sticky;
    top: 0;
    z-index: 1;
    background: var(--color-surface);
    border-bottom: 1px solid var(--color-border);
  }
  th {
    text-align: left;
    padding: var(--space-2) var(--space-3);
    font-weight: 600;
    color: var(--color-text-secondary);
    font-size: var(--text-xs);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    white-space: nowrap;
  }
  td { padding: var(--space-1) var(--space-3); }
  .col-check { width: 32px; text-align: center; }
  .col-drag { width: 24px; text-align: center; cursor: grab; }
  .col-drag:active { cursor: grabbing; }
  .drag-handle { color: var(--color-text-muted); font-size: 14px; user-select: none; letter-spacing: -2px; }
  .browser-row:hover .drag-handle { color: var(--color-text-secondary); }
  .col-size { width: 90px; text-align: right; white-space: nowrap; font-size: var(--text-xs); color: var(--color-text-secondary); }
  .col-type { width: 80px; font-size: var(--text-xs); color: var(--color-text-secondary); white-space: nowrap; }
  .col-date { width: 120px; font-size: var(--text-xs); color: var(--color-text-muted); white-space: nowrap; }

  .browser-row { border-bottom: 1px solid var(--color-border-light); }
  .browser-row:hover { background: var(--color-surface); }
  .browser-row.selected { background: var(--color-accent-light); }

  .row-name-btn {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    padding: 2px 0;
    cursor: pointer;
  }
  .row-icon { font-size: 14px; flex-shrink: 0; }
  .row-name-text { min-width: 0; }

  input[type="checkbox"] { cursor: pointer; }

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
</style>
