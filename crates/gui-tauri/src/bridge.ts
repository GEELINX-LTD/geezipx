/// GeeZipX GUI — Bridge types and invoke wrappers.
///
/// Import these types and functions in frontend code instead of calling
/// `invoke` directly, so the argument shapes stay in sync with the
/// Rust backend.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { revealItemInDir, openPath } from "@tauri-apps/plugin-opener";

// ---------------------------------------------------------------------------
// Types (mirrors of Rust #[derive(Serialize)] structs)
// ---------------------------------------------------------------------------

/** One bindable archive format returned by `get_file_associations`. */
export interface AssocItem {
  ext: string;
  exts: string[];
  mime: string;
  name: string;
  description: string;
  /** Whether GeeZipX is the OS default handler. `null` = OS does not expose it (Windows). */
  is_default: boolean | null;
  /** Whether GeeZipX is registered as a handler at all. */
  is_registered: boolean;
}

/** Result of `get_file_associations`. */
export interface AssociationsResult {
  platform: string;
  /** Whether toggling can make GeeZipX the OS default directly (false on Windows). */
  can_set_default: boolean;
  items: AssocItem[];
}

/** Information about a single archive/compression format. */
export interface FormatInfo {
  name: string;
  can_compress: boolean;
  can_decompress: boolean;
  supports_encryption: boolean;
  level_hint: string | null;
}

/** A single entry inside an archive (serialized for the frontend). */
export interface EntryInfo {
  path: string;
  size: number;
  compressed_size: number;
  crc32: number | null;
  modified: number | null;
  is_dir: boolean;
}

/** Result of an archive integrity test. */
export interface TestArchiveResult {
  format: string;
  entry_count: number;
  bytes_read: number;
  crc32_verified: boolean;
}

/** Result of previewing a single entry inside an archive. */
export interface PreviewResult {
  entry_path: string;
  kind: string;
  size_hint: string;
  content: string;
  total_size: number;
  truncated: boolean;
}

/** Progress event kind emitted by Rust backend tasks. */
export type TaskKind = "compress" | "extract";

/** Lifecycle status emitted for a running task. */
export type TaskStatus = "started" | "progress" | "finished" | "cancelled" | "failed";

/** High-level stage currently being performed. */
export type TaskStage =
  | "scanning"
  | "compressing"
  | "extracting"
  | "finalizing"
  | "completed"
  | "cancelled"
  | "failed";

/** Low-level I/O phase, if known. */
export type TaskPhase = "reading" | "writing" | "hashing";

/** Payload of the `task:progress` event emitted by Rust commands. */
export interface TaskProgressPayload {
  task_id: string;
  kind: TaskKind;
  status: TaskStatus;
  stage: TaskStage;
  phase: TaskPhase | null;
  message: string;
  current: number;
  total: number | null;
  percent: number | null;
  bytes_per_second: number | null;
  current_entry: string | null;
  completed_entries: number;
  total_entries: number | null;
}

/** Full settings structure persisted via tauri-plugin-store. */
export interface GeeZipXSettings {
  locale: 'en' | 'zh-CN';
  default_output_dir: string | null;
  overwrite_strategy: 'prompt' | 'skip' | 'overwrite';
  default_format: string;
  default_level: number | null;
  recursive: boolean;
  theme: 'system' | 'light' | 'dark';
  /** Behavior after a compress/extract task finishes. */
  on_complete: 'nothing' | 'open_output';
  /** Default password used to pre-fill encryption / extraction fields. */
  default_password: string | null;
  /** Whether to persist `default_password` in the settings store. */
  remember_password: boolean;
}

// ---------------------------------------------------------------------------
// Wrapper functions
// ---------------------------------------------------------------------------

/** Fetch the list of all supported formats from the engine. */
export async function getFormats(): Promise<FormatInfo[]> {
  return invoke<FormatInfo[]>("get_formats");
}

/** List entries inside an archive. */
export async function listArchive(
  archivePath: string,
  password?: string,
): Promise<EntryInfo[]> {
  return invoke<EntryInfo[]>("list_archive", {
    archivePath,
    password: password ?? null,
  });
}

/** Verify the integrity of an archive. */
export async function testArchive(
  archivePath: string,
  password?: string,
): Promise<TestArchiveResult> {
  return invoke<TestArchiveResult>("test_archive", {
    archivePath,
    password: password ?? null,
  });
}

/** Per-file error information. */
export interface ExtractErrorInfo {
  path: string;
  message: string;
}

/** Result of an extraction operation. */
export interface ExtractArchiveResult {
  files_extracted: number;
  bytes_extracted: number;
  files_skipped: number;
  errors: ExtractErrorInfo[];
}

