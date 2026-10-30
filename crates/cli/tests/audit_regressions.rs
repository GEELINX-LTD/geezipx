//! Regression cases for CLI output safety, stream dispatch, and error reporting.
use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::Path;

fn cli() -> Command {
    Command::cargo_bin("geezipx").unwrap()
}

fn compress(input: &Path, output: &Path, format: &str) {
    cli()
        .arg("compress")
        .arg(input)
        .arg("-o")
        .arg(output)
        .args(["-f", format, "--no-progress"])
        .assert()
        .success();
}

#[test]
fn equivalent_compress_paths_preserve_source() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    fs::write(&source, b"must survive").unwrap();
    let alias = dir.path().join("./source");
    cli()
        .arg("compress")
        .arg(&source)
        .arg("-o")
        .arg(alias)
        .args(["-f", "gzip"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot also be an input"));
    assert_eq!(fs::read(source).unwrap(), b"must survive");
}

#[test]
fn hard_link_compress_output_preserves_source() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    let output = dir.path().join("out.gz");
    fs::write(&source, b"must survive").unwrap();
    fs::hard_link(&source, &output).unwrap();
    let result = cli()
        .arg("compress")
        .arg(&source)
        .arg("-o")
        .arg(&output)
        .args(["-f", "gzip"])
        .output()
        .unwrap();
    #[cfg(unix)]
    assert!(!result.status.success());
    #[cfg(not(unix))]
    let _ = result;
    assert_eq!(fs::read(source).unwrap(), b"must survive");
}

#[test]
fn recursive_compression_rejects_existing_output_in_input_tree() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    fs::create_dir(&source).unwrap();
    let output = source.join("archive.zip");
    fs::write(source.join("data"), b"data").unwrap();
    fs::write(&output, b"old archive").unwrap();
    cli()
        .arg("compress")
        .arg(&source)
        .args(["-r", "-o"])
        .arg(&output)
        .assert()
        .failure();
    assert_eq!(fs::read(output).unwrap(), b"old archive");
}

#[cfg(unix)]
#[test]
fn recursive_symlink_cycle_is_rejected_without_writing() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    fs::create_dir(&source).unwrap();
    std::os::unix::fs::symlink(&source, source.join("cycle")).unwrap();
    let output = dir.path().join("out.zip");
    cli()
        .arg("compress")
        .arg(source)
        .args(["-r", "-o"])
        .arg(&output)
        .assert()
        .failure()
        .stderr(predicate::str::contains("symbolic link"));
    assert!(!output.exists());
}

#[test]
fn failed_compression_preserves_existing_destination() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("data");
    let output = dir.path().join("out.7z");
    fs::write(&input, b"data").unwrap();
    fs::write(&output, b"previous archive").unwrap();
    cli()
        .arg("compress")
        .arg(input)
        .arg("-o")
        .arg(&output)
        .args(["-f", "7z", "--7z-method", "invalid-method"])
        .assert()
        .failure();
    assert_eq!(fs::read(output).unwrap(), b"previous archive");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
}

#[test]
fn corrupt_stream_preserves_old_output_and_cleans_new_output() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("source");
    let archive = dir.path().join("data.gz");
    fs::write(&input, b"decoded contents").unwrap();
    compress(&input, &archive, "gzip");
    let mut bytes = fs::read(&archive).unwrap();
    let trailer = bytes.len() - 8;
    bytes[trailer] ^= 1;
    fs::write(&archive, bytes).unwrap();
    let out = dir.path().join("out");
    fs::create_dir(&out).unwrap();
    fs::write(out.join("data"), b"old output").unwrap();
    cli()
        .arg("decompress")
        .arg(&archive)
        .arg("-o")
        .arg(&out)
        .assert()
        .failure();
    assert_eq!(fs::read(out.join("data")).unwrap(), b"old output");
    assert_eq!(fs::read_dir(&out).unwrap().count(), 1);
    fs::remove_file(out.join("data")).unwrap();
    cli()
        .arg("decompress")
        .arg(&archive)
        .arg("-o")
        .arg(&out)
        .assert()
        .failure();
    assert_eq!(fs::read_dir(&out).unwrap().count(), 0);
}

