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
  el(`${mode}-run`).style.display = running ? "none" : "";
  el(`${mode}-cancel`).style.display = running ? "" : "none";
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
          <td>${e.is_dir ? "—" : formatBytes(e.size)}</td>
          <td>${e.compressed_size > 0 ? formatBytes(e.compressed_size) : "—"}</td>
          <td>${e.is_dir ? "—" : e.is_dir ? "—" : "—"}</td>
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
      Output: <code style="color:var(--text-muted)">${escapeHtml(result.output_path)}</code>
    </p>
  `;
}

function renderExtractResult(result: ExtractArchiveResult) {
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
  `;
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
  document.querySelectorAll(".mode-tab").forEach((t) => t.classList.remove("active"));

  // Show target panel and activate tab
  el(`panel-${mode}`).classList.add("active");
  document.querySelector(`.mode-tab[data-mode="${mode}"]`)?.classList.add("active");
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
    // The command will eventually return with "cancelled" error,
    // which updates the result panel via the catch block in run*.
  } catch (e) {
    // cancelTask itself may error if the task already completed.
    console.warn("cancelTask error:", e);
  }
}

// ---------------------------------------------------------------------------
// Initialization
// ---------------------------------------------------------------------------

window.addEventListener("DOMContentLoaded", async () => {
  // --- Load formats for Compress mode ---
  try {
    compressFormats = (await getFormats()).filter((f) => f.can_compress);
    const select = el<HTMLSelectElement>("compress-format");
    for (const fmt of compressFormats) {
      const opt = document.createElement("option");
      opt.value = fmt.name;
      opt.textContent = fmt.name;
      select.appendChild(opt);
    }
  } catch (e) {
    console.error("Failed to load formats:", e);
  }

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
    if (paths.length === 1) {
      input.value = paths[0];
    } else {
      input.value = `${paths.length} files selected`;
    }
  });

  el("compress-output-btn").addEventListener("click", async () => {
    const path = await pickSaveFile();
    if (!path) return;
    el<HTMLInputElement>("compress-output").value = path;
  });

  el("compress-run").addEventListener("click", runCompress);
  el("compress-cancel").addEventListener("click", () => handleCancel("compress"));

  // -----------------------------------------------------------------------
  // Extract
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

  el("extract-run").addEventListener("click", runExtract);
  el("extract-cancel").addEventListener("click", () => handleCancel("extract"));

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
