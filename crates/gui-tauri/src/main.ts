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
  type PreviewResult,
  type TestArchiveResult,
  type CompressArchiveResult,
  type ExtractArchiveResult,
  type TaskProgressPayload,
  type TaskPhase,
} from "./bridge";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  LOCALE_CHANGED_EVENT,
  applyI18n,
  getLocale,
  setLocale,
  t,
  type Locale,
} from "./i18n";

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

type ProgressPanel = "compress" | "extract";
type TranslationParams = Record<string, string | number>;
type TerminalTaskStatus = Extract<TaskProgressPayload["status"], "finished" | "cancelled" | "failed">;

type ProgressSnapshot =
  | {
    kind: "pending";
    taskId: string;
    messageKey: string;
    params?: TranslationParams;
  }
  | {
    kind: "payload";
    payload: TaskProgressPayload;
  }
  | {
    kind: "terminal";
    taskId: string;
    status: TerminalTaskStatus;
    message?: string;
  }
  | null;

type BrowserOperationState =
  | {
    kind: "success";
    titleKey: "browser.feedback.extractSuccessAll" | "browser.feedback.extractSuccessSelected";
    outputDir: string;
    result: ExtractArchiveResult;
  }
  | {
    kind: "error";
    message: string;
  }
  | null;

interface PreviewState {
  path: string;
  loading: boolean;
  result: PreviewResult | null;
  error: string | null;
}

const activeTaskIds: Record<ProgressPanel, string | null> = {
  compress: null,
  extract: null,
};

const runningTaskIds: Record<ProgressPanel, string | null> = {
  compress: null,
  extract: null,
};

const progressSnapshots: Record<ProgressPanel, ProgressSnapshot> = {
  compress: null,
  extract: null,
};

let compressFormats: FormatInfo[] = [];
let unlistenOpenedArchives: (() => void) | null = null;
let unlistenTaskProgress: (() => void) | null = null;
let currentMode = "home";
let lastCompressResult: CompressArchiveResult | null = null;
let lastExtractResult: ExtractArchiveResult | null = null;
let lastTestResult: TestArchiveResult | null = null;
let lastBrowserOperationState: BrowserOperationState = null;
let lastPreviewState: PreviewState | null = null;

// Archive browser state
let extractEntriesList: EntryInfo[] = [];
let extractArchivePath = "";
let extractPassword = "";
let extractCurrentDir = "";
let extractSelectedEntries = new Set<string>();
let extractRunning = false;
let extractToken = 0;
let currentDragTempId = "";
let currentDragTimeout: ReturnType<typeof setTimeout> | null = null;
const DRAG_CLEANUP_TIMEOUT_MS = 60_000;

const ARCHIVE_EXTS = /\.(zip|zipx|tar|tar\.gz|tar\.bz2|tar\.br|tar\.lz4|tar\.zst|tar\.xz|tgz|tbz|tbz2|tzst|txz|7z|rar|cab|asar|deb|cpio)$/i;

// Recent files storage
const RECENT_KEY = "geezipx_recent_paths";
const MAX_RECENT = 10;

interface RecentEntry {
  path: string;
  label: string;
  isArchive: boolean;
}

function progressRoot(mode: ProgressPanel): HTMLDivElement {
  return el<HTMLDivElement>(`${mode}-progress`);
}

function isTerminalTaskStatus(status: TaskProgressPayload["status"]): status is TerminalTaskStatus {
  return status === "finished" || status === "cancelled" || status === "failed";
}

function waitingProgressText(): string {
  return t("progress.waiting");
}

function terminalStage(status: TerminalTaskStatus): TaskProgressPayload["stage"] {
  switch (status) {
    case "finished":
      return "completed";
    case "cancelled":
      return "cancelled";
    case "failed":
      return "failed";
  }
}

function stageLabel(stage: TaskProgressPayload["stage"]): string {
  return t(`progress.stage.${stage}`);
}

function phaseLabel(phase: TaskPhase | null): string {
  return phase ? t(`progress.phase.${phase}`) : "";
}

function defaultTerminalMessage(mode: ProgressPanel, status: TerminalTaskStatus): string {
  switch (status) {
    case "finished":
      return mode === "compress"
        ? t("progress.terminal.compressFinished")
        : t("progress.terminal.extractFinished");
    case "cancelled":
      return t("progress.terminal.cancelled");
    case "failed":
      return t("progress.terminal.failed");
  }
}

function bindTaskProgress(
  mode: ProgressPanel,
  taskId: string,
  messageKey: string,
  params?: TranslationParams,
) {
  progressSnapshots[mode] = { kind: "pending", taskId, messageKey, params };
  activeTaskIds[mode] = taskId;
  runningTaskIds[mode] = taskId;
  const root = progressRoot(mode);
  root.hidden = false;
  root.dataset.status = "started";
  el(`${mode}-progress-stage`).textContent = t("progress.queued");
  el(`${mode}-progress-percent`).textContent = "--";
  el(`${mode}-progress-message`).textContent = t(messageKey, params);
  el(`${mode}-progress-entry`).textContent = "";
  el(`${mode}-progress-stats`).textContent = waitingProgressText();
  const bar = el<HTMLProgressElement>(`${mode}-progress-bar`);
  bar.max = 100;
  bar.removeAttribute("value");
}

function resetTaskProgress(mode: ProgressPanel) {
  progressSnapshots[mode] = null;
  activeTaskIds[mode] = null;
  runningTaskIds[mode] = null;
  const root = progressRoot(mode);
  root.hidden = true;
  delete root.dataset.status;
  el(`${mode}-progress-stage`).textContent = "";
  el(`${mode}-progress-percent`).textContent = "";
  el(`${mode}-progress-message`).textContent = "";
  el(`${mode}-progress-entry`).textContent = "";
  el(`${mode}-progress-stats`).textContent = "";
  const bar = el<HTMLProgressElement>(`${mode}-progress-bar`);
  bar.max = 100;
  bar.removeAttribute("value");
}

