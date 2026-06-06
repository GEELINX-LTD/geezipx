/// GeeZipX GUI — Main UI logic.
///
/// Wire up the tabs, file dialogs, run/cancel buttons, and result display
/// for the four modes: Compress, Extract, List, Test.

import {
  prepareDragEntries,
  cleanupDragTempDir,
  getFormats,
  listArchive,
  testArchive,
  compressArchive,
  extractArchive,
  cancelTask,
  revealFile,
  openFolder,
  previewEntry,
  getOpenedArchives,
  extractEntries,
  listen,
  type FormatInfo,
  type EntryInfo,
  type TestArchiveResult,
  type CompressArchiveResult,
  type ExtractArchiveResult,
} from "./bridge";
import { open, save } from "@tauri-apps/plugin-dialog";

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

let currentTaskId: string | null = null;
let compressFormats: FormatInfo[] = [];
let unlistenOpenedArchives: (() => void) | null = null;

// Archive browser state
let browserEntries: EntryInfo[] = [];
let browserArchivePath = "";
let browserPassword = "";
let browserCurrentDir = "";
let browserSelected = new Set<string>();
let browserExtractRunning = false;
let browserExtractToken = 0;
let currentDragTempId = "";
let currentDragTimeout: ReturnType<typeof setTimeout> | null = null;
const DRAG_CLEANUP_TIMEOUT_MS = 60_000;

const ARCHIVE_EXTS = /\.(zip|tar|tar\.gz|tar\.zst|tar\.xz|tgz|tzst|txz|7z|rar)$/i;

// Recent files storage
const RECENT_KEY = "geezipx_recent_paths";
const MAX_RECENT = 10;

interface RecentEntry {
  path: string;
  label: string;
  isArchive: boolean;
}

// ---------------------------------------------------------------------------
// DOM helpers
// ---------------------------------------------------------------------------

function el<T extends HTMLElement>(id: string): T {
  return document.getElementById(id) as T;
}

function setRunning(mode: string, running: boolean) {
  const panel = el(`panel-${mode}`);
  if (running) {
    panel.classList.add("running");
  } else {
    panel.classList.remove("running");
  }
  // Use visibility to avoid layout shift; Cancel hidden when not running
  const runBtn = el(`${mode}-run`);
  const cancelBtn = el(`${mode}-cancel`);
  if (running) {
    runBtn.classList.add("run-disabled");
    cancelBtn.style.visibility = "visible";
    cancelBtn.removeAttribute("aria-hidden");
  } else {
    runBtn.classList.remove("run-disabled");
    cancelBtn.style.visibility = "hidden";
    cancelBtn.setAttribute("aria-hidden", "true");
  }
}

function btnDisabled(id: string, disabled: boolean) {
  const btn = el<HTMLButtonElement>(id);
  btn.disabled = disabled;
}

function showError(panelId: string, msg: string) {
  el(panelId).innerHTML = `<div class="error-message">${escapeHtml(msg)}</div>`;
}

