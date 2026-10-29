use geezipx_core::{
    archive::{zip::ZipWriter, ArchiveWriter},
    config::CompressOptions,
    detect::ArchiveFormat,
};
use geezipx_egui::{
    engine::{self, CompressRequest, ReadRequest},
    job::{Control, CANCELLED},
};
use std::{
    collections::BTreeSet,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};
use tempfile::TempDir;

fn write(root: &Path, name: &str, data: &[u8]) -> PathBuf {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, data).unwrap();
    path
}
fn create(
    sources: Vec<PathBuf>,
    output: &Path,
    format: ArchiveFormat,
    password: Option<&str>,
    replace: bool,
) -> CompressRequest {
    CompressRequest {
        sources,
        output: output.to_owned(),
        format,
        options: CompressOptions {
            password: password.map(str::to_owned),
            ..Default::default()
        },
        replace,
    }
}
fn read(archive: &Path, destination: &Path) -> ReadRequest {
    ReadRequest {
        archive: archive.to_owned(),
        destination: destination.to_owned(),
        password: None,
        selection: BTreeSet::new(),
        overwrite: false,
    }
}
fn zip(path: &Path, entries: &[(&str, &[u8])]) {
    let mut writer = ZipWriter::new(fs::File::create(path).unwrap());
    for (name, data) in entries {
        writer
            .add_entry_from_reader(Path::new(name), &mut Cursor::new(*data))
            .unwrap();
    }
    Box::new(writer).finish().unwrap();
}

#[test]
fn every_creation_format_roundtrips_unicode_nested_and_empty_directories() {
    for &(format, _) in engine::formats::WRITE_FORMATS {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("资料");
        write(&source, "子目录/hello.txt", "归档测试\n".as_bytes());
        fs::create_dir_all(source.join("空文件夹")).unwrap();
        let archive = temp.path().join(format!("sample.{format}"));
        engine::compress(
            create(vec![source], &archive, format, None, false),
            &Control::default(),
        )
        .unwrap_or_else(|error| panic!("{format}: {error}"));
        let listing = engine::list(archive.clone(), None, &Control::default()).unwrap();
        assert_eq!(listing.format, format);
        assert!(
            listing
                .entries
                .iter()
                .any(|entry| entry.path.replace('\\', "/").contains("子目录/hello.txt")),
            "{format}"
        );
        let output = temp.path().join("out");
        engine::verify(read(&archive, &output), &Control::default()).unwrap();
        assert!(!output.exists(), "verification must not extract to disk");
        engine::extract(read(&archive, &output), &Control::default())
            .unwrap_or_else(|error| panic!("{format}: {error}"));
        assert_eq!(
            fs::read(output.join("资料/子目录/hello.txt")).unwrap(),
            "归档测试\n".as_bytes(),
            "{format}"
        );
        assert!(output.join("资料/空文件夹").is_dir(), "{format}");
    }
}

#[test]
fn encrypted_zip_and_seven_zip_require_the_right_password() {
    for format in [ArchiveFormat::Zip, ArchiveFormat::SevenZip] {
        let temp = TempDir::new().unwrap();
        let input = write(temp.path(), "secret.txt", b"private content");
        let archive = temp.path().join(format!("secret.{format}"));
        engine::compress(
            create(vec![input], &archive, format, Some("test-password"), false),
            &Control::default(),
        )
        .unwrap();
        let mut request = read(&archive, &temp.path().join("out"));
        request.password = Some("wrong-password".into());
        assert!(
            engine::extract(request, &Control::default()).is_err(),
            "{format}"
        );
        let mut request = read(&archive, &temp.path().join("out"));
        request.password = Some("test-password".into());
        engine::extract(request, &Control::default()).unwrap();
        assert_eq!(
            fs::read(temp.path().join("out/secret.txt")).unwrap(),
            b"private content"
        );
    }
}

#[test]
fn folder_selection_includes_descendants_but_not_similar_prefixes() {
    let temp = TempDir::new().unwrap();
    let archive = temp.path().join("nested.zip");
    zip(
        &archive,
        &[
            ("one/a.txt", b"a"),
            ("one/sub/b.txt", b"b"),
            ("one-other/c.txt", b"c"),
        ],
    );
    let mut request = read(&archive, &temp.path().join("out"));
    request.selection.insert("one".into());
    engine::extract(request, &Control::default()).unwrap();
    assert_eq!(
        fs::read(temp.path().join("out/one/sub/b.txt")).unwrap(),
        b"b"
    );
    assert!(!temp.path().join("out/one-other").exists());
}

