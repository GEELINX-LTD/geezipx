//! Integration tests for the `geezipx` CLI binary.
//!
//! Uses `assert_cmd` and `predicates` for process assertions and `tempfile`
//! for temporary test directories.
#![allow(dead_code)]

use std::io::Write as _;
use std::path::Path;
use std::path::PathBuf;

use assert_cmd::Command;
use cab::{CabinetBuilder, CompressionType};
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

fn assert_zipx_roundtrip(explicit_format: bool) {
    let tmp = TestDir::new();
    tmp.write("input.txt", "ZIPX round-trip test data.");
    let archive = tmp.join("output.zipx");
    let output = if explicit_format {
        tmp.join("extracted-explicit")
    } else {
        tmp.join("extracted-inferred")
    };

    let mut compress = geezipx();
    compress.arg("compress").arg(tmp.join("input.txt"));
    if explicit_format {
        compress.args(["-f", "zipx"]);
    }
    compress.arg("-o").arg(&archive).assert().success();

    assert!(archive.exists(), "ZIPX archive should exist");

    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("input.txt"));

    geezipx()
        .args(["test", archive.to_str().unwrap()])
        .assert()
        .success();

    std::fs::create_dir_all(&output).unwrap();
    geezipx()
        .arg("decompress")
        .arg(&archive)
        .arg("-o")
        .arg(&output)
        .assert()
        .success();

    let extracted = output.join("input.txt");
    assert!(extracted.exists(), "extracted ZIPX file should exist");
    assert_eq!(
        std::fs::read_to_string(&extracted).unwrap(),
        "ZIPX round-trip test data."
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
        .stdout(predicate::str::contains("zipx"))
        .stdout(predicate::str::contains("--recursive"))
        .stdout(predicate::str::contains("--level"))
        .stdout(predicate::str::contains("--jobs"));
}

