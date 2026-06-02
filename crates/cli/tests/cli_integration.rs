//! Integration tests for the `geezipx` CLI binary.
//!
//! Uses `assert_cmd` and `predicates` for process assertions and `tempfile`
//! for temporary test directories.
#![allow(dead_code)]

use std::path::Path;
use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a `geezipx` command fixture (the project's own binary).
fn geezipx() -> Command {
    Command::cargo_bin("geezipx").expect("geezipx binary not found")
}

/// Create a temporary test helper.
struct TestDir {
    dir: tempfile::TempDir,
}

impl TestDir {
    fn new() -> Self {
        Self {
            dir: tempfile::TempDir::new().unwrap(),
        }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn join(&self, rel: &str) -> PathBuf {
        self.dir.path().join(rel)
    }

    fn write(&self, rel: &str, content: &str) {
        let p = self.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&p, content).unwrap();
    }

    fn exists(&self, rel: &str) -> bool {
        self.join(rel).exists()
    }

    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.join(rel)).unwrap()
    }
}

// ---------------------------------------------------------------------------
// Interop helpers
// ---------------------------------------------------------------------------

/// Check if a system tool is available on PATH.
fn tool_available(name: &str) -> bool {
    std::process::Command::new(if cfg!(windows) { "where" } else { "which" })
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Skip a test if a required tool is not installed; returns `false` when
/// the tool is missing (caller should early-return with `return`).
fn require_tool(name: &str) -> bool {
    if !tool_available(name) {
        eprintln!("skipping interop test: {name} not available on PATH");
        return false;
    }
    true
}

/// Assert that a byte slice contains no ANSI escape sequences.
fn assert_no_ansi_escape(output: &[u8]) {
    let s = String::from_utf8_lossy(output);
    assert!(
        !s.contains('\x1b'),
        "stderr should not contain ANSI escape codes"
    );
    assert!(
        !s.contains('\r'),
        "stderr should not contain carriage returns"
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn help_available() {
    geezipx()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("compress"))
        .stdout(predicate::str::contains("decompress"))
        .stdout(predicate::str::contains("list"));
}

#[test]
fn compress_help_available() {
    geezipx()
        .args(["compress", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--format"))
        .stdout(predicate::str::contains("--recursive"))
        .stdout(predicate::str::contains("--level"))
        .stdout(predicate::str::contains("--jobs"));
}

#[test]
fn decompress_help_available() {
    geezipx()
        .args(["decompress", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--stdout"));
}

#[test]
fn list_help_available() {
    geezipx()
        .args(["list", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--json"));
}

#[test]
fn gzip_stdout_roundtrip() {
    let tmp = TestDir::new();
    tmp.write(
        "hello.txt",
        "Hello, GeeZipX! Round-trip through gzip --stdout.",
    );
    let archive = tmp.join("hello.txt.gz");

    // Compress to gzip.
    geezipx()
        .args([
            "compress",
            tmp.join("hello.txt").to_str().unwrap(),
            "-f",
            "gz",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(archive.exists(), "gzip archive should exist");

    // Decompress with --stdout and verify content.
    geezipx()
        .args(["decompress", archive.to_str().unwrap(), "--stdout"])
        .assert()
        .success()
        .stdout("Hello, GeeZipX! Round-trip through gzip --stdout.");
}

#[test]
fn zip_compress_list_decompress() {
    let tmp = TestDir::new();

    tmp.write("test.txt", "ZIP round-trip test data.");
    let archive = tmp.join("output.zip");
    let output = tmp.join("extracted");

    // Compress.
    geezipx()
        .args([
            "compress",
            tmp.join("test.txt").to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(archive.exists(), "ZIP archive should exist");

    // List.
    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("test.txt"));

    // Decompress.
    std::fs::create_dir_all(&output).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();

    let extracted = output.join("test.txt");
    assert!(extracted.exists(), "extracted file should exist");
    assert_eq!(
        std::fs::read_to_string(&extracted).unwrap(),
        "ZIP round-trip test data."
    );
}

#[test]
fn targz_recursive_roundtrip() {
    let tmp = TestDir::new();
    let src = tmp.join("src");
    std::fs::create_dir_all(src.join("nested")).unwrap();

    std::fs::write(src.join("root.txt"), "root level").unwrap();
    std::fs::write(src.join("nested").join("deep.txt"), "nested level").unwrap();

    let archive = tmp.join("out.tar.gz");
    let output = tmp.join("extracted");

    // Recursive compress.
    geezipx()
        .args([
            "compress",
            src.to_str().unwrap(),
            "-r",
            "-f",
            "tar.gz",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(archive.exists(), "tar.gz archive should exist");

    // List.
    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("root.txt"))
        .stdout(predicate::str::contains("nested/deep.txt"));

    // Decompress.
    std::fs::create_dir_all(&output).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(output.join("src").join("root.txt").exists());
    assert!(output.join("src").join("nested").join("deep.txt").exists());
}

#[test]
fn list_json_output() {
    let tmp = TestDir::new();
    tmp.write("data.txt", "JSON list test");
    let archive = tmp.join("test.zip");

    geezipx()
        .args([
            "compress",
            tmp.join("data.txt").to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    // List with --json.
    geezipx()
        .args(["list", archive.to_str().unwrap(), "--json"])
        .assert()
        .success()
        .stdout(predicate::str::is_match(r#""path"\s*:"#).unwrap())
        .stdout(predicate::str::is_match(r#""size"\s*:"#).unwrap())
        .stdout(predicate::str::contains(r#""compression_ratio""#))
        .stdout(predicate::str::contains(r#""modified""#));
}

#[test]
fn unsupported_format_fails() {
    let tmp = TestDir::new();
    let unknown = tmp.join("data.xyz");
    std::fs::write(&unknown, "not an archive").unwrap();

    geezipx()
        .args(["list", unknown.to_str().unwrap()])
        .assert()
        .failure();
}

#[test]
fn compress_no_inputs_fails() {
    geezipx()
        .args(["compress", "-o", "out.zip"])
        .assert()
        .failure();
}

#[test]
fn compress_directory_without_recursive_fails() {
    let tmp = TestDir::new();
    let dir = tmp.join("mydir");
    std::fs::create_dir(&dir).unwrap();

    geezipx()
        .args([
            "compress",
            dir.to_str().unwrap(),
            "-o",
            tmp.join("out.zip").to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--recursive"));
}

#[test]
fn gzip_multiple_inputs_fails() {
    let tmp = TestDir::new();
    let f1 = tmp.join("a.txt");
    let f2 = tmp.join("b.txt");
    std::fs::write(&f1, "first").unwrap();
    std::fs::write(&f2, "second").unwrap();

    geezipx()
        .args([
            "compress",
            f1.to_str().unwrap(),
            f2.to_str().unwrap(),
            "-f",
            "gz",
            "-o",
            tmp.join("out.gz").to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("single input"));
}

#[test]
fn stdout_with_archive_fails() {
    let tmp = TestDir::new();
    tmp.write("test.txt", "archive test");
    let archive = tmp.join("test.zip");

    geezipx()
        .args([
            "compress",
            tmp.join("test.txt").to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    geezipx()
        .args(["decompress", archive.to_str().unwrap(), "--stdout"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--stdout"));
}

#[test]
fn tar_compress_list_decompress() {
    let tmp = TestDir::new();
    tmp.write("tar_test.txt", "TAR round-trip");
    let archive = tmp.join("out.tar");

    // Compress.
    geezipx()
        .args([
            "compress",
            tmp.join("tar_test.txt").to_str().unwrap(),
            "-f",
            "tar",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(archive.exists());

    // List.
    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("tar_test.txt"));

    // Decompress.
    let output = tmp.join("extracted_tar");
    std::fs::create_dir_all(&output).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert_eq!(
        std::fs::read_to_string(output.join("tar_test.txt")).unwrap(),
        "TAR round-trip"
    );
}

#[test]
fn gzip_decompress_to_file() {
    let tmp = TestDir::new();
    tmp.write("hello.txt", "Gzip file decompression test.");
    let archive = tmp.join("hello.txt.gz");

    // Compress.
    geezipx()
        .args([
            "compress",
            tmp.join("hello.txt").to_str().unwrap(),
            "-f",
            "gzip",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Decompress to output dir.
    let output = tmp.join("out_dir");
    std::fs::create_dir_all(&output).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();

    let decompressed = output.join("hello.txt");
    assert!(decompressed.exists());
    assert_eq!(
        std::fs::read_to_string(&decompressed).unwrap(),
        "Gzip file decompression test."
    );
}

// ---------------------------------------------------------------------------
// Additional tests: auto-format, gzip list, missing inputs, auto-create dir
// ---------------------------------------------------------------------------

#[test]
fn list_gzip_table_output() {
    let tmp = TestDir::new();
    tmp.write("hello.txt", "List gzip table test.");
    let archive = tmp.join("hello.txt.gz");

    geezipx()
        .args([
            "compress",
            tmp.join("hello.txt").to_str().unwrap(),
            "-f",
            "gz",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    // List in table mode should show the file name, ratio, and modified.
    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello.txt"))
        .stdout(predicate::str::contains("Ratio"))
        .stdout(predicate::str::contains("Modified"));
}

#[test]
fn list_gzip_json_output() {
    let tmp = TestDir::new();
    tmp.write("data.txt", "List gzip JSON test.");
    let archive = tmp.join("data.txt.gz");

    geezipx()
        .args([
            "compress",
            tmp.join("data.txt").to_str().unwrap(),
            "-f",
            "gz",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    // List with --json.
    geezipx()
        .args(["list", archive.to_str().unwrap(), "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""path":"#))
        .stdout(predicate::str::contains(r#""compression_ratio""#))
        .stdout(predicate::str::contains(r#""modified""#));
}

#[test]
fn compress_auto_format_zip_from_extension() {
    // Without --format, zip is inferred from .zip extension.
    let tmp = TestDir::new();
    tmp.write("hello.txt", "Auto-format zip.");
    let archive = tmp.join("out.zip");

    geezipx()
        .args([
            "compress",
            tmp.join("hello.txt").to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Decompress and verify.
    let out2 = tmp.join("extract");
    std::fs::create_dir(&out2).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            out2.to_str().unwrap(),
        ])
        .assert()
        .success();
}

#[test]
fn compress_auto_format_targz_from_extension() {
    // Without --format, tar.gz is inferred from .tar.gz extension.
    let tmp = TestDir::new();
    tmp.write("hello.txt", "Auto-format tar.gz.");
    let archive = tmp.join("out.tar.gz");

    geezipx()
        .args([
            "compress",
            tmp.join("hello.txt").to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Decompress and verify.
    let out2 = tmp.join("extract");
    std::fs::create_dir(&out2).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            out2.to_str().unwrap(),
        ])
        .assert()
        .success();
}

#[test]
fn decompress_auto_creates_output_dir() {
    // Output directory that doesn't exist yet should be auto-created.
    let tmp = TestDir::new();
    tmp.write("hello.txt", "Auto-dir test.");
    let archive = tmp.join("out.zip");

    geezipx()
        .args([
            "compress",
            tmp.join("hello.txt").to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    let out_dir = tmp.join("new-dir");
    assert!(!out_dir.exists());

    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            out_dir.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(out_dir.is_dir());
    assert!(out_dir.join("hello.txt").exists());
}

#[test]
fn compress_nonexistent_input_fails() {
    geezipx()
        .args(["compress", "nonexistent.txt", "-o", "out.zip"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not exist"));
}

#[test]
fn decompress_nonexistent_archive_fails() {
    geezipx()
        .args(["decompress", "nonexistent.zip"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not exist"));
}

#[test]
fn list_nonexistent_file_fails() {
    geezipx()
        .args(["list", "nonexistent.zip"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not exist"));
}

// ---------------------------------------------------------------------------
// No-clobber / force tests
// ---------------------------------------------------------------------------

#[test]
fn decompress_no_clobber_skips_existing_files() {
    let tmp = TestDir::new();
    tmp.write("data.txt", "Original content");
    let archive = tmp.join("archive.zip");

    // Compress.
    geezipx()
        .args([
            "compress",
            tmp.join("data.txt").to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    let out = tmp.join("out");
    std::fs::create_dir_all(&out).unwrap();

    // First decompress (creates data.txt).
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(out.join("data.txt").exists());
    assert_eq!(
        std::fs::read_to_string(out.join("data.txt")).unwrap(),
        "Original content"
    );

    // Modify the extracted file.
    std::fs::write(out.join("data.txt"), "Modified content").unwrap();

    // Decompress with --no-clobber: should NOT overwrite.
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
            "--no-clobber",
        ])
        .assert()
        .success();

    // File content should still be "Modified content" (not overwritten).
    assert_eq!(
        std::fs::read_to_string(out.join("data.txt")).unwrap(),
        "Modified content",
        "--no-clobber should not overwrite existing files"
    );
}

#[test]
fn decompress_force_overwrites_existing_files() {
    let tmp = TestDir::new();
    tmp.write("data.txt", "Original content");
    let archive = tmp.join("archive.zip");

    // Compress.
    geezipx()
        .args([
            "compress",
            tmp.join("data.txt").to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    let out = tmp.join("out");
    std::fs::create_dir_all(&out).unwrap();

    // First decompress.
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Modify the extracted file.
    std::fs::write(out.join("data.txt"), "Modified content").unwrap();

    // Decompress with --force: should overwrite.
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
            "--force",
        ])
        .assert()
        .success();

    // File content should be restored to original (overwritten).
    assert_eq!(
        std::fs::read_to_string(out.join("data.txt")).unwrap(),
        "Original content",
        "--force should overwrite existing files"
    );
}

#[test]
fn decompress_default_overwrites_existing_files() {
    // Default behavior (no --no-clobber or --force) should also overwrite.
    let tmp = TestDir::new();
    tmp.write("data.txt", "Original content");
    let archive = tmp.join("archive.zip");

    geezipx()
        .args([
            "compress",
            tmp.join("data.txt").to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    let out = tmp.join("out");
    std::fs::create_dir_all(&out).unwrap();

    // First decompress.
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Modify the extracted file.
    std::fs::write(out.join("data.txt"), "Modified content").unwrap();

    // Decompress again with no flags: default should overwrite.
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert_eq!(
        std::fs::read_to_string(out.join("data.txt")).unwrap(),
        "Original content",
        "default behavior should overwrite existing files"
    );
}

#[test]
fn decompress_no_clobber_and_force_mutually_exclusive() {
    let tmp = TestDir::new();
    tmp.write("dummy.txt", "test");
    let archive = tmp.join("archive.zip");

    geezipx()
        .args([
            "compress",
            tmp.join("dummy.txt").to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Both --no-clobber and --force should fail.
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            tmp.join("out").to_str().unwrap(),
            "--no-clobber",
            "--force",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn decompress_no_clobber_shows_in_help() {
    geezipx()
        .args(["decompress", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--no-clobber"))
        .stdout(predicate::str::contains("--force"));
}

// ---------------------------------------------------------------------------
// No-clobber — gzip single-stream
// ---------------------------------------------------------------------------

#[test]
fn gzip_decompress_no_clobber_skips_existing() {
    let tmp = TestDir::new();
    tmp.write("hello.txt", "Gzip no-clobber test.");
    let archive = tmp.join("hello.txt.gz");

    // Compress.
    geezipx()
        .args([
            "compress",
            tmp.join("hello.txt").to_str().unwrap(),
            "-f",
            "gzip",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    let output_dir = tmp.join("out");
    std::fs::create_dir_all(&output_dir).unwrap();

    // First decompress (creates hello.txt).
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            output_dir.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Modify the decompressed file.
    std::fs::write(output_dir.join("hello.txt"), "MODIFIED").unwrap();

    // Second decompress with --no-clobber should NOT overwrite.
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            output_dir.to_str().unwrap(),
            "--no-clobber",
        ])
        .assert()
        .success();

    assert_eq!(
        std::fs::read_to_string(output_dir.join("hello.txt")).unwrap(),
        "MODIFIED",
        "--no-clobber should not overwrite existing gzip output"
    );
}

// ---------------------------------------------------------------------------
// No-clobber — mixed (some files exist, some don't)
// ---------------------------------------------------------------------------

#[test]
fn decompress_archive_no_clobber_mixed() {
    let tmp = TestDir::new();
    tmp.write("exists.txt", "This file exists before decompress");
    tmp.write("new.txt", "This file is new");
    let archive = tmp.join("archive.zip");

    // Compress.
    geezipx()
        .args([
            "compress",
            tmp.join("exists.txt").to_str().unwrap(),
            tmp.join("new.txt").to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    let out = tmp.join("out");
    std::fs::create_dir_all(&out).unwrap();

    // Pre-create "exists.txt" in output dir (but NOT "new.txt").
    std::fs::write(out.join("exists.txt"), "Pre-existing content").unwrap();

    // Decompress with --no-clobber.
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
            "--no-clobber",
        ])
        .assert()
        .success();

    // Pre-existing file should keep its content (not overwritten).
    assert_eq!(
        std::fs::read_to_string(out.join("exists.txt")).unwrap(),
        "Pre-existing content",
        "existing file should be preserved when --no-clobber"
    );

    // New file should have been extracted.
    assert_eq!(
        std::fs::read_to_string(out.join("new.txt")).unwrap(),
        "This file is new",
        "new file should be extracted even with --no-clobber"
    );
}

// ---------------------------------------------------------------------------
// Progress and verbosity tests
// ---------------------------------------------------------------------------

#[test]
fn compress_no_progress_no_escape_codes() {
    let tmp = TestDir::new();
    tmp.write("input.txt", "Test content for no-progress check.");
    let archive = tmp.join("output.gz");

    let output = geezipx()
        .args([
            "compress",
            tmp.join("input.txt").to_str().unwrap(),
            "-f",
            "gz",
            "-o",
            archive.to_str().unwrap(),
            "--no-progress",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_no_ansi_escape(&output.stderr);
}

#[test]
fn compress_verbose_prints_filenames() {
    let tmp = TestDir::new();
    tmp.write("input.txt", "Verbose compress test.");
    let archive = tmp.join("verbose_output.tar");

    geezipx()
        .args([
            "compress",
            tmp.join("input.txt").to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
            "-v",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("input.txt"));
}

#[test]
fn decompress_no_progress_no_escape_codes() {
    let tmp = TestDir::new();
    tmp.write("data.txt", "Decompress no-progress test.");
    let archive = tmp.join("data.zip");

    // First compress.
    geezipx()
        .args([
            "compress",
            tmp.join("data.txt").to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    let out_dir = tmp.join("out");
    std::fs::create_dir_all(&out_dir).unwrap();

    // Decompress with --no-progress.
    let output = geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            out_dir.to_str().unwrap(),
            "--no-progress",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_no_ansi_escape(&output.stderr);
}

#[test]
fn decompress_verbose_logs_info() {
    let tmp = TestDir::new();
    tmp.write("hello.txt", "Verbose decompress test.");
    let archive = tmp.join("verbose_decompress.tar");

    // First compress.
    geezipx()
        .args([
            "compress",
            tmp.join("hello.txt").to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    let out_dir = tmp.join("extracted");
    std::fs::create_dir_all(&out_dir).unwrap();

    // Decompress with -v.
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            out_dir.to_str().unwrap(),
            "-v",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("verbose_decompress.tar"));
}

#[test]
fn compress_piped_no_progress() {
    let tmp = TestDir::new();
    tmp.write("data.txt", "Piped no-progress test.");
    let archive = tmp.join("piped_data.tar.gz");

    // Compress with --no-progress.
    geezipx()
        .args([
            "compress",
            tmp.join("data.txt").to_str().unwrap(),
            "-f",
            "tar.gz",
            "-o",
            archive.to_str().unwrap(),
            "--no-progress",
        ])
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// M3-5: Format interop tests (against native system tools)
// ---------------------------------------------------------------------------

#[test]
fn interop_geezipx_zip_validates_with_unzip() {
    if !require_tool("unzip") {
        return;
    }
    let tmp = TestDir::new();
    let content = "Hello from GeeZipX ZIP — verifying with unzip.";
    tmp.write("hello.txt", content);
    let archive = tmp.join("test.zip");

    geezipx()
        .args([
            "compress",
            tmp.join("hello.txt").to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
            "--no-progress",
        ])
        .assert()
        .success();

    let output = std::process::Command::new("unzip")
        .args(["-t", archive.to_str().unwrap()])
        .output()
        .expect("unzip should execute");

    assert!(
        output.status.success(),
        "unzip -t should pass: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn interop_native_zip_decompresses_with_geezipx() {
    if !require_tool("zip") {
        return;
    }
    let tmp = TestDir::new();
    let content = "Native zip round-trip via GeeZipX.";
    tmp.write("hello.txt", content);
    let archive = tmp.join("native.zip");

    let status = std::process::Command::new("zip")
        .args([
            "-j",
            archive.to_str().unwrap(),
            tmp.join("hello.txt").to_str().unwrap(),
        ])
        .status()
        .expect("zip should execute");
    assert!(status.success(), "native zip should succeed");

    // GeeZipX list should show the file.
    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello.txt"));

    // GeeZipX decompress.
    let out = tmp.join("out");
    std::fs::create_dir_all(&out).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
            "--no-progress",
        ])
        .assert()
        .success();

    assert_eq!(
        std::fs::read_to_string(out.join("hello.txt")).unwrap(),
        content
    );
}

#[test]
fn interop_geezipx_tar_lists_with_native_tar() {
    if !require_tool("tar") {
        return;
    }
    let tmp = TestDir::new();
    let src = tmp.join("src");
    std::fs::create_dir_all(src.join("inner")).unwrap();
    std::fs::write(src.join("top.txt"), "top level").unwrap();
    std::fs::write(src.join("inner").join("deep.txt"), "nested").unwrap();

    let archive = tmp.join("out.tar");
    geezipx()
        .args([
            "compress",
            src.to_str().unwrap(),
            "-r",
            "-f",
            "tar",
            "-o",
            archive.to_str().unwrap(),
            "--no-progress",
        ])
        .assert()
        .success();

    let output = std::process::Command::new("tar")
        .args(["tf", archive.to_str().unwrap()])
        .output()
        .expect("tar should execute");
    assert!(output.status.success(), "tar tf should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("top.txt"),
        "tar tf output should contain top.txt: {stdout}"
    );
    assert!(
        stdout.contains("deep.txt") || stdout.contains("inner/deep.txt"),
        "tar tf output should contain deep.txt: {stdout}"
    );
}

#[test]
fn interop_native_tar_decompresses_with_geezipx() {
    if !require_tool("tar") {
        return;
    }
    let tmp = TestDir::new();
    let input = tmp.join("input");
    std::fs::create_dir_all(input.join("nested")).unwrap();
    std::fs::write(input.join("hello.txt"), "Hello from native tar").unwrap();
    std::fs::write(input.join("nested").join("world.txt"), "World").unwrap();

    let archive = tmp.join("native.tar");
    let status = std::process::Command::new("tar")
        .args([
            "cf",
            archive.to_str().unwrap(),
            "-C",
            input.to_str().unwrap(),
            ".",
        ])
        .status()
        .expect("tar should execute");
    assert!(status.success(), "native tar cf should succeed");

    let out = tmp.join("out");
    std::fs::create_dir_all(&out).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
            "--no-progress",
        ])
        .assert()
        .success();

    assert!(
        out.join("hello.txt").exists(),
        "hello.txt should exist in output"
    );
    assert!(
        out.join("nested").join("world.txt").exists(),
        "nested/world.txt should exist in output"
    );
}

#[test]
fn interop_geezipx_targz_lists_with_native_tar() {
    if !require_tool("tar") {
        return;
    }
    let tmp = TestDir::new();
    let src = tmp.join("src");
    std::fs::create_dir_all(src.join("sub")).unwrap();
    std::fs::write(src.join("a.txt"), "file a").unwrap();
    std::fs::write(src.join("sub").join("b.txt"), "file b").unwrap();

    let archive = tmp.join("archive.tar.gz");
    geezipx()
        .args([
            "compress",
            src.to_str().unwrap(),
            "-r",
            "-f",
            "tar.gz",
            "-o",
            archive.to_str().unwrap(),
            "--no-progress",
        ])
        .assert()
        .success();

    let output = std::process::Command::new("tar")
        .args(["tzf", archive.to_str().unwrap()])
        .output()
        .expect("tar should execute");
    assert!(output.status.success(), "tar tzf should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("a.txt"),
        "tar tzf output should contain a.txt: {stdout}"
    );
    assert!(
        stdout.contains("b.txt") || stdout.contains("sub/b.txt"),
        "tar tzf output should contain b.txt: {stdout}"
    );
}

#[test]
fn interop_native_targz_decompresses_with_geezipx() {
    if !require_tool("tar") {
        return;
    }
    let tmp = TestDir::new();
    let dir = tmp.join("mydir");
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("hello.txt"), "Hello from native tar.gz").unwrap();
    std::fs::write(dir.join("sub").join("deep.txt"), "Deep content").unwrap();

    let archive = tmp.join("native.tar.gz");
    let status = std::process::Command::new("tar")
        .args([
            "czf",
            archive.to_str().unwrap(),
            "-C",
            dir.to_str().unwrap(),
            ".",
        ])
        .status()
        .expect("tar should execute");
    assert!(status.success(), "native tar czf should succeed");

    let out = tmp.join("out");
    std::fs::create_dir_all(&out).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
            "--no-progress",
        ])
        .assert()
        .success();

    assert!(
        out.join("hello.txt").exists(),
        "hello.txt should exist in output"
    );
    assert!(
        out.join("sub").join("deep.txt").exists(),
        "sub/deep.txt should exist in output"
    );
}

#[test]
fn interop_geezipx_gzip_decompresses_with_native_gzip() {
    if !require_tool("gzip") {
        return;
    }
    let tmp = TestDir::new();
    let content = "GeeZipX-compressed data for native gzip -dc verification.";
    tmp.write("data.txt", content);

    let archive = tmp.join("data.gz");
    geezipx()
        .args([
            "compress",
            tmp.join("data.txt").to_str().unwrap(),
            "-f",
            "gz",
            "-o",
            archive.to_str().unwrap(),
            "--no-progress",
        ])
        .assert()
        .success();

    let output = std::process::Command::new("gzip")
        .args(["-dc", archive.to_str().unwrap()])
        .output()
        .expect("gzip should execute");
    assert!(output.status.success(), "native gzip -dc should succeed");
    assert_eq!(output.stdout, content.as_bytes());
}

#[test]
fn interop_native_gzip_decompresses_with_geezipx_stdout() {
    if !require_tool("gzip") {
        return;
    }
    let tmp = TestDir::new();
    let content = "Native gzip round-trip via GeeZipX --stdout.\n";
    tmp.write("input.txt", content);

    // Create native .gz file.
    let native_gz = tmp.join("native.gz");
    let output = std::process::Command::new("gzip")
        .args(["-c", tmp.join("input.txt").to_str().unwrap()])
        .output()
        .expect("gzip should execute");
    assert!(output.status.success(), "native gzip -c should succeed");
    std::fs::write(&native_gz, &output.stdout).unwrap();

    // GeeZipX decompress with --stdout.
    geezipx()
        .args([
            "decompress",
            native_gz.to_str().unwrap(),
            "--stdout",
            "--no-progress",
        ])
        .assert()
        .success()
        .stdout(content);
}

// ---------------------------------------------------------------------------
// Compression level tests (--level)
// ---------------------------------------------------------------------------

#[test]
fn gzip_level_9_success() {
    // Compress with --level 9, verify archive valid and decompress works.
    let tmp = TestDir::new();
    let content = "Level 9 gzip compression test.";
    tmp.write("test.txt", content);
    let archive = tmp.join("test.txt.gz");

    geezipx()
        .args([
            "compress",
            tmp.join("test.txt").to_str().unwrap(),
            "-f",
            "gz",
            "-L",
            "9",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(archive.exists(), "gzip archive with --level 9 should exist");

    // Decompress with --stdout and verify content.
    geezipx()
        .args(["decompress", archive.to_str().unwrap(), "--stdout"])
        .assert()
        .success()
        .stdout(content);
}

#[test]
fn gzip_level_0_success() {
    // Compress with --level 0 (store only, no compression).
    let tmp = TestDir::new();
    let content = "Level 0 gzip compression test.";
    tmp.write("test.txt", content);
    let archive = tmp.join("test.txt.gz");

    geezipx()
        .args([
            "compress",
            tmp.join("test.txt").to_str().unwrap(),
            "-f",
            "gz",
            "-L",
            "0",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(archive.exists(), "gzip archive with --level 0 should exist");

    // Decompress and verify.
    geezipx()
        .args(["decompress", archive.to_str().unwrap(), "--stdout"])
        .assert()
        .success()
        .stdout(content);
}

#[test]
fn targz_level_9_success() {
    let tmp = TestDir::new();
    tmp.write("hello.txt", "tar.gz with level 9");
    let archive = tmp.join("out.tar.gz");
    let out_dir = tmp.join("extracted");

    // Compress with --level 9.
    geezipx()
        .args([
            "compress",
            tmp.join("hello.txt").to_str().unwrap(),
            "-f",
            "tar.gz",
            "-L",
            "9",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(archive.exists(), "tar.gz with --level 9 should exist");

    // Decompress and verify.
    std::fs::create_dir_all(&out_dir).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            out_dir.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert_eq!(
        std::fs::read_to_string(out_dir.join("hello.txt")).unwrap(),
        "tar.gz with level 9"
    );
}

#[test]
fn zip_accepts_level_no_error() {
    // zip accepts --level but ignores it; should not error.
    let tmp = TestDir::new();
    tmp.write("test.txt", "zip with level");
    let archive = tmp.join("out.zip");

    geezipx()
        .args([
            "compress",
            tmp.join("test.txt").to_str().unwrap(),
            "-f",
            "zip",
            "-L",
            "5",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(archive.exists(), "zip with --level 5 should exist");
}

#[test]
fn tar_accepts_level_no_error() {
    // tar accepts --level but ignores it; should not error.
    let tmp = TestDir::new();
    tmp.write("test.txt", "tar with level");
    let archive = tmp.join("out.tar");

    geezipx()
        .args([
            "compress",
            tmp.join("test.txt").to_str().unwrap(),
            "-f",
            "tar",
            "-L",
            "5",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(archive.exists(), "tar with --level 5 should exist");
}

#[test]
fn compress_level_out_of_range_rejected() {
    // --level 23 should be rejected by clap validation (range 0..=22).
    let tmp = TestDir::new();
    tmp.write("test.txt", "level out of range");

    geezipx()
        .args([
            "compress",
            tmp.join("test.txt").to_str().unwrap(),
            "-f",
            "gz",
            "-L",
            "23",
            "-o",
            tmp.join("out.gz").to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("0..=22"));
}

#[test]
fn compress_level_negative_rejected() {
    // --level -1 should be rejected by clap (can't parse negative as u32).
    let tmp = TestDir::new();
    tmp.write("test.txt", "negative level");

    geezipx()
        .args([
            "compress",
            tmp.join("test.txt").to_str().unwrap(),
            "-f",
            "gz",
            "-L",
            "-1",
            "-o",
            tmp.join("out.gz").to_str().unwrap(),
        ])
        .assert()
        .failure();
}

#[test]
fn gzip_level_9_with_verbose() {
    // --level 9 combined with --verbose should work.
    let tmp = TestDir::new();
    let content = "Level 9 with verbose.";
    tmp.write("test.txt", content);
    let archive = tmp.join("test.txt.gz");

    geezipx()
        .args([
            "compress",
            tmp.join("test.txt").to_str().unwrap(),
            "-f",
            "gz",
            "-L",
            "9",
            "-o",
            archive.to_str().unwrap(),
            "-v",
        ])
        .assert()
        .success();

    // Decompress and verify content.
    geezipx()
        .args(["decompress", archive.to_str().unwrap(), "--stdout"])
        .assert()
        .success()
        .stdout(content);
}

#[test]
fn gzip_level_10_rejected_with_runtime_error() {
    // --level 10 passes clap validation (range 0..=22) but should be rejected
    // at runtime by gzip-specific level validation (gzip only supports 0..=9).
    let tmp = TestDir::new();
    tmp.write("test.txt", "level 10 gzip runtime reject");

    geezipx()
        .args([
            "compress",
            tmp.join("test.txt").to_str().unwrap(),
            "-f",
            "gz",
            "-L",
            "10",
            "-o",
            tmp.join("out.gz").to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "gzip compression level must be 0..=9",
        ));
}

// ---------------------------------------------------------------------------
// Completions tests
// ---------------------------------------------------------------------------

#[test]
fn completions_bash_success() {
    geezipx()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("_geezipx"))
        .stdout(predicate::str::contains("compress"))
        .stdout(predicate::str::contains("decompress"))
        .stdout(predicate::str::contains("list"));
}

#[test]
fn completions_zsh_success() {
    geezipx()
        .args(["completions", "zsh"])
        .assert()
        .success()
        .stdout(predicate::str::contains("_geezipx"))
        .stdout(predicate::str::contains("compress"))
        .stdout(predicate::str::contains("decompress"));
}

#[test]
fn completions_fish_success() {
    geezipx()
        .args(["completions", "fish"])
        .assert()
        .success()
        .stdout(predicate::str::contains("complete"))
        .stdout(predicate::str::contains("compress"))
        .stdout(predicate::str::contains("decompress"));
}

#[test]
fn completions_visible_alias_comp() {
    // The `comp` alias should work the same as `completions`.
    geezipx()
        .args(["comp", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("_geezipx"));
}

#[test]
fn completions_invalid_shell_fails() {
    geezipx()
        .args(["completions", "invalid_shell"])
        .assert()
        .failure();
}

#[test]
fn completions_powershell_success() {
    geezipx()
        .args(["completions", "powershell"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Register-ArgumentCompleter"));
}

#[test]
fn completions_elvish_success() {
    geezipx()
        .args(["completions", "elvish"])
        .assert()
        .success()
        .stdout(predicate::str::contains("edit:completion:arg-completer"))
        .stdout(predicate::str::contains("compress"))
        .stdout(predicate::str::contains("decompress"))
        .stdout(predicate::str::contains("list"));
}

// ---------------------------------------------------------------------------
// Zstandard (zst) tests
// ---------------------------------------------------------------------------

#[test]
fn zstd_roundtrip() {
    let tmp = TestDir::new();
    let content = "Hello, GeeZipX! Zstd round-trip test.";
    tmp.write("data.txt", content);
    let archive = tmp.join("data.txt.zst");

    // Compress to zstd (auto-detect by .zst extension).
    geezipx()
        .args([
            "compress",
            tmp.join("data.txt").to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(archive.exists(), "zstd archive should exist");

    // Decompress to output dir.
    let output_dir = tmp.join("out");
    std::fs::create_dir_all(&output_dir).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            output_dir.to_str().unwrap(),
        ])
        .assert()
        .success();

    let extracted = output_dir.join("data.txt");
    let actual = std::fs::read_to_string(&extracted).unwrap();
    assert_eq!(actual, content, "zstd round-trip content mismatch");
}

#[test]
fn zstd_stdout_roundtrip() {
    let tmp = TestDir::new();
    let content = "Hello, GeeZipX! Zstd --stdout round-trip.";
    tmp.write("hello.txt", content);
    let archive = tmp.join("hello.txt.zst");

    // Compress.
    geezipx()
        .args([
            "compress",
            tmp.join("hello.txt").to_str().unwrap(),
            "-f",
            "zstd",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(archive.exists(), "zstd archive should exist");

    // Decompress with --stdout.
    geezipx()
        .args(["decompress", archive.to_str().unwrap(), "--stdout"])
        .assert()
        .success()
        .stdout(content);
}

#[test]
fn zstd_explicit_format_roundtrip() {
    let tmp = TestDir::new();
    let content = "Zstd explicit -f zstd round-trip.";
    tmp.write("input.bin", content);
    let archive = tmp.join("out.zst");

    // Compress with -f zstd.
    geezipx()
        .args([
            "compress",
            tmp.join("input.bin").to_str().unwrap(),
            "-f",
            "zstd",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(archive.exists(), "zstd archive should exist");

    // Decompress to output dir.
    let output_dir = tmp.join("out");
    std::fs::create_dir_all(&output_dir).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            output_dir.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Output filename derived from archive name: out.zst -> out
    let extracted = output_dir.join("out");
    let actual = std::fs::read_to_string(&extracted).unwrap();
    assert_eq!(actual, content);
}

#[test]
fn zstd_multiple_inputs_fails() {
    let tmp = TestDir::new();
    let f1 = tmp.join("a.txt");
    let f2 = tmp.join("b.txt");
    std::fs::write(&f1, "first").unwrap();
    std::fs::write(&f2, "second").unwrap();

    geezipx()
        .args([
            "compress",
            f1.to_str().unwrap(),
            f2.to_str().unwrap(),
            "-f",
            "zstd",
            "-o",
            tmp.join("out.zst").to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("single"));
}

#[test]
fn list_zstd_table_output() {
    let tmp = TestDir::new();
    tmp.write("hello.txt", "List zstd table test.");
    let archive = tmp.join("hello.txt.zst");

    geezipx()
        .args([
            "compress",
            tmp.join("hello.txt").to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    // List in table mode.
    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello.txt"))
        .stdout(predicate::str::contains("Ratio"))
        .stdout(predicate::str::contains("Modified"));
}

#[test]
fn list_zstd_json_output() {
    let tmp = TestDir::new();
    tmp.write("data.txt", "List zstd JSON test.");
    let archive = tmp.join("data.txt.zst");

    geezipx()
        .args([
            "compress",
            tmp.join("data.txt").to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    // List with --json.
    geezipx()
        .args(["list", archive.to_str().unwrap(), "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""path":"#))
        .stdout(predicate::str::contains(r#""compression_ratio""#))
        .stdout(predicate::str::contains(r#""modified""#));
}

#[test]
fn zstd_compress_level_22() {
    let tmp = TestDir::new();
    let content = "Zstd compression at level 22.";
    tmp.write("test.txt", content);
    let archive = tmp.join("test.zst");

    geezipx()
        .args([
            "compress",
            tmp.join("test.txt").to_str().unwrap(),
            "-f",
            "zstd",
            "-L",
            "22",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(archive.exists(), "zstd level 22 archive should exist");

    // Decompress to output dir.
    let output_dir = tmp.join("out");
    std::fs::create_dir_all(&output_dir).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            output_dir.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Output filename derived from archive name: test.zst -> test
    let extracted = output_dir.join("test");
    let actual = std::fs::read_to_string(&extracted).unwrap();
    assert_eq!(actual, content);
}

// ---------------------------------------------------------------------------
// --jobs integration
// ---------------------------------------------------------------------------

#[test]
fn zstd_with_jobs_roundtrip() {
    let tmp = TestDir::new();
    let content = "hello from jobs test\n";
    tmp.write("jobs.txt", content);
    let archive = tmp.join("jobs.txt.zst");

    // Compress with --jobs 2
    geezipx()
        .args(["compress", "-j", "2", "-o"])
        .arg(&archive)
        .arg(tmp.join("jobs.txt"))
        .assert()
        .success();

    assert!(archive.is_file(), "compressed archive should exist");

    // Decompress back
    let out = tmp.join("out");
    std::fs::create_dir_all(&out).unwrap();
    geezipx()
        .args(["decompress", "-o"])
        .arg(&out)
        .arg(&archive)
        .assert()
        .success();

    let extracted = out.join("jobs.txt");
    assert!(
        extracted.is_file(),
        "decompressed file should exist: {:?}",
        extracted
    );
    let actual = std::fs::read_to_string(&extracted).unwrap();
    assert_eq!(actual, content);
}

#[test]
fn tarzst_with_jobs_roundtrip() {
    let tmp = TestDir::new();
    let content = "tar.zst with --jobs 2";
    tmp.write("jobs_tar.txt", content);
    let archive = tmp.join("out.tar.zst");

    // Compress tar.zst with --jobs 2 (exercises multithread path).
    geezipx()
        .args([
            "compress",
            tmp.join("jobs_tar.txt").to_str().unwrap(),
            "-f",
            "tar.zst",
            "-j",
            "2",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(archive.is_file(), "tar.zst archive should exist");

    // Decompress and verify.
    let out = tmp.join("extracted");
    std::fs::create_dir_all(&out).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
        ])
        .assert()
        .success();

    let extracted = out.join("jobs_tar.txt");
    assert!(extracted.is_file());
    assert_eq!(std::fs::read_to_string(&extracted).unwrap(), content);
}

#[test]
fn gzip_jobs_roundtrip() {
    let tmp = TestDir::new();
    let content = "gzip with --jobs 4 (should be silently ignored)";
    tmp.write("gzip_jobs.txt", content);
    let archive = tmp.join("gzip_jobs.txt.gz");

    // Compress gzip with --jobs 4 — gzip ignores jobs but should not fail.
    geezipx()
        .args([
            "compress",
            tmp.join("gzip_jobs.txt").to_str().unwrap(),
            "-f",
            "gz",
            "-j",
            "4",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(archive.is_file(), "gzip archive should exist");

    // Decompress and verify.
    let out = tmp.join("out");
    std::fs::create_dir_all(&out).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
        ])
        .assert()
        .success();

    let extracted = out.join("gzip_jobs.txt");
    assert!(extracted.is_file());
    assert_eq!(std::fs::read_to_string(&extracted).unwrap(), content);
}

#[test]
fn targz_jobs_roundtrip() {
    let tmp = TestDir::new();
    let content = "tar.gz with --jobs 4";
    tmp.write("targz_jobs.txt", content);
    let archive = tmp.join("out.tar.gz");

    // Compress tar.gz with --jobs 4 — tar.gz ignores jobs but should not fail.
    geezipx()
        .args([
            "compress",
            tmp.join("targz_jobs.txt").to_str().unwrap(),
            "-f",
            "tar.gz",
            "-j",
            "4",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(archive.is_file(), "tar.gz archive should exist");

    // Decompress and verify.
    let out = tmp.join("extracted");
    std::fs::create_dir_all(&out).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
        ])
        .assert()
        .success();

    let extracted = out.join("targz_jobs.txt");
    assert!(extracted.is_file());
    assert_eq!(std::fs::read_to_string(&extracted).unwrap(), content);
}

// ---------------------------------------------------------------------------
// TarZst (tar.zst / .tzst) integration tests
// ---------------------------------------------------------------------------

#[test]
fn tarzst_recursive_roundtrip() {
    let tmp = TestDir::new();
    let src = tmp.join("src");
    std::fs::create_dir_all(src.join("nested")).unwrap();

    std::fs::write(src.join("root.txt"), "root level").unwrap();
    std::fs::write(src.join("nested").join("deep.txt"), "nested level").unwrap();

    let archive = tmp.join("out.tar.zst");
    let output = tmp.join("extracted");

    // Recursive compress.
    geezipx()
        .args([
            "compress",
            src.to_str().unwrap(),
            "-r",
            "-f",
            "tar.zst",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(archive.exists(), "tar.zst archive should exist");

    // List.
    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("root.txt"))
        .stdout(predicate::str::contains("nested/deep.txt"));

    // Decompress.
    std::fs::create_dir_all(&output).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(output.join("src").join("root.txt").exists());
    assert!(output.join("src").join("nested").join("deep.txt").exists());
}

#[test]
fn tarzst_auto_format_from_tar_zst_extension() {
    // Without --format, tar.zst is inferred from .tar.zst extension.
    let tmp = TestDir::new();
    tmp.write("hello.txt", "Auto-format tar.zst.");
    let archive = tmp.join("out.tar.zst");

    geezipx()
        .args([
            "compress",
            tmp.join("hello.txt").to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(archive.exists(), "tar.zst archive should exist");

    // Decompress (auto-detect .tar.zst).
    let output = tmp.join("out2");
    std::fs::create_dir_all(&output).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();
}

#[test]
fn tarzst_auto_format_from_tzst_extension() {
    // .tzst extension should also auto-detect.
    let tmp = TestDir::new();
    tmp.write("data.txt", "Auto-format .tzst.");
    let archive = tmp.join("out.tzst");

    geezipx()
        .args([
            "compress",
            tmp.join("data.txt").to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(archive.exists(), "tzst archive should exist");

    let output = tmp.join("out2");
    std::fs::create_dir_all(&output).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();
}

#[test]
fn tarzst_explicit_format_roundtrip() {
    let tmp = TestDir::new();
    let content = "TarZst explicit -f tar.zst round-trip.";
    tmp.write("input.bin", content);
    // Use .tzst extension so decompress auto-detection works (decompress has no -f).
    let archive = tmp.join("out.tzst");

    // Compress with -f tar.zst (no extension-based inference).
    geezipx()
        .args([
            "compress",
            tmp.join("input.bin").to_str().unwrap(),
            "-f",
            "tar.zst",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(archive.exists(), "tzst archive should exist");

    // Decompress (auto-detected from .tzst extension).
    let output = tmp.join("out");
    std::fs::create_dir_all(&output).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();

    let extracted = output.join("input.bin");
    let actual = std::fs::read_to_string(&extracted).unwrap();
    assert_eq!(actual, content);
}

#[test]
fn tarzst_list_table_output() {
    let tmp = TestDir::new();
    tmp.write("hello.txt", "List tar.zst table test.");
    let archive = tmp.join("hello.tar.zst");

    geezipx()
        .args([
            "compress",
            tmp.join("hello.txt").to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    // List in table mode.
    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello.txt"))
        .stdout(predicate::str::contains("Ratio"))
        .stdout(predicate::str::contains("Modified"));
}

#[test]
fn tarzst_list_json_output() {
    let tmp = TestDir::new();
    tmp.write("data.txt", "List tar.zst JSON test.");
    let archive = tmp.join("data.tar.zst");

    geezipx()
        .args([
            "compress",
            tmp.join("data.txt").to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    // List with --json.
    geezipx()
        .args(["list", archive.to_str().unwrap(), "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""path":"#))
        .stdout(predicate::str::contains(r#""compression_ratio""#))
        .stdout(predicate::str::contains(r#""modified""#));
}

#[test]
fn tarzst_level_22() {
    let tmp = TestDir::new();
    let content = "TarZst compression at level 22.";
    tmp.write("test.txt", content);
    let archive = tmp.join("test.tar.zst");

    geezipx()
        .args([
            "compress",
            tmp.join("test.txt").to_str().unwrap(),
            "-f",
            "tar.zst",
            "-L",
            "22",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(archive.exists(), "tar.zst level 22 archive should exist");

    // Decompress and verify.
    let output_dir = tmp.join("out");
    std::fs::create_dir_all(&output_dir).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            output_dir.to_str().unwrap(),
        ])
        .assert()
        .success();

    let extracted = output_dir.join("test.txt");
    let actual = std::fs::read_to_string(&extracted).unwrap();
    assert_eq!(actual, content);
}

#[test]
fn tarzst_stdout_fails_for_archive_format() {
    // --stdout should be rejected for multi-file archive formats.
    let tmp = TestDir::new();
    tmp.write("data.txt", "stdout should fail");
    let archive = tmp.join("out.tar.zst");

    geezipx()
        .args([
            "compress",
            tmp.join("data.txt").to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    geezipx()
        .args(["decompress", archive.to_str().unwrap(), "--stdout"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--stdout"));
}

// ---------------------------------------------------------------------------
// TAR.XZ archive format tests
// ---------------------------------------------------------------------------

#[test]
fn tarxz_roundtrip() {
    let tmp = TestDir::new();
    tmp.write(
        "hello.txt",
        "Hello, GeeZipX! Round-trip through tar.xz compression.",
    );
    let archive = tmp.join("out.tar.xz");

    // Compress to tar.xz (auto-detect from .tar.xz extension).
    geezipx()
        .args([
            "compress",
            tmp.join("hello.txt").to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(archive.exists(), "tar.xz archive should exist");

    // List contents.
    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello.txt"));

    // Decompress.
    let output = tmp.join("out");
    std::fs::create_dir_all(&output).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(output.join("hello.txt").exists());
    assert_eq!(
        std::fs::read_to_string(output.join("hello.txt")).unwrap(),
        "Hello, GeeZipX! Round-trip through tar.xz compression."
    );
}

#[test]
fn tarxz_recursive_roundtrip() {
    let tmp = TestDir::new();
    let src = tmp.join("src");
    std::fs::create_dir_all(src.join("nested")).unwrap();

    std::fs::write(src.join("root.txt"), "root level").unwrap();
    std::fs::write(src.join("nested").join("deep.txt"), "nested level").unwrap();

    let archive = tmp.join("out.tar.xz");
    let output = tmp.join("extracted");

    // Recursive compress.
    geezipx()
        .args([
            "compress",
            src.to_str().unwrap(),
            "-r",
            "-f",
            "tar.xz",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(archive.exists(), "tar.xz archive should exist");

    // List.
    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("root.txt"))
        .stdout(predicate::str::contains("nested/deep.txt"));

    // Decompress.
    std::fs::create_dir_all(&output).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(output.join("src").join("root.txt").exists());
    assert!(output.join("src").join("nested").join("deep.txt").exists());
}

#[test]
fn tarxz_auto_format_from_tar_xz_extension() {
    // Without --format, tar.xz is inferred from .tar.xz extension.
    let tmp = TestDir::new();
    tmp.write("hello.txt", "Auto-format tar.xz.");
    let archive = tmp.join("out.tar.xz");

    geezipx()
        .args([
            "compress",
            tmp.join("hello.txt").to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(archive.exists(), "tar.xz archive should exist");

    // Decompress (auto-detect .tar.xz).
    let output = tmp.join("out2");
    std::fs::create_dir_all(&output).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();
}

#[test]
fn tarxz_auto_format_from_txz_extension() {
    // .txz extension should also auto-detect as TarXz.
    let tmp = TestDir::new();
    tmp.write("data.txt", "Auto-format .txz.");
    let archive = tmp.join("out.txz");

    geezipx()
        .args([
            "compress",
            tmp.join("data.txt").to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(archive.exists(), "txz archive should exist");

    let output = tmp.join("out2");
    std::fs::create_dir_all(&output).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();
}

#[test]
fn tarxz_explicit_format_roundtrip() {
    let tmp = TestDir::new();
    let content = "TarXz explicit -f tar.xz round-trip.";
    tmp.write("input.bin", content);
    // Use .txz extension so decompress auto-detection works (decompress has no -f).
    let archive = tmp.join("out.txz");

    // Compress with -f tar.xz (no extension-based inference).
    geezipx()
        .args([
            "compress",
            tmp.join("input.bin").to_str().unwrap(),
            "-f",
            "tar.xz",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(archive.exists(), "txz archive should exist");

    // Decompress (auto-detected from .txz extension).
    let output = tmp.join("out");
    std::fs::create_dir_all(&output).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();

    let extracted = output.join("input.bin");
    let actual = std::fs::read_to_string(&extracted).unwrap();
    assert_eq!(actual, content);
}

#[test]
fn tarxz_list_table_output() {
    let tmp = TestDir::new();
    tmp.write("hello.txt", "List tar.xz table test.");
    let archive = tmp.join("hello.tar.xz");

    geezipx()
        .args([
            "compress",
            tmp.join("hello.txt").to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    // List in table mode.
    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello.txt"))
        .stdout(predicate::str::contains("Ratio"))
        .stdout(predicate::str::contains("Modified"));
}

#[test]
fn tarxz_list_json_output() {
    let tmp = TestDir::new();
    tmp.write("data.txt", "List tar.xz JSON test.");
    let archive = tmp.join("data.tar.xz");

    geezipx()
        .args([
            "compress",
            tmp.join("data.txt").to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    // List with --json.
    geezipx()
        .args(["list", archive.to_str().unwrap(), "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""path":"#))
        .stdout(predicate::str::contains(r#""compression_ratio""#))
        .stdout(predicate::str::contains(r#""modified""#));
}

#[test]
fn tarxz_level_9() {
    let tmp = TestDir::new();
    let content = "TarXz compression at level 9.";
    tmp.write("test.txt", content);
    let archive = tmp.join("test.tar.xz");

    geezipx()
        .args([
            "compress",
            tmp.join("test.txt").to_str().unwrap(),
            "-f",
            "tar.xz",
            "-L",
            "9",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(archive.exists(), "tar.xz level 9 archive should exist");

    // Decompress and verify.
    let output_dir = tmp.join("out");
    std::fs::create_dir_all(&output_dir).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            output_dir.to_str().unwrap(),
        ])
        .assert()
        .success();

    let extracted = output_dir.join("test.txt");
    let actual = std::fs::read_to_string(&extracted).unwrap();
    assert_eq!(actual, content);
}

#[test]
fn tarxz_stdout_fails_for_archive_format() {
    // --stdout should be rejected for multi-file archive formats.
    let tmp = TestDir::new();
    tmp.write("data.txt", "stdout should fail");
    let archive = tmp.join("out.tar.xz");

    geezipx()
        .args([
            "compress",
            tmp.join("data.txt").to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    geezipx()
        .args(["decompress", archive.to_str().unwrap(), "--stdout"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--stdout"));
}

#[test]
fn tarxz_level_10_rejected() {
    let tmp = TestDir::new();
    tmp.write("test.txt", "Level 10 tar.xz reject test.");
    let archive = tmp.join("test.tar.xz");

    geezipx()
        .args([
            "compress",
            tmp.join("test.txt").to_str().unwrap(),
            "-f",
            "tar.xz",
            "-L",
            "10",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("0..=9"));
}

#[test]
fn xz_single_stream_stdout_still_works_after_tarxz() {
    // Verify that .xz single-stream --stdout is NOT broken by TarXz changes.
    let tmp = TestDir::new();
    tmp.write("hello.txt", "xz single-stream stdout test.");
    let archive = tmp.join("hello.txt.xz");

    geezipx()
        .args([
            "compress",
            tmp.join("hello.txt").to_str().unwrap(),
            "-f",
            "xz",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    geezipx()
        .args(["decompress", archive.to_str().unwrap(), "--stdout"])
        .assert()
        .success()
        .stdout("xz single-stream stdout test.");
}

// ---------------------------------------------------------------------------
// XZ single-stream tests
// ---------------------------------------------------------------------------

#[test]
fn xz_roundtrip() {
    let tmp = TestDir::new();
    tmp.write(
        "hello.txt",
        "Hello, GeeZipX! Round-trip through xz compression.",
    );
    let archive = tmp.join("hello.txt.xz");

    // Compress to xz.
    geezipx()
        .args([
            "compress",
            tmp.join("hello.txt").to_str().unwrap(),
            "-f",
            "xz",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(archive.exists(), "xz archive should exist");

    // Decompress with --stdout and verify content.
    geezipx()
        .args(["decompress", archive.to_str().unwrap(), "--stdout"])
        .assert()
        .success()
        .stdout("Hello, GeeZipX! Round-trip through xz compression.");
}

#[test]
fn xz_stdout_roundtrip() {
    let tmp = TestDir::new();
    tmp.write("hello.txt", "Hello, GeeZipX! xz --stdout round-trip.");
    let archive = tmp.join("hello.txt.xz");

    geezipx()
        .args([
            "compress",
            tmp.join("hello.txt").to_str().unwrap(),
            "-f",
            "xz",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    geezipx()
        .args(["decompress", archive.to_str().unwrap(), "--stdout"])
        .assert()
        .success()
        .stdout("Hello, GeeZipX! xz --stdout round-trip.");
}

#[test]
fn xz_explicit_format_roundtrip() {
    let tmp = TestDir::new();
    tmp.write("data.txt", "Explicit xz format test.");
    let archive = tmp.join("data.txt.xz");

    geezipx()
        .args([
            "compress",
            tmp.join("data.txt").to_str().unwrap(),
            "-f",
            "xz",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    let output = tmp.join("extracted_xz");
    std::fs::create_dir_all(&output).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(output.join("data.txt").exists());
    assert_eq!(
        std::fs::read_to_string(output.join("data.txt")).unwrap(),
        "Explicit xz format test."
    );
}

#[test]
fn xz_list_table_output() {
    let tmp = TestDir::new();
    tmp.write("data.txt", "List xz table test.");
    let archive = tmp.join("data.txt.xz");

    geezipx()
        .args([
            "compress",
            tmp.join("data.txt").to_str().unwrap(),
            "-f",
            "xz",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("data.txt"))
        .stdout(predicate::str::contains("Ratio"));
}

#[test]
fn xz_list_json_output() {
    let tmp = TestDir::new();
    tmp.write("data.txt", "List xz JSON test.");
    let archive = tmp.join("data.txt.xz");

    geezipx()
        .args([
            "compress",
            tmp.join("data.txt").to_str().unwrap(),
            "-f",
            "xz",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    geezipx()
        .args(["list", archive.to_str().unwrap(), "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""path":"#))
        .stdout(predicate::str::contains(r#""compression_ratio""#));
}

#[test]
fn xz_auto_extension_from_xz() {
    let tmp = TestDir::new();
    tmp.write("hello.txt", "Auto-extension xz.");
    let archive = tmp.join("out.xz");

    // Without --format, xz should be inferred from .xz extension.
    geezipx()
        .args([
            "compress",
            tmp.join("hello.txt").to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    let out2 = tmp.join("extract");
    std::fs::create_dir(&out2).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            out2.to_str().unwrap(),
        ])
        .assert()
        .success();
}

#[test]
fn xz_level_9_success() {
    let tmp = TestDir::new();
    let content = "Level 9 xz compression test.";
    tmp.write("test.txt", content);
    let archive = tmp.join("test.txt.xz");

    geezipx()
        .args([
            "compress",
            tmp.join("test.txt").to_str().unwrap(),
            "-f",
            "xz",
            "-L",
            "9",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    geezipx()
        .args(["decompress", archive.to_str().unwrap(), "--stdout"])
        .assert()
        .success()
        .stdout(content);
}

#[test]
fn xz_level_10_rejected() {
    let tmp = TestDir::new();
    tmp.write("test.txt", "Level 10 xz reject test.");
    let archive = tmp.join("test.txt.xz");

    geezipx()
        .args([
            "compress",
            tmp.join("test.txt").to_str().unwrap(),
            "-f",
            "xz",
            "-L",
            "10",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("0..=9"));
}

#[test]
fn xz_multiple_inputs_fails() {
    let tmp = TestDir::new();
    let f1 = tmp.join("a.txt");
    let f2 = tmp.join("b.txt");
    std::fs::write(&f1, "first").unwrap();
    std::fs::write(&f2, "second").unwrap();

    geezipx()
        .args([
            "compress",
            f1.to_str().unwrap(),
            f2.to_str().unwrap(),
            "-f",
            "xz",
            "-o",
            tmp.join("out.xz").to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("single input"));
}

// ---------------------------------------------------------------------------
// LZMA single-stream tests
// ---------------------------------------------------------------------------

#[test]
fn lzma_roundtrip() {
    let tmp = TestDir::new();
    tmp.write(
        "hello.txt",
        "Hello, GeeZipX! Round-trip through lzma compression.",
    );
    let archive = tmp.join("hello.txt.lzma");

    geezipx()
        .args([
            "compress",
            tmp.join("hello.txt").to_str().unwrap(),
            "-f",
            "lzma",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(archive.exists(), "lzma archive should exist");

    geezipx()
        .args(["decompress", archive.to_str().unwrap(), "--stdout"])
        .assert()
        .success()
        .stdout("Hello, GeeZipX! Round-trip through lzma compression.");
}

#[test]
fn lzma_stdout_roundtrip() {
    let tmp = TestDir::new();
    tmp.write("hello.txt", "Hello, GeeZipX! lzma --stdout.");
    let archive = tmp.join("hello.txt.lzma");

    geezipx()
        .args([
            "compress",
            tmp.join("hello.txt").to_str().unwrap(),
            "-f",
            "lzma",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    geezipx()
        .args(["decompress", archive.to_str().unwrap(), "--stdout"])
        .assert()
        .success()
        .stdout("Hello, GeeZipX! lzma --stdout.");
}

#[test]
fn lzma_list_json_output() {
    let tmp = TestDir::new();
    tmp.write("data.txt", "List lzma JSON test.");
    let archive = tmp.join("data.txt.lzma");

    geezipx()
        .args([
            "compress",
            tmp.join("data.txt").to_str().unwrap(),
            "-f",
            "lzma",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    geezipx()
        .args(["list", archive.to_str().unwrap(), "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""path":"#))
        .stdout(predicate::str::contains(r#""compression_ratio""#));
}

#[test]
fn lzma_auto_extension_from_lzma() {
    let tmp = TestDir::new();
    tmp.write("hello.txt", "Auto-extension lzma.");
    let archive = tmp.join("out.lzma");

    geezipx()
        .args([
            "compress",
            tmp.join("hello.txt").to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    let out2 = tmp.join("extract");
    std::fs::create_dir(&out2).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            out2.to_str().unwrap(),
        ])
        .assert()
        .success();
}

#[test]
fn lzma_level_10_rejected() {
    let tmp = TestDir::new();
    tmp.write("test.txt", "Level 10 lzma reject test.");
    let archive = tmp.join("test.txt.lzma");

    geezipx()
        .args([
            "compress",
            tmp.join("test.txt").to_str().unwrap(),
            "-f",
            "lzma",
            "-L",
            "10",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("0..=9"));
}

// ---------------------------------------------------------------------------
// Additional edge-case tests: Unicode, empty directories, corrupted input
// ---------------------------------------------------------------------------

#[test]
fn unicode_filename_zip_roundtrip() {
    let tmp = TestDir::new();
    let filename = "\u{6d4b}\u{8bd5}\u{6587}\u{4ef6}.txt"; // 测试文件.txt
    tmp.write(filename, "Chinese filename round-trip test.");
    let archive = tmp.join("unicode.zip");
    let output = tmp.join("extracted_unicode");

    // Compress.
    geezipx()
        .args([
            "compress",
            tmp.join(filename).to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
            "--no-progress",
        ])
        .assert()
        .success();

    assert!(archive.exists(), "ZIP archive should exist");

    // List should contain the filename.
    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains(filename));

    // Decompress.
    std::fs::create_dir_all(&output).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
            "--no-progress",
        ])
        .assert()
        .success();

    let extracted = output.join(filename);
    assert!(extracted.exists(), "extracted file should exist");
    assert_eq!(
        std::fs::read_to_string(&extracted).unwrap(),
        "Chinese filename round-trip test."
    );
}

#[test]
fn recursive_directory_targz_roundtrip() {
    let tmp = TestDir::new();
    let src = tmp.join("src");
    // Nested directory structure: empty dir + subdir with file + root file.
    std::fs::create_dir_all(src.join("empty_subdir")).unwrap();
    std::fs::create_dir_all(src.join("subdir")).unwrap();
    std::fs::write(src.join("root.txt"), "root level").unwrap();
    std::fs::write(src.join("subdir").join("nested.txt"), "nested content").unwrap();

    let archive = tmp.join("recursive_dir.tar.gz");
    let output = tmp.join("extracted");

    // Recursive compress.
    geezipx()
        .args([
            "compress",
            src.to_str().unwrap(),
            "-r",
            "-f",
            "tar.gz",
            "-o",
            archive.to_str().unwrap(),
            "--no-progress",
        ])
        .assert()
        .success();

    assert!(archive.exists(), "tar.gz archive should exist");

    // List should show the files in the archive.
    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("root.txt"))
        .stdout(predicate::str::contains("nested.txt"));

    // Decompress.
    std::fs::create_dir_all(&output).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
            "--no-progress",
        ])
        .assert()
        .success();

    // Verify both files and the empty directory are correctly restored.
    assert!(
        output.join("src").join("empty_subdir").is_dir(),
        "empty_subdir should be preserved as a directory in round-trip"
    );
    assert_eq!(
        std::fs::read_to_string(output.join("src").join("root.txt")).unwrap(),
        "root level"
    );
    assert_eq!(
        std::fs::read_to_string(output.join("src").join("subdir").join("nested.txt")).unwrap(),
        "nested content"
    );
}

#[test]
fn corrupted_zip_graceful_error() {
    let tmp = TestDir::new();
    tmp.write("good.txt", "Original file data for valid ZIP.");
    let archive = tmp.join("test.zip");

    // Create a valid ZIP archive first.
    geezipx()
        .args([
            "compress",
            tmp.join("good.txt").to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
            "--no-progress",
        ])
        .assert()
        .success();

    assert!(archive.exists(), "ZIP archive should exist");

    // Corrupt the archive: overwrite with garbage bytes.
    std::fs::write(&archive, b"CORRUPTEDGARBAGE").unwrap();

    // list should fail gracefully (not panic).
    let list_output = geezipx()
        .args(["list", archive.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        !list_output.status.success(),
        "list should fail on corrupted zip"
    );
    let stderr = String::from_utf8_lossy(&list_output.stderr);
    assert!(
        !stderr.contains("panicked"),
        "list should not panic on corrupted zip: {stderr}"
    );

    assert!(
        !stderr.is_empty(),
        "list should report error on stderr for corrupted zip"
    );

    // decompress should also fail gracefully (not panic).
    let output_dir = tmp.join("extracted");
    std::fs::create_dir_all(&output_dir).unwrap();
    let decompress_output = geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            output_dir.to_str().unwrap(),
            "--no-progress",
        ])
        .output()
        .unwrap();
    assert!(
        !decompress_output.status.success(),
        "decompress should fail on corrupted zip"
    );
    let stderr2 = String::from_utf8_lossy(&decompress_output.stderr);
    assert!(
        !stderr2.contains("panicked"),
        "decompress should not panic on corrupted zip: {stderr2}"
    );
    assert!(
        !stderr2.is_empty(),
        "decompress should report error on stderr for corrupted zip"
    );
}

#[test]
fn recursive_directory_zip_roundtrip_preserves_empty_dirs() {
    let tmp = TestDir::new();
    let src = tmp.join("src");
    // Nested directory structure: empty dir + subdir with file + root file.
    std::fs::create_dir_all(src.join("empty_subdir")).unwrap();
    std::fs::create_dir_all(src.join("subdir")).unwrap();
    std::fs::write(src.join("root.txt"), "root level").unwrap();
    std::fs::write(src.join("subdir").join("nested.txt"), "nested content").unwrap();

    let archive = tmp.join("recursive_dir.zip");
    let output = tmp.join("extracted");

    // Recursive compress.
    geezipx()
        .args([
            "compress",
            src.to_str().unwrap(),
            "-r",
            "-f",
            "zip",
            "-o",
            archive.to_str().unwrap(),
            "--no-progress",
        ])
        .assert()
        .success();

    assert!(archive.exists(), "zip archive should exist");

    // List should show files and the empty directory entry.
    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("root.txt"))
        .stdout(predicate::str::contains("nested.txt"))
        .stdout(predicate::str::contains("empty_subdir/"));

    // Decompress.
    std::fs::create_dir_all(&output).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
            "--no-progress",
        ])
        .assert()
        .success();

    // Verify both files and the empty directory are correctly restored.
    assert!(
        output.join("src").join("empty_subdir").is_dir(),
        "empty_subdir should be preserved as a directory in round-trip"
    );
    assert_eq!(
        std::fs::read_to_string(output.join("src").join("root.txt")).unwrap(),
        "root level"
    );
    assert_eq!(
        std::fs::read_to_string(output.join("src").join("subdir").join("nested.txt")).unwrap(),
        "nested content"
    );
}

#[test]
fn recursive_directory_tar_roundtrip_preserves_empty_dirs() {
    let tmp = TestDir::new();
    let src = tmp.join("src");
    // Nested directory structure: empty dir + subdir with file + root file.
    std::fs::create_dir_all(src.join("empty_subdir")).unwrap();
    std::fs::create_dir_all(src.join("subdir")).unwrap();
    std::fs::write(src.join("root.txt"), "root level").unwrap();
    std::fs::write(src.join("subdir").join("nested.txt"), "nested content").unwrap();

    let archive = tmp.join("recursive_dir.tar");
    let output = tmp.join("extracted");

    // Recursive compress.
    geezipx()
        .args([
            "compress",
            src.to_str().unwrap(),
            "-r",
            "-f",
            "tar",
            "-o",
            archive.to_str().unwrap(),
            "--no-progress",
        ])
        .assert()
        .success();

    assert!(archive.exists(), "tar archive should exist");

    // List should show files and the empty directory entry.
    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("root.txt"))
        .stdout(predicate::str::contains("nested.txt"))
        .stdout(predicate::str::contains("empty_subdir"));

    // Decompress.
    std::fs::create_dir_all(&output).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
            "--no-progress",
        ])
        .assert()
        .success();

    // Verify both files and the empty directory are correctly restored.
    assert!(
        output.join("src").join("empty_subdir").is_dir(),
        "empty_subdir should be preserved as a directory in round-trip"
    );
    assert_eq!(
        std::fs::read_to_string(output.join("src").join("root.txt")).unwrap(),
        "root level"
    );
    assert_eq!(
        std::fs::read_to_string(output.join("src").join("subdir").join("nested.txt")).unwrap(),
        "nested content"
    );
}

#[test]
fn recursive_directory_tarxz_roundtrip_preserves_empty_dirs() {
    let tmp = TestDir::new();
    let src = tmp.join("src");
    // Nested directory structure: empty dir + subdir with file + root file.
    std::fs::create_dir_all(src.join("empty_subdir")).unwrap();
    std::fs::create_dir_all(src.join("subdir")).unwrap();
    std::fs::write(src.join("root.txt"), "root level").unwrap();
    std::fs::write(src.join("subdir").join("nested.txt"), "nested content").unwrap();

    let archive = tmp.join("recursive_dir.tar.xz");
    let output = tmp.join("extracted");

    // Recursive compress.
    geezipx()
        .args([
            "compress",
            src.to_str().unwrap(),
            "-r",
            "-f",
            "tar.xz",
            "-o",
            archive.to_str().unwrap(),
            "--no-progress",
        ])
        .assert()
        .success();

    assert!(archive.exists(), "tar.xz archive should exist");

    // List should show files and the empty directory entry.
    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("root.txt"))
        .stdout(predicate::str::contains("nested.txt"))
        .stdout(predicate::str::contains("empty_subdir"));

    // Decompress.
    std::fs::create_dir_all(&output).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
            "--no-progress",
        ])
        .assert()
        .success();

    // Verify both files and the empty directory are correctly restored.
    assert!(
        output.join("src").join("empty_subdir").is_dir(),
        "empty_subdir should be preserved as a directory in round-trip"
    );
    assert_eq!(
        std::fs::read_to_string(output.join("src").join("root.txt")).unwrap(),
        "root level"
    );
    assert_eq!(
        std::fs::read_to_string(output.join("src").join("subdir").join("nested.txt")).unwrap(),
        "nested content"
    );
}

#[test]
fn recursive_directory_tarzst_roundtrip_preserves_empty_dirs() {
    let tmp = TestDir::new();
    let src = tmp.join("src");
    // Nested directory structure: empty dir + subdir with file + root file.
    std::fs::create_dir_all(src.join("empty_subdir")).unwrap();
    std::fs::create_dir_all(src.join("subdir")).unwrap();
    std::fs::write(src.join("root.txt"), "root level").unwrap();
    std::fs::write(src.join("subdir").join("nested.txt"), "nested content").unwrap();

    let archive = tmp.join("recursive_dir.tar.zst");
    let output = tmp.join("extracted");

    // Recursive compress.
    geezipx()
        .args([
            "compress",
            src.to_str().unwrap(),
            "-r",
            "-f",
            "tar.zst",
            "-o",
            archive.to_str().unwrap(),
            "--no-progress",
        ])
        .assert()
        .success();

    assert!(archive.exists(), "tar.zst archive should exist");

    // List should show files and the empty directory entry.
    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("root.txt"))
        .stdout(predicate::str::contains("nested.txt"))
        .stdout(predicate::str::contains("empty_subdir"));

    // Decompress.
    std::fs::create_dir_all(&output).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
            "--no-progress",
        ])
        .assert()
        .success();

    // Verify both files and the empty directory are correctly restored.
    assert!(
        output.join("src").join("empty_subdir").is_dir(),
        "empty_subdir should be preserved as a directory in round-trip"
    );
    assert_eq!(
        std::fs::read_to_string(output.join("src").join("root.txt")).unwrap(),
        "root level"
    );
    assert_eq!(
        std::fs::read_to_string(output.join("src").join("subdir").join("nested.txt")).unwrap(),
        "nested content"
    );
}