#[test]
fn extensionless_compressed_input_cannot_overwrite_itself() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("source");
    let archive = dir.path().join("archive");
    fs::write(&input, b"source contents").unwrap();
    compress(&input, &archive, "gzip");
    let bytes = fs::read(&archive).unwrap();
    cli()
        .arg("decompress")
        .arg(&archive)
        .arg("-o")
        .arg(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot also be an input"));
    assert_eq!(fs::read(archive).unwrap(), bytes);
}

#[test]
fn stdin_streams_support_img_bin_lz_and_create_output_directory() {
    let dir = tempfile::tempdir().unwrap();
    for format in ["img", "bin", "lz"] {
        let compressed = cli()
            .args(["compress", "--stdin", "--stdout", "-f", format])
            .write_stdin(b"stdin contents".to_vec())
            .output()
            .unwrap();
        assert!(compressed.status.success());
        cli()
            .args(["decompress", "--stdin", "--stdout", "-f", format])
            .write_stdin(compressed.stdout.clone())
            .assert()
            .success()
            .stdout("stdin contents");
        let output = dir.path().join(format).join("nested");
        cli()
            .args(["decompress", "--stdin", "-f", format, "-o"])
            .arg(&output)
            .write_stdin(compressed.stdout)
            .assert()
            .success();
        assert_eq!(fs::read(output.join("output")).unwrap(), b"stdin contents");
    }
}

#[test]
fn stdin_password_conflicts_and_unsupported_passwords_fail() {
    for command in ["compress", "decompress"] {
        cli()
            .args([
                command,
                "--stdin",
                "--stdout",
                "-f",
                "gzip",
                "--password-stdin",
            ])
            .write_stdin("payload")
            .assert()
            .failure()
            .stderr(predicate::str::contains("mutually exclusive"));
    }
    cli()
        .args([
            "decompress",
            "--stdin",
            "--stdout",
            "-f",
            "gzip",
            "--password",
            "secret",
        ])
        .write_stdin("payload")
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not support encryption"));
}

#[test]
fn wrong_aes_password_preserves_output() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    let archive = dir.path().join("data.enc");
    let output = dir.path().join("out");
    fs::write(&source, b"secret contents").unwrap();
    fs::create_dir(&output).unwrap();
    fs::write(output.join("data"), b"old output").unwrap();
    cli()
        .arg("compress")
        .arg(source)
        .arg("-o")
        .arg(&archive)
        .args(["--password", "correct"])
        .assert()
        .success();
    cli()
        .arg("decompress")
        .arg(archive)
        .arg("-o")
        .arg(&output)
        .args(["--password", "wrong"])
        .assert()
        .failure();
    assert_eq!(fs::read(output.join("data")).unwrap(), b"old output");
    assert_eq!(fs::read_dir(output).unwrap().count(), 1);
}

#[test]
fn stream_file_roundtrips_and_no_clobber() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    fs::write(&source, b"stream contents\n").unwrap();
    for (format, suffix) in [
        ("gzip", "gz"),
        ("bzip2", "bz2"),
        ("brotli", "br"),
        ("lz4", "lz4"),
        ("zstd", "zst"),
        ("xz", "xz"),
        ("lzma", "lzma"),
        ("lz", "lz"),
        ("uu", "uu"),
        ("xxe", "xxe"),
    ] {
        let archive = dir.path().join(format!("data.{suffix}"));
        let output = dir.path().join(format);
        compress(&source, &archive, format);
        cli()
            .arg("decompress")
            .arg(&archive)
            .arg("-o")
            .arg(&output)
            .assert()
            .success();
        assert_eq!(
            fs::read(output.join("data")).unwrap(),
            b"stream contents\n",
            "{format}"
        );
        fs::write(output.join("data"), b"keep").unwrap();
        cli()
            .arg("decompress")
            .arg(&archive)
            .arg("-o")
            .arg(&output)
            .arg("--no-clobber")
            .assert()
            .success();
        assert_eq!(fs::read(output.join("data")).unwrap(), b"keep", "{format}");
    }
}