#[test]
fn compress_help_mentions_zipx() {
    geezipx()
        .args(["compress", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("zipx"));
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
fn zipx_roundtrip_infers_format_from_output_extension() {
    assert_zipx_roundtrip(false);
}

#[test]
fn zipx_roundtrip_accepts_explicit_format_flag() {
    assert_zipx_roundtrip(true);
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

// ---------------------------------------------------------------------------
// Glob expansion tests for compress
// ---------------------------------------------------------------------------

#[test]
fn compress_glob_basic() {
    let tmp = TestDir::new();
    tmp.write("a.txt", "content a");
    tmp.write("b.txt", "content b");
    tmp.write("c.rs", "fn main() {}");
    let archive = tmp.join("out.zip");

    geezipx()
        .current_dir(tmp.path())
        .args(["compress", "*.txt", "-o", archive.to_str().unwrap()])
        .assert()
        .success();

    // List should contain a.txt and b.txt, but NOT c.rs
    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("a.txt"))
        .stdout(predicate::str::contains("b.txt"))
        .stdout(predicate::str::contains("c.rs").not());
}

#[test]
fn compress_glob_mixed() {
    let tmp = TestDir::new();
    tmp.write("a.txt", "txt content");
    tmp.write("b.rs", "rs content");
    tmp.write("c.rs", "rs content 2");
    let archive = tmp.join("out.zip");

    geezipx()
        .current_dir(tmp.path())
        .args(["compress", "a.txt", "*.rs", "-o", archive.to_str().unwrap()])
        .assert()
        .success();

    // List should contain a.txt and both .rs files
    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("a.txt"))
        .stdout(predicate::str::contains("b.rs"))
        .stdout(predicate::str::contains("c.rs"));
}

#[test]
fn compress_glob_no_match() {
    let tmp = TestDir::new();
    tmp.write("a.txt", "content");
    let archive = tmp.join("out.zip");

    geezipx()
        .current_dir(tmp.path())
        .args(["compress", "*.nonexistent", "-o", archive.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no files matched"));
}

#[test]
fn compress_glob_dedup() {
    let tmp = TestDir::new();
    tmp.write("a.txt", "dedup test");
    let archive = tmp.join("out.zip");

    geezipx()
        .current_dir(tmp.path())
        .args([
            "compress",
            "*.txt",
            "*.txt",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Decompress and verify only one file was stored
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

    assert!(out.join("a.txt").exists());
    assert_eq!(
        std::fs::read_to_string(out.join("a.txt")).unwrap(),
        "dedup test"
    );
}

#[test]
fn compress_glob_output_match() {
    let tmp = TestDir::new();
    tmp.write("a.txt", "content");
    // Pre-create the output file so glob `*` matches it
    std::fs::write(tmp.join("out.zip"), "").unwrap();

    geezipx()
        .current_dir(tmp.path())
        .args(["compress", "*", "-o", "out.zip"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot also be an input"));
}

#[test]
fn compress_glob_single_stream_single_match() {
    let tmp = TestDir::new();
    tmp.write("data.txt", "single-stream glob test");
    let archive = tmp.join("data.gz");

    geezipx()
        .current_dir(tmp.path())
        .args([
            "compress",
            "*.txt",
            "-f",
            "gz",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Decompress and verify
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

    assert_eq!(
        std::fs::read_to_string(out.join("data")).unwrap(),
        "single-stream glob test"
    );
}

#[test]
fn compress_glob_single_stream_multi_match() {
    let tmp = TestDir::new();
    tmp.write("a.txt", "first");
    tmp.write("b.txt", "second");

    geezipx()
        .current_dir(tmp.path())
        .args([
            "compress",
            "*.txt",
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
fn compress_glob_question_mark() {
    let tmp = TestDir::new();
    tmp.write("f1.txt", "content 1");
    tmp.write("f2.txt", "content 2");
    tmp.write("f10.txt", "content 10");
    tmp.write("other.rs", "fn main() {}");
    let archive = tmp.join("out.zip");

    // `f?.txt` matches f<single_char>.txt — f1.txt and f2.txt match, f10.txt does not
    geezipx()
        .current_dir(tmp.path())
        .args(["compress", "f?.txt", "-o", archive.to_str().unwrap()])
        .assert()
        .success();

    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("f1.txt"))
        .stdout(predicate::str::contains("f2.txt"))
        .stdout(predicate::str::contains("f10.txt").not())
        .stdout(predicate::str::contains("other.rs").not());
}

#[test]
fn compress_glob_invalid_pattern() {
    let tmp = TestDir::new();
    tmp.write("a.txt", "content");
    let archive = tmp.join("out.zip");

    geezipx()
        .current_dir(tmp.path())
        .args(["compress", "file[", "-o", archive.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid glob pattern"));
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

    // Compress tar.gz with --jobs 4 — tar.gz now supports parallel gzip via
    // the pigz-style `gzp` crate when jobs > 1.
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
fn tarzst_stdout_outputs_raw_tar_stream() {
    // --stdout on tar.zst decompresses the outer zstd layer, outputting
    // the raw tar stream (not the archive entries).
    let tmp = TestDir::new();
    tmp.write("data.txt", "raw tar stream through stdout");
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

    // Now --stdout should succeed, outputting the raw tar stream.
    // The raw tar stream contains "data.txt" in its headers.
    geezipx()
        .args(["decompress", archive.to_str().unwrap(), "--stdout"])
        .assert()
        .success()
        .stdout(predicate::str::contains("data.txt"));
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
fn tarxz_stdout_outputs_raw_tar_stream() {
    // --stdout on tar.xz decompresses the outer xz layer, outputting
    // the raw tar stream (not the archive entries).
    let tmp = TestDir::new();
    tmp.write("data.txt", "raw tar stream through stdout");
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

    // Now --stdout should succeed, outputting the raw tar stream.
    // The raw tar stream contains "data.txt" in its headers.
    geezipx()
        .args(["decompress", archive.to_str().unwrap(), "--stdout"])
        .assert()
        .success()
        .stdout(predicate::str::contains("data.txt"));
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
// XZ — no-clobber / force / progress / verbose / corrupted
// ---------------------------------------------------------------------------

#[test]
fn xz_decompress_no_clobber_skips_existing() {
    let tmp = TestDir::new();
    tmp.write("hello.txt", "XZ no-clobber test.");
    let archive = tmp.join("hello.txt.xz");

    // Compress.
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
        "--no-clobber should not overwrite existing xz output"
    );
}

#[test]
fn xz_decompress_force_overwrites_existing() {
    let tmp = TestDir::new();
    tmp.write("hello.txt", "XZ force test.");
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

    assert!(archive.exists(), "xz archive should exist");

    let output_dir = tmp.join("out");
    std::fs::create_dir_all(&output_dir).unwrap();

    // First decompress.
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

    // Decompress with --force: should overwrite.
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            output_dir.to_str().unwrap(),
            "--force",
        ])
        .assert()
        .success();

    // File content should be restored to original (overwritten).
    assert_eq!(
        std::fs::read_to_string(output_dir.join("hello.txt")).unwrap(),
        "XZ force test.",
        "--force should overwrite existing xz output"
    );
}

#[test]
fn xz_compress_no_progress_no_ansi() {
    let tmp = TestDir::new();
    tmp.write("input.txt", "XZ no-progress test.");
    let archive = tmp.join("output.xz");

    let output = geezipx()
        .args([
            "compress",
            tmp.join("input.txt").to_str().unwrap(),
            "-f",
            "xz",
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
fn xz_compress_verbose_prints_filename() {
    let tmp = TestDir::new();
    tmp.write("input.txt", "XZ verbose test.");
    let archive = tmp.join("output.xz");

    geezipx()
        .args([
            "compress",
            tmp.join("input.txt").to_str().unwrap(),
            "-f",
            "xz",
            "-o",
            archive.to_str().unwrap(),
            "-v",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("input.txt"));
}

#[test]
fn corrupted_xz_graceful_error() {
    let tmp = TestDir::new();
    tmp.write("good.txt", "Original data for valid xz.");
    let archive = tmp.join("good.txt.xz");

    // Create a valid xz archive first.
    geezipx()
        .args([
            "compress",
            tmp.join("good.txt").to_str().unwrap(),
            "-f",
            "xz",
            "-o",
            archive.to_str().unwrap(),
            "--no-progress",
        ])
        .assert()
        .success();

    assert!(archive.exists(), "xz archive should exist");

    // Corrupt the archive: overwrite with garbage bytes.
    std::fs::write(&archive, b"CORRUPTEDGARBAGE").unwrap();

    // list on corrupted data succeeds for single-stream formats because
    // the entry list is synthetic (file metadata derived); decoding does not run.
    // We still verify list doesn't panic.
    let list_output = geezipx()
        .args(["list", archive.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        !String::from_utf8_lossy(&list_output.stderr).contains("panicked"),
        "list should not panic on corrupted xz"
    );

    // decompress should fail gracefully (not panic).
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
        "decompress should fail on corrupted xz"
    );
    let stderr2 = String::from_utf8_lossy(&decompress_output.stderr);
    assert!(
        !stderr2.contains("panicked"),
        "decompress should not panic on corrupted xz: {stderr2}"
    );
    assert!(
        !stderr2.is_empty(),
        "decompress should report error on stderr for corrupted xz"
    );
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
// LZMA — no-clobber / force / progress / verbose / corrupted
// ---------------------------------------------------------------------------

#[test]
fn lzma_decompress_no_clobber_skips_existing() {
    let tmp = TestDir::new();
    tmp.write("hello.txt", "LZMA no-clobber test.");
    let archive = tmp.join("hello.txt.lzma");

    // Compress.
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
        "--no-clobber should not overwrite existing lzma output"
    );
}

#[test]
fn lzma_decompress_force_overwrites_existing() {
    let tmp = TestDir::new();
    tmp.write("hello.txt", "LZMA force test.");
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

    let output_dir = tmp.join("out");
    std::fs::create_dir_all(&output_dir).unwrap();

    // First decompress.
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

    // Decompress with --force: should overwrite.
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            output_dir.to_str().unwrap(),
            "--force",
        ])
        .assert()
        .success();

    // File content should be restored to original (overwritten).
    assert_eq!(
        std::fs::read_to_string(output_dir.join("hello.txt")).unwrap(),
        "LZMA force test.",
        "--force should overwrite existing lzma output"
    );
}

#[test]
fn lzma_compress_no_progress_no_ansi() {
    let tmp = TestDir::new();
    tmp.write("input.txt", "LZMA no-progress test.");
    let archive = tmp.join("output.lzma");

    let output = geezipx()
        .args([
            "compress",
            tmp.join("input.txt").to_str().unwrap(),
            "-f",
            "lzma",
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
fn lzma_compress_verbose_prints_filename() {
    let tmp = TestDir::new();
    tmp.write("input.txt", "LZMA verbose test.");
    let archive = tmp.join("output.lzma");

    geezipx()
        .args([
            "compress",
            tmp.join("input.txt").to_str().unwrap(),
            "-f",
            "lzma",
            "-o",
            archive.to_str().unwrap(),
            "-v",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("input.txt"));
}

#[test]
fn corrupted_lzma_graceful_error() {
    let tmp = TestDir::new();
    tmp.write("good.txt", "Original data for valid lzma.");
    let archive = tmp.join("good.txt.lzma");

    // Create a valid lzma archive first.
    geezipx()
        .args([
            "compress",
            tmp.join("good.txt").to_str().unwrap(),
            "-f",
            "lzma",
            "-o",
            archive.to_str().unwrap(),
            "--no-progress",
        ])
        .assert()
        .success();

    assert!(archive.exists(), "lzma archive should exist");

    // Corrupt the archive: overwrite with garbage bytes.
    std::fs::write(&archive, b"CORRUPTEDGARBAGE").unwrap();

    // list on corrupted data succeeds for single-stream formats because
    // the entry list is synthetic (file metadata derived); decoding does not run.
    // We still verify list doesn't panic.
    let list_output = geezipx()
        .args(["list", archive.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        !String::from_utf8_lossy(&list_output.stderr).contains("panicked"),
        "list should not panic on corrupted lzma"
    );

    // decompress should fail gracefully (not panic).
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
        "decompress should fail on corrupted lzma"
    );
    let stderr2 = String::from_utf8_lossy(&decompress_output.stderr);
    assert!(
        !stderr2.contains("panicked"),
        "decompress should not panic on corrupted lzma: {stderr2}"
    );
    assert!(
        !stderr2.is_empty(),
        "decompress should report error on stderr for corrupted lzma"
    );
}

// ---------------------------------------------------------------------------
// Dangerous path warning tests
// ---------------------------------------------------------------------------

/// Create a minimal POSIX tar archive with a single entry at the given
/// path.  This lets us craft entries with `../` traversal that would be
/// rejected by normal archive creation tools.
fn create_minimal_tar(entry_path: &str, content: &[u8]) -> Vec<u8> {
    let name_bytes = entry_path.as_bytes();
    let name_len = name_bytes.len().min(99);

    let mut header = [0u8; 512];
    header[..name_len].copy_from_slice(&name_bytes[..name_len]);
    // File mode 0644 (octal)
    header[100..108].copy_from_slice(b"0000644\0");
    // UID / GID
    header[108..116].copy_from_slice(b"0000000\0");
    header[116..124].copy_from_slice(b"0000000\0");
    // File size in octal (11 digits + NUL at 135, but we use 11 digits + space)
    let size_str = format!("{:011o}", content.len());
    header[124..135].copy_from_slice(size_str.as_bytes());
    header[135] = b' ';
    // Mtime
    header[136..147].copy_from_slice(b"00000000000");
    header[147] = b' ';
    // Type flag: '0' = regular file
    header[156] = b'0';
    // ustar magic
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");

    // Compute checksum: fill field with spaces, sum all bytes, then write
    for b in &mut header[148..156] {
        *b = b' ';
    }
    let checksum: u32 = header.iter().map(|&b| b as u32).sum();
    let ck = format!("{:06o}\0 ", checksum & 0o777777);
    header[148..156].copy_from_slice(ck.as_bytes());

    let mut result = Vec::new();
    result.extend_from_slice(&header);
    result.extend_from_slice(content);

    // Pad to 512-byte block boundary.
    let tail = result.len() % 512;
    if tail != 0 {
        result.resize(result.len() + 512 - tail, 0);
    }

    // End-of-archive marker: two zero blocks.
    result.extend_from_slice(&[0u8; 1024]);
    result
}

/// Create a minimal POSIX tar archive with multiple entries, each at the
/// given `(path, content)` pair.  Useful for testing dangerous path detection
/// where multiple entries must share the same archive.
fn create_minimal_tar_multi(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut result = Vec::new();

    for &(entry_path, content) in entries {
        let name_bytes = entry_path.as_bytes();
        let name_len = name_bytes.len().min(99);

        let mut header = [0u8; 512];
        header[..name_len].copy_from_slice(&name_bytes[..name_len]);
        // File mode 0644 (octal)
        header[100..108].copy_from_slice(b"0000644\0");
        // UID / GID
        header[108..116].copy_from_slice(b"0000000\0");
        header[116..124].copy_from_slice(b"0000000\0");
        // File size in octal (11 digits + NUL at 135, but we use 11 digits + space)
        let size_str = format!("{:011o}", content.len());
        header[124..135].copy_from_slice(size_str.as_bytes());
        header[135] = b' ';
        // Mtime
        header[136..147].copy_from_slice(b"00000000000");
        header[147] = b' ';
        // Type flag: '0' = regular file
        header[156] = b'0';
        // ustar magic
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");

        // Compute checksum: fill field with spaces, sum all bytes, then write
        for b in &mut header[148..156] {
            *b = b' ';
        }
        let checksum: u32 = header.iter().map(|&b| b as u32).sum();
        let ck = format!("{:06o}\0 ", checksum & 0o777777);
        header[148..156].copy_from_slice(ck.as_bytes());

        result.extend_from_slice(&header);
        result.extend_from_slice(content);

        // Pad to 512-byte block boundary.
        let tail = result.len() % 512;
        if tail != 0 {
            result.resize(result.len() + 512 - tail, 0);
        }
    }

    // End-of-archive marker: two zero blocks.
    result.extend_from_slice(&[0u8; 1024]);
    result
}

#[test]
fn list_shows_warning_for_dangerous_paths() {
    let tmp = TestDir::new();
    let tar_bytes = create_minimal_tar_multi(&[
        ("../evil.txt", b"dangerous"),
        ("foo/../../evil.txt", b"also dangerous"),
    ]);
    let archive = tmp.join("dangerous.tar");
    std::fs::write(&archive, &tar_bytes).unwrap();

    let output = geezipx()
        .args(["list", archive.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "list should succeed despite dangerous paths"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stdout.contains("../evil.txt"),
        "stdout should contain the first entry path: {stdout}"
    );
    assert!(
        stdout.contains("foo/../../evil.txt"),
        "stdout should contain the second entry path: {stdout}"
    );
    assert!(
        stderr.to_lowercase().contains("warning"),
        "stderr should contain a warning: {stderr}"
    );
    assert!(
        stderr.to_lowercase().contains("unsafe"),
        "stderr should mention 'unsafe': {stderr}"
    );
}

#[test]
fn list_json_shows_warning_for_dangerous_paths() {
    let tmp = TestDir::new();
    let tar_bytes = create_minimal_tar_multi(&[
        ("../evil.txt", b"dangerous"),
        ("foo/../../evil.txt", b"also dangerous"),
    ]);
    let archive = tmp.join("dangerous-json.tar");
    std::fs::write(&archive, &tar_bytes).unwrap();

    let output = geezipx()
        .args(["list", "--json", archive.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "list --json should succeed despite dangerous paths"
    );
    // Stdout must be valid JSON and contain both dangerous entries
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON");
    let entries = parsed.as_array().expect("JSON output should be an array");
    assert!(
        entries.iter().any(|e| e["path"] == "../evil.txt"),
        "JSON entries should contain '../evil.txt': {stdout}"
    );
    assert!(
        entries.iter().any(|e| e["path"] == "foo/../../evil.txt"),
        "JSON entries should contain 'foo/../../evil.txt': {stdout}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.to_lowercase().contains("warning"),
        "stderr should contain a warning: {stderr}"
    );
    assert!(
        stderr.to_lowercase().contains("unsafe"),
        "stderr should mention 'unsafe': {stderr}"
    );
}

#[test]
fn list_does_not_warn_for_safe_paths() {
    let tmp = TestDir::new();
    let tar_bytes = create_minimal_tar_multi(&[
        ("safe/readme.txt", b"hello"),
        ("normal/path/file.bin", b"data"),
    ]);
    let archive = tmp.join("safe.tar");
    std::fs::write(&archive, &tar_bytes).unwrap();

    let output = geezipx()
        .args(["list", archive.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "list should succeed for safe paths"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stdout.contains("safe/readme.txt"),
        "stdout should contain the first safe path: {stdout}"
    );
    assert!(
        stdout.contains("normal/path/file.bin"),
        "stdout should contain the second safe path: {stdout}"
    );
    assert!(
        !stderr.to_lowercase().contains("unsafe"),
        "stderr should NOT mention 'unsafe' for safe paths: {stderr}"
    );
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

// ---------------------------------------------------------------------------
// Corrupted input tests for remaining formats
// ---------------------------------------------------------------------------

#[test]
fn corrupted_gzip_graceful_error() {
    let tmp = TestDir::new();
    tmp.write("good.txt", "Original data for valid gzip.");
    let archive = tmp.join("good.txt.gz");

    // Create a valid gzip archive first.
    geezipx()
        .args([
            "compress",
            tmp.join("good.txt").to_str().unwrap(),
            "-f",
            "gz",
            "-o",
            archive.to_str().unwrap(),
            "--no-progress",
        ])
        .assert()
        .success();

    assert!(archive.exists(), "gzip archive should exist");

    // Corrupt the archive: overwrite with garbage bytes.
    std::fs::write(&archive, b"CORRUPTEDGARBAGE").unwrap();

    // list on corrupted data succeeds for single-stream formats because
    // the entry list is synthetic (file metadata derived); decoding does not run.
    // We still verify list doesn't panic.
    let list_output = geezipx()
        .args(["list", archive.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        !String::from_utf8_lossy(&list_output.stderr).contains("panicked"),
        "list should not panic on corrupted gzip"
    );

    // decompress should fail gracefully (not panic).
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
        "decompress should fail on corrupted gzip"
    );
    let stderr2 = String::from_utf8_lossy(&decompress_output.stderr);
    assert!(
        !stderr2.contains("panicked"),
        "decompress should not panic on corrupted gzip: {stderr2}"
    );
    assert!(
        !stderr2.is_empty(),
        "decompress should report error on stderr for corrupted gzip"
    );
}

#[test]
fn corrupted_zstd_graceful_error() {
    let tmp = TestDir::new();
    tmp.write("good.txt", "Original data for valid zstd.");
    let archive = tmp.join("good.txt.zst");

    // Create a valid zstd archive first (auto-detect from .zst extension).
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

    assert!(archive.exists(), "zstd archive should exist");

    // Corrupt the archive: overwrite with garbage bytes.
    std::fs::write(&archive, b"CORRUPTEDGARBAGE").unwrap();

    // list on corrupted data succeeds for single-stream formats because
    // the entry list is synthetic (file metadata derived); decoding does not run.
    // We still verify list doesn't panic.
    let list_output = geezipx()
        .args(["list", archive.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        !String::from_utf8_lossy(&list_output.stderr).contains("panicked"),
        "list should not panic on corrupted zstd"
    );

    // decompress should fail gracefully (not panic).
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
        "decompress should fail on corrupted zstd"
    );
    let stderr2 = String::from_utf8_lossy(&decompress_output.stderr);
    assert!(
        !stderr2.contains("panicked"),
        "decompress should not panic on corrupted zstd: {stderr2}"
    );
    assert!(
        !stderr2.is_empty(),
        "decompress should report error on stderr for corrupted zstd"
    );
}

#[test]
fn corrupted_tar_graceful_error() {
    let tmp = TestDir::new();
    tmp.write("good.txt", "Original data for valid tar.");
    let archive = tmp.join("good.tar");

    // Create a valid tar archive first.
    geezipx()
        .args([
            "compress",
            tmp.join("good.txt").to_str().unwrap(),
            "-f",
            "tar",
            "-o",
            archive.to_str().unwrap(),
            "--no-progress",
        ])
        .assert()
        .success();

    assert!(archive.exists(), "tar archive should exist");

    // Corrupt the archive: overwrite with garbage bytes.
    std::fs::write(&archive, b"CORRUPTEDGARBAGE").unwrap();

    // list should fail gracefully (not panic).
    let list_output = geezipx()
        .args(["list", archive.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        !list_output.status.success(),
        "list should fail on corrupted tar"
    );
    let stderr = String::from_utf8_lossy(&list_output.stderr);
    assert!(
        !stderr.contains("panicked"),
        "list should not panic on corrupted tar: {stderr}"
    );
    assert!(
        !stderr.is_empty(),
        "list should report error on stderr for corrupted tar"
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
        "decompress should fail on corrupted tar"
    );
    let stderr2 = String::from_utf8_lossy(&decompress_output.stderr);
    assert!(
        !stderr2.contains("panicked"),
        "decompress should not panic on corrupted tar: {stderr2}"
    );
    assert!(
        !stderr2.is_empty(),
        "decompress should report error on stderr for corrupted tar"
    );
}

#[test]
fn corrupted_targz_graceful_error() {
    let tmp = TestDir::new();
    tmp.write("good.txt", "Original data for valid tar.gz.");
    let archive = tmp.join("good.tar.gz");

    // Create a valid tar.gz archive first.
    geezipx()
        .args([
            "compress",
            tmp.join("good.txt").to_str().unwrap(),
            "-f",
            "tar.gz",
            "-o",
            archive.to_str().unwrap(),
            "--no-progress",
        ])
        .assert()
        .success();

    assert!(archive.exists(), "tar.gz archive should exist");

    // Corrupt the archive: overwrite with garbage bytes.
    std::fs::write(&archive, b"CORRUPTEDGARBAGE").unwrap();

    // list should fail gracefully (not panic).
    let list_output = geezipx()
        .args(["list", archive.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        !list_output.status.success(),
        "list should fail on corrupted tar.gz"
    );
    let stderr = String::from_utf8_lossy(&list_output.stderr);
    assert!(
        !stderr.contains("panicked"),
        "list should not panic on corrupted tar.gz: {stderr}"
    );
    assert!(
        !stderr.is_empty(),
        "list should report error on stderr for corrupted tar.gz"
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
        "decompress should fail on corrupted tar.gz"
    );
    let stderr2 = String::from_utf8_lossy(&decompress_output.stderr);
    assert!(
        !stderr2.contains("panicked"),
        "decompress should not panic on corrupted tar.gz: {stderr2}"
    );
    assert!(
        !stderr2.is_empty(),
        "decompress should report error on stderr for corrupted tar.gz"
    );
}

#[test]
fn corrupted_tarzst_graceful_error() {
    let tmp = TestDir::new();
    tmp.write("good.txt", "Original data for valid tar.zst.");
    let archive = tmp.join("good.tar.zst");

    // Create a valid tar.zst archive first.
    geezipx()
        .args([
            "compress",
            tmp.join("good.txt").to_str().unwrap(),
            "-f",
            "tar.zst",
            "-o",
            archive.to_str().unwrap(),
            "--no-progress",
        ])
        .assert()
        .success();

    assert!(archive.exists(), "tar.zst archive should exist");

    // Corrupt the archive: overwrite with garbage bytes.
    std::fs::write(&archive, b"CORRUPTEDGARBAGE").unwrap();

    // list should fail gracefully (not panic).
    let list_output = geezipx()
        .args(["list", archive.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        !list_output.status.success(),
        "list should fail on corrupted tar.zst"
    );
    let stderr = String::from_utf8_lossy(&list_output.stderr);
    assert!(
        !stderr.contains("panicked"),
        "list should not panic on corrupted tar.zst: {stderr}"
    );
    assert!(
        !stderr.is_empty(),
        "list should report error on stderr for corrupted tar.zst"
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
        "decompress should fail on corrupted tar.zst"
    );
    let stderr2 = String::from_utf8_lossy(&decompress_output.stderr);
    assert!(
        !stderr2.contains("panicked"),
        "decompress should not panic on corrupted tar.zst: {stderr2}"
    );
    assert!(
        !stderr2.is_empty(),
        "decompress should report error on stderr for corrupted tar.zst"
    );
}

#[test]
fn corrupted_tarxz_graceful_error() {
    let tmp = TestDir::new();
    tmp.write("good.txt", "Original data for valid tar.xz.");
    let archive = tmp.join("good.tar.xz");

    // Create a valid tar.xz archive first.
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

    assert!(archive.exists(), "tar.xz archive should exist");

    // Corrupt the archive: overwrite with garbage bytes.
    std::fs::write(&archive, b"CORRUPTEDGARBAGE").unwrap();

    // list should fail gracefully (not panic).
    let list_output = geezipx()
        .args(["list", archive.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        !list_output.status.success(),
        "list should fail on corrupted tar.xz"
    );
    let stderr = String::from_utf8_lossy(&list_output.stderr);
    assert!(
        !stderr.contains("panicked"),
        "list should not panic on corrupted tar.xz: {stderr}"
    );
    assert!(
        !stderr.is_empty(),
        "list should report error on stderr for corrupted tar.xz"
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
        "decompress should fail on corrupted tar.xz"
    );
    let stderr2 = String::from_utf8_lossy(&decompress_output.stderr);
    assert!(
        !stderr2.contains("panicked"),
        "decompress should not panic on corrupted tar.xz: {stderr2}"
    );
    assert!(
        !stderr2.is_empty(),
        "decompress should report error on stderr for corrupted tar.xz"
    );
}

// ---------------------------------------------------------------------------
// `geezipx test` — archive integrity verification
// ---------------------------------------------------------------------------

#[test]
fn test_help_available() {
    geezipx()
        .args(["test", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--json"));
}

#[test]
fn test_zip_valid() {
    let tmp = TestDir::new();
    tmp.write("data.txt", "ZIP test verification content.");
    let archive = tmp.join("archive.zip");

    // Create a zip.
    geezipx()
        .args([
            "compress",
            tmp.join("data.txt").to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Test it (text output).
    geezipx()
        .args(["test", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Status:"))
        .stdout(predicate::str::contains("ok"))
        .stdout(predicate::str::contains("result: OK"));
}

#[test]
fn test_zip_valid_json() {
    let tmp = TestDir::new();
    tmp.write("data.txt", "ZIP JSON test.");
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

    // Test with --json.
    geezipx()
        .args(["test", archive.to_str().unwrap(), "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains(r#"ok":true"#))
        .stdout(predicate::str::contains(r#"format":"zip""#))
        .stdout(predicate::str::contains(r#"archive":"#));
}

#[test]
fn test_corrupted_zip_fails() {
    let tmp = TestDir::new();
    let archive = tmp.join("broken.zip");
    // Write garbage that's not a valid zip.
    std::fs::write(&archive, b"not a zip file").unwrap();

    geezipx()
        .args(["test", archive.to_str().unwrap()])
        .assert()
        .failure();
}

#[test]
fn test_corrupted_zip_json_fails() {
    let tmp = TestDir::new();
    let archive = tmp.join("broken.zip");
    std::fs::write(&archive, b"not a zip file").unwrap();

    geezipx()
        .args(["test", archive.to_str().unwrap(), "--json"])
        .assert()
        .failure()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains(r#"ok":false"#));
}

#[test]
fn test_tar_gz_valid() {
    let tmp = TestDir::new();
    tmp.write("data.txt", "tar.gz test content.");
    let archive = tmp.join("archive.tar.gz");

    geezipx()
        .args([
            "compress",
            tmp.join("data.txt").to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Test it.
    geezipx()
        .args(["test", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Format:"))
        .stdout(predicate::str::contains("tar.gz"))
        .stdout(predicate::str::contains("result: OK"));
}

#[test]
fn test_gzip_corrupted_fails() {
    let tmp = TestDir::new();
    let archive = tmp.join("broken.gz");
    // Write only the gzip magic header, no body/footer.
    std::fs::write(&archive, [0x1F, 0x8B]).unwrap();

    geezipx()
        .args(["test", archive.to_str().unwrap()])
        .assert()
        .failure();
}

#[test]
fn test_zstd_valid() {
    let tmp = TestDir::new();
    let archive = tmp.join("test.zst");
    tmp.write("data.txt", "zstd verification content.");

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
        .args(["test", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Status:"))
        .stdout(predicate::str::contains("ok"));
}

#[test]
fn test_nonexistent_fails() {
    geezipx()
        .args(["test", "/nonexistent/archive.zip"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not exist"));
}

// ---------------------------------------------------------------------------
// ZIP AES-256 password tests
// ---------------------------------------------------------------------------

#[test]
fn zip_password_roundtrip() {
    let td = TestDir::new();
    td.write("secret.txt", "classified content");
    let zip_path = td.join("encrypted.zip");

    // Compress with password
    geezipx()
        .args([
            "compress",
            td.path().join("secret.txt").to_str().unwrap(),
            "-f",
            "zip",
            "-o",
            zip_path.to_str().unwrap(),
            "--password",
            "mypassword",
        ])
        .assert()
        .success();

    // Decompress with correct password
    let out_dir = td.join("out");
    geezipx()
        .args([
            "decompress",
            zip_path.to_str().unwrap(),
            "-o",
            out_dir.to_str().unwrap(),
            "--password",
            "mypassword",
        ])
        .assert()
        .success();

    let out_file = out_dir.join("secret.txt");
    assert!(out_file.exists(), "expected decompressed file");
    let content = std::fs::read_to_string(&out_file).unwrap();
    assert_eq!(content, "classified content");
}

#[test]
fn zip_password_wrong_password_fails() {
    let td = TestDir::new();
    td.write("secret.txt", "classified content");
    let zip_path = td.join("encrypted.zip");

    // Compress with password
    geezipx()
        .args([
            "compress",
            td.path().join("secret.txt").to_str().unwrap(),
            "-f",
            "zip",
            "-o",
            zip_path.to_str().unwrap(),
            "--password",
            "correctpw",
        ])
        .assert()
        .success();

    // Decompress with wrong password
    let out_dir = td.join("out");
    geezipx()
        .args([
            "decompress",
            zip_path.to_str().unwrap(),
            "-o",
            out_dir.to_str().unwrap(),
            "--password",
            "wrongpw",
        ])
        .assert()
        .failure();
}

#[test]
fn zip_password_no_password_fails_on_encrypted() {
    let td = TestDir::new();
    td.write("secret.txt", "classified content");
    let zip_path = td.join("encrypted.zip");

    // Compress with password
    geezipx()
        .args([
            "compress",
            td.path().join("secret.txt").to_str().unwrap(),
            "-f",
            "zip",
            "-o",
            zip_path.to_str().unwrap(),
            "--password",
            "secret123",
        ])
        .assert()
        .success();

    // Decompress WITHOUT password (should fail)
    let out_dir = td.join("out");
    geezipx()
        .args([
            "decompress",
            zip_path.to_str().unwrap(),
            "-o",
            out_dir.to_str().unwrap(),
        ])
        .assert()
        .failure();
}

#[test]
fn zip_password_flag_on_non_zip_fails() {
    let td = TestDir::new();
    td.write("data.txt", "some data");
    let out_path = td.join("output.tar.gz");

    geezipx()
        .args([
            "compress",
            td.path().join("data.txt").to_str().unwrap(),
            "-f",
            "tar.gz",
            "-o",
            out_path.to_str().unwrap(),
            "--password",
            "mypassword",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("supported for ZIP"));
}

#[test]
fn zip_password_empty_password_fails() {
    let td = TestDir::new();
    td.write("data.txt", "some data");
    let out_path = td.join("out.zip");

    geezipx()
        .args([
            "compress",
            td.path().join("data.txt").to_str().unwrap(),
            "-f",
            "zip",
            "-o",
            out_path.to_str().unwrap(),
            "--password",
            "",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("empty"));
}

#[test]
fn zip_password_list_on_encrypted_without_password_fails() {
    let td = TestDir::new();
    td.write("secret.txt", "classified content");
    let zip_path = td.join("encrypted.zip");

    // Compress with password
    geezipx()
        .args([
            "compress",
            td.path().join("secret.txt").to_str().unwrap(),
            "-f",
            "zip",
            "-o",
            zip_path.to_str().unwrap(),
            "--password",
            "secret123",
        ])
        .assert()
        .success();

    // List entries on encrypted zip should still work
    geezipx()
        .args(["list", zip_path.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Password"));
}

#[test]
fn zip_password_test_on_encrypted_with_password() {
    let td = TestDir::new();
    td.write("secret.txt", "classified content");
    let zip_path = td.join("encrypted.zip");

    // Compress with password
    geezipx()
        .args([
            "compress",
            td.path().join("secret.txt").to_str().unwrap(),
            "-f",
            "zip",
            "-o",
            zip_path.to_str().unwrap(),
            "--password",
            "secret123",
        ])
        .assert()
        .success();

    // Test integrity with correct password
    geezipx()
        .args([
            "test",
            zip_path.to_str().unwrap(),
            "--password",
            "secret123",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("ok"));
}

// ---------------------------------------------------------------------------
// Password file / password stdin tests
// ---------------------------------------------------------------------------

#[test]
fn zip_password_file_roundtrip() {
    let td = TestDir::new();
    td.write("secret.txt", "classified content");
    td.write("passwd.txt", "filepassword\n");
    let zip_path = td.join("encrypted.zip");

    // Compress with --password-file
    geezipx()
        .args([
            "compress",
            td.path().join("secret.txt").to_str().unwrap(),
            "-f",
            "zip",
            "-o",
            zip_path.to_str().unwrap(),
            "--password-file",
            td.path().join("passwd.txt").to_str().unwrap(),
        ])
        .assert()
        .success();

    // Decompress with --password-file
    let out_dir = td.join("out");
    geezipx()
        .args([
            "decompress",
            zip_path.to_str().unwrap(),
            "-o",
            out_dir.to_str().unwrap(),
            "--password-file",
            td.path().join("passwd.txt").to_str().unwrap(),
        ])
        .assert()
        .success();

    let out_file = out_dir.join("secret.txt");
    assert!(out_file.exists(), "expected decompressed file");
    assert_eq!(
        std::fs::read_to_string(&out_file).unwrap(),
        "classified content"
    );
}

#[test]
fn zip_password_stdin_roundtrip() {
    let td = TestDir::new();
    td.write("secret.txt", "stdin password test");
    let zip_path = td.join("encrypted.zip");

    // Compress with --password
    geezipx()
        .args([
            "compress",
            td.path().join("secret.txt").to_str().unwrap(),
            "-f",
            "zip",
            "-o",
            zip_path.to_str().unwrap(),
            "--password",
            "stdinpw",
        ])
        .assert()
        .success();

    // Decompress with --password-stdin
    let out_dir = td.join("out");
    geezipx()
        .args([
            "decompress",
            zip_path.to_str().unwrap(),
            "-o",
            out_dir.to_str().unwrap(),
            "--password-stdin",
        ])
        .write_stdin("stdinpw\n")
        .assert()
        .success();

    let out_file = out_dir.join("secret.txt");
    assert!(out_file.exists(), "expected decompressed file");
    assert_eq!(
        std::fs::read_to_string(&out_file).unwrap(),
        "stdin password test"
    );
}

#[test]
fn zip_password_mutual_exclusion_password_and_password_file() {
    let td = TestDir::new();
    td.write("data.txt", "test");
    td.write("passwd.txt", "secret");

    geezipx()
        .args([
            "compress",
            td.path().join("data.txt").to_str().unwrap(),
            "-f",
            "zip",
            "-o",
            td.path().join("out.zip").to_str().unwrap(),
            "--password",
            "mypassword",
            "--password-file",
            td.path().join("passwd.txt").to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("mutually exclusive"));
}

#[test]
fn zip_password_mutual_exclusion_password_and_password_stdin() {
    let td = TestDir::new();
    td.write("data.txt", "test");

    geezipx()
        .args([
            "compress",
            td.path().join("data.txt").to_str().unwrap(),
            "-f",
            "zip",
            "-o",
            td.path().join("out.zip").to_str().unwrap(),
            "--password",
            "mypassword",
            "--password-stdin",
        ])
        .write_stdin("secret\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("mutually exclusive"));
}

#[test]
fn zip_password_file_empty_fails() {
    let td = TestDir::new();
    td.write("data.txt", "test");
    td.write("empty.txt", "");

    geezipx()
        .args([
            "compress",
            td.path().join("data.txt").to_str().unwrap(),
            "-f",
            "zip",
            "-o",
            td.path().join("out.zip").to_str().unwrap(),
            "--password-file",
            td.path().join("empty.txt").to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("empty"));
}

#[test]
fn zip_password_file_on_non_zip_fails() {
    let td = TestDir::new();
    td.write("data.txt", "some data");
    td.write("passwd.txt", "secret");
    let out_path = td.join("output.tar.gz");

    geezipx()
        .args([
            "compress",
            td.path().join("data.txt").to_str().unwrap(),
            "-f",
            "tar.gz",
            "-o",
            out_path.to_str().unwrap(),
            "--password-file",
            td.path().join("passwd.txt").to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("supported for ZIP"));
}

#[test]
fn zip_password_stdin_on_non_zip_fails() {
    let td = TestDir::new();
    td.write("data.txt", "some data");
    let out_path = td.join("output.tar.gz");

    geezipx()
        .args([
            "compress",
            td.path().join("data.txt").to_str().unwrap(),
            "-f",
            "tar.gz",
            "-o",
            out_path.to_str().unwrap(),
            "--password-stdin",
        ])
        .write_stdin("secret\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("supported for ZIP"));
}

#[test]
fn zip_password_file_on_decompress_non_zip_fails() {
    // Password on decompress with a non-encrypted format, via --password-file
    let td = TestDir::new();
    td.write("data.txt", "some data");
    td.write("passwd.txt", "secret");
    let gz_path = td.join("data.gz");

    // Compress without password
    geezipx()
        .args([
            "compress",
            td.path().join("data.txt").to_str().unwrap(),
            "-f",
            "gz",
            "-o",
            gz_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Decompress with --password-file (only relevant for encrypted archive formats)
    geezipx()
        .args([
            "decompress",
            gz_path.to_str().unwrap(),
            "-o",
            td.path().to_str().unwrap(),
            "--password-file",
            td.path().join("passwd.txt").to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("supported for ZIP"));
}

#[test]
fn zip_password_stdin_empty_fails() {
    let td = TestDir::new();
    td.write("data.txt", "test");

    geezipx()
        .args([
            "compress",
            td.path().join("data.txt").to_str().unwrap(),
            "-f",
            "zip",
            "-o",
            td.path().join("out.zip").to_str().unwrap(),
            "--password-stdin",
        ])
        .write_stdin("\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("empty"));
}

#[test]
fn zip_password_file_test_encrypted() {
    let td = TestDir::new();
    td.write("secret.txt", "classified content");
    td.write("passwd.txt", "secret123");
    let zip_path = td.join("encrypted.zip");

    // Compress with --password-file
    geezipx()
        .args([
            "compress",
            td.path().join("secret.txt").to_str().unwrap(),
            "-f",
            "zip",
            "-o",
            zip_path.to_str().unwrap(),
            "--password-file",
            td.path().join("passwd.txt").to_str().unwrap(),
        ])
        .assert()
        .success();

    // Test with --password-file
    geezipx()
        .args([
            "test",
            zip_path.to_str().unwrap(),
            "--password-file",
            td.path().join("passwd.txt").to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("ok"));
}

// ---------------------------------------------------------------------------
// Single-stream format password rejection tests
// ---------------------------------------------------------------------------

#[test]
fn compress_gzip_with_password_fails() {
    let td = TestDir::new();
    td.write("data.txt", "some data");
    let out_path = td.join("output.gz");

    geezipx()
        .args([
            "compress",
            td.path().join("data.txt").to_str().unwrap(),
            "-f",
            "gz",
            "-o",
            out_path.to_str().unwrap(),
            "--password",
            "mypassword",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("supported for ZIP"));
}

#[test]
fn compress_gzip_with_password_file_fails() {
    let td = TestDir::new();
    td.write("data.txt", "some data");
    td.write("passwd.txt", "secret");
    let out_path = td.join("output.gz");

    geezipx()
        .args([
            "compress",
            td.path().join("data.txt").to_str().unwrap(),
            "-f",
            "gz",
            "-o",
            out_path.to_str().unwrap(),
            "--password-file",
            td.path().join("passwd.txt").to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("supported for ZIP"));
}

#[test]
fn compress_gzip_with_password_stdin_fails() {
    let td = TestDir::new();
    td.write("data.txt", "some data");
    let out_path = td.join("output.gz");

    geezipx()
        .args([
            "compress",
            td.path().join("data.txt").to_str().unwrap(),
            "-f",
            "gz",
            "-o",
            out_path.to_str().unwrap(),
            "--password-stdin",
        ])
        .write_stdin("secret\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("supported for ZIP"));
}

#[test]
fn test_gzip_with_password_fails() {
    let td = TestDir::new();
    td.write("data.txt", "some data");
    let gz_path = td.join("data.gz");

    // Compress WITHOUT password
    geezipx()
        .args([
            "compress",
            td.path().join("data.txt").to_str().unwrap(),
            "-f",
            "gz",
            "-o",
            gz_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Test with --password (should fail for non-encrypted formats)
    geezipx()
        .args([
            "test",
            gz_path.to_str().unwrap(),
            "--password",
            "mypassword",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("supported for ZIP"));
}

// ---------------------------------------------------------------------------
// 7z tests
// ---------------------------------------------------------------------------

/// Create a test .7z archive with the given files.
fn create_7z_archive(files: &[(&str, &[u8])]) -> (tempfile::TempDir, std::path::PathBuf) {
    use sevenz_rust2::compress_to_path;

    let src_dir = tempfile::TempDir::new().unwrap();
    let src_path = src_dir.path();

    for (name, data) in files {
        let path = src_path.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, data).unwrap();
    }

    let out_dir = tempfile::TempDir::new().unwrap();
    let archive_path = out_dir.path().join("test.7z");
    compress_to_path(src_path, &archive_path).expect("create 7z archive");

    (out_dir, archive_path)
}

#[test]
fn compress_7z_roundtrip() {
    let td = TestDir::new();
    td.write("hello.txt", "hello from geezipx 7z");
    let archive = td.join("hello.7z");
    let out_dir = td.join("out");

    geezipx()
        .args([
            "compress",
            td.path().join("hello.txt").to_str().unwrap(),
            "-f",
            "7z",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello.txt"));

    geezipx()
        .args(["test", archive.to_str().unwrap()])
        .assert()
        .success();

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
        "hello from geezipx 7z"
    );
}

#[test]
fn compress_7z_multiple_inputs_recursive_roundtrip() {
    let td = TestDir::new();
    std::fs::create_dir_all(td.path().join("a_empty_input")).unwrap();
    td.write("top.txt", "top-level");
    std::fs::create_dir_all(td.path().join("src/a_empty")).unwrap();
    std::fs::create_dir_all(td.path().join("src/b_nested")).unwrap();
    std::fs::write(td.path().join("src/b_nested/file.txt"), "nested file").unwrap();

    let archive = td.join("bundle.7z");
    let out_dir = td.join("out");

    geezipx()
        .args([
            "compress",
            td.path().join("a_empty_input").to_str().unwrap(),
            td.path().join("top.txt").to_str().unwrap(),
            td.path().join("src").to_str().unwrap(),
            "-r",
            "-f",
            "7z",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    geezipx()
        .args(["test", archive.to_str().unwrap()])
        .assert()
        .success();

    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            out_dir.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(out_dir.join("a_empty_input").is_dir());
    assert_eq!(
        std::fs::read_to_string(out_dir.join("top.txt")).unwrap(),
        "top-level"
    );
    assert_eq!(
        std::fs::read_to_string(out_dir.join("src/b_nested/file.txt")).unwrap(),
        "nested file"
    );
    assert!(out_dir.join("src/a_empty").is_dir());
}

#[test]
fn compress_7z_with_password_roundtrip() {
    let td = TestDir::new();
    td.write("input.txt", "hello world");

    let archive = td.join("encrypted.7z");
    let out_dir = td.join("out");

    geezipx()
        .args([
            "compress",
            td.path().join("input.txt").to_str().unwrap(),
            "-f",
            "7z",
            "-o",
            archive.to_str().unwrap(),
            "--password",
            "secret123",
        ])
        .assert()
        .success();

    geezipx()
        .args(["list", archive.to_str().unwrap(), "--password", "secret123"])
        .assert()
        .success()
        .stdout(predicate::str::contains("input.txt"));

    geezipx()
        .args(["test", archive.to_str().unwrap(), "--password", "secret123"])
        .assert()
        .success();

    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("password").or(predicate::str::contains("Password")));

    geezipx()
        .args(["list", archive.to_str().unwrap(), "--password", "wrongpw"])
        .assert()
        .failure();

    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            out_dir.to_str().unwrap(),
            "--password",
            "wrongpw",
        ])
        .assert()
        .failure();

    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            out_dir.to_str().unwrap(),
            "--password",
            "secret123",
        ])
        .assert()
        .success();

    assert_eq!(
        std::fs::read_to_string(out_dir.join("input.txt")).unwrap(),
        "hello world"
    );
}

#[test]
fn compress_7z_with_password_file_roundtrip() {
    let td = TestDir::new();
    td.write("input.txt", "hello from password file");
    td.write("passwd.txt", "filepw\n");

    let archive = td.join("file-password.7z");
    let out_dir = td.join("file-out");

    geezipx()
        .args([
            "compress",
            td.path().join("input.txt").to_str().unwrap(),
            "-f",
            "7z",
            "-o",
            archive.to_str().unwrap(),
            "--password-file",
            td.path().join("passwd.txt").to_str().unwrap(),
        ])
        .assert()
        .success();

    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            out_dir.to_str().unwrap(),
            "--password-file",
            td.path().join("passwd.txt").to_str().unwrap(),
        ])
        .assert()
        .success();

    assert_eq!(
        std::fs::read_to_string(out_dir.join("input.txt")).unwrap(),
        "hello from password file"
    );
}

#[test]
fn compress_7z_with_password_stdin_roundtrip() {
    let td = TestDir::new();
    td.write("input.txt", "hello from password stdin");

    let archive = td.join("stdin-password.7z");
    let out_dir = td.join("stdin-out");

    geezipx()
        .args([
            "compress",
            td.path().join("input.txt").to_str().unwrap(),
            "-f",
            "7z",
            "-o",
            archive.to_str().unwrap(),
            "--password-stdin",
        ])
        .write_stdin("stdinpw\n")
        .assert()
        .success();

    geezipx()
        .args(["list", archive.to_str().unwrap(), "--password-stdin"])
        .write_stdin("stdinpw\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("input.txt"));

    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            out_dir.to_str().unwrap(),
            "--password-stdin",
        ])
        .write_stdin("stdinpw\n")
        .assert()
        .success();

    assert_eq!(
        std::fs::read_to_string(out_dir.join("input.txt")).unwrap(),
        "hello from password stdin"
    );
}

#[test]
fn list_7z_table_output() {
    let (_dir, archive) =
        create_7z_archive(&[("hello.txt", b"hello world"), ("data.bin", b"\x00\x01\x02")]);

    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello.txt"))
        .stdout(predicate::str::contains("data.bin"))
        .stderr(predicate::str::contains("7z"));
}

#[test]
fn list_7z_json_output() {
    let (_dir, archive) = create_7z_archive(&[("file.txt", b"test content")]);

    geezipx()
        .args(["list", "--json", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("file.txt"));
}

#[test]
fn decompress_7z_roundtrip() {
    let td = TestDir::new();
    let (_dir, archive) = create_7z_archive(&[("data.txt", b"7z decompress test")]);

    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            td.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(td.exists("data.txt"), "data.txt should exist");
    assert_eq!(td.read("data.txt"), "7z decompress test");
}

#[test]
fn test_7z_valid() {
    let (_dir, archive) = create_7z_archive(&[("test.txt", b"hello")]);

    geezipx()
        .args(["test", archive.to_str().unwrap()])
        .assert()
        .success();
}

#[test]
fn test_7z_valid_json() {
    let (_dir, archive) = create_7z_archive(&[("test.txt", b"hello")]);

    geezipx()
        .args(["test", "--json", archive.to_str().unwrap()])
        .assert()
        .success();
}

#[test]
fn test_7z_corrupted_fails() {
    let td = TestDir::new();
    let bad_7z = td.join("bad.7z");
    std::fs::write(&bad_7z, b"not a real 7z file").unwrap();

    geezipx()
        .args(["test", bad_7z.to_str().unwrap()])
        .assert()
        .failure();
}

#[test]
fn list_nonexistent_7z_fails() {
    geezipx()
        .args(["list", "/nonexistent/path.7z"])
        .assert()
        .failure();
}

// ---------------------------------------------------------------------------
// Encrypted 7z tests
// ---------------------------------------------------------------------------

/// Create an encrypted test 7z archive with password protection.
fn create_encrypted_7z_archive(
    files: &[(&str, &[u8])],
    password: &str,
) -> (tempfile::TempDir, std::path::PathBuf) {
    use sevenz_rust2::compress_to_path_encrypted;
    use sevenz_rust2::Password;

    let src_dir = tempfile::TempDir::new().unwrap();
    let src_path = src_dir.path();

    for (name, data) in files {
        let path = src_path.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, data).unwrap();
    }

    let out_dir = tempfile::TempDir::new().unwrap();
    let archive_path = out_dir.path().join("encrypted.7z");
    compress_to_path_encrypted(src_path, &archive_path, Password::from(password))
        .expect("create encrypted 7z");

    (out_dir, archive_path)
}

#[test]
fn list_encrypted_7z_with_password() {
    let (_dir, archive) =
        create_encrypted_7z_archive(&[("secret.txt", b"hidden content")], "correctpw");

    geezipx()
        .args(["list", archive.to_str().unwrap(), "--password", "correctpw"])
        .assert()
        .success()
        .stdout(predicate::str::contains("secret.txt"));
}

#[test]
fn list_encrypted_7z_without_password_fails() {
    let (_dir, archive) =
        create_encrypted_7z_archive(&[("secret.txt", b"hidden content")], "correctpw");

    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Password").or(predicate::str::contains("password")));
}

#[test]
fn list_encrypted_7z_with_wrong_password_fails() {
    let (_dir, archive) =
        create_encrypted_7z_archive(&[("secret.txt", b"hidden content")], "correctpw");

    geezipx()
        .args(["list", archive.to_str().unwrap(), "--password", "wrongpw"])
        .assert()
        .failure();
}

#[test]
fn decompress_encrypted_7z_with_password() {
    let td = TestDir::new();
    let (_dir, archive) =
        create_encrypted_7z_archive(&[("data.txt", b"7z encrypted content")], "correctpw");
    let out_dir = td.path();

    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            out_dir.to_str().unwrap(),
            "--password",
            "correctpw",
        ])
        .assert()
        .success();

    assert!(td.exists("data.txt"), "data.txt should exist");
    assert_eq!(td.read("data.txt"), "7z encrypted content");
}

#[test]
fn list_encrypted_7z_with_password_file() {
    let td = TestDir::new();
    td.write("passwd.txt", "correctpw\n");
    let (_dir, archive) =
        create_encrypted_7z_archive(&[("secret.txt", b"hidden content")], "correctpw");

    geezipx()
        .args([
            "list",
            archive.to_str().unwrap(),
            "--password-file",
            td.path().join("passwd.txt").to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("secret.txt"));
}

// ---------------------------------------------------------------------------
// list --password* tests for encrypted ZIP
// ---------------------------------------------------------------------------

#[test]
fn list_encrypted_zip_with_password_file() {
    let td = TestDir::new();
    td.write("secret.txt", "classified content");
    td.write("passwd.txt", "secret123\n");
    let zip_path = td.join("encrypted.zip");

    // Compress with password
    geezipx()
        .args([
            "compress",
            td.path().join("secret.txt").to_str().unwrap(),
            "-f",
            "zip",
            "-o",
            zip_path.to_str().unwrap(),
            "--password",
            "secret123",
        ])
        .assert()
        .success();

    // List with --password-file
    geezipx()
        .args([
            "list",
            zip_path.to_str().unwrap(),
            "--password-file",
            td.path().join("passwd.txt").to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("secret.txt"));
}

#[test]
fn list_encrypted_zip_with_password_stdin() {
    let td = TestDir::new();
    td.write("secret.txt", "stdin test");
    let zip_path = td.join("encrypted.zip");

    // Compress with password
    geezipx()
        .args([
            "compress",
            td.path().join("secret.txt").to_str().unwrap(),
            "-f",
            "zip",
            "-o",
            zip_path.to_str().unwrap(),
            "--password",
            "stdinpw",
        ])
        .assert()
        .success();

    // List with --password-stdin
    geezipx()
        .args(["list", zip_path.to_str().unwrap(), "--password-stdin"])
        .write_stdin("stdinpw\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("secret.txt"));
}

#[test]
fn list_encrypted_zip_without_password_fails() {
    let td = TestDir::new();
    td.write("secret.txt", "test");
    let zip_path = td.join("encrypted.zip");

    // Compress with password
    geezipx()
        .args([
            "compress",
            td.path().join("secret.txt").to_str().unwrap(),
            "-f",
            "zip",
            "-o",
            zip_path.to_str().unwrap(),
            "--password",
            "secret123",
        ])
        .assert()
        .success();

    // List without password fails
    geezipx()
        .args(["list", zip_path.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Password"));
}

#[test]
fn list_encrypted_zip_with_password() {
    // List encrypted zip with direct --password should succeed
    let td = TestDir::new();
    td.write("secret.txt", "direct password test");
    let zip_path = td.join("encrypted.zip");

    // Compress with password
    geezipx()
        .args([
            "compress",
            td.path().join("secret.txt").to_str().unwrap(),
            "-f",
            "zip",
            "-o",
            zip_path.to_str().unwrap(),
            "--password",
            "directpw",
        ])
        .assert()
        .success();

    // List with --password direct
    geezipx()
        .args(["list", zip_path.to_str().unwrap(), "--password", "directpw"])
        .assert()
        .success()
        .stdout(predicate::str::contains("secret.txt"));
}

#[test]
fn list_plain_zip_with_password_fails() {
    // Using --password on an unencrypted zip should succeed
    // (password is just passed through and ignored for unencrypted entries)
    let td = TestDir::new();
    td.write("file.txt", "hello");
    let zip_path = td.join("plain.zip");

    // Compress without password
    geezipx()
        .args([
            "compress",
            td.path().join("file.txt").to_str().unwrap(),
            "-f",
            "zip",
            "-o",
            zip_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    // List with --password (password ignored for unencrypted entries)
    geezipx()
        .args([
            "list",
            zip_path.to_str().unwrap(),
            "--password",
            "irrelevant",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("file.txt"));
}

#[test]
fn list_gzip_with_password_fails() {
    let td = TestDir::new();
    td.write("data.txt", "some data");
    let gz_path = td.join("data.gz");

    // Compress without password
    geezipx()
        .args([
            "compress",
            td.path().join("data.txt").to_str().unwrap(),
            "-f",
            "gz",
            "-o",
            gz_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    // List with --password should fail
    geezipx()
        .args(["list", gz_path.to_str().unwrap(), "--password", "foo"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("supported for ZIP"));
}

#[test]
fn list_zstd_with_password_file_fails() {
    let td = TestDir::new();
    td.write("data.txt", "some data");
    td.write("passwd.txt", "secret\n");
    let zst_path = td.join("data.zst");

    // Compress without password
    geezipx()
        .args([
            "compress",
            td.path().join("data.txt").to_str().unwrap(),
            "-f",
            "zst",
            "-o",
            zst_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    // List with --password-file should fail
    geezipx()
        .args([
            "list",
            zst_path.to_str().unwrap(),
            "--password-file",
            td.path().join("passwd.txt").to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("supported for ZIP"));
}

#[test]
fn list_xz_with_password_stdin_fails() {
    let td = TestDir::new();
    td.write("data.txt", "some data");
    let xz_path = td.join("data.xz");

    // Compress without password
    geezipx()
        .args([
            "compress",
            td.path().join("data.txt").to_str().unwrap(),
            "-f",
            "xz",
            "-o",
            xz_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    // List with --password-stdin should fail
    geezipx()
        .args(["list", xz_path.to_str().unwrap(), "--password-stdin"])
        .write_stdin("irrelevant\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("supported for ZIP"));
}

// ---------------------------------------------------------------------------
// Stdin/stdout pipe mode tests (Phase 2.5)
// ---------------------------------------------------------------------------

#[test]
fn compress_stdin_gzip_to_file() {
    let td = TestDir::new();
    let out_path = td.join("out.gz");

    geezipx()
        .args([
            "compress",
            "--stdin",
            "-f",
            "gz",
            "-o",
            out_path.to_str().unwrap(),
        ])
        .write_stdin("Hello from stdin pipe\n")
        .assert()
        .success();

    assert!(out_path.exists(), "gzip output should exist");

    // Roundtrip: decompress and verify
    geezipx()
        .args(["decompress", out_path.to_str().unwrap(), "--stdout"])
        .assert()
        .success()
        .stdout("Hello from stdin pipe\n");
}

#[test]
fn compress_stdin_zstd_to_file() {
    let td = TestDir::new();
    let out_path = td.join("out.zst");

    geezipx()
        .args([
            "compress",
            "--stdin",
            "-f",
            "zst",
            "-o",
            out_path.to_str().unwrap(),
        ])
        .write_stdin("Zstd stdin test data\n")
        .assert()
        .success();

    assert!(out_path.exists(), "zstd output should exist");

    // Roundtrip
    geezipx()
        .args(["decompress", out_path.to_str().unwrap(), "--stdout"])
        .assert()
        .success()
        .stdout("Zstd stdin test data\n");
}

#[test]
fn compress_stdin_xz_to_file() {
    let td = TestDir::new();
    let out_path = td.join("out.xz");

    geezipx()
        .args([
            "compress",
            "--stdin",
            "-f",
            "xz",
            "-o",
            out_path.to_str().unwrap(),
        ])
        .write_stdin("Xz stdin pipe content\n")
        .assert()
        .success();

    assert!(out_path.exists(), "xz output should exist");

    geezipx()
        .args(["decompress", out_path.to_str().unwrap(), "--stdout"])
        .assert()
        .success()
        .stdout("Xz stdin pipe content\n");
}

#[test]
fn compress_stdin_lzma_to_file() {
    let td = TestDir::new();
    let out_path = td.join("out.lzma");

    geezipx()
        .args([
            "compress",
            "--stdin",
            "-f",
            "lzma",
            "-o",
            out_path.to_str().unwrap(),
        ])
        .write_stdin("Lzma stdin data\n")
        .assert()
        .success();

    assert!(out_path.exists(), "lzma output should exist");

    geezipx()
        .args(["decompress", out_path.to_str().unwrap(), "--stdout"])
        .assert()
        .success()
        .stdout("Lzma stdin data\n");
}

#[test]
fn compress_stdin_stdout_roundtrip() {
    // stdin -> gzip -> stdout -> decompress stdin -> stdout
    let input_data = "Full pipe roundtrip test\n";

    // pipe: echo "..." | compress --stdin -f gz --stdout | decompress --stdin -f gz --stdout
    // We can't easily chain commands in assert_cmd, so test each direction separately.

    // Direction 1: stdin -> stdout compress
    geezipx()
        .args(["compress", "--stdin", "-f", "gz", "--stdout"])
        .write_stdin(input_data)
        .assert()
        .success()
        .stdout(predicate::function(|output: &[u8]| !output.is_empty()));
}

#[test]
fn compress_file_to_stdout() {
    let td = TestDir::new();
    td.write("hello.txt", "Hello stdout compression\n");

    geezipx()
        .args([
            "compress",
            td.path().join("hello.txt").to_str().unwrap(),
            "-f",
            "gz",
            "--stdout",
        ])
        .assert()
        .success()
        .stdout(predicate::function(|output: &[u8]| !output.is_empty()));
}

#[test]
fn decompress_stdin_to_stdout() {
    let td = TestDir::new();
    td.write("data.txt", "decompress stdin to stdout\n");
    let gz_path = td.join("data.txt.gz");

    // First create a gzip file
    geezipx()
        .args([
            "compress",
            td.path().join("data.txt").to_str().unwrap(),
            "-f",
            "gz",
            "-o",
            gz_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Now read the gzip bytes and pipe through decompress --stdin --stdout
    let compressed_bytes = std::fs::read(&gz_path).unwrap();

    geezipx()
        .args(["decompress", "--stdin", "-f", "gz", "--stdout"])
        .write_stdin(compressed_bytes.clone())
        .assert()
        .success()
        .stdout("decompress stdin to stdout\n");
}

#[test]
fn decompress_stdin_to_dir() {
    let td = TestDir::new();
    td.write("data.txt", "decompress stdin to dir\n");
    let gz_path = td.join("data.txt.gz");

    // Create gzip file
    geezipx()
        .args([
            "compress",
            td.path().join("data.txt").to_str().unwrap(),
            "-f",
            "gz",
            "-o",
            gz_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Stdin decompress to directory: should create {output_dir}/output
    let out_dir = td.join("extracted");
    std::fs::create_dir_all(&out_dir).unwrap();
    let compressed_bytes = std::fs::read(&gz_path).unwrap();

    geezipx()
        .args([
            "decompress",
            "--stdin",
            "-f",
            "gz",
            "-o",
            out_dir.to_str().unwrap(),
        ])
        .write_stdin(compressed_bytes)
        .assert()
        .success();

    let output_file = out_dir.join("output");
    assert!(output_file.exists(), "output file should exist");
    assert_eq!(
        std::fs::read_to_string(&output_file).unwrap(),
        "decompress stdin to dir\n"
    );
}

#[test]
fn decompress_stdin_to_dir_no_clobber() {
    let td = TestDir::new();
    td.write("data.txt", "decompress stdin no-clobber\n");
    let gz_path = td.join("data.txt.gz");

    // Create gzip file
    geezipx()
        .args([
            "compress",
            td.path().join("data.txt").to_str().unwrap(),
            "-f",
            "gz",
            "-o",
            gz_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Create the output file ahead of time
    let out_dir = td.join("extracted");
    std::fs::create_dir_all(&out_dir).unwrap();
    let output_file = out_dir.join("output");
    std::fs::write(&output_file, "dummy content\n").unwrap();

    let compressed_bytes = std::fs::read(&gz_path).unwrap();

    // Stdin decompress to directory with no-clobber (default) should skip
    geezipx()
        .args([
            "decompress",
            "--stdin",
            "-f",
            "gz",
            "-o",
            out_dir.to_str().unwrap(),
            "--no-clobber",
        ])
        .write_stdin(compressed_bytes.clone())
        .assert()
        .success()
        .stderr(predicate::str::contains("skipping"));

    // Output file should still contain original dummy content
    assert_eq!(
        std::fs::read_to_string(&output_file).unwrap(),
        "dummy content\n"
    );

    // Now with --force it should overwrite
    geezipx()
        .args([
            "decompress",
            "--stdin",
            "-f",
            "gz",
            "-o",
            out_dir.to_str().unwrap(),
            "--force",
        ])
        .write_stdin(compressed_bytes)
        .assert()
        .success();

    // Output file should now contain decompressed data
    assert_eq!(
        std::fs::read_to_string(&output_file).unwrap(),
        "decompress stdin no-clobber\n"
    );
}

#[test]
fn compress_stdin_requires_format() {
    geezipx()
        .args(["compress", "--stdin", "-o", "out.gz"])
        .write_stdin("data")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--format").or(predicate::str::contains("format")));
}

#[test]
fn compress_stdin_with_zip_fails() {
    geezipx()
        .args(["compress", "--stdin", "-f", "zip", "-o", "out.zip"])
        .write_stdin("data")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("only supported for single-stream")
                .or(predicate::str::contains("single-stream")),
        );
}

#[test]
fn compress_stdin_with_tar_fails() {
    geezipx()
        .args(["compress", "--stdin", "-f", "tar", "-o", "out.tar"])
        .write_stdin("data")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("only supported for single-stream")
                .or(predicate::str::contains("single-stream")),
        );
}

#[test]
fn compress_stdin_targz_pipe_to_file_and_back() {
    // stdin -> tar.gz (compress to file) -> decompress --stdout -> original tar
    let td = TestDir::new();
    td.write("test_data.txt", "hello from tar.gz stdin test\n");

    // Create raw tar with compress -f tar.
    let tar_path = td.join("test.tar");
    geezipx()
        .args([
            "compress",
            td.path().join("test_data.txt").to_str().unwrap(),
            "-f",
            "tar",
            "-o",
            tar_path.to_str().unwrap(),
        ])
        .assert()
        .success();
    let tar_bytes = std::fs::read(&tar_path).unwrap();
    assert!(!tar_bytes.is_empty(), "raw tar should have data");

    // Pipe raw tar through compress --stdin -f tar.gz.
    let gz_path = td.join("out.tar.gz");
    geezipx()
        .args([
            "compress",
            "--stdin",
            "-f",
            "tar.gz",
            "-o",
            gz_path.to_str().unwrap(),
        ])
        .write_stdin(tar_bytes.clone())
        .assert()
        .success();
    assert!(gz_path.exists(), "tar.gz output should exist");

    // Round-trip: decompress --stdin --stdout, verify raw tar is recovered.
    let compressed_bytes = std::fs::read(&gz_path).unwrap();
    geezipx()
        .args(["decompress", "--stdin", "-f", "tar.gz", "--stdout"])
        .write_stdin(compressed_bytes)
        .assert()
        .success()
        .stdout(tar_bytes.clone());
}

#[test]
fn compress_stdin_tarzst_pipe_to_file_and_back() {
    // stdin -> tar.zst (compress to file) -> decompress --stdout -> original tar
    let td = TestDir::new();
    td.write("test_data.txt", "hello from tar.zst stdin test\n");

    // Create raw tar with compress -f tar.
    let tar_path = td.join("test.tar");
    geezipx()
        .args([
            "compress",
            td.path().join("test_data.txt").to_str().unwrap(),
            "-f",
            "tar",
            "-o",
            tar_path.to_str().unwrap(),
        ])
        .assert()
        .success();
    let tar_bytes = std::fs::read(&tar_path).unwrap();
    assert!(!tar_bytes.is_empty(), "raw tar should have data");

    // Pipe raw tar through compress --stdin -f tar.zst.
    let zst_path = td.join("out.tar.zst");
    geezipx()
        .args([
            "compress",
            "--stdin",
            "-f",
            "tar.zst",
            "-o",
            zst_path.to_str().unwrap(),
        ])
        .write_stdin(tar_bytes.clone())
        .assert()
        .success();
    assert!(zst_path.exists(), "tar.zst output should exist");

    // Round-trip: decompress --stdin --stdout, verify raw tar is recovered.
    let compressed_bytes = std::fs::read(&zst_path).unwrap();
    geezipx()
        .args(["decompress", "--stdin", "-f", "tar.zst", "--stdout"])
        .write_stdin(compressed_bytes)
        .assert()
        .success()
        .stdout(tar_bytes.clone());
}

#[test]
fn compress_stdin_tarxz_pipe_to_file_and_back() {
    // stdin -> tar.xz (compress to file) -> decompress --stdout -> original tar
    let td = TestDir::new();
    td.write("test_data.txt", "hello from tar.xz stdin test\n");

    // Create raw tar with compress -f tar.
    let tar_path = td.join("test.tar");
    geezipx()
        .args([
            "compress",
            td.path().join("test_data.txt").to_str().unwrap(),
            "-f",
            "tar",
            "-o",
            tar_path.to_str().unwrap(),
        ])
        .assert()
        .success();
    let tar_bytes = std::fs::read(&tar_path).unwrap();
    assert!(!tar_bytes.is_empty(), "raw tar should have data");

    // Pipe raw tar through compress --stdin -f tar.xz.
    let xz_path = td.join("out.tar.xz");
    geezipx()
        .args([
            "compress",
            "--stdin",
            "-f",
            "tar.xz",
            "-o",
            xz_path.to_str().unwrap(),
        ])
        .write_stdin(tar_bytes.clone())
        .assert()
        .success();
    assert!(xz_path.exists(), "tar.xz output should exist");

    // Round-trip: decompress --stdin --stdout, verify raw tar is recovered.
    let compressed_bytes = std::fs::read(&xz_path).unwrap();
    geezipx()
        .args(["decompress", "--stdin", "-f", "tar.xz", "--stdout"])
        .write_stdin(compressed_bytes)
        .assert()
        .success()
        .stdout(tar_bytes.clone());
}

#[test]
fn compress_stdin_with_7z_fails() {
    geezipx()
        .args(["compress", "--stdin", "-f", "7z", "-o", "out.7z"])
        .write_stdin("data")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("only supported for single-stream")
                .or(predicate::str::contains("single-stream")),
        );
}

#[test]
fn decompress_stdin_requires_format() {
    geezipx()
        .args(["decompress", "--stdin", "-o", "."])
        .write_stdin("data")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--format").or(predicate::str::contains("format")));
}

#[test]
fn decompress_stdin_with_zip_fails() {
    geezipx()
        .args(["decompress", "--stdin", "-f", "zip", "-o", "."])
        .write_stdin("data")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("only supported for single-stream")
                .or(predicate::str::contains("single-stream")),
        );
}

#[test]
fn decompress_stdin_with_tar_fails() {
    geezipx()
        .args(["decompress", "--stdin", "-f", "tar", "-o", "."])
        .write_stdin("data")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("only supported for single-stream")
                .or(predicate::str::contains("single-stream")),
        );
}

#[test]
fn decompress_stdin_with_7z_fails() {
    geezipx()
        .args(["decompress", "--stdin", "-f", "7z", "-o", "."])
        .write_stdin("data")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("only supported for single-stream")
                .or(predicate::str::contains("single-stream")),
        );
}

#[test]
fn compress_stdout_with_zip_fails() {
    let td = TestDir::new();
    td.write("test.txt", "data");
    geezipx()
        .args([
            "compress",
            td.path().join("test.txt").to_str().unwrap(),
            "--stdout",
            "-f",
            "zip",
        ])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("only supported for single-stream")
                .or(predicate::str::contains("single-stream")),
        );
}

#[test]
fn compress_stdin_and_file_conflict() {
    let td = TestDir::new();
    td.write("input.txt", "data");
    geezipx()
        .args([
            "compress",
            td.path().join("input.txt").to_str().unwrap(),
            "--stdin",
            "-f",
            "gz",
            "-o",
            "out.gz",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("mutually exclusive"));
}

#[test]
fn decompress_stdin_and_archive_conflict() {
    let td = TestDir::new();
    td.write("test.gz", "dummy");
    geezipx()
        .args([
            "decompress",
            td.path().join("test.gz").to_str().unwrap(),
            "--stdin",
            "-f",
            "gz",
            "-o",
            ".",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("mutually exclusive"));
}

#[test]
fn compress_stdout_requires_format() {
    let td = TestDir::new();
    td.write("in.txt", "data");
    geezipx()
        .args([
            "compress",
            td.path().join("in.txt").to_str().unwrap(),
            "--stdout",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--format").or(predicate::str::contains("format")));
}

#[test]
fn bzip2_stdout_roundtrip() {
    let tmp = TestDir::new();
    let content = "Hello, GeeZipX! bzip2 --stdout round-trip.";
    tmp.write("hello.txt", content);
    let archive = tmp.join("hello.txt.bz2");

    geezipx()
        .args([
            "compress",
            tmp.join("hello.txt").to_str().unwrap(),
            "-f",
            "bz2",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello.txt"));

    geezipx()
        .args(["decompress", archive.to_str().unwrap(), "--stdout"])
        .assert()
        .success()
        .stdout(content);
}

#[test]
fn bzip2_auto_format_from_bz2_extension() {
    let tmp = TestDir::new();
    let content = "Auto-format bzip2 via .bz2 extension.";
    tmp.write("auto.txt", content);
    let archive = tmp.join("auto.txt.bz2");

    geezipx()
        .args([
            "compress",
            tmp.join("auto.txt").to_str().unwrap(),
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
fn tarbz2_recursive_roundtrip() {
    let tmp = TestDir::new();
    let src = tmp.join("src");
    std::fs::create_dir_all(src.join("nested")).unwrap();
    std::fs::write(src.join("root.txt"), "root level").unwrap();
    std::fs::write(src.join("nested").join("deep.txt"), "nested level").unwrap();

    let archive = tmp.join("out.tar.bz2");
    let output = tmp.join("extracted");

    geezipx()
        .args([
            "compress",
            src.to_str().unwrap(),
            "-r",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("root.txt"))
        .stdout(predicate::str::contains("nested/deep.txt"));

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
fn tarbz2_stdout_outputs_raw_tar_stream() {
    let tmp = TestDir::new();
    tmp.write("data.txt", "raw tar from tar.bz2 stdout");
    let archive = tmp.join("data.tar.bz2");

    geezipx()
        .args([
            "compress",
            tmp.join("data.txt").to_str().unwrap(),
            "-f",
            "tar.bz2",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    geezipx()
        .args(["decompress", archive.to_str().unwrap(), "--stdout"])
        .assert()
        .success()
        .stdout(predicate::str::contains("data.txt"));
}

#[test]
fn tarbz2_stdout_with_multiple_inputs_requires_output_file() {
    let td = TestDir::new();
    let a = td.join("a.txt");
    let b = td.join("b.txt");
    std::fs::write(&a, "first").unwrap();
    std::fs::write(&b, "second").unwrap();

    geezipx()
        .args([
            "compress",
            a.to_str().unwrap(),
            b.to_str().unwrap(),
            "--stdout",
            "-f",
            "tar.bz2",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("raw tar input via --stdin"))
        .stderr(predicate::str::contains("-o/--output"));
}

#[test]
fn compress_stdin_tarbz2_pipe_to_file_and_back() {
    let td = TestDir::new();
    td.write("test_data.txt", "hello from tar.bz2 stdin test\n");

    let tar_path = td.join("test.tar");
    geezipx()
        .args([
            "compress",
            td.path().join("test_data.txt").to_str().unwrap(),
            "-f",
            "tar",
            "-o",
            tar_path.to_str().unwrap(),
        ])
        .assert()
        .success();
    let tar_bytes = std::fs::read(&tar_path).unwrap();
    assert!(!tar_bytes.is_empty(), "raw tar should have data");

    let bz2_path = td.join("out.tar.bz2");
    geezipx()
        .args([
            "compress",
            "--stdin",
            "-f",
            "tar.bz2",
            "-o",
            bz2_path.to_str().unwrap(),
        ])
        .write_stdin(tar_bytes.clone())
        .assert()
        .success();
    assert!(bz2_path.exists(), "tar.bz2 output should exist");

    let compressed_bytes = std::fs::read(&bz2_path).unwrap();
    geezipx()
        .args(["decompress", "--stdin", "-f", "tar.bz2", "--stdout"])
        .write_stdin(compressed_bytes)
        .assert()
        .success()
        .stdout(tar_bytes.clone());
}

#[test]
fn brotli_stdout_roundtrip() {
    let tmp = TestDir::new();
    let content = "Hello, GeeZipX! brotli --stdout round-trip.";
    tmp.write("hello.txt", content);
    let archive = tmp.join("hello.txt.br");

    geezipx()
        .args([
            "compress",
            tmp.join("hello.txt").to_str().unwrap(),
            "-f",
            "brotli",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello.txt"));

    geezipx()
        .args(["decompress", archive.to_str().unwrap(), "--stdout"])
        .assert()
        .success()
        .stdout(content);
}

#[test]
fn brotli_auto_format_from_br_extension() {
    let tmp = TestDir::new();
    let content = "Auto-format brotli via .br extension.";
    tmp.write("auto.txt", content);
    let archive = tmp.join("auto.txt.br");

    geezipx()
        .args([
            "compress",
            tmp.join("auto.txt").to_str().unwrap(),
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
fn brotli_level_12_rejected() {
    let tmp = TestDir::new();
    tmp.write("test.txt", "brotli level reject test");

    geezipx()
        .args([
            "compress",
            tmp.join("test.txt").to_str().unwrap(),
            "-f",
            "brotli",
            "-L",
            "12",
            "-o",
            tmp.join("out.br").to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("0..=11"));
}

#[test]
fn lz4_stdout_roundtrip() {
    let tmp = TestDir::new();
    let content = "Hello, GeeZipX! lz4 --stdout round-trip.";
    tmp.write("hello.txt", content);
    let archive = tmp.join("hello.txt.lz4");

    geezipx()
        .args([
            "compress",
            tmp.join("hello.txt").to_str().unwrap(),
            "-f",
            "lz4",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello.txt"));

    geezipx()
        .args(["decompress", archive.to_str().unwrap(), "--stdout"])
        .assert()
        .success()
        .stdout(content);
}

#[test]
fn lz4_auto_format_from_lz4_extension() {
    let tmp = TestDir::new();
    let content = "Auto-format lz4 via .lz4 extension.";
    tmp.write("auto.txt", content);
    let archive = tmp.join("auto.txt.lz4");

    geezipx()
        .args([
            "compress",
            tmp.join("auto.txt").to_str().unwrap(),
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
fn lz4_level_1_rejected() {
    let tmp = TestDir::new();
    tmp.write("test.txt", "lz4 level reject test");

    geezipx()
        .args([
            "compress",
            tmp.join("test.txt").to_str().unwrap(),
            "-f",
            "lz4",
            "-L",
            "1",
            "-o",
            tmp.join("out.lz4").to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("use 0 or omit"));
}

#[test]
fn tarbr_auto_format_from_tar_br_extension() {
    let tmp = TestDir::new();
    tmp.write("hello.txt", "Auto-format tar.br.");
    let archive = tmp.join("out.tar.br");
    let output = tmp.join("out");

    geezipx()
        .args([
            "compress",
            tmp.join("hello.txt").to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello.txt"));

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
        std::fs::read_to_string(output.join("hello.txt")).unwrap(),
        "Auto-format tar.br."
    );
}

#[test]
fn tarbr_explicit_format_roundtrip() {
    let tmp = TestDir::new();
    let src = tmp.join("src");
    std::fs::create_dir_all(src.join("nested")).unwrap();
    std::fs::write(src.join("root.txt"), "root level").unwrap();
    std::fs::write(src.join("nested").join("deep.txt"), "nested level").unwrap();
    let archive = tmp.join("out.tar.br");
    let output = tmp.join("extracted");

    geezipx()
        .args([
            "compress",
            src.to_str().unwrap(),
            "-r",
            "-f",
            "tar.br",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

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
fn tarbr_stdout_outputs_raw_tar_stream() {
    let tmp = TestDir::new();
    tmp.write("data.txt", "raw tar from tar.br stdout");
    let archive = tmp.join("data.tar.br");

    geezipx()
        .args([
            "compress",
            tmp.join("data.txt").to_str().unwrap(),
            "-f",
            "tar.br",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    geezipx()
        .args(["decompress", archive.to_str().unwrap(), "--stdout"])
        .assert()
        .success()
        .stdout(predicate::str::contains("data.txt"));
}

#[test]
fn tarbr_stdout_with_directory_requires_output_file() {
    let td = TestDir::new();
    let src = td.join("src");
    std::fs::create_dir_all(src.join("nested")).unwrap();
    std::fs::write(src.join("root.txt"), "root level").unwrap();
    std::fs::write(src.join("nested").join("deep.txt"), "nested level").unwrap();

    geezipx()
        .args([
            "compress",
            src.to_str().unwrap(),
            "-r",
            "--stdout",
            "-f",
            "tar.br",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("raw tar input via --stdin"))
        .stderr(predicate::str::contains("-o/--output"));
}

#[test]
fn compress_stdin_tarbr_pipe_to_file_and_back() {
    let td = TestDir::new();
    td.write("test_data.txt", "hello from tar.br stdin test\n");

    let tar_path = td.join("test.tar");
    geezipx()
        .args([
            "compress",
            td.path().join("test_data.txt").to_str().unwrap(),
            "-f",
            "tar",
            "-o",
            tar_path.to_str().unwrap(),
        ])
        .assert()
        .success();
    let tar_bytes = std::fs::read(&tar_path).unwrap();
    assert!(!tar_bytes.is_empty(), "raw tar should have data");

    let br_path = td.join("out.tar.br");
    geezipx()
        .args([
            "compress",
            "--stdin",
            "-f",
            "tar.br",
            "-o",
            br_path.to_str().unwrap(),
        ])
        .write_stdin(tar_bytes.clone())
        .assert()
        .success();
    assert!(br_path.exists(), "tar.br output should exist");

    let compressed_bytes = std::fs::read(&br_path).unwrap();
    geezipx()
        .args(["decompress", "--stdin", "-f", "tar.br", "--stdout"])
        .write_stdin(compressed_bytes)
        .assert()
        .success()
        .stdout(tar_bytes.clone());
}

#[test]
fn tarlz4_auto_format_from_tar_lz4_extension() {
    let tmp = TestDir::new();
    tmp.write("hello.txt", "Auto-format tar.lz4.");
    let archive = tmp.join("out.tar.lz4");
    let output = tmp.join("out");

    geezipx()
        .args([
            "compress",
            tmp.join("hello.txt").to_str().unwrap(),
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello.txt"));

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
        std::fs::read_to_string(output.join("hello.txt")).unwrap(),
        "Auto-format tar.lz4."
    );
}

#[test]
fn tarlz4_explicit_format_roundtrip() {
    let tmp = TestDir::new();
    let src = tmp.join("src");
    std::fs::create_dir_all(src.join("nested")).unwrap();
    std::fs::write(src.join("root.txt"), "root level").unwrap();
    std::fs::write(src.join("nested").join("deep.txt"), "nested level").unwrap();
    let archive = tmp.join("out.tar.lz4");
    let output = tmp.join("extracted");

    geezipx()
        .args([
            "compress",
            src.to_str().unwrap(),
            "-r",
            "-f",
            "tar.lz4",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

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
fn tarlz4_stdout_outputs_raw_tar_stream() {
    let tmp = TestDir::new();
    tmp.write("data.txt", "raw tar from tar.lz4 stdout");
    let archive = tmp.join("data.tar.lz4");

    geezipx()
        .args([
            "compress",
            tmp.join("data.txt").to_str().unwrap(),
            "-f",
            "tar.lz4",
            "-o",
            archive.to_str().unwrap(),
        ])
        .assert()
        .success();

    geezipx()
        .args(["decompress", archive.to_str().unwrap(), "--stdout"])
        .assert()
        .success()
        .stdout(predicate::str::contains("data.txt"));
}

#[test]
fn tarlz4_stdout_with_file_requires_output_file() {
    let td = TestDir::new();
    td.write("data.txt", "tar.lz4 stdout should stay on archive path");

    geezipx()
        .args([
            "compress",
            td.join("data.txt").to_str().unwrap(),
            "--stdout",
            "-f",
            "tar.lz4",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("raw tar input via --stdin"))
        .stderr(predicate::str::contains("-o/--output"));
}

#[test]
fn compress_stdin_tarlz4_pipe_to_file_and_back() {
    let td = TestDir::new();
    td.write("test_data.txt", "hello from tar.lz4 stdin test\n");

    let tar_path = td.join("test.tar");
    geezipx()
        .args([
            "compress",
            td.path().join("test_data.txt").to_str().unwrap(),
            "-f",
            "tar",
            "-o",
            tar_path.to_str().unwrap(),
        ])
        .assert()
        .success();
    let tar_bytes = std::fs::read(&tar_path).unwrap();
    assert!(!tar_bytes.is_empty(), "raw tar should have data");

    let lz4_path = td.join("out.tar.lz4");
    geezipx()
        .args([
            "compress",
            "--stdin",
            "-f",
            "tar.lz4",
            "-o",
            lz4_path.to_str().unwrap(),
        ])
        .write_stdin(tar_bytes.clone())
        .assert()
        .success();
    assert!(lz4_path.exists(), "tar.lz4 output should exist");

    let compressed_bytes = std::fs::read(&lz4_path).unwrap();
    geezipx()
        .args(["decompress", "--stdin", "-f", "tar.lz4", "--stdout"])
        .write_stdin(compressed_bytes)
        .assert()
        .success()
        .stdout(tar_bytes.clone());
}

#[test]
fn zip_alias_formats_list_and_decompress() {
    let tmp = TestDir::new();

    for alias in ["jar", "war", "apk", "ipa", "xpi"] {
        let input_name = format!("input-{alias}.txt");
        let content = format!("ZIP alias round-trip for {alias}");
        tmp.write(&input_name, &content);
        let archive = tmp.join(&format!("bundle.{alias}"));
        let output = tmp.join(&format!("out-{alias}"));
        std::fs::create_dir_all(&output).unwrap();

        geezipx()
            .args([
                "compress",
                tmp.join(&input_name).to_str().unwrap(),
                "-f",
                alias,
                "-o",
                archive.to_str().unwrap(),
            ])
            .assert()
            .success();

        geezipx()
            .args(["list", archive.to_str().unwrap()])
            .assert()
            .success()
            .stdout(predicate::str::contains(&input_name));

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
            std::fs::read_to_string(output.join(&input_name)).unwrap(),
            content
        );
    }
}

// ---------------------------------------------------------------------------
// DEB read-only tests
// ---------------------------------------------------------------------------

fn append_tar_file(out: &mut Vec<u8>, path: &str, data: &[u8]) {
    let path_bytes = path.as_bytes();
    let mut header = [0u8; 512];
    let name_len = path_bytes.len().min(99);
    header[..name_len].copy_from_slice(&path_bytes[..name_len]);
    header[100..108].copy_from_slice(b"0000644\0");
    let size_oct = format!("{:011o}\0", data.len());
    header[124..136].copy_from_slice(size_oct.as_bytes());
    header[156] = b'0';
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    for b in header.iter_mut().take(156).skip(148) {
        *b = b' ';
    }
    let cksum: u32 = header.iter().map(|&b| b as u32).sum();
    let cksum_str = format!("{:06o}\0 ", cksum);
    header[148..156].copy_from_slice(cksum_str.as_bytes());

    out.extend_from_slice(&header);
    out.extend_from_slice(data);
    let padding = (512 - data.len() % 512) % 512;
    out.extend(std::iter::repeat_n(0, padding));
}

fn append_ar_member(out: &mut Vec<u8>, name: &str, data: &[u8]) {
    assert!(name.len() <= 16, "DEB ar member name too long: {name}");
    let header = format!(
        "{:<16}{:<12}{:<6}{:<6}{:<8o}{:<10}`\n",
        name,
        0,
        0,
        0,
        0o100644,
        data.len()
    );
    assert_eq!(header.len(), 60, "invalid ar header length for {name}");
    out.extend_from_slice(header.as_bytes());
    out.extend_from_slice(data);
    if !data.len().is_multiple_of(2) {
        out.push(b'\n');
    }
}

fn build_test_deb() -> Vec<u8> {
    let mut data_tar = Vec::new();
    append_tar_file(&mut data_tar, "usr/bin/hello", b"hello");
    append_tar_file(&mut data_tar, "usr/share/doc/readme.txt", b"docs");
    data_tar.extend_from_slice(&[0u8; 1024]);

    let mut out = Vec::new();
    out.extend_from_slice(b"!<arch>\n");
    append_ar_member(&mut out, "debian-binary", b"2.0\n");
    append_ar_member(&mut out, "control.tar.gz", b"ignored control payload");
    append_ar_member(&mut out, "data.tar", &data_tar);
    out
}

fn build_test_cab(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut builder = CabinetBuilder::new();
    {
        let folder = builder.add_folder(CompressionType::MsZip);
        for (path, _) in entries {
            folder.add_file(*path);
        }
    }

    let cursor = std::io::Cursor::new(Vec::new());
    let mut writer = builder.build(cursor).unwrap();
    let mut index = 0usize;
    while let Some(mut file_writer) = writer.next_file().unwrap() {
        file_writer.write_all(entries[index].1).unwrap();
        index += 1;
    }
    writer.finish().unwrap().into_inner()
}

fn push_newc_hex(out: &mut Vec<u8>, value: u64, width: usize) {
    out.extend_from_slice(format!("{value:0width$X}", width = width).as_bytes());
}

fn build_test_cpio(entries: &[(&str, &[u8])]) -> Vec<u8> {
    fn append_newc_entry(out: &mut Vec<u8>, inode: u32, path: &str, data: &[u8]) {
        out.extend_from_slice(b"070701");
        push_newc_hex(out, u64::from(inode), 8);
        push_newc_hex(out, 0o100644, 8);
        push_newc_hex(out, 0, 8);
        push_newc_hex(out, 0, 8);
        push_newc_hex(out, 1, 8);
        push_newc_hex(out, 0, 8);
        push_newc_hex(out, data.len() as u64, 8);
        push_newc_hex(out, 0, 8);
        push_newc_hex(out, 0, 8);
        push_newc_hex(out, 0, 8);
        push_newc_hex(out, 0, 8);
        push_newc_hex(out, (path.len() + 1) as u64, 8);
        push_newc_hex(out, 0, 8);
        out.extend_from_slice(path.as_bytes());
        out.push(0);
        while !out.len().is_multiple_of(4) {
            out.push(0);
        }
        out.extend_from_slice(data);
        while !out.len().is_multiple_of(4) {
            out.push(0);
        }
    }

    let mut out = Vec::new();
    for (index, (path, data)) in entries.iter().enumerate() {
        append_newc_entry(&mut out, (index + 1) as u32, path, data);
    }
    append_newc_entry(&mut out, 0, "TRAILER!!!", b"");
    out
}

#[test]
fn deb_list_shows_only_data_payload_entries() {
    let td = TestDir::new();
    let archive = td.join("package.deb");
    std::fs::write(&archive, build_test_deb()).unwrap();

    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("usr/bin/hello"))
        .stdout(predicate::str::contains("usr/share/doc/readme.txt"))
        .stdout(predicate::str::contains("control.tar.gz").not())
        .stdout(predicate::str::contains("debian-binary").not());
}

#[test]
fn deb_decompress_extracts_data_payload_only() {
    let td = TestDir::new();
    let archive = td.join("package.deb");
    let output = td.join("out");
    std::fs::write(&archive, build_test_deb()).unwrap();
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
        std::fs::read_to_string(output.join("usr/bin/hello")).unwrap(),
        "hello"
    );
    assert_eq!(
        std::fs::read_to_string(output.join("usr/share/doc/readme.txt")).unwrap(),
        "docs"
    );
    assert!(!output.join("control.tar.gz").exists());
    assert!(!output.join("debian-binary").exists());
}

#[test]
fn deb_test_valid() {
    let td = TestDir::new();
    let archive = td.join("package.deb");
    std::fs::write(&archive, build_test_deb()).unwrap();

    geezipx()
        .args(["test", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Format:  deb"))
        .stdout(predicate::str::contains("result: OK"));
}

#[test]
fn img_compress_decompress_roundtrip() {
    let td = TestDir::new();
    td.write("payload.bin", "raw image data test");

    // Compress
    geezipx()
        .args([
            "compress",
            "-f",
            "img",
            td.join("payload.bin").to_str().unwrap(),
            "-o",
            td.join("out.img").to_str().unwrap(),
        ])
        .assert()
        .success();
    assert!(td.join("out.img").exists());

    // Decompress — output filename is archive stem (strips .img)
    let out_dir = td.join("extracted");
    std::fs::create_dir(&out_dir).unwrap();
    geezipx()
        .args([
            "decompress",
            td.join("out.img").to_str().unwrap(),
            "-o",
            out_dir.to_str().unwrap(),
        ])
        .assert()
        .success();

    // IMG is pass-through: content must be byte-identical
    let extracted = std::fs::read(out_dir.join("out")).unwrap();
    assert_eq!(extracted, b"raw image data test");
}

#[test]
fn aes_encrypt_decrypt_roundtrip() {
    let td = TestDir::new();
    td.write("secret.txt", "top secret content for AES encryption");

    // Encrypt
    geezipx()
        .args([
            "compress",
            "-f",
            "aes",
            "--password",
            "testpw",
            td.join("secret.txt").to_str().unwrap(),
            "-o",
            td.join("secret.enc").to_str().unwrap(),
        ])
        .assert()
        .success();
    assert!(td.join("secret.enc").exists());

    // Decrypt
    let out_dir = td.join("extracted");
    std::fs::create_dir(&out_dir).unwrap();
    geezipx()
        .args([
            "decompress",
            "--password",
            "testpw",
            td.join("secret.enc").to_str().unwrap(),
            "-o",
            out_dir.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Verify roundtrip (output file name is derived from the archive name, stripping .enc)
    let decrypted = std::fs::read(out_dir.join("secret")).unwrap();
    assert_eq!(decrypted, b"top secret content for AES encryption");
}

#[test]
fn aes_wrong_password_fails() {
    let td = TestDir::new();
    td.write("data.txt", "encrypted data");

    geezipx()
        .args([
            "compress",
            "-f",
            "aes",
            "--password",
            "pw1",
            td.join("data.txt").to_str().unwrap(),
            "-o",
            td.join("data.enc").to_str().unwrap(),
        ])
        .assert()
        .success();

    let out_dir = td.join("extracted");
    std::fs::create_dir(&out_dir).unwrap();
    geezipx()
        .args([
            "decompress",
            "--password",
            "wrongpw",
            td.join("data.enc").to_str().unwrap(),
            "-o",
            out_dir.to_str().unwrap(),
        ])
        .assert()
        .failure();
}

#[test]
fn deb_compress_basic() {
    let td = TestDir::new();
    td.write("payload.txt", "deb write test");

    geezipx()
        .args([
            "compress",
            td.join("payload.txt").to_str().unwrap(),
            "-f",
            "deb",
            "-o",
            td.join("out.deb").to_str().unwrap(),
        ])
        .assert()
        .success();

    // Verify the output file exists and is valid.
    assert!(td.join("out.deb").exists());

    // Roundtrip: decompress and check content.
    let out_dir = td.join("extracted");
    std::fs::create_dir(&out_dir).unwrap();
    geezipx()
        .args([
            "decompress",
            td.join("out.deb").to_str().unwrap(),
            "-o",
            out_dir.to_str().unwrap(),
        ])
        .assert()
        .success();

    let extracted = std::fs::read_to_string(out_dir.join("payload.txt")).unwrap();
    assert_eq!(extracted, "deb write test");
}

#[test]
fn deb_password_is_rejected() {
    let td = TestDir::new();
    let archive = td.join("package.deb");
    std::fs::write(&archive, build_test_deb()).unwrap();

    geezipx()
        .args(["list", archive.to_str().unwrap(), "--password", "secret"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("supported for ZIP"));
}

#[test]
fn cab_list_shows_entries() {
    let td = TestDir::new();
    let archive = td.join("archive.cab");
    std::fs::write(
        &archive,
        build_test_cab(&[("docs\\hello.txt", b"hello"), ("readme.txt", b"readme")]),
    )
    .unwrap();

    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("docs/hello.txt"))
        .stdout(predicate::str::contains("readme.txt"));
}

#[test]
fn cab_decompress_extracts_files() {
    let td = TestDir::new();
    let archive = td.join("archive.cab");
    let output = td.join("out");
    std::fs::write(
        &archive,
        build_test_cab(&[("docs\\hello.txt", b"hello"), ("readme.txt", b"readme")]),
    )
    .unwrap();
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
        std::fs::read_to_string(output.join("docs/hello.txt")).unwrap(),
        "hello"
    );
    assert_eq!(
        std::fs::read_to_string(output.join("readme.txt")).unwrap(),
        "readme"
    );
}

#[test]
fn cab_test_valid() {
    let td = TestDir::new();
    let archive = td.join("archive.cab");
    std::fs::write(&archive, build_test_cab(&[("hello.txt", b"hello")])).unwrap();

    geezipx()
        .args(["test", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Format:  cab"))
        .stdout(predicate::str::contains("result: OK"));
}

#[test]
fn cab_compress_and_list() {
    let td = TestDir::new();
    td.write("payload.txt", "cab write test data");
    let output = td.join("out.cab");

    geezipx()
        .args([
            "compress",
            td.join("payload.txt").to_str().unwrap(),
            "-f",
            "cab",
            "-o",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(output.exists(), "output.cab should exist");

    // Verify the CAB can be listed.
    geezipx()
        .args(["list", output.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("payload.txt"));

    // Round-trip: decompress and verify.
    let extract_dir = td.join("extracted");
    std::fs::create_dir(&extract_dir).unwrap();
    geezipx()
        .args([
            "decompress",
            output.to_str().unwrap(),
            "-o",
            extract_dir.to_str().unwrap(),
        ])
        .assert()
        .success();

    let extracted = extract_dir.join("payload.txt");
    assert!(extracted.exists());
    assert_eq!(
        std::fs::read_to_string(&extracted).unwrap(),
        "cab write test data"
    );
}

#[test]
fn cab_password_is_rejected() {
    let td = TestDir::new();
    let archive = td.join("archive.cab");
    std::fs::write(&archive, build_test_cab(&[("hello.txt", b"hello")])).unwrap();

    geezipx()
        .args(["list", archive.to_str().unwrap(), "--password", "secret"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("supported for ZIP"));
}

#[test]
fn cpio_list_shows_entries() {
    let td = TestDir::new();
    let archive = td.join("archive.cpio");
    std::fs::write(
        &archive,
        build_test_cpio(&[("docs/hello.txt", b"hello"), ("readme.txt", b"readme")]),
    )
    .unwrap();

    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("docs/hello.txt"))
        .stdout(predicate::str::contains("readme.txt"));
}

#[test]
fn cpio_decompress_extracts_files() {
    let td = TestDir::new();
    let archive = td.join("archive.cpio");
    let output = td.join("out");
    std::fs::write(
        &archive,
        build_test_cpio(&[("docs/hello.txt", b"hello"), ("readme.txt", b"readme")]),
    )
    .unwrap();
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
        std::fs::read_to_string(output.join("docs/hello.txt")).unwrap(),
        "hello"
    );
    assert_eq!(
        std::fs::read_to_string(output.join("readme.txt")).unwrap(),
        "readme"
    );
}

#[test]
fn cpio_test_valid() {
    let td = TestDir::new();
    let archive = td.join("archive.cpio");
    std::fs::write(&archive, build_test_cpio(&[("hello.txt", b"hello")])).unwrap();

    geezipx()
        .args(["test", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Format:  cpio"))
        .stdout(predicate::str::contains("result: OK"));
}

#[test]
fn cpio_compress_creates_valid_archive() {
    let td = TestDir::new();
    td.write("payload.txt", "cpio write should succeed");
    let output = td.join("out.cpio");

    geezipx()
        .args([
            "compress",
            td.join("payload.txt").to_str().unwrap(),
            "-f",
            "cpio",
            "-o",
            output.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("Created"))
        .stderr(predicate::str::contains("out.cpio"));

    // Verify the output is valid by listing it
    geezipx()
        .args(["list", output.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("payload.txt"));
}

#[test]
fn cpio_password_is_rejected() {
    let td = TestDir::new();
    let archive = td.join("archive.cpio");
    std::fs::write(&archive, build_test_cpio(&[("hello.txt", b"hello")])).unwrap();

    geezipx()
        .args(["list", archive.to_str().unwrap(), "--password", "secret"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("supported for ZIP"));
}

#[test]
fn cpio_test_password_is_rejected() {
    let td = TestDir::new();
    let archive = td.join("archive.cpio");
    std::fs::write(&archive, build_test_cpio(&[("hello.txt", b"hello")])).unwrap();

    geezipx()
        .args(["test", archive.to_str().unwrap(), "--password", "secret"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("supported for ZIP"));
}

// ---------------------------------------------------------------------------
// LZH read/write tests
// ---------------------------------------------------------------------------

fn lzh_crc16(data: &[u8]) -> u16 {
    let mut sum = 0u16;
    for &byte in data {
        sum ^= u16::from(byte);
        for _ in 0..8 {
            if sum & 1 == 1 {
                sum = (sum >> 1) ^ 0xA001;
            } else {
                sum >>= 1;
            }
        }
    }
    sum
}

fn append_lzh_member(out: &mut Vec<u8>, path: &str, data: &[u8]) {
    let name = path.as_bytes();
    assert!(
        name.len() <= u8::MAX as usize,
        "LZH pathname too long: {path}"
    );

    let mut header = Vec::new();
    header.extend_from_slice(b"-lh0-");
    header.extend_from_slice(&(data.len() as u32).to_le_bytes());
    header.extend_from_slice(&(data.len() as u32).to_le_bytes());
    header.extend_from_slice(&0u32.to_le_bytes());
    header.push(0x20);
    header.push(0);
    header.push(name.len() as u8);
    header.extend_from_slice(name);
    header.extend_from_slice(&lzh_crc16(data).to_le_bytes());

    let checksum = header.iter().fold(0u8, |acc, byte| acc.wrapping_add(*byte));
    out.push(header.len() as u8);
    out.push(checksum);
    out.extend_from_slice(&header);
    out.extend_from_slice(data);
}

fn build_test_lzh(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    for (path, data) in entries {
        append_lzh_member(&mut out, path, data);
    }
    out.push(0);
    out
}

#[test]
fn lzh_list_shows_entries() {
    let td = TestDir::new();
    let archive = td.join("archive.lzh");
    std::fs::write(
        &archive,
        build_test_lzh(&[("docs/hello.txt", b"hello"), ("readme.txt", b"readme")]),
    )
    .unwrap();

    geezipx()
        .args(["list", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("docs/hello.txt"))
        .stdout(predicate::str::contains("readme.txt"));
}

#[test]
fn lzh_decompress_extracts_files() {
    let td = TestDir::new();
    let archive = td.join("archive.lzh");
    let output = td.join("out");
    std::fs::write(
        &archive,
        build_test_lzh(&[("docs/hello.txt", b"hello"), ("readme.txt", b"readme")]),
    )
    .unwrap();
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
        std::fs::read_to_string(output.join("docs/hello.txt")).unwrap(),
        "hello"
    );
    assert_eq!(
        std::fs::read_to_string(output.join("readme.txt")).unwrap(),
        "readme"
    );
}

#[test]
fn lzh_test_reports_crc16_integrity() {
    let td = TestDir::new();
    let archive = td.join("archive.lha");
    std::fs::write(&archive, build_test_lzh(&[("hello.txt", b"hello")])).unwrap();

    geezipx()
        .args(["test", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Format:  lzh"))
        .stdout(predicate::str::contains("Integrity:  verified (CRC-16)"))
        .stdout(predicate::str::contains("result: OK"));
}

#[test]
fn lzh_decompress_rejects_dangerous_raw_paths() {
    let td = TestDir::new();
    let archive = td.join("dangerous.lzh");
    let output = td.join("out");
    std::fs::write(&archive, build_test_lzh(&[("../evil.txt", b"bad")])).unwrap();
    std::fs::create_dir_all(&output).unwrap();

    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("path traversal detected"))
        .stderr(predicate::str::contains("../evil.txt"))
        .stderr(predicate::str::contains("0 files, 0 bytes, 0 skipped"));

    assert!(!output.join("evil.txt").exists());
}

#[test]
fn lzh_compress_format_flag_creates_archive_and_lists_entries() {
    let td = TestDir::new();
    let output = td.join("out.lzh");
    td.write("payload.txt", "lzh explicit write works");

    geezipx()
        .args([
            "compress",
            td.join("payload.txt").to_str().unwrap(),
            "-f",
            "lzh",
            "-o",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(output.exists(), "lzh output should be created");

    geezipx()
        .args(["list", output.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("payload.txt"));
}

#[test]
fn lha_compress_format_flag_creates_archive_and_extracts_entries() {
    let td = TestDir::new();
    let output = td.join("out.lha");
    let extract_dir = td.join("extract-explicit");
    td.write("payload.txt", "lha explicit write works");
    std::fs::create_dir_all(&extract_dir).unwrap();

    geezipx()
        .args([
            "compress",
            td.join("payload.txt").to_str().unwrap(),
            "-f",
            "lha",
            "-o",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();

    geezipx()
        .args([
            "decompress",
            output.to_str().unwrap(),
            "-o",
            extract_dir.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert_eq!(
        std::fs::read_to_string(extract_dir.join("payload.txt")).unwrap(),
        "lha explicit write works"
    );
}

#[cfg(not(windows))]
#[test]
fn lzh_compress_rejects_windows_drive_relative_source_name() {
    let td = TestDir::new();
    let output = td.join("out.lzh");
    td.write("C:evil.txt", "drive-relative names must be rejected");

    geezipx()
        .args([
            "compress",
            td.join("C:evil.txt").to_str().unwrap(),
            "-f",
            "lzh",
            "-o",
            output.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid LZH entry path"))
        .stderr(predicate::str::contains("C:evil.txt"));

    assert!(
        !output.exists(),
        "failed lzh write should remove the partial output file"
    );
}

#[test]
fn lzh_compress_output_inference_overwrites_existing_output_with_archive() {
    let td = TestDir::new();
    let output = td.join("out.lzh");
    td.write("payload.txt", "lzh inferred write works");
    std::fs::write(&output, "keep existing lzh output").unwrap();

    geezipx()
        .args([
            "compress",
            td.join("payload.txt").to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert_ne!(
        std::fs::read(&output).unwrap(),
        b"keep existing lzh output",
        "existing lzh output should be replaced by a real archive"
    );

    geezipx()
        .args(["list", output.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("payload.txt"));
}

#[test]
fn lha_compress_output_inference_overwrites_existing_output_with_archive() {
    let td = TestDir::new();
    let output = td.join("out.lha");
    let extract_dir = td.join("extract-inferred");
    td.write("payload.txt", "lha inferred write works");
    std::fs::write(&output, "keep existing lha output").unwrap();
    std::fs::create_dir_all(&extract_dir).unwrap();

    geezipx()
        .args([
            "compress",
            td.join("payload.txt").to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();

    geezipx()
        .args([
            "decompress",
            output.to_str().unwrap(),
            "-o",
            extract_dir.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert_eq!(
        std::fs::read_to_string(extract_dir.join("payload.txt")).unwrap(),
        "lha inferred write works"
    );
}
