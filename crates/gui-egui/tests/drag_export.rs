use geezipx_core::{config::CompressOptions, detect::ArchiveFormat};
use geezipx_egui::{
    engine::{self, CompressRequest},
    export::{self, ExportKey},
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