fn raw_tar(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for (name, data) in entries {
        let mut header = [0u8; 512];
        header[..name.len()].copy_from_slice(name.as_bytes());
        header[100..108].copy_from_slice(b"0000644\0");
        header[124..136].copy_from_slice(format!("{:011o}\0", data.len()).as_bytes());
        header[148..156].fill(b' ');
        header[156] = b'0';
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        let checksum: u32 = header.iter().map(|byte| *byte as u32).sum();
        header[148..156].copy_from_slice(format!("{checksum:06o}\0 ").as_bytes());
        bytes.extend_from_slice(&header);
        bytes.extend_from_slice(data);
        bytes.resize(bytes.len().next_multiple_of(512), 0);
    }
    bytes.resize(bytes.len() + 1024, 0);
    bytes
}

#[test]
fn archive_partial_extraction_errors_produce_failure_status() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("archive.tar");
    let output = dir.path().join("out");
    fs::write(
        &archive,
        raw_tar(&[("valid.txt", b"good"), ("blocked", b"bad")]),
    )
    .unwrap();
    fs::create_dir_all(output.join("blocked")).unwrap();
    cli()
        .arg("decompress")
        .arg(archive)
        .arg("-o")
        .arg(&output)
        .assert()
        .failure()
        .stderr(predicate::str::contains("extraction failed for"));
    assert_eq!(fs::read(output.join("valid.txt")).unwrap(), b"good");
    assert!(output.join("blocked").is_dir());
}

#[test]
fn archive_cannot_replace_its_own_input() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("archive.tar");
    let bytes = raw_tar(&[("archive.tar", b"replacement")]);
    fs::write(&archive, &bytes).unwrap();
    cli()
        .arg("decompress")
        .arg(&archive)
        .arg("-o")
        .arg(dir.path())
        .assert()
        .failure();
    assert_eq!(fs::read(archive).unwrap(), bytes);
}

#[test]
fn archive_preflight_and_split_volume_roundtrips() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("data");
    fs::write(&source, b"archive contents").unwrap();
    for format in [
        "zip", "tar", "7z", "tar.gz", "tar.bz2", "tar.br", "tar.lz4", "tar.zst", "tar.xz",
    ] {
        let archive = dir.path().join(format!("archive.{format}"));
        let output = dir.path().join(format!("out-{format}"));
        compress(&source, &archive, format);
        cli()
            .arg("decompress")
            .arg(&archive)
            .arg("-o")
            .arg(&output)
            .assert()
            .success();
        assert_eq!(fs::read(output.join("data")).unwrap(), b"archive contents");
        if format == "tar" || format == "tar.br" {
            let split = dir.path().join(format!("split.{format}"));
            cli()
                .arg("compress")
                .arg(&source)
                .arg("-o")
                .arg(&split)
                .args(["-f", format, "--split-size", "50"])
                .assert()
                .success();
            let first = split.with_file_name(format!("split.{format}.001"));
            assert!(first.exists());
            assert!(!split.exists());
            let output = dir.path().join(format!("split-out-{format}"));
            cli()
                .arg("decompress")
                .arg(first)
                .arg("-o")
                .arg(&output)
                .assert()
                .success();
            assert_eq!(fs::read(output.join("data")).unwrap(), b"archive contents");
        }
    }
}

#[test]
fn split_collision_preserves_existing_outputs_and_inputs() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("out.tar.001");
    let output = dir.path().join("out.tar");
    fs::write(&source, b"source and old volume").unwrap();
    fs::write(&output, b"old archive").unwrap();
    cli()
        .arg("compress")
        .arg(&source)
        .arg("-o")
        .arg(&output)
        .args(["--split-size", "50"])
        .assert()
        .failure();
    assert_eq!(fs::read(source).unwrap(), b"source and old volume");
    assert_eq!(fs::read(output).unwrap(), b"old archive");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
}

