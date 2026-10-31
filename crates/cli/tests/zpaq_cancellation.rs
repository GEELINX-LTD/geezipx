#![cfg(all(target_os = "linux", feature = "zpaq"))]

use std::fs;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

fn compression_has_started(pid: u32, input_len: u64) -> bool {
    let directory = format!("/proc/{pid}/fd");
    let Ok(handles) = fs::read_dir(directory) else {
        return false;
    };
    handles.flatten().any(|handle| {
        let path = handle.path();
        let Ok(link) = fs::read_link(&path) else {
            return false;
        };
        if !link.to_string_lossy().contains("(deleted)") {
            return false;
        }
        if fs::metadata(&path).map(|m| m.len()).ok() != Some(input_len) {
            return false;
        }
        let info_path = format!(
            "/proc/{pid}/fdinfo/{}",
            handle.file_name().to_string_lossy()
        );
        let Ok(info) = fs::read_to_string(info_path) else {
            return false;
        };
        let position = info
            .lines()
            .find_map(|line| line.strip_prefix("pos:")?.trim().parse::<u64>().ok());
        // Staging is complete, and the anonymous source has been rewound and
        // read by the compressor. This does not race against input staging.
        position.is_some_and(|pos| pos > 0 && pos < input_len)
    })
}

#[test]
fn zpaq_sigint_after_input_staging_preserves_existing_archive() {
    let work = tempfile::Builder::new()
        .prefix("geezipx-zpaq-cancel-")
        .tempdir()
        .unwrap();
    let source = work.path().join("source.bin");
    let size = 32 * 1024 * 1024;
    fs::File::create(&source).unwrap().set_len(size).unwrap();
    let target = work.path().join("existing.zpaq");
    fs::write(&target, b"existing archive").unwrap();
    let stderr = work.path().join("stderr.log");
    let mut child = ChildGuard(
        Command::new(env!("CARGO_BIN_EXE_geezipx"))
            .args([
                "compress",
                source.to_str().unwrap(),
                "-o",
                target.to_str().unwrap(),
                "-f",
                "zpaq",
                "--level",
                "1",
                "--no-progress",
            ])
            .env("TMPDIR", work.path())
            .stdout(Stdio::null())
            .stderr(fs::File::create(&stderr).unwrap())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "CLI exited before staged compression was observed: {}",
            fs::read_to_string(&stderr).unwrap_or_default()
        );
        if compression_has_started(child.0.id(), size) {
            break;
        }
        assert!(Instant::now() < deadline, "staged input was not observed");
        thread::sleep(Duration::from_millis(2));
    }
    let signal = Command::new("kill")
        .args(["-INT", &child.0.id().to_string()])
        .status()
        .unwrap();
    assert!(signal.success());
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(!status.success(), "cancelled compression returned success");
            break;
        }
        assert!(
            Instant::now() < deadline,
            "staged ZPAQ compression ignored SIGINT"
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(fs::read(&target).unwrap(), b"existing archive");
    assert!(source.exists());
}
