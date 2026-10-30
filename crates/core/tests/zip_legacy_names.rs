use geezipx_core::archive::{zip::ZipReader, ArchiveReader};
use std::io::{Cursor, Write};

fn fixture(names: &[&[u8]]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let mut replacements = Vec::new();
    for (index, name) in names.iter().enumerate() {
        let placeholder = char::from(b'Q' + index as u8)
            .to_string()
            .repeat(name.len());
        writer
            .start_file(&placeholder, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"legacy contents").unwrap();
        replacements.push((placeholder.into_bytes(), *name));
    }
    let mut bytes = writer.finish().unwrap().into_inner();
    for (placeholder, raw) in replacements {
        let offsets: Vec<_> = bytes
            .windows(raw.len())
            .enumerate()
            .filter_map(|(i, data)| (data == placeholder).then_some(i))
            .collect();
        assert_eq!(offsets.len(), 2);
        for offset in offsets {
            bytes[offset..offset + raw.len()].copy_from_slice(raw);
        }
    }
    bytes
}

#[test]
fn chinese_legacy_names_list_visit_preview_and_extract_consistently() {
    let names = ["项目/中文目录/说明.txt", "资料/𠀀.txt"];
    let encoded: Vec<_> = names
        .iter()
        .map(|name| {
            let (bytes, _, errors) = encoding_rs::GB18030.encode(name);
            assert!(!errors);
            bytes.into_owned()
        })
        .collect();
    let bytes = fixture(&encoded.iter().map(Vec::as_slice).collect::<Vec<_>>());
    let mut reader = ZipReader::from_buf(bytes).unwrap();
    let entries = reader.entries().unwrap();
    assert_eq!(
        entries.iter().map(|e| e.path.as_str()).collect::<Vec<_>>(),
        names
    );
    let mut visited = Vec::new();
    reader
        .visit_entries(
            &mut |entry, input| {
                let mut contents = Vec::new();
                input.read_to_end(&mut contents).unwrap();
                assert_eq!(contents, b"legacy contents");
                visited.push(entry.path.clone());
                Ok(true)
            },
            &|| false,
        )
        .unwrap();
    assert_eq!(visited, names);
    for entry in &entries {
        let mut contents = Vec::new();
        reader.extract(entry, &mut contents).unwrap();
        assert_eq!(contents, b"legacy contents");
    }
    let out = tempfile::tempdir().unwrap();
    reader.extract_all(out.path(), false).unwrap();
    for name in names {
        assert_eq!(
            std::fs::read(out.path().join(name)).unwrap(),
            b"legacy contents"
        );
    }
}

#[test]
fn utf8_and_cp437_names_keep_their_original_meaning() {
    let bytes = fixture(&["中文/说明.txt".as_bytes(), b"caf\x82.txt"]);
    let entries = ZipReader::from_buf(bytes).unwrap().entries().unwrap();
    assert_eq!(entries[0].path, "中文/说明.txt");
    assert_eq!(entries[1].path, "café.txt");
}

#[test]
fn unicode_path_metadata_takes_precedence_over_legacy_name() {
    let raw = "legacy.txt";
    let name = "中文目录/说明.txt";
    let mut field = vec![1];
    field.extend_from_slice(&crc32fast::hash(raw.as_bytes()).to_le_bytes());
    field.extend_from_slice(name.as_bytes());
    let mut options = zip::write::FileOptions::<zip::write::ExtendedFileOptions>::default();
    options.add_extra_field(0x7075, field, false).unwrap();
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer.start_file(raw, options).unwrap();
    writer.write_all(b"unicode metadata").unwrap();
    let mut reader = ZipReader::from_buf(writer.finish().unwrap().into_inner()).unwrap();
    let entries = reader.entries().unwrap();
    assert_eq!(entries[0].path, name);
    let mut contents = Vec::new();
    reader.extract(&entries[0], &mut contents).unwrap();
    assert_eq!(contents, b"unicode metadata");
}

#[test]
fn ambiguous_decoded_names_are_rejected() {
    let name = "中文.txt";
    let (legacy, _, _) = encoding_rs::GB18030.encode(name);
    assert!(ZipReader::from_buf(fixture(&[&legacy, name.as_bytes()])).is_err());
}
