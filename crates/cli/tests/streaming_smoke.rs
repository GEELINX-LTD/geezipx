//! Streaming smoke tests — large file round-trips with chunk-by-chunk
//! verification.  Fixture data is generated deterministically at a reasonable
//! total size; the comparison loop reads both files incrementally without
//! loading either entirely into memory.
//!
//! `cargo test --workspace`. Run them explicitly via:
//!
//! ```sh
//! cargo test -p geezipx --test streaming_smoke -- --test-threads=1 --ignored
//! ```
//!
//! ## Coverage
//!
//! - **gzip round-trip**: 16 MiB single-stream compress → decompress,
//!   chunk-by-chunk content comparison.
//! - **tar.gz round-trip**: Directory with 4 × 8 MiB files (32 MiB total)
//!   recursive compress → decompress, per-file chunk-by-chunk comparison.
//!
//! Data is generated with a deterministic 256-byte repeating pattern so
//! results are reproducible across platforms without external tools.

use std::fs;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use assert_cmd::Command;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Size of each comparison chunk (1 MiB).
const CHUNK_SIZE: usize = 1024 * 1024;

/// Single-stream gzip test: target file size (16 MiB).
const GZIP_TARGET_SIZE: u64 = 16 * 1024 * 1024;

/// Archive test: number of files in the source directory.
const ARCHIVE_FILE_COUNT: usize = 4;

/// Archive test: size per file (8 MiB — 4 × 8 = 32 MiB total payload).
const ARCHIVE_FILE_SIZE: u64 = 8 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn geezipx() -> Command {
    Command::cargo_bin("geezipx").expect("geezipx binary not found")
}

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
}

/// Generate deterministic, reproducible data of `size` bytes.
///
/// Uses a simple repeating 0..=255 byte pattern that is compressible
/// enough to exercise the full streaming pipeline while remaining
/// deterministic across platforms and invocations.
fn generate_data(size: u64) -> Vec<u8> {
    let pattern: Vec<u8> = (0u8..=255).collect();
    let mut data = Vec::with_capacity(size as usize);
    while data.len() < size as usize {
        let remaining = size as usize - data.len();
        let take = remaining.min(pattern.len());
        data.extend_from_slice(&pattern[..take]);
    }
    data
}

/// Compare two files chunk-by-chunk without loading either entirely into
/// memory.  Uses a fixed `CHUNK_SIZE` buffer per side and a loop that fills
/// each chunk even when `Read::read()` returns fewer bytes than requested.
fn compare_files_chunked(orig_path: &Path, dec_path: &Path) {
    let mut orig = BufReader::new(fs::File::open(orig_path).unwrap());
    let mut dec = BufReader::new(fs::File::open(dec_path).unwrap());

    let mut buf1 = vec![0u8; CHUNK_SIZE];
    let mut buf2 = vec![0u8; CHUNK_SIZE];

    loop {
        let n1 = read_fully(&mut orig, &mut buf1);
        let n2 = read_fully(&mut dec, &mut buf2);

        assert_eq!(n1, n2, "file sizes differ (orig={n1}, dec={n2})");
        if n1 == 0 {
            break;
        }
        assert_eq!(
            &buf1[..n1],
            &buf2[..n2],
            "content differs at a chunk boundary"
        );
    }
}

/// Read up to `buf.len()` bytes, looping `read()` to handle partial fills.
/// Returns the total number of bytes read (0 means EOF).
fn read_fully<R: Read>(reader: &mut R, buf: &mut [u8]) -> usize {
    let mut total = 0;
    while total < buf.len() {
        match reader.read(&mut buf[total..]) {
            Ok(0) => break,
            Ok(n) => total += n,
            Err(e) => panic!("I/O error: {e}"),
        }
    }
    total
}

/// Write a deterministic-data file under `dir/rel`, return its path.
fn write_data(dir: &Path, rel: &str, size: u64) -> PathBuf {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let data = generate_data(size);
    fs::write(&path, &data).unwrap();
    path
}

// ---------------------------------------------------------------------------
// Streaming smoke tests (all #[ignore] — opt-in via --ignored)
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn streaming_gzip_roundtrip() {
    let tmp = TestDir::new();

    // Generate the original 16 MiB data file.
    let original = write_data(tmp.path(), "large.bin", GZIP_TARGET_SIZE);
    let archive = tmp.join("large.bin.gz");
    let out_dir = tmp.join("out");

    // Compress (gzip single-stream).
    geezipx()
        .args([
            "compress",
            original.to_str().unwrap(),
            "-f",
            "gzip",
            "-o",
            archive.to_str().unwrap(),
            "--no-progress",
        ])
        .assert()
        .success();

    assert!(archive.exists(), "gzip archive should exist");

    // Decompress.
    fs::create_dir_all(&out_dir).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            out_dir.to_str().unwrap(),
            "--no-progress",
        ])
        .assert()
        .success();

    // Chunk-by-chunk comparison — no full-file read.
    let decompressed = out_dir.join("large.bin");
    assert!(decompressed.exists(), "decompressed file should exist");
    compare_files_chunked(&original, &decompressed);
}

#[test]
#[ignore]
fn streaming_targz_recursive_roundtrip() {
    let tmp = TestDir::new();

    // Create a directory with several multi-megabyte files.
    let src_dir = tmp.join("src");
    fs::create_dir_all(&src_dir).unwrap();

    for i in 0..ARCHIVE_FILE_COUNT {
        let filename = format!("data_{}.bin", i);
        write_data(&src_dir, &filename, ARCHIVE_FILE_SIZE);
    }

    let archive = tmp.join("large.tar.gz");
    let out_dir = tmp.join("extracted");

    // Recursive compress to tar.gz.
    geezipx()
        .args([
            "compress",
            src_dir.to_str().unwrap(),
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

    // Decompress.
    fs::create_dir_all(&out_dir).unwrap();
    geezipx()
        .args([
            "decompress",
            archive.to_str().unwrap(),
            "-o",
            out_dir.to_str().unwrap(),
            "--no-progress",
        ])
        .assert()
        .success();

    // Per-file chunk-by-chunk comparison.
    for i in 0..ARCHIVE_FILE_COUNT {
        let filename = format!("data_{}.bin", i);
        let original = src_dir.join(&filename);
        let decompressed = out_dir.join("src").join(&filename);
        assert!(
            decompressed.exists(),
            "decompressed file {filename} should exist"
        );
        compare_files_chunked(&original, &decompressed);
    }
}
