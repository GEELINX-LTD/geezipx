use super::*;
use geezipx_core::{archive::Entry, config::CompressOptions, detect::ArchiveFormat};
use geezipx_egui::{
    engine::{self, CompressRequest},
    export::ExportKey,
    job::Control,
};
use std::collections::BTreeSet;

#[test]
fn windows_descriptors_and_cancel_do_not_read_archive_data() {
    let entries = [Entry {
        path: "中文目录/说明.txt".into(),
        size: 123,
        compressed_size: 90,
        crc32: None,
        modified: None,
        is_dir: false,
    }];
    let export = Arc::new(
        DeferredExport::new(
            ExportKey {
                archive: "missing.zip".into(),
                selection: BTreeSet::from(["中文目录".into()]),
            },
            &entries,
            None,
            Control::default(),
        )
        .unwrap(),
    );
    // SAFETY: Test-owned COM objects and output media stay live through each call;
    // ReleaseStgMedium frees each successful HGLOBAL result exactly once.
    unsafe {
        OleInitialize(None).unwrap();
        let virtual_files = VirtualFiles::new(export.clone()).unwrap();
        let mut formats = virtual_files.formats();
        let files: IDataObject = virtual_files.into();
        assert_eq!(files.QueryGetData(&formats[0]), S_OK);
        let mut medium = files.GetData(&formats[0]).unwrap();
        let handle = medium.u.hGlobal;
        let data = GlobalLock(handle).cast::<u8>();
        assert_eq!(data.cast::<u32>().read_unaligned(), 2);
        let descriptor = data
            .add(size_of::<u32>())
            .cast::<FILEDESCRIPTORW>()
            .read_unaligned();
        let name = descriptor.cFileName;
        assert_eq!(
            String::from_utf16(&name[..name.iter().position(|c| *c == 0).unwrap()]).unwrap(),
            "中文目录"
        );
        let attributes = descriptor.dwFileAttributes;
        assert_eq!(attributes, FILE_ATTRIBUTE_DIRECTORY.0);
        let _ = GlobalUnlock(handle);
        ReleaseStgMedium(&mut medium);
        formats[1].lindex = 1;
        assert_eq!(files.QueryGetData(&formats[1]), S_OK);
        assert_eq!(files.GetData(&formats[1]).err().unwrap().code(), E_PENDING);
        let source: IDropSource = Source {
            export: export.clone(),
            cancel: Arc::new(AtomicBool::new(false)),
        }
        .into();
        assert_eq!(
            source.QueryContinueDrag(true, MK_LBUTTON),
            DRAGDROP_S_CANCEL
        );
        assert!(!export.is_dropped());
        assert!(export.control.snapshot().stage.is_empty());
        OleUninitialize();
    }
}

#[test]
fn windows_file_contents_are_streamed_once_after_mouse_release() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("readme.txt");
    let payload = "松手后才解压".as_bytes();
    std::fs::write(&source, payload).unwrap();
    let archive = temp.path().join("virtual.zip");
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
    let export = Arc::new(
        DeferredExport::new(
            ExportKey {
                archive: archive.clone(),
                selection: BTreeSet::from(["readme.txt".into()]),
            },
            &loaded.entries,
            None,
            Control::default(),
        )
        .unwrap(),
    );
    // SAFETY: Same COM/medium ownership as above; Read writes at most buffer.len()
    // bytes into the live byte buffer and records the actual count separately.
    unsafe {
        OleInitialize(None).unwrap();
        let virtual_files = VirtualFiles::new(export.clone()).unwrap();
        let mut format = virtual_files.formats()[1];
        format.lindex = 0;
        let files: IDataObject = virtual_files.into();
        let source: IDropSource = Source {
            export: export.clone(),
            cancel: Arc::new(AtomicBool::new(false)),
        }
        .into();
        assert_eq!(source.QueryContinueDrag(false, MK_LBUTTON), S_OK);
        assert!(!export.is_dropped());
        assert_eq!(
            source.QueryContinueDrag(false, MODIFIERKEYS_FLAGS(0)),
            DRAGDROP_S_DROP
        );
        assert!(export.is_dropped());
        for attempt in 0..2 {
            let mut medium = files.GetData(&format).unwrap();
            if attempt == 0 {
                std::fs::remove_file(&archive).unwrap();
            }
            let stream = medium.u.pstm.as_ref().unwrap();
            let mut bytes = [0u8; 128];
            let mut read = 0;
            stream
                .Read(
                    bytes.as_mut_ptr().cast(),
                    bytes.len() as u32,
                    Some(&mut read),
                )
                .ok()
                .unwrap();
            assert_eq!(&bytes[..read as usize], payload);
            ReleaseStgMedium(&mut medium);
        }
        OleUninitialize();
    }
}
