/// GeeZipX GUI — Main UI logic.
///
/// Wire up the tabs, file dialogs, run/cancel buttons, and result display
/// for the four modes: Compress, Extract, List, Test.

import {
  getFormats,
  listArchive,
  testArchive,
  compressArchive,
  extractArchive,
  cancelTask,
  revealFile,
  openFolder,
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
        // Switch to Extract, fill archive path, auto-infer output dir
        switchMode("extract");
        el<HTMLInputElement>("extract-archive").value = path;
        el<HTMLInputElement>("extract-output").value = inferOutputDir(path);
        el("extract-result").innerHTML =
          `<div class="result-empty">Ready to extract. Click Extract to start.</div>`;
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
// Result display functions
// ---------------------------------------------------------------------------

function renderListResult(entries: EntryInfo[]) {
  const panel = el("list-result");
  if (entries.length === 0) {
    panel.innerHTML = `<div class="success-message">Archive is empty (0 entries).</div>`;
    return;
  }

  let rows = entries
    .map(
      (e) =>
        `<tr class="${e.is_dir ? "dir" : ""}">
          <td>${escapeHtml(e.path)}${e.is_dir ? "/" : ""}</td>
          <td>${e.is_dir ? "\u2014" : formatBytes(e.size)}</td>
          <td>${e.compressed_size > 0 ? formatBytes(e.compressed_size) : "\u2014"}</td>
          <td>${e.is_dir ? "\u2014" : "\u2014"}</td>
        </tr>`,
    )
    .join("");

  panel.innerHTML = `
    <h3>Archive Contents (${entries.length} entries)</h3>
    <div class="table-scroll">
      <table class="result-table">
        <thead>
          <tr>
            <th>Path</th>
            <th>Size</th>
            <th>Compressed</th>
            <th>Type</th>
          </tr>
        </thead>
        <tbody>${rows}</tbody>
      </table>
    </div>
    <p style="margin-top:0.5rem;color:var(--text-muted);font-size:0.78rem;">
      Directories shown in <span style="color:var(--orange)">orange</span>.
    </p>
  `;
}

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
  let errorsHtml = "";
  if (result.errors.length > 0) {
    errorsHtml =
      `<div style="margin-top:0.5rem">
        <p style="color:var(--red);font-size:0.8rem">Per-file errors:</p>
        <ul style="padding-left:1.2rem;font-size:0.8rem;color:var(--text-muted)">
          ${result.errors
            .map(
              (e) =>
                `<li>${escapeHtml(e.path)}: ${escapeHtml(e.message)}</li>`,
            )
            .join("")}
        </ul>
      </div>`;
  }

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

function wireRevealButtons(containerId: string) {
  const container = el(containerId);
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
    // All archives — switch to Extract
    if (archives.length === 1) {
      switchMode("extract");
      el<HTMLInputElement>("extract-archive").value = archives[0];
      el<HTMLInputElement>("extract-output").value = inferOutputDir(archives[0]);
      el("extract-result").innerHTML =
        `<div class="result-empty">Ready to extract. Click Extract to start.</div>`;
    } else {
      // Multiple archives — extract first one, mention the rest
      switchMode("extract");
      el<HTMLInputElement>("extract-archive").value = archives[0];
      el<HTMLInputElement>("extract-output").value = inferOutputDir(archives[0]);
      el("extract-result").innerHTML =
        `<div class="info-message">${archives.length} archives dropped. Using first: ${escapeHtml(getBasename(archives[0]))}. Drop individually for others.</div>`;
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

async function runList() {
  const archivePath = el<HTMLInputElement>("list-archive").value.trim();
  if (!archivePath) {
    showError("list-result", "Please select an archive file.");
    return;
  }
  const password = el<HTMLInputElement>("list-password").value.trim() || undefined;

  const runButton = el<HTMLButtonElement>("list-run");
  runButton.disabled = true;
  el("list-result").innerHTML = `<div class="running-text"><span class="running-spinner"></span> Listing contents...</div>`;
  try {
    const entries = await listArchive(archivePath, password);
    renderListResult(entries);
  } catch (e) {
    showError("list-result", String(e));
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
  // List
  // -----------------------------------------------------------------------

  el("list-archive-btn").addEventListener("click", async () => {
    const path = await pickSingleFile();
    if (!path) return;
    el<HTMLInputElement>("list-archive").value = path;
  });

  el("list-run").addEventListener("click", runList);

  // -----------------------------------------------------------------------
  // Test
  // -----------------------------------------------------------------------

  el("test-archive-btn").addEventListener("click", async () => {
    const path = await pickSingleFile();
    if (!path) return;
    el<HTMLInputElement>("test-archive").value = path;
  });

  el("test-run").addEventListener("click", runTest);
});
