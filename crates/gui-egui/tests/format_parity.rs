//! Exercise the native engine's format bridge with real files and core fixtures.
use geezipx_core::{config::CompressOptions, detect::ArchiveFormat, sfx::SfxTarget};
use geezipx_egui::{
    engine::{self, CompressRequest, ReadRequest},
    job::{Control, CANCELLED},
};
use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

fn create(input: &Path, output: &Path, format: ArchiveFormat) -> CompressRequest {
    CompressRequest {
        sources: vec![input.to_owned()],
        output: output.to_owned(),
        format,
        options: CompressOptions::default(),
        replace: false,
        sfx_target: None,
    }
}
fn read(archive: &Path, destination: &Path) -> ReadRequest {
    ReadRequest {
        index: None,
        archive: archive.to_owned(),
        destination: destination.to_owned(),
        selection: BTreeSet::new(),
        password: None,
        overwrite: false,
    }
}
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../core/tests/fixtures")
        .join(name)
}

#[test]
fn every_single_stream_creation_lists_previews_verifies_and_extracts_real_bytes() {
    for &(format, _) in engine::formats::WRITE_FORMATS {
        if !engine::streams::is_stream(format) {
            continue;
        }
        let temp = tempfile::tempdir().unwrap();
        let input = temp.path().join("说明.txt");
        let payload = "单流内容 Ω\n".repeat(1024);
        fs::write(&input, &payload).unwrap();
        let archive = temp
            .path()
            .join(format!("说明.txt.{}", engine::formats::extension(format)));
        let password = (format == ArchiveFormat::Aes).then(|| "测试密码".to_owned());
        let mut request = create(&input, &archive, format);
        request.options.password = password.clone();
        engine::compress(request, &Control::default()).unwrap_or_else(|e| panic!("{format}: {e}"));
        let listing = engine::list(archive.clone(), password.clone(), &Control::default())
            .unwrap_or_else(|e| panic!("{format}: {e}"));
        assert_eq!(listing.format, format);
        assert_eq!(listing.entries.len(), 1);
        assert_eq!(listing.entries[0].path, "说明.txt");
        assert_eq!(listing.entries[0].size, payload.len() as u64);
        let preview = engine::preview(
            archive.clone(),
            password.clone(),
            listing.entries[0].clone(),
            &Control::default(),
        )
        .unwrap();
        assert_eq!(preview.text, payload, "{format}");
        let destination = engine::default_destination(&archive);
        assert_ne!(destination, input);
        let mut request = read(&archive, &destination);
        request.password = password.clone();
        request.selection.insert("说明.txt".into());
        let mut verification = read(&archive, &destination);
        verification.password = password;
        engine::verify(verification, &Control::default()).unwrap();
        assert!(!destination.exists());
        engine::extract(request, &Control::default()).unwrap();
        assert_eq!(
            fs::read_to_string(destination.join("说明.txt")).unwrap(),
            payload,
            "{format}"
        );
    }
}

