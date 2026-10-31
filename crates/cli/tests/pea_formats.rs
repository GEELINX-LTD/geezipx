use assert_cmd::Command;
use std::fs;
use std::path::{Path, PathBuf};

fn cli() -> Command {
    Command::cargo_bin("geezipx").unwrap()
}
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../core/tests/fixtures/pea")
        .join(name)
}

#[test]
fn pea_magic_listing_and_single_pass_verification_json() {
    let work = tempfile::tempdir().unwrap();
    let archive = work.path().join("renamed.dat");
    fs::copy(fixture("sha256-pcompress2.pea"), &archive).unwrap();
    let output = cli()
        .arg("list")
        .arg(&archive)
        .arg("--json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(report.as_array().unwrap().len(), 4);
    assert!(report
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["path"] == "source/中文 Ω.txt"));
    let output = cli()
        .arg("test")
        .arg(&archive)
        .arg("--json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(report["format"], "pea");
    assert_eq!(report["ok"], true);
    assert_eq!(report["entry_count"], 4);
    assert_eq!(report["bytes_read"], 136256);
}
#[test]
fn pea_cli_extracts_unicode_empty_files_and_directories() {
    let work = tempfile::tempdir().unwrap();
    let dest = work.path().join("output");
    cli()
        .arg("decompress")
        .arg(fixture("mixed.pea"))
        .arg("-o")
        .arg(&dest)
        .arg("--no-progress")
        .assert()
        .success();
    assert!(dest.join("source/empty-dir").is_dir());
    assert_eq!(fs::read(dest.join("source/empty.bin")).unwrap(), b"");
    let mut expected = b"PEA independently generated block\n".repeat(4000);
    expected.extend(0u8..=255);
    assert_eq!(fs::read(dest.join("source/中文 Ω.txt")).unwrap(), expected);
}
#[test]
fn pea_corruption_json_and_failed_extract_preserve_existing_output() {
    let work = tempfile::tempdir().unwrap();
    let archive = work.path().join("bad.pea");
    let mut data = fs::read(fixture("crc32-pcompress2.pea")).unwrap();
    *data.last_mut().unwrap() ^= 1;
    fs::write(&archive, data).unwrap();
    let output = cli()
        .arg("test")
        .arg(&archive)
        .arg("--json")
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(report["ok"], false);
    let dest = work.path().join("output");
    fs::create_dir_all(dest.join("source")).unwrap();
    let target = dest.join("source/中文 Ω.txt");
    fs::write(&target, b"keep existing").unwrap();
    cli()
        .arg("decompress")
        .arg(&archive)
        .arg("-o")
        .arg(&dest)
        .arg("--no-progress")
        .assert()
        .failure();
    assert_eq!(fs::read(target).unwrap(), b"keep existing");
}
#[test]
fn pea_creation_is_rejected_and_preserves_existing_archive() {
    let work = tempfile::tempdir().unwrap();
    let input = work.path().join("input.txt");
    fs::write(&input, b"input").unwrap();
    let archive = work.path().join("existing.pea");
    fs::write(&archive, b"keep old archive").unwrap();
    cli()
        .arg("compress")
        .arg(input)
        .arg("-o")
        .arg(&archive)
        .args(["-f", "pea", "--no-progress"])
        .assert()
        .failure();
    assert_eq!(fs::read(archive).unwrap(), b"keep old archive");
}