function escapeHtml(s: string): string {
  const map: Record<string, string> = {
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  };
  return s.replace(/[&<>"']/g, (ch) => map[ch] ?? ch);
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  const val = bytes / Math.pow(1024, i);
  return `${val.toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
}

function getBasename(p: string): string {
  // Normalise backslashes for cross-platform
  const normalised = p.replace(/\\/g, "/");
  return normalised.split("/").pop() ?? p;
}

function getParentDir(p: string): string {
  const normalised = p.replace(/\\/g, "/");
  const idx = normalised.lastIndexOf("/");
  return idx >= 0 ? normalised.substring(0, idx) : ".";
}

/** Return the stem of a filename without archive extension.
 *  Handles double extensions like .tar.gz, .tar.zst, .tar.xz */
function stripArchiveExt(name: string): string {
  const double = /\.(tar\.gz|tar\.zst|tar\.xz|tgz|tzst|txz)$/i;
  const single = /\.(zip|tar|7z|rar)$/i;
  if (double.test(name)) {
    return name.replace(double, "");
  }
  if (single.test(name)) {
    return name.replace(single, "");
  }
  return name;
}

function isArchiveExt(p: string): boolean {
  return ARCHIVE_EXTS.test(p);
}

// ---------------------------------------------------------------------------
// Smart path inference
// ---------------------------------------------------------------------------

function inferOutputPath(sources: string[], format: string): string {
  if (sources.length === 0) return "";
  // Derive extension from format name (e.g. "tar.gz" → ".tar.gz", "zip" → ".zip")
  const ext = `.${format.toLowerCase()}`;

  if (sources.length === 1) {
    const src = sources[0];
    const parent = getParentDir(src);
    const base = getBasename(src);
    const stem = stripArchiveExt(base);
    return `${parent}/${stem}${ext}`;
  }
  // Multiple sources — parent of the first source, use "archive"
  const parent = getParentDir(sources[0]);
  return `${parent}/archive${ext}`;
}

function inferOutputDir(archivePath: string): string {
  const parent = getParentDir(archivePath);
  const base = getBasename(archivePath);
  const stem = stripArchiveExt(base);
  return `${parent}/${stem}_extracted`;
}

function syncExtractFormFromArchive(archivePath: string, password = "") {
  const archiveInput = el<HTMLInputElement>("extract-archive");
  const outputInput = el<HTMLInputElement>("extract-output");
  const passwordInput = el<HTMLInputElement>("extract-password");
  const previousArchive = archiveInput.value.trim();

  archiveInput.value = archivePath;
  if (!outputInput.value.trim() || previousArchive !== archivePath) {
    outputInput.value = inferOutputDir(archivePath);
  }
  passwordInput.value = password;
}

function syncBrowserExtractOutputFromArchive(archivePath: string) {
  el<HTMLInputElement>("browser-extract-output").value = inferOutputDir(archivePath);
}

// ---------------------------------------------------------------------------
// File chips display
// ---------------------------------------------------------------------------

function updateCompressSourceChips(paths: string[]) {
  const container = el("compress-source-chips");
  if (paths.length === 0) {
    container.innerHTML = "";
    return;
  }
  const maxShow = 3;
  const parts: string[] = [];
  const shown = paths.slice(0, maxShow);
  for (const p of shown) {
    const name = getBasename(p);
    parts.push(
      `<span class="file-chip"><span class="chip-icon">\u{1F4C4}</span>${escapeHtml(name)}</span>`
    );
  }
  const remaining = paths.length - maxShow;
  if (remaining > 0) {
    parts.push(
      `<span class="file-chip more-chip">+${remaining} more</span>`
    );
  }
  container.innerHTML = parts.join("");
}

// ---------------------------------------------------------------------------
// Recent files
// ---------------------------------------------------------------------------

function loadRecent(): RecentEntry[] {
  try {
    const raw = localStorage.getItem(RECENT_KEY);
    return raw ? JSON.parse(raw) : [];
  } catch {
    return [];
  }
}

function saveRecent(entries: RecentEntry[]) {
  try {
    localStorage.setItem(RECENT_KEY, JSON.stringify(entries.slice(0, MAX_RECENT)));
  } catch { /* quota exceeded — ignore */ }
}

function addRecent(path: string) {
  if (!path) return;
  const entries = loadRecent();
  const isArchive = isArchiveExt(path);
  const label = getBasename(path);
  // Remove duplicate
  const filtered = entries.filter((e) => e.path !== path);
  filtered.unshift({ path, label, isArchive });
  saveRecent(filtered);
  renderRecentChips();
}

function removeRecent(path: string) {
  const entries = loadRecent().filter((e) => e.path !== path);
  saveRecent(entries);
  renderRecentChips();
}

function renderRecentChips() {
  const entries = loadRecent();
  const bar = el("recent-bar");
  const container = el("recent-chips");
  if (entries.length === 0) {
    bar.style.display = "none";
    return;
  }
  bar.style.display = "flex";
  container.innerHTML = entries
    .map(
      (e) =>
        `<span class="recent-chip" data-path="${escapeHtml(e.path)}" data-archive="${e.isArchive}">
          ${escapeHtml(e.label)}
          <span class="chip-close" data-path="${escapeHtml(e.path)}">&times;</span>
        </span>`
    )
    .join("");

  // Click on chip: fill path and switch mode
  container.querySelectorAll(".recent-chip").forEach((chip) => {
    chip.addEventListener("click", (ev) => {
      const target = ev.target as HTMLElement;
      if (target.classList.contains("chip-close")) {
        ev.stopPropagation();
        removeRecent(target.dataset.path ?? "");
        return;
      }
      const path = (chip as HTMLElement).dataset.path ?? "";
      const isArchive = (chip as HTMLElement).dataset.archive === "true";
      if (isArchive) {
        // Switch to List, fill archive path, auto-run list
        switchMode("list");
        el<HTMLInputElement>("list-archive").value = path;
        el<HTMLInputElement>("list-password").value = "";
        el("list-result").innerHTML =
          `<div class="result-empty">Opening archive...</div>`;
        // Auto-run list
        setTimeout(() => runListWithPath(path), 50);
      } else {
        // Switch to Compress, fill source
        switchMode("compress");
        const input = el<HTMLInputElement>("compress-sources");
        input.value = path;
        input.dataset.paths = path;
        updateCompressSourceChips([path]);
        // Auto-infer output path
        const format = el<HTMLSelectElement>("compress-format").value;
        el<HTMLInputElement>("compress-output").value = inferOutputPath([path], format);
      }
    });
  });
}

// ---------------------------------------------------------------------------
// Archive Browser — helper functions
// ---------------------------------------------------------------------------

/** Entry icon */
function entryIcon(isDir: boolean): string {
  return isDir ? "\u{1F4C1}" : "\u{1F4C4}";
}

/** Format a unix timestamp to a human-readable date. */
function formatTimestamp(ts: number | null): string {
  if (!ts || ts === 0) return "\u2014";
  const d = new Date(ts * 1000);
  try {
    return d.toLocaleDateString(undefined, {
      month: "short", day: "numeric", year: "numeric",
      hour: "2-digit", minute: "2-digit",
    });
  } catch {
    return String(ts);
  }
}

/** Format CRC32 as hex string. */
function formatCrc32(crc: number | null): string {
  if (crc === null || crc === 0) return "\u2014";
  return crc.toString(16).padStart(8, "0").toUpperCase();
}

/** Get immediate children (files and directories) under the current browser directory. */
function getCurrentDirChildren(): { name: string; isDir: boolean; entry: EntryInfo | null }[] {
  const prefix = browserCurrentDir; // "" for root, "subdir/" otherwise
  const items = new Map<string, { isDir: boolean; entry: EntryInfo | null }>();

  for (const entry of browserEntries) {
    if (!entry.path.startsWith(prefix)) continue;
    let relative = entry.path.substring(prefix.length);
    if (!relative || relative === "/") continue;
    if (relative.startsWith("/")) relative = relative.substring(1);

    const slashIdx = relative.indexOf("/");
    if (slashIdx >= 0) {
      // Nested entry — show the parent directory name
      const dirName = relative.substring(0, slashIdx);
      if (!items.has(dirName)) {
        items.set(dirName, { isDir: true, entry: null });
      }
    } else {
      // Direct child
      if (!items.has(relative)) {
        items.set(relative, { isDir: entry.is_dir, entry });
      }
    }
  }

  const result = Array.from(items.entries()).map(([name, info]) => ({
    name, isDir: info.isDir, entry: info.entry,
  }));

  // Sort: directories first, then alphabetically
  result.sort((a, b) => {
    if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
    return a.name.localeCompare(b.name);
  });

  return result;
}

// ---------------------------------------------------------------------------
// Archive Browser — render functions
// ---------------------------------------------------------------------------

/** Render the breadcrumb navigation. */
function renderBreadcrumb() {
  const container = el("browser-breadcrumb");
  if (!browserCurrentDir) {
    container.innerHTML = `<span class="bc-root bc-active">/</span>`;
    return;
  }

  const parts = browserCurrentDir.replace(/\/$/, "").split("/");
  let html = `<a href="#" class="bc-root" data-dir="">/</a>`;
  let accumulated = "";
  for (let i = 0; i < parts.length; i++) {
    accumulated += (i > 0 ? "/" : "") + parts[i];
    const isLast = i === parts.length - 1;
    const dirPath = accumulated + "/";
    html += `<span class="bc-sep">/</span>`;
    if (isLast) {
      html += `<span class="bc-active">${escapeHtml(parts[i])}</span>`;
    } else {
      html += `<a href="#" class="bc-link" data-dir="${escapeHtml(dirPath)}">${escapeHtml(parts[i])}</a>`;
    }
  }

  container.innerHTML = html;

  // Wire click handlers
  container.querySelectorAll("[data-dir]").forEach((a) => {
    a.addEventListener("click", (e) => {
      e.preventDefault();
      browserCurrentDir = (a as HTMLElement).dataset.dir ?? "";
      browserSelected.clear();
      renderArchiveBrowser();
    });
  });
}

/** Render the archive browser table with current directory contents. */
function renderArchiveBrowser() {
  const panel = el("list-result");
  const children = getCurrentDirChildren();
  const entryCount = browserEntries.length;
  const hasArchive = browserArchivePath.trim().length > 0;

  el("browser-bar").style.display = hasArchive ? "flex" : "none";
  if (!hasArchive) {
    panel.innerHTML = `<div class="result-empty">Select an archive and click Open Archive</div>`;
    el("browser-preview").style.display = "none";
    updateSelectionUI();
    return;
  }

  renderBreadcrumb();

  if (children.length === 0) {
    panel.innerHTML = `
      <div class="browser-info">Archive: ${entryCount} entr${entryCount === 1 ? "y" : "ies"} &middot; showing 0 items</div>
      <div class="result-empty">This directory is empty.</div>`;
    el("browser-preview").style.display = "none";
    updateSelectionUI();
    return;
  }

  // Build table rows
  let rows = "";
  for (let i = 0; i < children.length; i++) {
    const { name, isDir, entry } = children[i];
    const fullPath = browserCurrentDir + name;
    const checked = browserSelected.has(fullPath) ? "checked" : "";
    const dirClass = isDir ? "dir" : "";

    rows += `
      <tr class="browser-row ${dirClass}" draggable="true" data-path="${escapeHtml(fullPath)}" data-is-dir="${isDir}" data-index="${i}">
        <td class="cb-cell"><input type="checkbox" class="browser-cb" ${checked} /></td>
        <td class="icon-cell">${entryIcon(isDir)}</td>
        <td class="name-cell">${escapeHtml(name)}${isDir ? "/" : ""}</td>
        <td class="size-cell">${isDir ? "\u2014" : (entry ? formatBytes(entry.size) : "\u2014")}</td>
        <td class="compressed-cell">${(!isDir && entry && entry.compressed_size > 0) ? formatBytes(entry.compressed_size) : "\u2014"}</td>
        <td class="modified-cell">${entry ? formatTimestamp(entry.modified) : "\u2014"}</td>
        <td class="crc-cell">${(!isDir && entry) ? formatCrc32(entry.crc32) : "\u2014"}</td>
      </tr>`;
  }

  panel.innerHTML = `
    <div class="browser-info">Archive: ${entryCount} entr${entryCount === 1 ? "y" : "ies"} &middot; showing ${children.length} item${children.length === 1 ? "" : "s"}</div>
    <div class="table-scroll">
      <table class="result-table browser-table">
        <thead>
          <tr>
            <th class="cb-th"></th>
            <th class="icon-th"></th>
            <th>Name</th>
            <th>Size</th>
            <th>Compressed</th>
            <th>Modified</th>
            <th>CRC32</th>
          </tr>
        </thead>
        <tbody>${rows}</tbody>
      </table>
    </div>`;

  wireBrowserEvents();
  updateSelectionUI();
}

// ---------------------------------------------------------------------------
// Archive Browser — event wiring
// ---------------------------------------------------------------------------

function wireBrowserEvents() {
  const tbody = document.querySelector("#list-result tbody");
  if (!tbody) return;

  // Checkbox change events
  tbody.querySelectorAll(".browser-cb").forEach((cb) => {
    cb.addEventListener("change", (e) => {
      const target = e.target as HTMLInputElement;
      const row = target.closest("tr") as HTMLElement;
      const path = row.dataset.path ?? "";
      if (target.checked) {
        browserSelected.add(path);
      } else {
        browserSelected.delete(path);
      }
      updateSelectionUI();
    });
  });

  // Row click events
  tbody.querySelectorAll(".browser-row").forEach((row) => {
    row.addEventListener("click", (e) => {
      // Don't toggle if clicking directly on checkbox
      if ((e.target as HTMLElement).classList.contains("browser-cb")) return;

      // Toggle checkbox
      const cb = row.querySelector(".browser-cb") as HTMLInputElement;
      cb.checked = !cb.checked;
      cb.dispatchEvent(new Event("change", { bubbles: true }));
    });

    // Double-click: navigate into dir or preview file
    row.addEventListener("dblclick", () => {
      const isDir = row.dataset.isDir === "true";
      const path = row.dataset.path ?? "";

      if (isDir) {
        browserCurrentDir = path.endsWith("/") ? path : path + "/";
        browserSelected.clear();
        renderArchiveBrowser();
      } else {
        showPreview(path);
      }
    });

    // Drag-start: extract entries to temp, then invoke Tauri plugin drag.
    row.addEventListener("dragstart", async (e) => {
      // Prevent default browser native drag — we want the Tauri plugin
      // to manage the drag gesture on the operating system level.
      e.preventDefault();

      const path = row.dataset.path ?? "";
      if (!path || !browserArchivePath) return;

      // Collect the selected entries, or just this row if nothing selected.
      const paths =
        browserSelected.size > 0
          ? [...browserSelected]
          : [path];

      // Show drag status.
      const dragStatus = el("browser-drag-status");
      dragStatus.textContent = "Preparing files for drag...";
      dragStatus.classList.remove("drag-error");
      dragStatus.style.display = "";

      let tempId = "";
      try {
        // Extract entries to a temp directory.
        const tempDir = await prepareDragEntries(
          browserArchivePath,
          paths,
          browserPassword || undefined,
        );

        // Extract a short temp id from the returned path.
        tempId = tempDir.split("/").pop() ?? "";
        currentDragTempId = tempId;

        dragStatus.textContent =
          paths.length > 1 ? `Dragging ${paths.length} items...` : "Dragging...";

        // Dynamically import the Tauri drag plugin — no static import
        // so that preview/browser mode doesn't hard-fail.
        const { startDrag } = await import(
          "@crabnebula/tauri-plugin-drag"
        ).catch(() => {
          throw new Error(
            "Drag plugin not available in this environment. Use Extract Selected instead.",
          );
        });

        // Clear any previous fallback timeout.
        if (currentDragTimeout !== null) {
          clearTimeout(currentDragTimeout);
          currentDragTimeout = null;
        }

        // Start the OS drag operation.  Clean up temp dir in the onEvent
        // callback rather than in dragend (the Promise may resolve before
        // the user has finished dragging).
        await startDrag(
          {
            item: [tempDir],
            // Icon path — in production, set to app icon path for a
            // preview thumbnail during drag.
            icon: "",
          },
          (payload) => {
            // Clean up temp dir on completion.
            if (
              payload.result === "Dropped" ||
              payload.result === "Cancelled"
            ) {
              cleanupDragTempDir(tempId).catch(() => {});
              // Clear fallback timeout so it doesn't fire after successful cleanup.
              if (currentDragTimeout !== null) {
                clearTimeout(currentDragTimeout);
                currentDragTimeout = null;
              }
              currentDragTempId = "";
            }
          },
        );

        // Fallback: if the onEvent callback never fires (plugin bug or
        // irregular platform behaviour), clean up after a timeout.
        currentDragTimeout = setTimeout(() => {
          if (currentDragTempId) {
            cleanupDragTempDir(currentDragTempId).catch(() => {});
            currentDragTempId = "";
          }
          currentDragTimeout = null;
        }, DRAG_CLEANUP_TIMEOUT_MS);
      } catch (err) {
        // Plugin or extraction failed — tell the user to use the
        // fallback Extract Selected path.
        dragStatus.textContent =
          "Drag not available: " +
          (err instanceof Error ? err.message : String(err)) +
          ". Use Extract Selected instead.";
        dragStatus.classList.add("drag-error");
        // Auto-hide fallback message after 5 seconds.
        setTimeout(() => {
          dragStatus.style.display = "none";
        }, 5000);
      }
    });

    // Drag-end: only reset UI state; temp dir cleanup is handled by the
    // startDrag onEvent callback (or fallback timeout above).
    row.addEventListener("dragend", () => {
      const dragStatus = el("browser-drag-status");
      dragStatus.style.display = "none";
      dragStatus.classList.remove("drag-error");

      // Don't clean up temp here — the startDrag onEvent callback or
      // fallback timeout handles that, ensuring files remain available
      // for the duration of the OS drag gesture.
    });
  });
}

// ---------------------------------------------------------------------------
// Archive Browser — preview
// ---------------------------------------------------------------------------

async function showPreview(path: string) {
  const previewPanel = el("browser-preview");
  const title = el("preview-title");
  const size = el("preview-size");
  const content = el("preview-content");

  title.textContent = path;
  size.textContent = "Loading...";
  content.textContent = "";
  previewPanel.style.display = "block";

  try {
    const result = await previewEntry(
      browserArchivePath,
      path,
      browserPassword || undefined,
    );
    title.textContent = result.entry_path;
    size.textContent = result.size_hint + (result.truncated ? " (truncated)" : "");

    if (result.kind === "dir") {
      content.textContent = result.content + "\n\n(Double-click to browse into directory)";
    } else if (result.kind === "text") {
      content.textContent = result.content;
    } else if (result.kind === "binary") {
      content.textContent = result.content;
    } else if (result.kind === "error") {
      content.textContent = "Error: " + result.content;
    } else {
      content.textContent = result.content;
    }
  } catch (e) {
    title.textContent = path;
    size.textContent = "Error";
    content.textContent = String(e);
  }
}

// ---------------------------------------------------------------------------
// Archive Browser — selection & extraction
// ---------------------------------------------------------------------------

function beginBrowserExtract(): number | null {
  if (!browserArchivePath || browserExtractRunning) {
    return null;
  }

  browserExtractRunning = true;
  browserExtractToken += 1;
  updateSelectionUI();
  return browserExtractToken;
}

function finishBrowserExtract(token: number) {
  if (browserExtractToken !== token) {
    return;
  }

  browserExtractRunning = false;
  updateSelectionUI();
}

function isBrowserExtractBlocked(): boolean {
  if (!browserExtractRunning) {
    return false;
  }

  if (browserArchivePath) {
    el<HTMLInputElement>("list-archive").value = browserArchivePath;
    renderArchiveBrowser();
  }
  el<HTMLInputElement>("list-password").value = browserPassword;
  renderBrowserExtractError("Extraction is running, wait until it finishes.");
  return true;
}

function updateSelectionUI() {
  const count = el("browser-selection-count");
  const extractAllBtn = el<HTMLButtonElement>("browser-extract-all");
  const extractSelectedBtn = el<HTMLButtonElement>("browser-extract-selected");
  const extractOutputInput = el<HTMLInputElement>("browser-extract-output");
  const extractOutputBtn = el<HTMLButtonElement>("browser-extract-output-btn");
  const listArchiveInput = el<HTMLInputElement>("list-archive");
  const listArchiveBtn = el<HTMLButtonElement>("list-archive-btn");
  const listRunBtn = el<HTMLButtonElement>("list-run");
  const listPasswordInput = el<HTMLInputElement>("list-password");

  const hasArchive = browserArchivePath.trim().length > 0;
  const selectedCount = browserSelected.size;
  count.textContent = browserExtractRunning
    ? selectedCount > 0
      ? `${selectedCount} selected · extracting...`
      : "Extracting..."
    : selectedCount > 0
      ? `${selectedCount} selected`
      : "";
  extractAllBtn.disabled = !hasArchive || browserExtractRunning;
  extractSelectedBtn.disabled = !hasArchive || browserExtractRunning || selectedCount === 0;
  extractOutputInput.disabled = !hasArchive || browserExtractRunning;
  extractOutputBtn.disabled = !hasArchive || browserExtractRunning;
  listArchiveInput.disabled = browserExtractRunning;
  listArchiveBtn.disabled = browserExtractRunning;
  listRunBtn.disabled = browserExtractRunning;
  listPasswordInput.disabled = browserExtractRunning;
}

function buildExtractErrorsHtml(errors: ExtractArchiveResult["errors"]): string {
  if (errors.length === 0) {
    return "";
  }

  return `
    <div style="margin-top:0.5rem">
      <p style="color:var(--red);font-size:0.8rem">Per-file errors:</p>
      <ul style="padding-left:1.2rem;font-size:0.8rem;color:var(--text-muted)">
        ${errors
          .map(
            (error) =>
              `<li>${escapeHtml(error.path)}: ${escapeHtml(error.message)}</li>`,
          )
          .join("")}
      </ul>
    </div>`;
}

function clearBrowserOperationFeedback() {
  el("list-result")
    .querySelectorAll(".browser-operation-feedback")
    .forEach((node) => node.remove());
}

function renderBrowserExtractFeedback(
  title: string,
  outputDir: string,
  result: ExtractArchiveResult,
) {
  clearBrowserOperationFeedback();

  const feedback = document.createElement("div");
  feedback.className = "browser-operation-feedback";
  feedback.innerHTML = `
    <div class="success-message">${escapeHtml(title)}</div>
    <div class="result-summary" style="margin-top:0.75rem">
      <div class="result-summary-item">
        <span class="label">Files Extracted</span>
        <span class="value">${result.files_extracted}</span>
      </div>
      <div class="result-summary-item">
        <span class="label">Bytes Extracted</span>
        <span class="value">${formatBytes(result.bytes_extracted)}</span>
      </div>
      <div class="result-summary-item">
        <span class="label">Skipped</span>
        <span class="value">${result.files_skipped}</span>
      </div>
    </div>
    ${buildExtractErrorsHtml(result.errors)}
    <div class="result-footer">
      <button class="btn-reveal" data-path="${escapeHtml(outputDir)}" data-action="reveal">
        \u{1F4C2} Reveal in Folder
      </button>
      <button class="btn-reveal" data-path="${escapeHtml(outputDir)}" data-action="open">
        \u{1F4C1} Open Folder
      </button>
    </div>`;

  el("list-result").appendChild(feedback);
  addRecent(outputDir);
  wireRevealButtons(feedback);
}

function renderBrowserExtractError(msg: string) {
  clearBrowserOperationFeedback();

  const feedback = document.createElement("div");
  feedback.className = "browser-operation-feedback";
  feedback.innerHTML = `<div class="error-message">${escapeHtml(msg)}</div>`;
  el("list-result").appendChild(feedback);
}

async function runExtractAll() {
  if (!browserArchivePath) {
    renderBrowserExtractError("Please open an archive first.");
    return;
  }

  syncExtractFormFromArchive(browserArchivePath, browserPassword);
  const outputInput = el<HTMLInputElement>("browser-extract-output");
  if (!outputInput.value.trim()) {
    outputInput.value = inferOutputDir(browserArchivePath);
  }

  const outputDir = outputInput.value.trim();
  if (!outputDir) {
    renderBrowserExtractError("Could not determine an output directory.");
    return;
  }

  const token = beginBrowserExtract();
  if (token === null) {
    return;
  }

  const archivePath = browserArchivePath;
  const password = browserPassword;
  const overwrite = el<HTMLInputElement>("browser-overwrite").checked;
  el<HTMLInputElement>("extract-overwrite").checked = overwrite;

  const extractAllBtn = el<HTMLButtonElement>("browser-extract-all");
  const taskId = `task-extract-all-${Date.now()}`;

  extractAllBtn.textContent = "Extracting...";
  clearBrowserOperationFeedback();

  try {
    const result = await extractArchive(
      archivePath,
      outputDir,
      overwrite,
      password || undefined,
      taskId,
    );

    if (browserExtractToken !== token || browserArchivePath !== archivePath) {
      return;
    }

    renderBrowserExtractFeedback("Archive extracted successfully", outputDir, result);
  } catch (e) {
    if (browserExtractToken === token) {
      renderBrowserExtractError(String(e));
    }
  } finally {
    extractAllBtn.textContent = "Extract All";
    finishBrowserExtract(token);
  }
}

async function extractSelected() {
  if (!browserArchivePath || browserSelected.size === 0) return;

  const token = beginBrowserExtract();
  if (token === null) {
    return;
  }

  const archivePath = browserArchivePath;
  const password = browserPassword;
  const entryPaths = Array.from(browserSelected);
  const overwrite = el<HTMLInputElement>("browser-overwrite").checked;
  const taskId = `task-extract-entries-${Date.now()}`;
  const extractSelectedBtn = el<HTMLButtonElement>("browser-extract-selected");

  extractSelectedBtn.textContent = "Choose Output...";

  try {
    const outputDir = await pickDirectory();
    if (!outputDir) {
      return;
    }

    if (!browserExtractRunning || browserExtractToken !== token || browserArchivePath !== archivePath) {
      return;
    }

    syncExtractFormFromArchive(archivePath, password);
    el<HTMLInputElement>("extract-overwrite").checked = overwrite;

    extractSelectedBtn.textContent = "Extracting...";
    clearBrowserOperationFeedback();

    const result = await extractEntries(
      archivePath,
      entryPaths,
      outputDir,
      overwrite,
      password || undefined,
      taskId,
    );

    if (browserExtractToken !== token || browserArchivePath !== archivePath) {
      return;
    }

    renderBrowserExtractFeedback("Selected entries extracted successfully", outputDir, result);
  } catch (e) {
    if (browserExtractToken === token) {
      renderBrowserExtractError(String(e));
    }
  } finally {
    extractSelectedBtn.textContent = "Extract Selected";
    finishBrowserExtract(token);
  }
}

// ---------------------------------------------------------------------------
// Result display functions (for other modes)
// ---------------------------------------------------------------------------

function renderTestResult(result: TestArchiveResult) {
  el("test-result").innerHTML = `
    <div class="result-summary">
      <div class="result-summary-item">
        <span class="label">Format</span>
        <span class="value">${escapeHtml(result.format)}</span>
      </div>
      <div class="result-summary-item">
        <span class="label">Entries</span>
        <span class="value">${result.entry_count}</span>
      </div>
      <div class="result-summary-item">
        <span class="label">Bytes Read</span>
        <span class="value">${formatBytes(result.bytes_read)}</span>
      </div>
      <div class="result-summary-item">
        <span class="label">CRC32</span>
        <span class="value ${result.crc32_verified ? "success" : "fail"}">
          ${result.crc32_verified ? "Verified" : "Not verified"}
        </span>
      </div>
    </div>
    <p style="margin-top:0.5rem;color:var(--text-muted);font-size:0.78rem;">
      Archive integrity test ${result.crc32_verified ? "passed" : "completed"}.
    </p>
  `;
}

function renderCompressResult(result: CompressArchiveResult) {
  const outputPath = result.output_path;
  el("compress-result").innerHTML = `
    <div class="success-message">Compression completed successfully</div>
    <div class="result-summary" style="margin-top:0.75rem">
      <div class="result-summary-item">
        <span class="label">Files Added</span>
        <span class="value">${result.files_added}</span>
      </div>
      <div class="result-summary-item">
        <span class="label">Directories</span>
        <span class="value">${result.directories_added}</span>
      </div>
      <div class="result-summary-item">
        <span class="label">Bytes Written</span>
        <span class="value">${formatBytes(result.bytes_written)}</span>
      </div>
      <div class="result-summary-item">
        <span class="label">Skipped</span>
        <span class="value">${result.skipped}</span>
      </div>
    </div>
    <p style="margin-top:0.5rem;font-size:0.82rem;">
      Format: <strong>${escapeHtml(result.format)}</strong><br />
      Output: <code style="color:var(--text-muted)">${escapeHtml(outputPath)}</code>
    </p>
    <div class="result-footer">
      <button class="btn-reveal" data-path="${escapeHtml(outputPath)}" data-action="reveal">
        \u{1F4C2} Reveal in Folder
      </button>
      <button class="btn-reveal" data-path="${escapeHtml(getParentDir(outputPath))}" data-action="open">
        \u{1F4C1} Open Folder
      </button>
    </div>
  `;
  addRecent(outputPath);
  // Wire reveal buttons
  wireRevealButtons("compress-result");
}

function renderExtractResult(result: ExtractArchiveResult) {
  const outputDir = el<HTMLInputElement>("extract-output").value.trim();
  const errorsHtml = buildExtractErrorsHtml(result.errors);

  el("extract-result").innerHTML = `
    <div class="success-message">Extraction completed successfully</div>
    <div class="result-summary" style="margin-top:0.75rem">
      <div class="result-summary-item">
        <span class="label">Files Extracted</span>
        <span class="value">${result.files_extracted}</span>
      </div>
      <div class="result-summary-item">
        <span class="label">Bytes Extracted</span>
        <span class="value">${formatBytes(result.bytes_extracted)}</span>
      </div>
      <div class="result-summary-item">
        <span class="label">Skipped</span>
        <span class="value">${result.files_skipped}</span>
      </div>
    </div>
    ${errorsHtml}
    ${outputDir ? `
    <div class="result-footer">
      <button class="btn-reveal" data-path="${escapeHtml(outputDir)}" data-action="reveal">
        \u{1F4C2} Reveal in Folder
      </button>
      <button class="btn-reveal" data-path="${escapeHtml(outputDir)}" data-action="open">
        \u{1F4C1} Open Folder
      </button>
    </div>` : ""}
  `;
  if (outputDir) addRecent(outputDir);
  wireRevealButtons("extract-result");
}

function wireRevealButtons(containerIdOrEl: string | HTMLElement) {
  const container = typeof containerIdOrEl === "string" ? el(containerIdOrEl) : containerIdOrEl;
  container.querySelectorAll(".btn-reveal").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const path = (btn as HTMLElement).dataset.path ?? "";
      const action = (btn as HTMLElement).dataset.action ?? "reveal";
      if (action === "reveal") {
        const ok = await revealFile(path);
        if (!ok) {
          // Fallback: try openPath on parent directory
          await openFolder(getParentDir(path));
        }
      } else {
        await openFolder(path);
      }
    });
  });
}

function renderCancelNotice(mode: string) {
  el(`${mode}-result`).innerHTML = `<div class="error-message">Operation cancelled by user.</div>`;
}

// ---------------------------------------------------------------------------
// Mode switching
// ---------------------------------------------------------------------------

function switchMode(mode: string) {
  // Hide all panels, deactivate all tabs
  document.querySelectorAll(".panel").forEach((p) => p.classList.remove("active"));
  document.querySelectorAll(".mode-tab").forEach((t) => {
    t.classList.remove("active");
    t.setAttribute("aria-selected", "false");
  });

  // Show target panel and activate tab
  el(`panel-${mode}`).classList.add("active");
  const tab = document.querySelector(`.mode-tab[data-mode="${mode}"]`);
  if (tab) {
    tab.classList.add("active");
    tab.setAttribute("aria-selected", "true");
  }

  // Hide preview when switching away
  if (mode !== "list") {
    el("browser-preview").style.display = "none";
  }
}

// ---------------------------------------------------------------------------
// Dialog helpers
// ---------------------------------------------------------------------------

async function pickFiles(): Promise<string[]> {
  const result = await open({ multiple: true, title: "Select files" });
  if (!result) return [];
  return Array.isArray(result) ? result : [result];
}

async function pickDirectory(): Promise<string | null> {
  return await open({ directory: true, multiple: false, title: "Select directory" });
}

async function pickSaveFile(defaultName?: string): Promise<string | null> {
  return await save({ title: "Save archive as", defaultPath: defaultName });
}

async function pickSingleFile(): Promise<string | null> {
  const result = await open({ multiple: false, title: "Select archive file" });
  if (!result) return null;
  return Array.isArray(result) ? result[0] : result;
}

// ---------------------------------------------------------------------------
// Drag and drop
// ---------------------------------------------------------------------------

let dragCounter = 0;

function setupDragDrop() {
  const overlay = el("drop-overlay");

  document.addEventListener("dragenter", (e) => {
    e.preventDefault();
    dragCounter++;
    if (dragCounter === 1) {
      overlay.classList.add("active");
    }
  });

  document.addEventListener("dragleave", (e) => {
    e.preventDefault();
    dragCounter--;
    if (dragCounter <= 0) {
      dragCounter = 0;
      overlay.classList.remove("active");
    }
  });

  document.addEventListener("dragover", (e) => {
    e.preventDefault();
  });

  document.addEventListener("drop", (e) => {
    e.preventDefault();
    dragCounter = 0;
    overlay.classList.remove("active");
    handleDrop(e.dataTransfer);
  });

  // Also allow drag over specific archive input rows to highlight them
  document.querySelectorAll(".input-row input[readonly]").forEach((input) => {
    const row = input.closest(".input-row");
    if (!row) return;
    input.addEventListener("dragenter", () => row.classList.add("drag-over"));
    input.addEventListener("dragleave", () => row.classList.remove("drag-over"));
    input.addEventListener("drop", () => row.classList.remove("drag-over"));
  });
}

async function handleDrop(dt: DataTransfer | null) {
  if (!dt) return;
  const files: File[] = [];
  // Collect all dropped files/items
  if (dt.files && dt.files.length > 0) {
    for (let i = 0; i < dt.files.length; i++) {
      files.push(dt.files[i]);
    }
  }
  if (files.length === 0) return;

  // Extract paths; Tauri webview may have .path, browser preview will not
  const paths: string[] = [];
  for (const f of files) {
    if ((f as any).path) {
      paths.push((f as any).path as string);
    }
  }

  // No native paths — show friendly error
  if (paths.length === 0) {
    showError(
      el("panel-compress").classList.contains("active") ? "compress-result" : "extract-result",
      "This browser preview doesn't support drop-to-path. Please use the Browse button or run in the Tauri desktop app.",
    );
    return;
  }

  // Determine if any are archives
  const archives = paths.filter((p) => isArchiveExt(p));
  const nonArchives = paths.filter((p) => !isArchiveExt(p));

  if (archives.length > 0 && nonArchives.length === 0) {
    // All archives — switch to List and auto-open
    if (archives.length === 1) {
      switchMode("list");
      el<HTMLInputElement>("list-archive").value = archives[0];
      el<HTMLInputElement>("list-password").value = "";
      el("list-result").innerHTML =
        `<div class="running-text"><span class="running-spinner"></span> Opening archive...</div>`;
      setTimeout(() => runListWithPath(archives[0]), 50);
    } else {
      switchMode("list");
      el<HTMLInputElement>("list-archive").value = archives[0];
      el<HTMLInputElement>("list-password").value = "";
      el("list-result").innerHTML =
        `<div class="info-message">${archives.length} archives dropped. Opening first: ${escapeHtml(getBasename(archives[0]))}. Drop individually for others.</div>`;
      setTimeout(() => runListWithPath(archives[0]), 50);
    }
  } else if (nonArchives.length > 0) {
    // Switch to Compress
    switchMode("compress");
    const input = el<HTMLInputElement>("compress-sources");
    input.dataset.paths = paths.join("\n");
    input.value = paths.length === 1 ? paths[0] : `${paths.length} files selected`;
    updateCompressSourceChips(paths);
    const format = el<HTMLSelectElement>("compress-format").value;
    el<HTMLInputElement>("compress-output").value = inferOutputPath(paths, format);
    el("compress-result").innerHTML =
      `<div class="result-empty">Configured from dropped files. Click Compress to start.</div>`;
  }
}

// ---------------------------------------------------------------------------
// Run handlers
// ---------------------------------------------------------------------------

async function runCompress() {
  const sources = (el<HTMLInputElement>("compress-sources").dataset.paths ?? "").split("\n").filter(Boolean);
  if (sources.length === 0) {
    showError("compress-result", "Please select at least one source file or directory.");
    return;
  }
  const outputPath = el<HTMLInputElement>("compress-output").value.trim();
  if (!outputPath) {
    showError("compress-result", "Please select an output path.");
    return;
  }
  const format = el<HTMLSelectElement>("compress-format").value;
  const levelRaw = el<HTMLInputElement>("compress-level").value.trim();
  const jobsRaw = el<HTMLInputElement>("compress-jobs").value.trim();
  const password = el<HTMLInputElement>("compress-password").value.trim() || undefined;
  const level = levelRaw ? parseInt(levelRaw, 10) : undefined;
  const jobs = jobsRaw ? parseInt(jobsRaw, 10) : undefined;
  const taskId = `task-${Date.now()}`;
  currentTaskId = taskId;

  setRunning("compress", true);
  try {
    const result = await compressArchive(sources, outputPath, format, level, jobs, password, taskId);
    renderCompressResult(result);
    currentTaskId = null;
  } catch (e) {
    const msg = String(e);
    if (msg.toLowerCase().includes("cancelled")) {
      renderCancelNotice("compress");
    } else {
      showError("compress-result", msg);
    }
    currentTaskId = null;
  } finally {
    setRunning("compress", false);
  }
}

async function runExtract() {
  const archivePath = el<HTMLInputElement>("extract-archive").value.trim();
  if (!archivePath) {
    showError("extract-result", "Please select an archive file.");
    return;
  }
  const outputDir = el<HTMLInputElement>("extract-output").value.trim();
  if (!outputDir) {
    showError("extract-result", "Please select an output directory.");
    return;
  }
  const overwrite = el<HTMLInputElement>("extract-overwrite").checked;
  const password = el<HTMLInputElement>("extract-password").value.trim() || undefined;
  const taskId = `task-${Date.now()}`;
  currentTaskId = taskId;

  setRunning("extract", true);
  try {
    const result = await extractArchive(archivePath, outputDir, overwrite, password, taskId);
    renderExtractResult(result);
    currentTaskId = null;
  } catch (e) {
    const msg = String(e);
    if (msg.toLowerCase().includes("cancelled")) {
      renderCancelNotice("extract");
    } else {
      showError("extract-result", msg);
    }
    currentTaskId = null;
  } finally {
    setRunning("extract", false);
  }
}

/** Run list with the path already in the input field (normal interaction). */
async function runList() {
  const archivePath = el<HTMLInputElement>("list-archive").value.trim();
  if (!archivePath) {
    showError("list-result", "Please select an archive file.");
    return;
  }
  await runListWithPath(archivePath);
}

/** Run list with an explicit archive path (from drop, recent chip, opened-archives, etc.) */
async function runListWithPath(archivePath: string) {
  if (isBrowserExtractBlocked()) {
    return;
  }

  el<HTMLInputElement>("list-archive").value = archivePath;
  const password = el<HTMLInputElement>("list-password").value.trim() || undefined;

  const runButton = el<HTMLButtonElement>("list-run");
  runButton.disabled = true;
  browserEntries = [];
  browserArchivePath = "";
  browserPassword = password ?? "";
  browserCurrentDir = "";
  browserSelected.clear();
  updateSelectionUI();
  el("browser-bar").style.display = "none";
  el("browser-preview").style.display = "none";
  el("list-result").innerHTML =
    `<div class="running-text"><span class="running-spinner"></span> Listing contents...</div>`;

  try {
    const entries = await listArchive(archivePath, password);
    browserEntries = entries;
    browserArchivePath = archivePath;
    browserPassword = password ?? "";
    browserCurrentDir = "";
    browserSelected.clear();

    syncExtractFormFromArchive(archivePath, browserPassword);
    syncBrowserExtractOutputFromArchive(archivePath);

    addRecent(archivePath);
    renderArchiveBrowser();
  } catch (e) {
    browserArchivePath = "";
    showError("list-result", String(e));
    updateSelectionUI();
  } finally {
    runButton.disabled = false;
  }
}


async function runTest() {
  const archivePath = el<HTMLInputElement>("test-archive").value.trim();
  if (!archivePath) {
    showError("test-result", "Please select an archive file.");
    return;
  }
  const password = el<HTMLInputElement>("test-password").value.trim() || undefined;

  const runButton = el<HTMLButtonElement>("test-run");
  runButton.disabled = true;
  el("test-result").innerHTML = `<div class="running-text"><span class="running-spinner"></span> Testing integrity...</div>`;
  try {
    const result = await testArchive(archivePath, password);
    renderTestResult(result);
  } catch (e) {
    showError("test-result", String(e));
  } finally {
    runButton.disabled = false;
  }
}

async function handleCancel(mode: string) {
  if (!currentTaskId) return;
  try {
    await cancelTask(currentTaskId);
  } catch (e) {
    console.warn("cancelTask error:", e);
  }
}

// ---------------------------------------------------------------------------
// Initialization
// ---------------------------------------------------------------------------

window.addEventListener("DOMContentLoaded", async () => {
  // --- Load formats for Compress mode ---
  const fallbackFormats = [
    { name: "zip", extension: ".zip", can_compress: true },
    { name: "tar", extension: ".tar", can_compress: true },
    { name: "tar.gz", extension: ".tar.gz", can_compress: true },
    { name: "tar.zst", extension: ".tar.zst", can_compress: true },
    { name: "tar.xz", extension: ".tar.xz", can_compress: true },
  ];
  try {
    const formats = await getFormats();
    compressFormats = formats.filter((f) => f.can_compress);
  } catch (e) {
    console.error("Failed to load formats, using fallback:", e);
    compressFormats = fallbackFormats as FormatInfo[];
  }

  if (compressFormats.length === 0) {
    compressFormats = fallbackFormats as FormatInfo[];
  }

  const select = el<HTMLSelectElement>("compress-format");
  for (const fmt of compressFormats) {
    const opt = document.createElement("option");
    opt.value = fmt.name;
    opt.textContent = fmt.name;
    select.appendChild(opt);
  }

  // --- Recent files ---
  renderRecentChips();

  // --- Drag and drop ---
  setupDragDrop();

  // --- Mode tabs ---
  document.querySelectorAll(".mode-tab").forEach((tab) => {
    tab.addEventListener("click", () => {
      const mode = (tab as HTMLElement).dataset.mode;
      if (mode) switchMode(mode);
    });
  });

  // -----------------------------------------------------------------------
  // Compress
  // -----------------------------------------------------------------------

  el("compress-source-btn").addEventListener("click", async () => {
    const paths = await pickFiles();
    if (paths.length === 0) return;
    const input = el<HTMLInputElement>("compress-sources");
    input.dataset.paths = paths.join("\n");
    input.value = paths.length === 1 ? paths[0] : `${paths.length} files selected`;
    updateCompressSourceChips(paths);
    // Auto-infer output path
    const format = el<HTMLSelectElement>("compress-format").value;
    el<HTMLInputElement>("compress-output").value = inferOutputPath(paths, format);
    el("compress-result").innerHTML =
      `<div class="result-empty">Configured. Click Compress to start.</div>`;
  });

  el("compress-output-btn").addEventListener("click", async () => {
    // Suggest default filename based on current source
    const sources = (el<HTMLInputElement>("compress-sources").dataset.paths ?? "").split("\n").filter(Boolean);
    const format = el<HTMLSelectElement>("compress-format").value;
    let defaultName: string | undefined;
    if (sources.length > 0) {
      defaultName = inferOutputPath(sources, format);
    }
    const path = await pickSaveFile(defaultName);
    if (!path) return;
    el<HTMLInputElement>("compress-output").value = path;
  });

  el("compress-run").addEventListener("click", runCompress);
  el("compress-cancel").addEventListener("click", () => handleCancel("compress"));

  // When format changes, re-infer output path
  el<HTMLSelectElement>("compress-format").addEventListener("change", () => {
    const sources = (el<HTMLInputElement>("compress-sources").dataset.paths ?? "").split("\n").filter(Boolean);
    if (sources.length > 0) {
      el<HTMLInputElement>("compress-output").value = inferOutputPath(sources, el<HTMLSelectElement>("compress-format").value);
    }
  });

  // -----------------------------------------------------------------------
  // Extract
  // -----------------------------------------------------------------------

  el("extract-archive-btn").addEventListener("click", async () => {
    const path = await pickSingleFile();
    if (!path) return;
    el<HTMLInputElement>("extract-archive").value = path;
    el<HTMLInputElement>("extract-output").value = inferOutputDir(path);
    el("extract-result").innerHTML =
      `<div class="result-empty">Archive selected. Click Extract to start.</div>`;
  });

  el("extract-output-btn").addEventListener("click", async () => {
    const path = await pickDirectory();
    if (!path) return;
    el<HTMLInputElement>("extract-output").value = path;
  });

  el("extract-run").addEventListener("click", runExtract);
  el("extract-cancel").addEventListener("click", () => handleCancel("extract"));

  // When extract archive path changes, auto-infer output dir
  el("extract-archive").addEventListener("change", () => {
    const path = el<HTMLInputElement>("extract-archive").value.trim();
    if (path) {
      el<HTMLInputElement>("extract-output").value = inferOutputDir(path);
    }
  });

  // -----------------------------------------------------------------------
  // List — archive browser
  // -----------------------------------------------------------------------

  el("list-archive-btn").addEventListener("click", async () => {
    const path = await pickSingleFile();
    if (!path) return;
    el<HTMLInputElement>("list-archive").value = path;
  });

  el("browser-extract-output-btn").addEventListener("click", async () => {
    const path = await pickDirectory();
    if (!path) return;
    el<HTMLInputElement>("browser-extract-output").value = path;
  });

  el("list-run").addEventListener("click", runList);
  el("browser-extract-all").addEventListener("click", runExtractAll);

  // Browser: Extract Selected button
  el("browser-extract-selected").addEventListener("click", extractSelected);

  // Browser: Preview close button
  el("preview-close").addEventListener("click", () => {
    el("browser-preview").style.display = "none";
  });

  // -----------------------------------------------------------------------
  // Test
  // -----------------------------------------------------------------------

  el("test-archive-btn").addEventListener("click", async () => {
    const path = await pickSingleFile();
    if (!path) return;
    el<HTMLInputElement>("test-archive").value = path;
  });

  el("test-run").addEventListener("click", runTest);

  // -----------------------------------------------------------------------
  // Opened archives (cold start + hot start via file association)
  // -----------------------------------------------------------------------

  // Cold start: check for pending archives from command-line args
  try {
    const pending = await getOpenedArchives();
    if (pending.length > 0) {
      const firstPath = pending[0];
      switchMode("list");
      el<HTMLInputElement>("list-archive").value = firstPath;
      el<HTMLInputElement>("list-password").value = "";
      el("list-result").innerHTML =
        `<div class="running-text"><span class="running-spinner"></span> Opening archive from file association...</div>`;
      setTimeout(() => runListWithPath(firstPath), 100);
    }
  } catch (e) {
    // Not available in browser preview — this is expected
    console.debug("getOpenedArchives skipped (browser preview):", e);
  }

  // Hot start: listen for opened-archives event from single-instance / macOS Opened
  try {
    const unlisten = await listen<string[]>("opened-archives", (event) => {
      const paths = event.payload;
      if (paths.length > 0) {
        const firstPath = paths[0];
        switchMode("list");
        el<HTMLInputElement>("list-archive").value = firstPath;
        el<HTMLInputElement>("list-password").value = "";
        el("list-result").innerHTML =
          `<div class="running-text"><span class="running-spinner"></span> Opening archive...</div>`;
        runListWithPath(firstPath);
      }
    });
    unlistenOpenedArchives = unlisten;
  } catch (e) {
    // Not available in browser preview — expected
    console.debug("listen for opened-archives skipped (browser preview):", e);
  }

  // Clean up opened-archives listener on page unload
  window.addEventListener("beforeunload", () => {
    if (unlistenOpenedArchives) {
      unlistenOpenedArchives();
      unlistenOpenedArchives = null;
    }
  });
});