function settleTaskProgressFallback(
  mode: ProgressPanel,
  taskId: string,
  status: TerminalTaskStatus,
  message?: string,
) {
  progressSnapshots[mode] = { kind: "terminal", taskId, status, message };
  if (runningTaskIds[mode] !== taskId) {
    return;
  }

  runningTaskIds[mode] = null;
  if (activeTaskIds[mode] !== taskId) {
    return;
  }

  const root = progressRoot(mode);
  root.hidden = false;
  root.dataset.status = status;
  el(`${mode}-progress-stage`).textContent = stageLabel(terminalStage(status));

  const percentEl = el(`${mode}-progress-percent`);
  if (status === "finished") {
    percentEl.textContent = "100%";
  } else if (!percentEl.textContent.trim()) {
    percentEl.textContent = "--";
  }

  const finalMessage = message ?? defaultTerminalMessage(mode, status);
  el(`${mode}-progress-message`).textContent = finalMessage;
  el(`${mode}-progress-entry`).textContent = "";

  const statsEl = el(`${mode}-progress-stats`);
  if (!statsEl.textContent.trim() || statsEl.textContent === waitingProgressText()) {
    statsEl.textContent = finalMessage;
  }

  const bar = el<HTMLProgressElement>(`${mode}-progress-bar`);
  bar.max = 100;
  if (status === "finished") {
    bar.value = 100;
  } else if (!bar.hasAttribute("value")) {
    bar.removeAttribute("value");
  }
}

function formatTaskPercent(payload: TaskProgressPayload): string {
  if (typeof payload.percent === "number") {
    return `${Math.round(payload.percent)}%`;
  }
  if (payload.status === "finished") {
    return "100%";
  }
  return "--";
}

function formatTaskStats(payload: TaskProgressPayload): string {
  const bytes = payload.total == null
    ? t("progress.stats.processed", { current: formatBytes(payload.current) })
    : t("progress.stats.total", {
      current: formatBytes(payload.current),
      total: formatBytes(payload.total),
    });
  const entries = payload.total_entries == null
    ? t("progress.stats.entries", { count: payload.completed_entries })
    : t("progress.stats.entriesTotal", {
      current: payload.completed_entries,
      total: payload.total_entries,
    });
  const rate = payload.bytes_per_second == null
    ? ""
    : t("progress.stats.rate", {
      rate: formatBytes(Math.max(0, Math.round(payload.bytes_per_second))),
    });
  return [bytes, entries, rate].filter(Boolean).join(" · ");
}

function resolveProgressPanel(taskId: string): ProgressPanel | null {
  if (activeTaskIds.compress === taskId) return "compress";
  if (activeTaskIds.extract === taskId) return "extract";
  return null;
}

function updateTaskProgress(payload: TaskProgressPayload) {
  const mode = resolveProgressPanel(payload.task_id);
  if (!mode) return;

  progressSnapshots[mode] = { kind: "payload", payload };
  const root = progressRoot(mode);
  root.hidden = false;
  root.dataset.status = payload.status;

  const phaseText = phaseLabel(payload.phase);
  const stageText = phaseText
    ? `${stageLabel(payload.stage)} · ${phaseText}`
    : stageLabel(payload.stage);
  el(`${mode}-progress-stage`).textContent = stageText;
  el(`${mode}-progress-percent`).textContent = formatTaskPercent(payload);
  el(`${mode}-progress-message`).textContent = payload.message;
  el(`${mode}-progress-entry`).textContent = payload.current_entry
    ? t("common.currentEntry", { entry: payload.current_entry })
    : "";
  el(`${mode}-progress-stats`).textContent = formatTaskStats(payload);

  const bar = el<HTMLProgressElement>(`${mode}-progress-bar`);
  bar.max = 100;
  if (typeof payload.percent === "number") {
    bar.value = Math.max(0, Math.min(100, payload.percent));
  } else if (payload.status === "finished") {
    bar.value = 100;
  } else {
    bar.removeAttribute("value");
  }

  if (isTerminalTaskStatus(payload.status)) {
    runningTaskIds[mode] = null;
  }
}