#[test]
fn windows_entry_separators_match_the_browser_and_extraction_tree() {
    let temp = TempDir::new().unwrap();
    let archive = temp.path().join("windows.zip");
    // ZIP writer normalizes paths; replace names in the local and central headers.
    zip(&archive, &[("folder/a.txt", b"a")]);
    let bytes = fs::read(&archive).unwrap();
    let bytes = bytes
        .windows(12)
        .enumerate()
        .filter_map(|(index, bytes)| (bytes == b"folder/a.txt").then_some(index))
        .collect::<Vec<_>>();
    let mut data = fs::read(&archive).unwrap();
    for offset in bytes {
        data[offset + 6] = b'\\';
    }
    fs::write(&archive, data).unwrap();
    let mut request = read(&archive, &temp.path().join("out"));
    request.selection.insert("folder".into());
    engine::extract(request, &Control::default()).unwrap();
    assert_eq!(
        fs::read(temp.path().join("out/folder/a.txt")).unwrap(),
        b"a"
    );
}

#[test]
fn skip_and_replace_are_explicit_and_preserve_existing_files() {
    let temp = TempDir::new().unwrap();
    let archive = temp.path().join("data.zip");
    zip(&archive, &[("file.txt", b"new")]);
    let output = temp.path().join("out");
    write(&output, "file.txt", b"old");
    let summary = engine::extract(read(&archive, &output), &Control::default()).unwrap();
    assert!(summary.message.contains("跳过 1"));
    assert_eq!(fs::read(output.join("file.txt")).unwrap(), b"old");
    let mut request = read(&archive, &output);
    request.overwrite = true;
    engine::extract(request, &Control::default()).unwrap();
    assert_eq!(fs::read(output.join("file.txt")).unwrap(), b"new");
}

#[test]
fn overlapping_sources_preserve_the_same_directory_tree_in_either_order() {
    for parent_first in [false, true] {
        let temp = TempDir::new().unwrap();
        let child = write(temp.path(), "folder/child.txt", b"content");
        let parent = child.parent().unwrap().to_owned();
        let sources = if parent_first {
            vec![parent, child]
        } else {
            vec![child, parent]
        };
        let output = temp.path().join("result.zip");
        engine::compress(
            create(sources, &output, ArchiveFormat::Zip, None, false),
            &Control::default(),
        )
        .unwrap();
        let archive = engine::list(output, None, &Control::default()).unwrap();
        let names = archive
            .entries
            .iter()
            .filter(|entry| !entry.is_dir)
            .map(|entry| entry.path.replace('\\', "/"))
            .collect::<Vec<_>>();
        assert_eq!(names, ["folder/child.txt"]);
    }
}

#[test]
fn output_collisions_and_duplicate_roots_fail_without_touching_destination() {
    let temp = TempDir::new().unwrap();
    let first = write(temp.path(), "one/file.txt", b"first");
    let second = write(temp.path(), "two/file.txt", b"second");
    let output = write(temp.path(), "result.zip", b"keep existing");
    assert!(engine::compress(
        create(
            vec![first.clone()],
            &output,
            ArchiveFormat::Zip,
            None,
            false
        ),
        &Control::default()
    )
    .is_err());
    assert!(engine::compress(
        create(vec![first, second], &output, ArchiveFormat::Zip, None, true),
        &Control::default()
    )
    .is_err());
    assert_eq!(fs::read(output).unwrap(), b"keep existing");
}

#[test]
fn compression_rejects_outputs_inside_inputs_and_input_aliases() {
    let temp = TempDir::new().unwrap();
    let source = write(temp.path(), "folder/input.txt", b"content");
    let inside = temp.path().join("folder/out.zip");
    assert!(engine::compress(
        create(
            vec![source.parent().unwrap().to_owned()],
            &inside,
            ArchiveFormat::Zip,
            None,
            false
        ),
        &Control::default()
    )
    .is_err());
    let alias = temp.path().join("alias.zip");
    fs::hard_link(&source, &alias).unwrap();
    assert!(engine::compress(
        create(vec![source.clone()], &alias, ArchiveFormat::Zip, None, true),
        &Control::default()
    )
    .is_err());
    assert_eq!(fs::read(source).unwrap(), b"content");
}