#[test]
fn new_readonly_adapters_accept_external_core_fixtures() {
    for (name, format) in [
        ("pm1.pma", ArchiveFormat::Lzh),
        ("pm2.pma", ArchiveFormat::Lzh),
        ("ncompress.Z", ArchiveFormat::Z),
        ("egg/lzma5.egg", ArchiveFormat::Egg),
        ("egg/posix.egg", ArchiveFormat::Egg),
        ("pea/mixed.pea", ArchiveFormat::Pea),
        ("nsis/store-nonsolid.bin", ArchiveFormat::Nsis),
        ("nsis/zlib-solid.bin", ArchiveFormat::Nsis),
        ("nsis/bzip2-solid.bin", ArchiveFormat::Nsis),
        ("nsis/lzma-solid.bin", ArchiveFormat::Nsis),
        #[cfg(feature = "wim")]
        ("wimlib-xpress.wim", ArchiveFormat::Wim),
    ] {
        let archive = fixture(name);
        let temp = tempfile::tempdir().unwrap();
        let listing = engine::list(archive.clone(), None, &Control::default())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(listing.format, format, "{name}");
        assert!(!listing.entries.is_empty(), "{name}");
        engine::verify(
            read(&archive, &temp.path().join("test-only")),
            &Control::default(),
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        let destination = temp.path().join("out");
        engine::extract(read(&archive, &destination), &Control::default())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        for entry in listing.entries {
            let path = destination.join(entry.path);
            if entry.is_dir {
                assert!(path.is_dir(), "{name}: {}", path.display());
            } else {
                assert_eq!(fs::metadata(path).unwrap().len(), entry.size, "{name}");
            }
        }
    }
}

#[test]
fn aescrypt_requires_authentication_before_listing_or_replacing_output() {
    let temp = tempfile::tempdir().unwrap();
    let archive = fixture("aescrypt/official-v3.aes");
    assert!(engine::list(archive.clone(), None, &Control::default()).is_err());
    assert!(engine::list(archive.clone(), Some("wrong".into()), &Control::default()).is_err());
    let password = Some("Unicode 密码 🔑".to_owned());
    let listing = engine::list(archive.clone(), password.clone(), &Control::default()).unwrap();
    let preview = engine::preview(
        archive.clone(),
        password.clone(),
        listing.entries[0].clone(),
        &Control::default(),
    )
    .unwrap();
    assert_eq!(preview.text, "Official AES Crypt v3 fixture\n");
    let destination = temp.path().join("out");
    fs::create_dir(&destination).unwrap();
    let target = destination.join(&listing.entries[0].path);
    fs::write(&target, b"preserve").unwrap();
    let mut request = read(&archive, &destination);
    request.password = Some("wrong".into());
    request.overwrite = true;
    assert!(engine::extract(request, &Control::default()).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"preserve");
    let mut request = read(&archive, &destination);
    request.password = password;
    request.overwrite = true;
    engine::extract(request, &Control::default()).unwrap();
    assert_eq!(
        fs::read_to_string(target).unwrap(),
        "Official AES Crypt v3 fixture\n"
    );
}

#[test]
fn disk_image_outputs_expose_iso_files_and_copies_remain_identical() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("hello.txt");
    fs::write(&input, b"image contents").unwrap();
    let iso = temp.path().join("source.iso");
    engine::compress(
        create(&input, &iso, ArchiveFormat::Iso),
        &Control::default(),
    )
    .unwrap();
    for format in [ArchiveFormat::Img, ArchiveFormat::Bin, ArchiveFormat::Isz] {
        let image = temp.path().join(format!("image.{format}"));
        engine::compress(create(&iso, &image, format), &Control::default()).unwrap();
        if format != ArchiveFormat::Isz {
            assert_eq!(fs::read(&image).unwrap(), fs::read(&iso).unwrap());
        }
        let listing = engine::list(image.clone(), None, &Control::default()).unwrap();
        assert_eq!(listing.format, format);
        assert!(listing.entries.iter().any(|e| e.path == "hello.txt"));
        let destination = temp.path().join(format!("out-{format}"));
        engine::verify(read(&image, &destination), &Control::default()).unwrap();
        engine::extract(read(&image, &destination), &Control::default()).unwrap();
        assert_eq!(
            fs::read(destination.join("hello.txt")).unwrap(),
            b"image contents"
        );
    }
}

