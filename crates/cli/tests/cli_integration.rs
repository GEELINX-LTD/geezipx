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
        .stdout(predicate::str::contains("--recursive"));
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
        .stdout(predicate::str::is_match(r#""size"\s*:"#).unwrap());
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

    // List in table mode should show the file name.
    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello.txt"));
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
        .stdout(predicate::str::contains(r#""path":"#));
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