#[test]
fn traversal_and_source_archive_overwrite_are_rejected_before_writing() {
    let temp = TempDir::new().unwrap();
    let archive = temp.path().join("source.zip");
    zip(
        &archive,
        &[("safe.txt", b"safe"), ("../escape.txt", b"bad")],
    );
    let output = temp.path().join("out");
    assert!(engine::extract(read(&archive, &output), &Control::default()).is_err());
    assert!(!output.exists());
    assert!(!temp.path().join("escape.txt").exists());
    zip(
        &archive,
        &[("safe.txt", b"safe"), ("source.zip", b"replace source")],
    );
    let before = fs::read(&archive).unwrap();
    let mut request = read(&archive, temp.path());
    request.overwrite = true;
    assert!(engine::extract(request, &Control::default()).is_err());
    assert_eq!(fs::read(archive).unwrap(), before);
    assert!(!temp.path().join("safe.txt").exists());
}

#[test]
fn cancellation_removes_unfinished_archive_and_keeps_existing_destination() {
    let temp = TempDir::new().unwrap();
    let input = write(temp.path(), "input.txt", b"content");
    let output = write(temp.path(), "result.zip", b"old archive");
    let holder = Arc::new(OnceLock::<Control>::new());
    let worker_holder = holder.clone();
    let control = Control::new(move || {
        if let Some(control) = worker_holder.get() {
            if !control.is_cancelled() && control.snapshot().completed > 0 {
                control.cancel();
            }
        }
    });
    holder.set(control.clone()).ok().unwrap();
    let error = engine::compress(
        create(vec![input], &output, ArchiveFormat::Zip, None, true),
        &control,
    )
    .unwrap_err();
    assert_eq!(error, CANCELLED);
    assert_eq!(fs::read(&output).unwrap(), b"old archive");
    assert_eq!(
        fs::read_dir(temp.path()).unwrap().count(),
        2,
        "temporary output leaked"
    );
}

#[test]
fn text_preview_stays_bounded_for_zip_tar_and_seven_zip() {
    for format in [
        ArchiveFormat::Zip,
        ArchiveFormat::TarGz,
        ArchiveFormat::SevenZip,
    ] {
        let temp = TempDir::new().unwrap();
        let content = vec![b'a'; engine::PREVIEW_LIMIT * 4];
        let source = write(temp.path(), "long.txt", &content);
        let output = temp.path().join(format!("text.{format}"));
        engine::compress(
            create(vec![source], &output, format, None, false),
            &Control::default(),
        )
        .unwrap();
        let archive = engine::list(output.clone(), None, &Control::default()).unwrap();
        let preview = engine::preview(
            output,
            None,
            archive.entries[0].clone(),
            &Control::default(),
        )
        .unwrap();
        assert!(preview.truncated);
        assert_eq!(preview.text.len(), engine::PREVIEW_LIMIT);
    }
}

#[test]
fn corrupt_zip_never_replaces_an_existing_extraction_target() {
    let temp = TempDir::new().unwrap();
    let archive = temp.path().join("broken.zip");
    zip(
        &archive,
        &[(
            "file.txt",
            b"expected contents expected contents expected contents",
        )],
    );
    let mut data = fs::read(&archive).unwrap();
    let offset = 30
        + u16::from_le_bytes([data[26], data[27]]) as usize
        + u16::from_le_bytes([data[28], data[29]]) as usize;
    data[offset] ^= 0xff;
    fs::write(&archive, data).unwrap();
    let output = temp.path().join("out");
    write(&output, "file.txt", b"old contents");
    assert!(engine::verify(read(&archive, &output), &Control::default()).is_err());
    let mut request = read(&archive, &output);
    request.overwrite = true;
    assert!(engine::extract(request, &Control::default()).is_err());
    assert_eq!(fs::read(output.join("file.txt")).unwrap(), b"old contents");
    assert_eq!(fs::read_dir(output).unwrap().count(), 1);
}

#[cfg(unix)]
#[test]
fn input_links_are_skipped_and_extraction_cannot_follow_output_links() {
    use std::os::unix::fs::symlink;
    let temp = TempDir::new().unwrap();
    let input = write(temp.path(), "input/file.txt", b"content");
    symlink(&input, temp.path().join("input/link.txt")).unwrap();
    let archive = temp.path().join("data.zip");
    let summary = engine::compress(
        create(
            vec![input.parent().unwrap().to_owned()],
            &archive,
            ArchiveFormat::Zip,
            None,
            false,
        ),
        &Control::default(),
    )
    .unwrap();
    assert!(summary.message.contains("跳过 1"));
    let out = temp.path().join("out");
    let external = temp.path().join("external");
    fs::create_dir_all(&out).unwrap();
    fs::create_dir_all(&external).unwrap();
    symlink(&external, out.join("input")).unwrap();
    assert!(engine::extract(read(&archive, &out), &Control::default()).is_err());
    assert_eq!(fs::read_dir(external).unwrap().count(), 0);
}
