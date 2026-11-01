use geezipx_core::{
    archive::{zip::ZipWriter, ArchiveWriter},
    config::CompressOptions,
    detect::ArchiveFormat,
};
use geezipx_egui::{
    engine::{self, CompressRequest, ReadRequest},
    export::{DeferredExport, ExportKey},
    job::Control,
};
use std::{collections::BTreeSet, fs, path::Path};

#[test]
fn direct_drop_preserves_selected_roots_without_a_staging_copy() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("outer/资料");
    fs::create_dir_all(source.join("空目录")).unwrap();
    fs::write(source.join("说明.txt"), "直接写入目标").unwrap();
    fs::write(temp.path().join("outer/other.txt"), "unselected").unwrap();
    let path = temp.path().join("source.tar.gz");
    engine::compress(
        CompressRequest {
            sources: vec![temp.path().join("outer")],
            output: path.clone(),
            format: ArchiveFormat::TarGz,
            options: CompressOptions::default(),
            replace: false,
            sfx_target: None,
        },
        &Control::default(),
    )
    .unwrap();
    let archive = engine::list(path.clone(), None, &Control::default()).unwrap();
    let export = DeferredExport::new(
        ExportKey {
            archive: path,
            selection: BTreeSet::from(["outer/资料".into(), "outer/资料/说明.txt".into()]),
        },
        &archive.entries,
        None,
        Control::default(),
    )
    .unwrap()
    .with_index(archive.index);
    let destination = temp.path().join("target");
    assert!(export
        .extract_to(destination.clone(), &Control::default())
        .is_err());
    assert!(
        !destination.exists(),
        "hover must not create the destination"
    );
    export.confirm_drop();
    export
        .extract_to(destination.clone(), &Control::default())
        .unwrap();
    assert_eq!(
        fs::read_to_string(destination.join("资料/说明.txt")).unwrap(),
        "直接写入目标"
    );
    assert!(destination.join("资料/空目录").is_dir());
    assert!(!destination.join("outer").exists());
    assert!(!destination.join("other.txt").exists());
    drop(export);
    assert!(
        destination.join("资料/说明.txt").exists(),
        "output must belong to the destination"
    );
}

#[test]
fn stale_metadata_rejects_replaced_source_before_creating_output() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("source.zip");
    let mut writer = ZipWriter::new(fs::File::create(&path).unwrap());
    writer
        .add_entry_from_reader(Path::new("file.txt"), &mut b"original".as_slice())
        .unwrap();
    Box::new(writer).finish().unwrap();
    let archive = engine::list(path.clone(), None, &Control::default()).unwrap();
    fs::write(&path, b"modified source").unwrap();
    let destination = temp.path().join("target");
    let error = engine::extract(
        ReadRequest {
            archive: path,
            index: archive.index,
            password: None,
            selection: BTreeSet::new(),
            destination: destination.clone(),
            overwrite: false,
        },
        &Control::default(),
    )
    .unwrap_err();
    assert!(
        error.contains("变化") || error.contains("changed"),
        "{error}"
    );
    assert!(!destination.exists());
}

#[test]
fn single_stream_cache_uses_the_new_operations_control() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("file.txt");
    fs::write(&source, b"cached decoded contents").unwrap();
    let path = temp.path().join("file.txt.gz");
    engine::compress(
        CompressRequest {
            sources: vec![source],
            output: path.clone(),
            format: ArchiveFormat::Gzip,
            options: CompressOptions::default(),
            replace: false,
            sfx_target: None,
        },
        &Control::default(),
    )
    .unwrap();
    let listing_control = Control::default();
    let archive = engine::list(path.clone(), None, &listing_control).unwrap();
    listing_control.cancel();
    let destination = temp.path().join("target");
    engine::extract(
        ReadRequest {
            archive: path,
            index: archive.index,
            password: None,
            selection: BTreeSet::new(),
            destination: destination.clone(),
            overwrite: false,
        },
        &Control::default(),
    )
    .unwrap();
    assert_eq!(
        fs::read(destination.join("file.txt")).unwrap(),
        b"cached decoded contents"
    );
}

#[test]
fn verification_rereads_source_instead_of_trusting_the_decoded_cache() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("file.txt");
    fs::write(&source, b"contents to authenticate again").unwrap();
    let path = temp.path().join("file.txt.gz");
    engine::compress(
        CompressRequest {
            sources: vec![source],
            output: path.clone(),
            format: ArchiveFormat::Gzip,
            options: CompressOptions::default(),
            replace: false,
            sfx_target: None,
        },
        &Control::default(),
    )
    .unwrap();
    let archive = engine::list(path.clone(), None, &Control::default()).unwrap();
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    let mut encoded = fs::read(&path).unwrap();
    let crc_offset = encoded.len() - 8;
    encoded[crc_offset] ^= 1;
    fs::write(&path, encoded).unwrap();
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(modified))
        .unwrap();
    let result = engine::verify(
        ReadRequest {
            archive: path,
            index: archive.index,
            password: None,
            selection: BTreeSet::new(),
            destination: temp.path().join("unused"),
            overwrite: false,
        },
        &Control::default(),
    );
    assert!(
        result.is_err(),
        "verification must detect a bad source CRC despite a valid decoded cache"
    );
}
