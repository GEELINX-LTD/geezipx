/// GeeZipX GUI — Bridge types and invoke wrappers.
///
/// Import these types and functions in frontend code instead of calling
/// `invoke` directly, so the argument shapes stay in sync with the
/// Rust backend.

import { invoke } from "@tauri-apps/api/core";

// ---------------------------------------------------------------------------
// Types (mirrors of Rust #[derive(Serialize)] structs)
// ---------------------------------------------------------------------------

/** Information about a single archive/compression format. */
export interface FormatInfo {
  name: string;
  can_compress: boolean;
  can_decompress: boolean;
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
 * Supported formats: zip, tar, tar.gz, tar.zst, tar.xz.
 * Single-stream formats (gzip, zstd, xz, lzma) will be rejected with
 * a clear error message.
 */
export async function compressArchive(
  sourcePaths: string[],
  outputPath: string,
  format: string,
  level?: number,
  jobs?: number,
  password?: string,
  taskId?: string,
): Promise<CompressArchiveResult> {
  return invoke<CompressArchiveResult>("compress_archive", {
    sourcePaths,
    outputPath,
    format,
    level: level ?? null,
    jobs: jobs ?? null,
    password: password ?? null,
    taskId: taskId ?? null,
  });
}
