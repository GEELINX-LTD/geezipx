use geezipx_core::{config::CompressOptions, detect::ArchiveFormat};
use geezipx_egui::{
    engine::{self, CompressRequest},
    export::{self, DeferredExport, ExportKey},
    job::{Control, CANCELLED},
};
use std::{collections::BTreeSet, fs};

#[test]
fn drag_export_contains_only_selected_roots_and_keeps_unicode_and_empty_folders() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("资料");
    fs::create_dir_all(source.join("空文件夹")).unwrap();
    fs::write(source.join("说明.txt"), "中文内容").unwrap();
    let other = temp.path().join("other.txt");
    fs::write(&other, b"do not export").unwrap();
    let archive = temp.path().join("test.zip");
    engine::compress(
        CompressRequest {
            sources: vec![source, other],
            output: archive.clone(),
            format: ArchiveFormat::Zip,
            options: CompressOptions::default(),
            replace: false,
        },
        &Control::default(),
    )
    .unwrap();
    let prepared = export::prepare(
        ExportKey {
            archive,
            selection: BTreeSet::from(["资料".into(), "资料/说明.txt".into()]),
        },
        None,
        &Control::default(),
    )
    .unwrap();
    assert_eq!(prepared.paths.len(), 1, "parent and child exported twice");
    let directory = prepared.paths[0].clone();
    assert_eq!(directory.file_name().unwrap(), "资料");
    assert_eq!(
        fs::read_to_string(directory.join("说明.txt")).unwrap(),
        "中文内容"
    );
    assert!(directory.join("空文件夹").is_dir());
    assert!(!directory.parent().unwrap().join("other.txt").exists());
    assert!(
        directory.exists(),
        "staged files expired before the OS could copy them"
    );
    drop(prepared);
    assert!(
        !directory.exists(),
        "staged files leaked after the owner was dropped"
    );
}

#[test]
fn deferred_drag_reads_nothing_until_drop_and_materializes_only_once() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("资料");
    fs::create_dir_all(source.join("空目录")).unwrap();
    fs::write(source.join("说明.txt"), "松手之后才读取").unwrap();
    let archive = temp.path().join("deferred.zip");
    engine::compress(
        CompressRequest {
            sources: vec![source],
            output: archive.clone(),
            format: ArchiveFormat::Zip,
            options: CompressOptions::default(),
            replace: false,
        },
        &Control::default(),
    )
    .unwrap();
    let loaded = engine::list(archive.clone(), None, &Control::default()).unwrap();
    let control = Control::default();
    let export = DeferredExport::new(
        ExportKey {
            archive: archive.clone(),
            selection: BTreeSet::from(["资料".into(), "资料/说明.txt".into()]),
        },
        &loaded.entries,
        None,
        control.clone(),
    )
    .unwrap();
    assert_eq!(
        export
            .items
            .iter()
            .filter(|item| !item.name.contains('/'))
            .count(),
        1
    );
    assert!(export
        .items
        .iter()
        .any(|item| item.name == "资料/空目录" && item.is_dir));
    let hidden = archive.with_extension("hidden");
    fs::rename(&archive, &hidden).unwrap();
    assert!(
        export.paths().is_err(),
        "data was read before the mouse was released"
    );
    assert!(
        control.snapshot().stage.is_empty(),
        "hover opened the archive"
    );
    fs::rename(&hidden, &archive).unwrap();
    export.confirm_drop();
    let paths = export.paths().unwrap();
    assert_eq!(
        fs::read_to_string(paths[0].join("说明.txt")).unwrap(),
        "松手之后才读取"
    );
    fs::remove_file(archive).unwrap();
    assert_eq!(
        export.paths().unwrap(),
        paths,
        "a second content request reopened the archive"
    );
    let target = temp.path().join("destination/renamed");
    export.write_promised_root("资料", &target).unwrap();
    assert_eq!(
        fs::read_to_string(target.join("说明.txt")).unwrap(),
        "松手之后才读取"
    );
    assert!(target.join("空目录").is_dir());
    drop(export);
    assert!(!paths[0].exists());
}

#[test]
fn drag_export_rejects_empty_selection_and_respects_cancellation() {
    let temp = tempfile::tempdir().unwrap();
    let archive = temp.path().join("not-opened.zip");
    assert!(export::prepare(
        ExportKey {
            archive: archive.clone(),
            selection: BTreeSet::new()
        },
        None,
        &Control::default()
    )
    .is_err());
    let control = Control::default();
    control.cancel();
    let error = export::prepare(
        ExportKey {
            archive,
            selection: BTreeSet::from(["file".into()]),
        },
        None,
        &control,
    )
    .err()
    .expect("cancelled export succeeded");
    assert_eq!(error, CANCELLED);
}