/** Extract all entries from an archive to a directory. */
export async function extractArchive(
  archivePath: string,
  outputDir: string,
  overwrite: boolean,
  password?: string,
  taskId?: string,
): Promise<ExtractArchiveResult> {
  return invoke<ExtractArchiveResult>("extract_archive", {
    archivePath,
    outputDir,
    overwrite,
    password: password ?? null,
    taskId: taskId ?? null,
  });
}

/** Cancel a running task by its id. */
export async function cancelTask(taskId: string): Promise<void> {
  return invoke<void>("cancel_task", { taskId });
}

/** Retrieve pending archive paths received via file association / open-with. */
export async function getOpenedArchives(): Promise<string[]> {
  return invoke<string[]>("get_opened_archives");
}

/** Return the list of bindable archive formats and their current OS binding state. */
export async function getFileAssociations(): Promise<AssociationsResult> {
  return invoke<AssociationsResult>("get_file_associations");
}

/** Enable or disable GeeZipX as the handler/default for the given extension. */
export async function setFileAssociation(ext: string, enabled: boolean): Promise<void> {
  return invoke<void>("set_file_association", { ext, enabled });
}

/** Open the OS "Default Apps" settings page (mainly needed on Windows). */
export async function openAssociationSettings(ext?: string): Promise<void> {
  return invoke<void>("open_association_settings", { ext: ext ?? null });
}

/** Selectively extract specific entries from an archive. */
export async function extractEntries(
  archivePath: string,
  entryPaths: string[],
  outputDir: string,
  overwrite: boolean,
  password?: string,
  taskId?: string,
): Promise<ExtractArchiveResult> {
  return invoke<ExtractArchiveResult>("extract_entries", {
    archivePath,
    entryPaths,
    outputDir,
    overwrite,
    password: password ?? null,
    taskId: taskId ?? null,
  });
}

/** Preview a single entry inside an archive (text/binary/dir). */
export async function previewEntry(
  archivePath: string,
  entryPath: string,
  password?: string,
): Promise<PreviewResult> {
  return invoke<PreviewResult>("preview_entry", {
    archivePath,
    entryPath,
    password: password ?? null,
  });
}

// Re-export listen for frontend use
export { listen };

// ---------------------------------------------------------------------------
// Drag-out types and wrappers
// ---------------------------------------------------------------------------

/** Prepare selected archive entries for drag-out by extracting to a temp directory.
 * Returns the absolute path to the temp directory containing the extracted files. */
export async function prepareDragEntries(
  archivePath: string,
  entryPaths: string[],
  password?: string,
): Promise<string> {
  return invoke<string>("prepare_drag_entries", {
    archivePath,
    entryPaths,
    password: password ?? null,
  });
}

/** Clean up a specific drag-out temp directory after drag completes/cancels. */
export async function cleanupDragTempDir(tempId: string): Promise<void> {
  return invoke<void>("cleanup_drag_temp_dir", { tempId });
}

/** Clean up all stale drag-out temp directories (older than 24h). */
export async function cleanupStaleDragTempDirs(): Promise<number> {
  return invoke<number>("cleanup_stale_drag_temp_dirs");
}
export type { UnlistenFn };

// ---------------------------------------------------------------------------
// Compress types and wrapper
// ---------------------------------------------------------------------------

/** Result of a compression operation. */
export interface CompressArchiveResult {
  files_added: number;
  directories_added: number;
  bytes_written: number;
  output_path: string;
  format: string;
  skipped: number;
}

/**
 * Create an archive from source paths.
 *
 * Supported formats: zip, zipx, tar, 7z, tar.gz, tar.bz2, tar.br, tar.lz4, tar.zst, tar.xz.
 * Single-stream formats (gzip, bzip2, brotli, lz4, zstd, xz, lzma) will be rejected with
 * a clear error message.
 */
export async function compressArchive(
  sourcePaths: string[],
  outputPath: string,
  format: string,
  level?: number,
  jobs?: number,
  password?: string,
  recursive?: boolean,
  taskId?: string,
): Promise<CompressArchiveResult> {
  return invoke<CompressArchiveResult>("compress_archive", {
    sourcePaths,
    outputPath,
    format,
    level: level ?? null,
    jobs: jobs ?? null,
    password: password ?? null,
    recursive: recursive ?? true,
    taskId: taskId ?? null,
  });
}

/** Reveal a file in the file manager (e.g., Finder, Explorer, Nautilus).
 * Returns false if the opener plugin is unavailable (preview/development mode). */
export async function revealFile(path: string): Promise<boolean> {
  try {
    await revealItemInDir(path);
    return true;
  } catch {
    console.warn("revealItemInDir not available (running in browser?)", path);
    return false;
  }
}

/** Open a directory or file in the default system application.
 * Returns false if the opener plugin is unavailable. */
export async function openFolder(path: string): Promise<boolean> {
  try {
    await openPath(path);
    return true;
  } catch {
    console.warn("openPath not available (running in browser?)", path);
    return false;
  }
}