#[test]
fn isz_verification_reads_chunks_outside_the_iso_filesystem() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("hello.txt");
    fs::write(&input, b"image contents").unwrap();
    let iso = temp.path().join("source.iso");
    engine::compress(
        create(&input, &iso, ArchiveFormat::Iso),
        &Control::default(),
    )
    .unwrap();
    // This payload is outside the ISO volume and never needed to browse files.
    fs::OpenOptions::new()
        .append(true)
        .open(&iso)
        .unwrap()
        .write_all(&vec![b'X'; 3 * 1024 * 1024])
        .unwrap();
    let image = temp.path().join("image.isz");
    engine::compress(
        create(&iso, &image, ArchiveFormat::Isz),
        &Control::default(),
    )
    .unwrap();
    let mut data = fs::read(&image).unwrap();
    *data.last_mut().unwrap() ^= 1; // Last zlib block's Adler32.
    fs::write(&image, data).unwrap();
    assert!(engine::list(image.clone(), None, &Control::default()).is_ok());
    let destination = temp.path().join("out");
    assert!(engine::verify(read(&image, &destination), &Control::default()).is_err());
    assert!(!destination.exists());
}

#[test]
fn corrupt_single_streams_fail_before_replacing_files() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("hello.txt");
    fs::write(&input, b"preserve existing output on CRC failure").unwrap();
    let archive = temp.path().join("hello.txt.gz");
    engine::compress(
        create(&input, &archive, ArchiveFormat::Gzip),
        &Control::default(),
    )
    .unwrap();
    let mut data = fs::read(&archive).unwrap();
    let trailer = data.len() - 8;
    data[trailer] ^= 1;
    fs::write(&archive, data).unwrap();
    assert!(engine::list(archive.clone(), None, &Control::default()).is_err());
    let destination = temp.path().join("out");
    fs::create_dir(&destination).unwrap();
    let output = destination.join("hello.txt");
    fs::write(&output, b"preserve").unwrap();
    let mut request = read(&archive, &destination);
    request.overwrite = true;
    assert!(engine::extract(request, &Control::default()).is_err());
    assert_eq!(fs::read(&output).unwrap(), b"preserve");
}

#[test]
fn sfx_creation_is_readable_and_does_not_need_an_archive_extension() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("hello.txt");
    fs::write(&input, b"SFX payload").unwrap();
    for target in [SfxTarget::Linux, SfxTarget::Windows, SfxTarget::MacOS] {
        let output = temp
            .path()
            .join(format!("sfx-{target:?}{}", target.extension()));
        let mut request = create(&input, &output, ArchiveFormat::Zip);
        request.sfx_target = Some(target);
        if !target.is_available() {
            assert!(engine::compress(request, &Control::default()).is_err());
            assert!(!output.exists());
            continue;
        }
        engine::compress(request, &Control::default()).unwrap();
        let listing = engine::list(output.clone(), None, &Control::default()).unwrap();
        assert_eq!(listing.entries[0].path, "hello.txt");
        let destination = temp.path().join(format!("out-{target:?}"));
        engine::extract(read(&output, &destination), &Control::default()).unwrap();
        assert_eq!(
            fs::read(destination.join("hello.txt")).unwrap(),
            b"SFX payload"
        );
    }
}

#[test]
fn stream_input_rules_and_cancellation_preserve_existing_output() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("input.txt");
    fs::write(&input, b"input").unwrap();
    for format in [
        ArchiveFormat::Gzip,
        ArchiveFormat::Img,
        ArchiveFormat::Bin,
        ArchiveFormat::Isz,
    ] {
        let output = temp
            .path()
            .join(format!("out.{}", engine::formats::extension(format)));
        fs::write(&output, b"preserve").unwrap();
        let mut request = create(temp.path(), &output, format);
        request.replace = true;
        assert!(engine::compress(request, &Control::default()).is_err());
        let mut request = create(&input, &output, format);
        request.sources.push(input.clone());
        request.replace = true;
        assert!(engine::compress(request, &Control::default()).is_err());
        let mut request = create(&input, &output, format);
        request.replace = true;
        let control = Control::default();
        control.cancel();
        assert_eq!(engine::compress(request, &control).unwrap_err(), CANCELLED);
        assert_eq!(fs::read(&output).unwrap(), b"preserve");
    }
    let output = temp.path().join("no-password.enc");
    assert!(engine::compress(
        create(&input, &output, ArchiveFormat::Aes),
        &Control::default()
    )
    .is_err());
    assert!(!output.exists());
}
