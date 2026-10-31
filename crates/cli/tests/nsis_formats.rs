use assert_cmd::Command;
use std::{
    fs,
    path::{Path, PathBuf},
};
fn cli() -> Command {
    Command::cargo_bin("geezipx").unwrap()
}
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../core/tests/fixtures/nsis")
        .join(format!("{name}.bin"))
}

#[test]
fn nsis_native_all_codecs_detect_list_and_verify_json() {
    let work = tempfile::tempdir().unwrap();
    for name in [
        "store-nonsolid",
        "zlib-nonsolid",
        "zlib-solid",
        "bzip2-nonsolid",
        "bzip2-solid",
        "lzma-nonsolid",
        "lzma-solid",
    ] {
        let archive = work.path().join(format!("{name}.data"));
        fs::copy(fixture(name), &archive).unwrap();
        let output = cli()
            .arg("list")
            .arg(&archive)
            .arg("--json")
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let entries: serde_json::Value = serde_json::from_slice(&output).unwrap();
        let entries = entries.as_array().unwrap();
        assert_eq!(entries.len(), 6, "{name}");
        assert!(
            entries
                .iter()
                .any(|entry| entry["path"] == "nested/中文 Ω.txt"),
            "{name}"
        );
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
        assert_eq!(report["format"], "nsis", "{name}");
        assert_eq!(report["ok"], true);
        assert_eq!(report["entry_count"], 6);
        assert_eq!(report["bytes_read"], 395069);
        assert_eq!(report["crc32_verified"], true);
    }
}
#[test]
fn nsis_solid_extracts_unicode_files_and_explicit_empty_directories() {
    let work = tempfile::tempdir().unwrap();
    for name in ["zlib-solid", "bzip2-solid", "lzma-solid"] {
        let output = work.path().join(name);
        cli()
            .arg("decompress")
            .arg(fixture(name))
            .arg("-o")
            .arg(&output)
            .arg("--no-progress")
            .assert()
            .success();
        assert_eq!(
            fs::read(output.join("hello.txt")).unwrap(),
            b"GeeZipX independent NSIS fixture\n"
        );
        assert_eq!(fs::read(output.join("empty.bin")).unwrap(), b"");
        assert!(output.join("empty").is_dir());
        assert_eq!(
            fs::read(output.join("nested/中文 Ω.txt")).unwrap(),
            "Unicode 中文 Ω\n".repeat(101).as_bytes()
        );
    }
}
#[test]
fn nsis_corruption_is_reported_before_existing_output_changes() {
    let work = tempfile::tempdir().unwrap();
    let archive = work.path().join("bad.exe");
    let mut bytes = fs::read(fixture("zlib-solid")).unwrap();
    let end = bytes.len();
    bytes[end - 5] ^= 1;
    fs::write(&archive, bytes).unwrap();
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
    fs::create_dir(&dest).unwrap();
    fs::write(dest.join("hello.txt"), b"keep existing").unwrap();
    cli()
        .arg("decompress")
        .arg(&archive)
        .arg("-o")
        .arg(&dest)
        .arg("--overwrite")
        .arg("--no-progress")
        .assert()
        .failure();
    assert_eq!(fs::read(dest.join("hello.txt")).unwrap(), b"keep existing");
    assert_eq!(fs::read_dir(&dest).unwrap().count(), 1);
}
#[test]
fn nsis_creation_password_and_plain_executable_are_rejected() {
    let work = tempfile::tempdir().unwrap();
    let source = work.path().join("input.txt");
    let dest = work.path().join("keep.exe");
    fs::write(&source, b"input").unwrap();
    fs::write(&dest, b"keep existing").unwrap();
    cli()
        .arg("compress")
        .arg(&source)
        .arg("-f")
        .arg("nsis")
        .arg("-o")
        .arg(&dest)
        .arg("--overwrite")
        .assert()
        .failure();
    assert_eq!(fs::read(&dest).unwrap(), b"keep existing");
    cli()
        .arg("list")
        .arg(fixture("store-nonsolid"))
        .arg("--password")
        .arg("not-supported")
        .assert()
        .failure();
    let mut bytes = fs::read(fixture("store-nonsolid")).unwrap();
    let marker = b"\xef\xbe\xad\xdeNullsoftInst";
    let offset = bytes
        .windows(marker.len())
        .position(|b| b == marker)
        .unwrap();
    bytes[offset] = 0;
    let plain = work.path().join("ordinary.exe");
    fs::write(&plain, bytes).unwrap();
    cli().arg("list").arg(plain).assert().failure();
}