#[test]
fn explicit_format_hint_is_honored_for_files() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("bytes.unknown");
    fs::write(&archive, b"pass through").unwrap();
    cli()
        .arg("decompress")
        .arg(archive)
        .args(["--stdout", "-f", "img"])
        .assert()
        .success()
        .stdout("pass through");
}

#[test]
fn optical_images_extract_contents_and_reject_random_data() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("payload.txt");
    fs::write(&source, b"optical payload").unwrap();
    for extension in ["bin", "img", "ima"] {
        let archive = dir.path().join(format!("optical.{extension}"));
        compress(&source, &archive, "iso");
        let cooked = fs::read(&archive).unwrap();
        let mut raw = Vec::new();
        for sector in cooked.chunks(2048) {
            let mut block = [0; 2352];
            block[..12].copy_from_slice(&[0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0]);
            block[15] = 1;
            block[16..16 + sector.len()].copy_from_slice(sector);
            raw.extend_from_slice(&block);
        }
        fs::write(&archive, raw).unwrap();
        cli().arg("test").arg(&archive).assert().success();
        cli()
            .arg("list")
            .arg(&archive)
            .assert()
            .success()
            .stdout(predicate::str::contains("payload.txt"));
        let output = dir.path().join("out");
        cli()
            .arg("decompress")
            .arg(&archive)
            .arg("-o")
            .arg(&output)
            .assert()
            .success();
        assert_eq!(
            fs::read(output.join("payload.txt")).unwrap(),
            b"optical payload"
        );
        fs::write(&archive, b"arbitrary bytes are not an image").unwrap();
        cli().arg("list").arg(&archive).assert().failure();
        cli().arg("test").arg(&archive).assert().failure();
    }
}

#[test]
fn single_stream_test_and_list_are_consistent() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    fs::write(&source, b"data").unwrap();
    for (format, extension) in [("lz", "lz"), ("isz", "isz")] {
        let archive = dir.path().join(format!("data.{extension}"));
        compress(&source, &archive, format);
        let verification = cli()
            .arg("test")
            .arg(&archive)
            .arg("--json")
            .output()
            .unwrap();
        assert!(
            verification.status.success(),
            "{format}: {}",
            String::from_utf8_lossy(&verification.stderr)
        );
        let report: serde_json::Value = serde_json::from_slice(&verification.stdout).unwrap();
        assert_eq!(report["ok"], true);
        let listing = cli()
            .arg("list")
            .arg(&archive)
            .arg("--json")
            .output()
            .unwrap();
        assert!(listing.status.success());
        let entries: serde_json::Value = serde_json::from_slice(&listing.stdout).unwrap();
        assert_eq!(entries.as_array().unwrap().len(), 1);
    }
}

#[cfg(unix)]
#[test]
fn cancellation_preserves_destination_and_removes_temporary_output() {
    use std::io::Write;
    use std::process::{Command as Process, Stdio};
    use std::time::{Duration, Instant};
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("out.gz");
    fs::write(&output, b"old archive").unwrap();
    let mut child = Process::new(assert_cmd::cargo::cargo_bin("geezipx"))
        .args(["compress", "--stdin", "-f", "gzip", "-o"])
        .arg(&output)
        .stdin(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while fs::read_dir(dir.path()).unwrap().count() < 2 {
        assert!(
            Instant::now() < deadline,
            "CLI did not create temporary output"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(Process::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .unwrap()
        .success());
    std::thread::sleep(Duration::from_millis(50));
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(b"unblock input");
    }
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            child.kill().unwrap();
            panic!("CLI did not cancel");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(child.wait().unwrap().code(), Some(130));
    assert_eq!(fs::read(output).unwrap(), b"old archive");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}