function rerenderProgressPanels() {
  (Object.keys(progressSnapshots) as ProgressPanel[]).forEach((mode) => {
    const snapshot = progressSnapshots[mode];
    if (!snapshot) {
      return;
    }

    if (snapshot.kind === "pending") {
      bindTaskProgress(mode, snapshot.taskId, snapshot.messageKey, snapshot.params);
      return;
    }

    if (snapshot.kind === "payload") {
      updateTaskProgress(snapshot.payload);
      return;
    }

    settleTaskProgressFallback(mode, snapshot.taskId, snapshot.status, snapshot.message);
  });
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
 *  Handles double extensions like .tar.gz, .tar.bz2, .tar.br, .tar.lz4, .tar.zst, .tar.xz. */
function stripArchiveExt(name: string): string {
  const double = /\.(tar\.gz|tar\.bz2|tar\.br|tar\.lz4|tar\.zst|tar\.xz|tgz|tbz|tbz2|tzst|txz)$/i;
  const single = /\.(zip|zipx|tar|7z|rar|cab|asar|deb|cpio)$/i;
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

function setExtractArchiveForm(archivePath: string, password = "") {
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

function setExtractOutputDir(archivePath: string) {
  el<HTMLInputElement>("extract-output").value = inferOutputDir(archivePath);
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
      `<span class="file-chip"><span class="chip-icon">\u{1F4C4}</span>${escapeHtml(name)}</span>`,
    );
  }
  const remaining = paths.length - maxShow;
  if (remaining > 0) {
    parts.push(
      `<span class="file-chip more-chip">${escapeHtml(t("common.moreCount", { count: remaining }))}</span>`,
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
        </span>`,
    )
    .join("");

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
        switchMode("extract");
        el<HTMLInputElement>("extract-archive").value = path;
        el<HTMLInputElement>("extract-password").value = "";
        el("extract-result").innerHTML =
          `<div class="result-empty">${escapeHtml(t("list.opening"))}</div>`;
        setTimeout(() => openExtractArchive(path), 50);
      } else {
        switchMode("compress");
        const input = el<HTMLInputElement>("compress-sources");
        input.value = path;
        input.dataset.paths = path;
        updateCompressSourceChips([path]);
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
  if (!ts || ts === 0) return "—";
  const d = new Date(ts * 1000);
  try {
    return d.toLocaleDateString(getLocale(), {
      month: "short",
      day: "numeric",
      year: "numeric",
      hour: "2-digit",
      minute: "2-digit",
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

function summaryItem(labelKey: string, value: string, valueClass = "value"): string {
  return `
    <div class="result-summary-item">
      <span class="label">${escapeHtml(t(labelKey))}</span>
      <span class="${escapeHtml(valueClass)}">${value}</span>
    </div>`;
}

function closePreview(resetState = true) {
  if (resetState) {
    lastPreviewState = null;
  }
  el("extract-preview").style.display = "none";
}

function renderPreviewState() {
  const previewPanel = el("extract-preview");
  const title = el("extract-preview-title");
  const size = el("extract-preview-size");
  const content = el("extract-preview-content");

  if (!lastPreviewState) {
    previewPanel.style.display = "none";
    title.textContent = "";
    size.textContent = "";
    content.textContent = "";
    return;
  }

  previewPanel.style.display = "block";
  title.textContent = lastPreviewState.result?.entry_path ?? lastPreviewState.path;

  if (lastPreviewState.loading) {
    size.textContent = t("browser.preview.loading");
    content.textContent = "";
    return;
  }

  if (lastPreviewState.result) {
    const result = lastPreviewState.result;
    size.textContent = result.size_hint + (result.truncated ? t("browser.preview.truncated") : "");

    if (result.kind === "dir") {
      content.textContent = result.content + t("browser.preview.dirHint");
      return;
    }

    if (result.kind === "error") {
      content.textContent = t("browser.preview.errorPrefix") + result.content;
      return;
    }

    content.textContent = result.content;
    return;
  }

  size.textContent = t("browser.preview.error");
  content.textContent = lastPreviewState.error ?? "";
}

/** Get immediate children (files and directories) under the current browser directory. */
function getCurrentDirChildren(): { name: string; isDir: boolean; entry: EntryInfo | null }[] {
  const prefix = extractCurrentDir; // "" for root, "subdir/" otherwise
  const items = new Map<string, { isDir: boolean; entry: EntryInfo | null }>();

  for (const entry of extractEntriesList) {
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
  const container = el("extract-browser-breadcrumb");
  if (!extractCurrentDir) {
    container.innerHTML = `<span class="bc-root bc-active">/</span>`;
    return;
  }

  const parts = extractCurrentDir.replace(/\/$/, "").split("/");
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
      extractCurrentDir = (a as HTMLElement).dataset.dir ?? "";
      extractSelectedEntries.clear();
      renderArchiveBrowser();
    });
  });
}

/** Render the archive browser table with current directory contents. */
function renderArchiveBrowser() {
  const panel = el("extract-result");
  const children = getCurrentDirChildren();
  const entryCount = extractEntriesList.length;
  const hasArchive = extractArchivePath.trim().length > 0;

  el("extract-browser-bar").style.display = hasArchive ? "flex" : "none";
  if (!hasArchive) {
    panel.innerHTML = `<div class="result-empty">${escapeHtml(t("list.result.empty"))}</div>`;
    closePreview();
    updateSelectionUI();
    return;
  }

  renderBreadcrumb();

  if (children.length === 0) {
    panel.innerHTML = `
      <div class="browser-info">${escapeHtml(t("browser.info", { entryCount, itemCount: 0 }))}</div>
      <div class="result-empty">${escapeHtml(t("browser.emptyDir"))}</div>`;
    closePreview();
    updateSelectionUI();
    return;
  }

  let rows = "";
  for (let i = 0; i < children.length; i++) {
    const { name, isDir, entry } = children[i];
    const fullPath = extractCurrentDir + name;
    const checked = extractSelectedEntries.has(fullPath) ? "checked" : "";
    const dirClass = isDir ? "dir" : "";

    rows += `
      <tr class="browser-row ${dirClass}" draggable="true" data-path="${escapeHtml(fullPath)}" data-is-dir="${isDir}" data-index="${i}">
        <td class="cb-cell"><input type="checkbox" class="browser-cb" ${checked} /></td>
        <td class="icon-cell">${entryIcon(isDir)}</td>
        <td class="name-cell">${escapeHtml(name)}${isDir ? "/" : ""}</td>
        <td class="size-cell">${isDir ? "—" : (entry ? formatBytes(entry.size) : "—")}</td>
        <td class="compressed-cell">${(!isDir && entry && entry.compressed_size > 0) ? formatBytes(entry.compressed_size) : "—"}</td>
        <td class="modified-cell">${entry ? formatTimestamp(entry.modified) : "—"}</td>
        <td class="crc-cell">${(!isDir && entry) ? formatCrc32(entry.crc32) : "—"}</td>
      </tr>`;
  }

  panel.innerHTML = `
    <div class="browser-info">${escapeHtml(t("browser.info", { entryCount, itemCount: children.length }))}</div>
    <div class="table-scroll">
      <table class="result-table browser-table">
        <thead>
          <tr>
            <th class="cb-th"></th>
            <th class="icon-th"></th>
            <th>${escapeHtml(t("browser.table.name"))}</th>
            <th>${escapeHtml(t("browser.table.size"))}</th>
            <th>${escapeHtml(t("browser.table.compressed"))}</th>
            <th>${escapeHtml(t("browser.table.modified"))}</th>
            <th>${escapeHtml(t("browser.table.crc32"))}</th>
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
  const tbody = document.querySelector("#extract-result tbody");
  if (!tbody) return;

  // Checkbox change events
  tbody.querySelectorAll(".browser-cb").forEach((cb) => {
    cb.addEventListener("change", (e) => {
      const target = e.target as HTMLInputElement;
      const row = target.closest("tr") as HTMLElement;
      const path = row.dataset.path ?? "";
      if (target.checked) {
        extractSelectedEntries.add(path);
      } else {
        extractSelectedEntries.delete(path);
      }
      updateSelectionUI();
    });
  });

  // Row click events
  tbody.querySelectorAll<HTMLElement>(".browser-row").forEach((row) => {
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
        extractCurrentDir = path.endsWith("/") ? path : path + "/";
        extractSelectedEntries.clear();
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
      if (!path || !extractArchivePath) return;

      // Collect the selected entries, or just this row if nothing selected.
      const paths =
        extractSelectedEntries.size > 0
          ? [...extractSelectedEntries]
          : [path];

      // Show drag status.
      const dragStatus = el("extract-browser-drag-status");
      dragStatus.textContent = t("browser.dragStatus.preparing");
      dragStatus.classList.remove("drag-error");
      dragStatus.style.display = "";

      let tempId = "";
      try {
        // Extract entries to a temp directory.
        const tempDir = await prepareDragEntries(
          extractArchivePath,
          paths,
          extractPassword || undefined,
        );

        // Extract a short temp id from the returned path.
        tempId = tempDir.split("/").pop() ?? "";
        currentDragTempId = tempId;

        dragStatus.textContent = paths.length > 1
          ? t("browser.dragStatus.dragging", { count: paths.length })
          : t("browser.dragStatus.draggingSingle");

        // Dynamically import the Tauri drag plugin — no static import
        // so that preview/browser mode doesn't hard-fail.
        const { startDrag } = await import(
          "@crabnebula/tauri-plugin-drag"
        ).catch(() => {
          throw new Error("drag-plugin-unavailable");
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
          (payload: { result: string }) => {
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
          t("browser.dragStatus.unavailablePrefix") +
          (err instanceof Error ? err.message : String(err)) +
          t("browser.dragStatus.unavailableSuffix");
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
      const dragStatus = el("extract-browser-drag-status");
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
  lastPreviewState = {
    path,
    loading: true,
    result: null,
    error: null,
  };
  renderPreviewState();

  try {
    const result = await previewEntry(
      extractArchivePath,
      path,
      extractPassword || undefined,
    );
    lastPreviewState = {
      path: result.entry_path,
      loading: false,
      result,
      error: null,
    };
  } catch (e) {
    lastPreviewState = {
      path,
      loading: false,
      result: null,
      error: String(e),
    };
  }

  renderPreviewState();
}

// ---------------------------------------------------------------------------
// Archive Browser — selection & extraction
// ---------------------------------------------------------------------------

function beginExtractOperation(): number | null {
  if (!extractArchivePath || extractRunning) {
    return null;
  }

  extractRunning = true;
  extractToken += 1;
  updateSelectionUI();
  return extractToken;
}

function finishExtractOperation(token: number) {
  if (extractToken !== token) {
    return;
  }

  extractRunning = false;
  updateSelectionUI();
}

function isExtractOperationBlocked(): boolean {
  if (!extractRunning) {
    return false;
  }

  if (extractArchivePath) {
    el<HTMLInputElement>("extract-archive").value = extractArchivePath;
    renderArchiveBrowser();
  }
  el<HTMLInputElement>("extract-password").value = extractPassword;
  renderBrowserExtractError(t("browser.error.running"));
  return true;
}

function updateSelectionUI() {
  const count = el("extract-browser-selection-count");
  const extractAllBtn = el<HTMLButtonElement>("extract-browser-all");
  const extractSelectedBtn = el<HTMLButtonElement>("extract-browser-selected");
  const extractOutputInput = el<HTMLInputElement>("extract-output");
  const extractOutputBtn = el<HTMLButtonElement>("extract-output-btn");
  const listArchiveInput = el<HTMLInputElement>("extract-archive");
  const listArchiveBtn = el<HTMLButtonElement>("extract-archive-btn");
  const listRunBtn = el<HTMLButtonElement>("extract-run");
  const listPasswordInput = el<HTMLInputElement>("extract-password");

  const hasArchive = extractArchivePath.trim().length > 0;
  const selectedCount = extractSelectedEntries.size;
  count.textContent = extractRunning
    ? selectedCount > 0
      ? t("browser.selection.extractingSelected", { count: selectedCount })
      : t("browser.selection.extracting")
    : selectedCount > 0
      ? t("browser.selection.selected", { count: selectedCount })
      : "";
  extractAllBtn.disabled = !hasArchive || extractRunning;
  extractSelectedBtn.disabled = !hasArchive || extractRunning || selectedCount === 0;
  extractOutputInput.disabled = !hasArchive || extractRunning;
  extractOutputBtn.disabled = !hasArchive || extractRunning;
  listArchiveInput.disabled = extractRunning;
  listArchiveBtn.disabled = extractRunning;
  listRunBtn.disabled = extractRunning;
  listPasswordInput.disabled = extractRunning;
}

function buildExtractErrorsHtml(errors: ExtractArchiveResult["errors"]): string {
  if (errors.length === 0) {
    return "";
  }

  return `
    <div style="margin-top:0.5rem">
      <p style="color:var(--red);font-size:0.8rem">${escapeHtml(t("browser.feedback.perFileErrors"))}</p>
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

function clearBrowserOperationFeedback(resetState = true) {
  if (resetState) {
    lastBrowserOperationState = null;
  }
  el("extract-result")
    .querySelectorAll(".browser-operation-feedback")
    .forEach((node) => node.remove());
}

function renderBrowserExtractFeedback(
  titleKey: "browser.feedback.extractSuccessAll" | "browser.feedback.extractSuccessSelected",
  outputDir: string,
  result: ExtractArchiveResult,
  recordRecent = true,
) {
  clearBrowserOperationFeedback();
  lastBrowserOperationState = { kind: "success", titleKey, outputDir, result };

  const feedback = document.createElement("div");
  feedback.className = "browser-operation-feedback";
  feedback.innerHTML = `
    <div class="success-message">${escapeHtml(t(titleKey))}</div>
    <div class="result-summary" style="margin-top:0.75rem">
      ${summaryItem("browser.feedback.filesExtracted", String(result.files_extracted))}
      ${summaryItem("browser.feedback.bytesExtracted", formatBytes(result.bytes_extracted))}
      ${summaryItem("browser.feedback.skipped", String(result.files_skipped))}
    </div>
    ${buildExtractErrorsHtml(result.errors)}
    <div class="result-footer">
      <button class="btn-reveal" data-path="${escapeHtml(outputDir)}" data-action="reveal">
        \u{1F4C2} ${escapeHtml(t("common.revealInFolder"))}
      </button>
      <button class="btn-reveal" data-path="${escapeHtml(outputDir)}" data-action="open">
        \u{1F4C1} ${escapeHtml(t("common.openFolder"))}
      </button>
    </div>`;

  el("extract-result").appendChild(feedback);
  if (recordRecent) addRecent(outputDir);
  wireRevealButtons(feedback);
}

function renderBrowserExtractError(msg: string) {
  clearBrowserOperationFeedback();
  lastBrowserOperationState = { kind: "error", message: msg };

  const feedback = document.createElement("div");
  feedback.className = "browser-operation-feedback";
  feedback.innerHTML = `<div class="error-message">${escapeHtml(msg)}</div>`;
  el("extract-result").appendChild(feedback);
}

async function runExtractAll() {
  if (!extractArchivePath) {
    renderBrowserExtractError(t("browser.error.openFirst"));
    return;
  }

  setExtractArchiveForm(extractArchivePath, extractPassword);
  const outputInput = el<HTMLInputElement>("extract-output");
  if (!outputInput.value.trim()) {
    outputInput.value = inferOutputDir(extractArchivePath);
  }

  const outputDir = outputInput.value.trim();
  if (!outputDir) {
    renderBrowserExtractError(t("browser.error.outputUnknown"));
    return;
  }

  const token = beginExtractOperation();
  if (token === null) {
    return;
  }

  const archivePath = extractArchivePath;
  const password = extractPassword;
  const overwrite = el<HTMLInputElement>("extract-overwrite").checked;
  el<HTMLInputElement>("extract-overwrite").checked = overwrite;

  const extractAllBtn = el<HTMLButtonElement>("extract-browser-all");
  const taskId = `task-extract-all-${Date.now()}`;
  let terminalStatus: TerminalTaskStatus | null = null;
  let terminalMessage: string | undefined;

  bindTaskProgress("extract", taskId, "browser.progress.preparingAll");
  extractAllBtn.textContent = t("browser.selection.extracting");
  clearBrowserOperationFeedback();

  try {
    const result = await extractArchive(
      archivePath,
      outputDir,
      overwrite,
      password || undefined,
      taskId,
    );
    terminalStatus = "finished";

    if (extractToken !== token || extractArchivePath !== archivePath) {
      return;
    }

    renderBrowserExtractFeedback("browser.feedback.extractSuccessAll", outputDir, result);
  } catch (e) {
    const msg = String(e);
    terminalStatus = msg.toLowerCase().includes("cancelled") ? "cancelled" : "failed";
    terminalMessage = terminalStatus === "cancelled" ? undefined : msg;
    if (extractToken === token) {
      renderBrowserExtractError(msg);
    }
  } finally {
    extractAllBtn.textContent = t("browser.extractAll");
    if (terminalStatus) {
      settleTaskProgressFallback("extract", taskId, terminalStatus, terminalMessage);
    }
    finishExtractOperation(token);
  }
}

async function extractSelected() {
  if (!extractArchivePath || extractSelectedEntries.size === 0) return;

  const token = beginExtractOperation();
  if (token === null) {
    return;
  }

  const archivePath = extractArchivePath;
  const password = extractPassword;
  const entryPaths = Array.from(extractSelectedEntries);
  const overwrite = el<HTMLInputElement>("extract-overwrite").checked;
  const taskId = `task-extract-entries-${Date.now()}`;
  const extractSelectedBtn = el<HTMLButtonElement>("extract-browser-selected");
  let terminalStatus: TerminalTaskStatus | null = null;
  let terminalMessage: string | undefined;

  extractSelectedBtn.textContent = t("dialog.chooseOutput");

  try {
    const outputDir = await pickDirectory();
    if (!outputDir) {
      return;
    }

    if (!extractRunning || extractToken !== token || extractArchivePath !== archivePath) {
      return;
    }

    setExtractArchiveForm(archivePath, password);
    el<HTMLInputElement>("extract-overwrite").checked = overwrite;

    bindTaskProgress("extract", taskId, "browser.progress.preparingSelected");
    extractSelectedBtn.textContent = t("browser.selection.extracting");
    clearBrowserOperationFeedback();

    const result = await extractEntries(
      archivePath,
      entryPaths,
      outputDir,
      overwrite,
      password || undefined,
      taskId,
    );
    terminalStatus = "finished";

    if (extractToken !== token || extractArchivePath !== archivePath) {
      return;
    }

    renderBrowserExtractFeedback("browser.feedback.extractSuccessSelected", outputDir, result);
  } catch (e) {
    const msg = String(e);
    terminalStatus = msg.toLowerCase().includes("cancelled") ? "cancelled" : "failed";
    terminalMessage = terminalStatus === "cancelled" ? undefined : msg;
    if (extractToken === token) {
      renderBrowserExtractError(msg);
    }
  } finally {
    extractSelectedBtn.textContent = t("browser.extractSelected");
    if (terminalStatus) {
      settleTaskProgressFallback("extract", taskId, terminalStatus, terminalMessage);
    }
    finishExtractOperation(token);
  }
}

// ---------------------------------------------------------------------------
// Result display functions (for other modes)
// ---------------------------------------------------------------------------

function renderTestResult(result: TestArchiveResult) {
  lastTestResult = result;
  el("test-result").innerHTML = `
    <div class="result-summary">
      ${summaryItem("test.result.format", escapeHtml(result.format))}
      ${summaryItem("test.result.entries", String(result.entry_count))}
      ${summaryItem("test.result.bytesRead", formatBytes(result.bytes_read))}
      ${summaryItem(
        "test.result.crc32",
        escapeHtml(t(result.crc32_verified ? "test.result.verified" : "test.result.notVerified")),
        `value ${result.crc32_verified ? "success" : "fail"}`,
      )}
    </div>
    <p style="margin-top:0.5rem;color:var(--text-muted);font-size:0.78rem;">
      ${escapeHtml(t(result.crc32_verified ? "test.result.passed" : "test.result.completed"))}
    </p>
  `;
}

function renderCompressResult(result: CompressArchiveResult, recordRecent = true) {
  lastCompressResult = result;
  const outputPath = result.output_path;
  el("compress-result").innerHTML = `
    <div class="success-message">${escapeHtml(t("compress.result.success"))}</div>
    <div class="result-summary" style="margin-top:0.75rem">
      ${summaryItem("compress.result.filesAdded", String(result.files_added))}
      ${summaryItem("compress.result.directories", String(result.directories_added))}
      ${summaryItem("compress.result.bytesWritten", formatBytes(result.bytes_written))}
      ${summaryItem("compress.result.skipped", String(result.skipped))}
    </div>
    <p style="margin-top:0.5rem;font-size:0.82rem;">
      ${escapeHtml(t("compress.result.format"))}: <strong>${escapeHtml(result.format)}</strong><br />
      ${escapeHtml(t("compress.result.output"))}: <code style="color:var(--text-muted)">${escapeHtml(outputPath)}</code>
    </p>
    <div class="result-footer">
      <button class="btn-reveal" data-path="${escapeHtml(outputPath)}" data-action="reveal">
        \u{1F4C2} ${escapeHtml(t("common.revealInFolder"))}
      </button>
      <button class="btn-reveal" data-path="${escapeHtml(getParentDir(outputPath))}" data-action="open">
        \u{1F4C1} ${escapeHtml(t("common.openFolder"))}
      </button>
    </div>
  `;
  if (recordRecent) addRecent(outputPath);
  wireRevealButtons("compress-result");
}

function renderExtractResult(result: ExtractArchiveResult, recordRecent = true) {
  lastExtractResult = result;
  const outputDir = el<HTMLInputElement>("extract-output").value.trim();
  const errorsHtml = buildExtractErrorsHtml(result.errors);

  el("extract-result").innerHTML = `
    <div class="success-message">${escapeHtml(t("extract.result.success"))}</div>
    <div class="result-summary" style="margin-top:0.75rem">
      ${summaryItem("extract.result.filesExtracted", String(result.files_extracted))}
      ${summaryItem("extract.result.bytesExtracted", formatBytes(result.bytes_extracted))}
      ${summaryItem("extract.result.skipped", String(result.files_skipped))}
    </div>
    ${errorsHtml}
    ${outputDir ? `
    <div class="result-footer">
      <button class="btn-reveal" data-path="${escapeHtml(outputDir)}" data-action="reveal">
        \u{1F4C2} ${escapeHtml(t("common.revealInFolder"))}
      </button>
      <button class="btn-reveal" data-path="${escapeHtml(outputDir)}" data-action="open">
        \u{1F4C1} ${escapeHtml(t("common.openFolder"))}
      </button>
    </div>` : ""}
  `;
  if (recordRecent && outputDir) addRecent(outputDir);
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
  el(`${mode}-result`).innerHTML = `<div class="error-message">${escapeHtml(t("common.cancelled"))}</div>`;
}

function syncModeTitle(mode: string = currentMode) {
  const titleText = t(`page.${mode}`);
  const titleEl = document.getElementById("page-title");
  if (titleEl) {
    titleEl.textContent = titleText;
  }
  document.title = mode === "home"
    ? t("app.title")
    : `${titleText} · ${t("app.title")}`;
}

function syncLocaleControl() {
  const localeSelect = document.getElementById("settings-locale") as HTMLSelectElement | null;
  if (localeSelect) {
    localeSelect.value = getLocale();
  }
}

function rerenderLocalizedUi() {
  applyI18n();
  syncLocaleControl();
  syncModeTitle();
  renderRecentChips();
  rerenderProgressPanels();

  if (lastCompressResult) {
    renderCompressResult(lastCompressResult, false);
  }
  if (lastExtractResult) {
    renderExtractResult(lastExtractResult, false);
  }
  if (lastTestResult) {
    renderTestResult(lastTestResult);
  }

  if (extractArchivePath) {
    renderArchiveBrowser();
    if (lastBrowserOperationState?.kind === "success") {
      renderBrowserExtractFeedback(
        lastBrowserOperationState.titleKey,
        lastBrowserOperationState.outputDir,
        lastBrowserOperationState.result,
        false,
      );
    } else if (lastBrowserOperationState?.kind === "error") {
      renderBrowserExtractError(lastBrowserOperationState.message);
    }
    if (lastPreviewState) {
      renderPreviewState();
    }
  }

  updateSelectionUI();
}

// ---------------------------------------------------------------------------
// Mode switching
// ---------------------------------------------------------------------------

function switchMode(mode: string) {
  currentMode = mode;
  document.querySelectorAll(".panel").forEach((p) => p.classList.remove("active"));
  document.querySelectorAll(".nav-item").forEach((n) => {
    n.classList.remove("active");
  });

  const panel = document.getElementById(`panel-${mode}`);
  if (panel) {
    panel.classList.add("active");
  }

  const navItem = document.querySelector(`.nav-item[data-mode="${mode}"]`);
  if (navItem) {
    navItem.classList.add("active");
  }

  syncModeTitle(mode);

  if (mode !== "extract") {
    closePreview();
  }
}

// ---------------------------------------------------------------------------
// Dialog helpers
// ---------------------------------------------------------------------------

async function pickFiles(): Promise<string[]> {
  const result = await open({ multiple: true, title: t("dialog.selectFiles") });
  if (!result) return [];
  return Array.isArray(result) ? result : [result];
}

async function pickDirectory(): Promise<string | null> {
  return await open({ directory: true, multiple: false, title: t("dialog.selectDirectory") });
}

async function pickSaveFile(defaultName?: string): Promise<string | null> {
  return await save({ title: t("dialog.saveArchiveAs"), defaultPath: defaultName });
}

async function pickSingleFile(): Promise<string | null> {
  const result = await open({ multiple: false, title: t("dialog.selectArchiveFile") });
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
  if (dt.files && dt.files.length > 0) {
    for (let i = 0; i < dt.files.length; i++) {
      files.push(dt.files[i]);
    }
  }
  if (files.length === 0) return;

  const paths: string[] = [];
  for (const f of files) {
    if ((f as any).path) {
      paths.push((f as any).path as string);
    }
  }

  if (paths.length === 0) {
    const targetId = currentMode === "extract"
      ? "extract-result"
      : currentMode === "test"
        ? "test-result"
        : "compress-result";
    showError(targetId, t("preview.dropUnsupported"));
    return;
  }

  const archives = paths.filter((p) => isArchiveExt(p));
  const nonArchives = paths.filter((p) => !isArchiveExt(p));

  if (archives.length > 0 && nonArchives.length === 0) {
    switchMode("extract");
    el<HTMLInputElement>("extract-archive").value = archives[0];
    el<HTMLInputElement>("extract-password").value = "";

    if (archives.length === 1) {
      el("extract-result").innerHTML =
        `<div class="running-text"><span class="running-spinner"></span> ${escapeHtml(t("list.opening"))}</div>`;
    } else {
      el("extract-result").innerHTML =
        `<div class="info-message">${escapeHtml(t("list.multipleDropped", {
          count: archives.length,
          name: getBasename(archives[0]),
        }))}</div>`;
    }

    setTimeout(() => openExtractArchive(archives[0]), 50);
    return;
  }

  if (nonArchives.length > 0) {
    switchMode("compress");
    const input = el<HTMLInputElement>("compress-sources");
    input.dataset.paths = paths.join("\n");
    input.value = paths.length === 1 ? paths[0] : t("common.filesSelected", { count: paths.length });
    updateCompressSourceChips(paths);
    const format = el<HTMLSelectElement>("compress-format").value;
    el<HTMLInputElement>("compress-output").value = inferOutputPath(paths, format);
    el("compress-result").innerHTML =
      `<div class="result-empty">${escapeHtml(t("compress.configuredDropped"))}</div>`;
  }
}

// ---------------------------------------------------------------------------
// Run handlers
// ---------------------------------------------------------------------------

async function runCompress() {
  const sources = (el<HTMLInputElement>("compress-sources").dataset.paths ?? "")
    .split("\n")
    .filter(Boolean);
  if (sources.length === 0) {
    showError("compress-result", t("compress.validation.sources"));
    return;
  }
  const outputPath = el<HTMLInputElement>("compress-output").value.trim();
  if (!outputPath) {
    showError("compress-result", t("compress.validation.output"));
    return;
  }
  const format = el<HTMLSelectElement>("compress-format").value;
  const levelRaw = el<HTMLInputElement>("compress-level").value.trim();
  const jobsRaw = el<HTMLInputElement>("compress-jobs").value.trim();
  const password = el<HTMLInputElement>("compress-password").value.trim() || undefined;
  const level = levelRaw ? parseInt(levelRaw, 10) : undefined;
  const jobs = jobsRaw ? parseInt(jobsRaw, 10) : undefined;
  const taskId = `task-${Date.now()}`;
  let terminalStatus: TerminalTaskStatus | null = null;
  let terminalMessage: string | undefined;

  lastCompressResult = null;
  bindTaskProgress("compress", taskId, "compress.progress.preparing");
  setRunning("compress", true);
  try {
    const result = await compressArchive(sources, outputPath, format, level, jobs, password, taskId);
    terminalStatus = "finished";
    renderCompressResult(result);
  } catch (e) {
    const msg = String(e);
    terminalStatus = msg.toLowerCase().includes("cancelled") ? "cancelled" : "failed";
    terminalMessage = terminalStatus === "cancelled" ? undefined : msg;
    lastCompressResult = null;
    if (terminalStatus === "cancelled") {
      renderCancelNotice("compress");
    } else {
      showError("compress-result", msg);
    }
  } finally {
    if (terminalStatus) {
      settleTaskProgressFallback("compress", taskId, terminalStatus, terminalMessage);
    }
    setRunning("compress", false);
  }
}

async function runExtract() {
  const archivePath = el<HTMLInputElement>("extract-archive").value.trim();
  if (!archivePath) {
    showError("extract-result", t("extract.validation.archive"));
    return;
  }
  const outputDir = el<HTMLInputElement>("extract-output").value.trim();
  if (!outputDir) {
    showError("extract-result", t("extract.validation.output"));
    return;
  }
  const overwrite = el<HTMLInputElement>("extract-overwrite").checked;
  const password = el<HTMLInputElement>("extract-password").value.trim() || undefined;
  const taskId = `task-${Date.now()}`;
  let terminalStatus: TerminalTaskStatus | null = null;
  let terminalMessage: string | undefined;

  lastExtractResult = null;
  bindTaskProgress("extract", taskId, "extract.progress.preparing");
  setRunning("extract", true);
  try {
    const result = await extractArchive(archivePath, outputDir, overwrite, password, taskId);
    terminalStatus = "finished";
    renderExtractResult(result);
  } catch (e) {
    const msg = String(e);
    terminalStatus = msg.toLowerCase().includes("cancelled") ? "cancelled" : "failed";
    terminalMessage = terminalStatus === "cancelled" ? undefined : msg;
    lastExtractResult = null;
    if (terminalStatus === "cancelled") {
      renderCancelNotice("extract");
    } else {
      showError("extract-result", msg);
    }
  } finally {
    if (terminalStatus) {
      settleTaskProgressFallback("extract", taskId, terminalStatus, terminalMessage);
    }
    setRunning("extract", false);
  }
}


/** Run list with an explicit archive path (from drop, recent chip, opened-archives, etc.) */
async function openExtractArchive(archivePath: string) {
  if (isExtractOperationBlocked()) {
    return;
  }

  el<HTMLInputElement>("extract-archive").value = archivePath;
  const password = el<HTMLInputElement>("extract-password").value.trim() || undefined;

  extractEntriesList = [];
  extractArchivePath = "";
  extractPassword = password ?? "";
  extractCurrentDir = "";
  extractSelectedEntries.clear();
  lastBrowserOperationState = null;
  lastPreviewState = null;
  updateSelectionUI();
  resetTaskProgress("extract");
  el("extract-browser-bar").style.display = "none";
  closePreview();
  el("extract-result").innerHTML =
    `<div class="running-text"><span class="running-spinner"></span> ${escapeHtml(t("list.running"))}</div>`;

  try {
    const entries = await listArchive(archivePath, password);
    extractEntriesList = entries;
    extractArchivePath = archivePath;
    extractPassword = password ?? "";
    extractCurrentDir = "";
    extractSelectedEntries.clear();

    setExtractArchiveForm(archivePath, extractPassword);
    setExtractOutputDir(archivePath);

    addRecent(archivePath);
    renderArchiveBrowser();
  } catch (e) {
    extractArchivePath = "";
    showError("extract-result", String(e));
    updateSelectionUI();
  } finally {
  }
}


async function runTest() {
  const archivePath = el<HTMLInputElement>("test-archive").value.trim();
  if (!archivePath) {
    showError("test-result", t("test.validation.archive"));
    return;
  }
  const password = el<HTMLInputElement>("test-password").value.trim() || undefined;

  const runButton = el<HTMLButtonElement>("test-run");
  runButton.disabled = true;
  lastTestResult = null;
  el("test-result").innerHTML =
    `<div class="running-text"><span class="running-spinner"></span> ${escapeHtml(t("test.running"))}</div>`;
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
  const progressMode = mode as ProgressPanel;
  const taskId = runningTaskIds[progressMode];
  if (!taskId) return;
  try {
    await cancelTask(taskId);
  } catch (e) {
    console.warn("cancelTask error:", e);
  }
}

// ---------------------------------------------------------------------------
// Initialization
// ---------------------------------------------------------------------------

window.addEventListener("DOMContentLoaded", async () => {
  applyI18n();
  syncLocaleControl();

  // --- Load formats for Compress mode ---
  const fallbackFormats: FormatInfo[] = [
    { name: "zip", can_compress: true, can_decompress: true },
    { name: "zipx", can_compress: true, can_decompress: true },
    { name: "tar", can_compress: true, can_decompress: true },
    { name: "7z", can_compress: true, can_decompress: true },
    { name: "tar.gz", can_compress: true, can_decompress: true },
    { name: "tar.bz2", can_compress: true, can_decompress: true },
    { name: "tar.br", can_compress: true, can_decompress: true },
    { name: "tar.lz4", can_compress: true, can_decompress: true },
    { name: "tar.zst", can_compress: true, can_decompress: true },
    { name: "tar.xz", can_compress: true, can_decompress: true },
  ];
  try {
    const formats = await getFormats();
    compressFormats = formats.filter((f) => f.can_compress);
  } catch (e) {
    console.error("Failed to load formats, using fallback:", e);
    compressFormats = fallbackFormats;
  }

  if (compressFormats.length === 0) {
    compressFormats = fallbackFormats;
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

  resetTaskProgress("compress");
  resetTaskProgress("extract");
  resetTaskProgress("extract");
  updateSelectionUI();

  window.addEventListener(LOCALE_CHANGED_EVENT, () => {
    rerenderLocalizedUi();
  });

  try {
    const unlisten = await listen<TaskProgressPayload>("task:progress", (event: { payload: TaskProgressPayload }) => {
      updateTaskProgress(event.payload);
    });
    unlistenTaskProgress = unlisten;
  } catch (e) {
    console.debug("listen for task:progress skipped (browser preview):", e);
  }

  // --- Sidebar navigation ---
  document.querySelectorAll(".nav-item").forEach((item) => {
    item.addEventListener("click", () => {
      const mode = (item as HTMLElement).dataset.mode;
      if (mode) switchMode(mode);
    });
  });

  el<HTMLSelectElement>("settings-locale").addEventListener("change", (event) => {
    const value = (event.currentTarget as HTMLSelectElement).value;
    if (value === "zh-CN" || value === "en") {
      setLocale(value as Locale);
    }
  });

  // -----------------------------------------------------------------------
  // Compress
  // -----------------------------------------------------------------------

  el("compress-source-btn").addEventListener("click", async () => {
    const paths = await pickFiles();
    if (paths.length === 0) return;
    const input = el<HTMLInputElement>("compress-sources");
    input.dataset.paths = paths.join("\n");
    input.value = paths.length === 1 ? paths[0] : t("common.filesSelected", { count: paths.length });
    updateCompressSourceChips(paths);
    const format = el<HTMLSelectElement>("compress-format").value;
    el<HTMLInputElement>("compress-output").value = inferOutputPath(paths, format);
    el("compress-result").innerHTML =
      `<div class="result-empty">${escapeHtml(t("compress.configured"))}</div>`;
  });

  el("compress-output-btn").addEventListener("click", async () => {
    const sources = (el<HTMLInputElement>("compress-sources").dataset.paths ?? "")
      .split("\n")
      .filter(Boolean);
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

  el<HTMLSelectElement>("compress-format").addEventListener("change", () => {
    const sources = (el<HTMLInputElement>("compress-sources").dataset.paths ?? "")
      .split("\n")
      .filter(Boolean);
    if (sources.length > 0) {
      el<HTMLInputElement>("compress-output").value = inferOutputPath(
        sources,
        el<HTMLSelectElement>("compress-format").value,
      );
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
      `<div class="result-empty">${escapeHtml(t("extract.result.selected"))}</div>`;
  });

  el("extract-output-btn").addEventListener("click", async () => {
    const path = await pickDirectory();
    if (!path) return;
    el<HTMLInputElement>("extract-output").value = path;
  });

  el("extract-run").addEventListener("click", runExtract);
  el("extract-cancel").addEventListener("click", () => handleCancel("extract"));

  el("extract-archive").addEventListener("change", () => {
    const path = el<HTMLInputElement>("extract-archive").value.trim();
    if (path) {
      el<HTMLInputElement>("extract-output").value = inferOutputDir(path);
    }
  });

  // -----------------------------------------------------------------------
  // List — archive browser
  // -----------------------------------------------------------------------

  el("extract-archive-btn").addEventListener("click", async () => {
    const path = await pickSingleFile();
    if (!path) return;
    el<HTMLInputElement>("extract-archive").value = path;
  });

  el("extract-output-btn").addEventListener("click", async () => {
    const path = await pickDirectory();
    if (!path) return;
    el<HTMLInputElement>("extract-output").value = path;
  });

  el("extract-browser-all").addEventListener("click", runExtractAll);
  el("extract-browser-selected").addEventListener("click", extractSelected);
  el("extract-preview-close").addEventListener("click", () => {
    closePreview();
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

  // Start on home page
  switchMode("home");
  syncLocaleControl();

  // -----------------------------------------------------------------------
  // Opened archives (cold start + hot start via file association)
  // -----------------------------------------------------------------------

  try {
    const pending = await getOpenedArchives();
    if (pending.length > 0) {
      const firstPath = pending[0];
      switchMode("extract");
      el<HTMLInputElement>("extract-archive").value = firstPath;
      el<HTMLInputElement>("extract-password").value = "";
      el("extract-result").innerHTML =
        `<div class="running-text"><span class="running-spinner"></span> ${escapeHtml(t("list.openingFromAssociation"))}</div>`;
      setTimeout(() => openExtractArchive(firstPath), 100);
    }
  } catch (e) {
    console.debug("getOpenedArchives skipped (browser preview):", e);
  }

  try {
    const unlisten = await listen<string[]>("opened-archives", (event: { payload: string[] }) => {
      const paths = event.payload;
      if (paths.length > 0) {
        const firstPath = paths[0];
        switchMode("extract");
        el<HTMLInputElement>("extract-archive").value = firstPath;
        el<HTMLInputElement>("extract-password").value = "";
        el("extract-result").innerHTML =
          `<div class="running-text"><span class="running-spinner"></span> ${escapeHtml(t("list.opening"))}</div>`;
        openExtractArchive(firstPath);
      }
    });
    unlistenOpenedArchives = unlisten;
  } catch (e) {
    console.debug("listen for opened-archives skipped (browser preview):", e);
  }

  window.addEventListener("beforeunload", () => {
    if (unlistenOpenedArchives) {
      unlistenOpenedArchives();
      unlistenOpenedArchives = null;
    }
    if (unlistenTaskProgress) {
      unlistenTaskProgress();
      unlistenTaskProgress = null;
    }
  });
});
