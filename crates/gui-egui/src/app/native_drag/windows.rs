//! Explorer virtual files: metadata while hovering, file streams only after drop.
use geezipx_egui::export::DeferredExport;
use std::{
    mem::{size_of, ManuallyDrop},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use windows::{
    core::*,
    Win32::{
        Foundation::*,
        Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL},
        System::{
            Com::{Urlmon::E_PENDING, *},
            DataExchange::RegisterClipboardFormatW,
            Memory::*,
            Ole::*,
            SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS},
        },
        UI::Shell::{
            SHCreateStdEnumFmtEtc, SHCreateStreamOnFileEx, CFSTR_FILECONTENTS,
            CFSTR_FILEDESCRIPTORW, FD_ATTRIBUTES, FD_FILESIZE, FD_PROGRESSUI, FD_UNICODE,
            FILEDESCRIPTORW,
        },
    },
};

#[implement(IDataObject)]
struct VirtualFiles {
    export: Arc<DeferredExport>,
    descriptor: u16,
    contents: u16,
}
impl VirtualFiles {
    fn new(export: Arc<DeferredExport>) -> Result<Self> {
        for item in &export.items {
            if item.name.encode_utf16().count() >= 260 {
                return Err(Error::new(
                    E_INVALIDARG,
                    "所选路径过长，请进入子目录后拖出。",
                ));
            }
        }
        // SAFETY: Clipboard format names are static, terminated UTF-16 strings.
        let (descriptor, contents) = unsafe {
            (
                RegisterClipboardFormatW(CFSTR_FILEDESCRIPTORW),
                RegisterClipboardFormatW(CFSTR_FILECONTENTS),
            )
        };
        if descriptor == 0 || contents == 0 {
            return Err(Error::from_win32());
        }
        Ok(Self {
            export,
            descriptor: descriptor as u16,
            contents: contents as u16,
        })
    }
    fn formats(&self) -> [FORMATETC; 2] {
        [
            FORMATETC {
                cfFormat: self.descriptor,
                dwAspect: DVASPECT_CONTENT.0,
                lindex: -1,
                tymed: TYMED_HGLOBAL.0 as u32,
                ..Default::default()
            },
            FORMATETC {
                cfFormat: self.contents,
                dwAspect: DVASPECT_CONTENT.0,
                lindex: -1,
                tymed: TYMED_ISTREAM.0 as u32,
                ..Default::default()
            },
        ]
    }
    fn query(&self, format: &FORMATETC) -> HRESULT {
        if format.dwAspect != DVASPECT_CONTENT.0 {
            return DV_E_DVASPECT;
        }
        if format.cfFormat == self.descriptor && format.tymed & TYMED_HGLOBAL.0 as u32 != 0 {
            return S_OK;
        }
        if format.cfFormat == self.contents && format.tymed & TYMED_ISTREAM.0 as u32 != 0 {
            if format.lindex < 0
                || self
                    .export
                    .items
                    .get(format.lindex as usize)
                    .is_some_and(|item| !item.is_dir)
            {
                return S_OK;
            }
            return DV_E_LINDEX;
        }
        DV_E_FORMATETC
    }
    fn descriptors(&self) -> Result<STGMEDIUM> {
        let count =
            u32::try_from(self.export.items.len()).map_err(|_| Error::from(E_OUTOFMEMORY))?;
        let size = self
            .export
            .items
            .len()
            .checked_mul(size_of::<FILEDESCRIPTORW>())
            .and_then(|n| n.checked_add(size_of::<u32>()))
            .ok_or_else(|| Error::from(E_OUTOFMEMORY))?;
        // SAFETY: Allocate exactly count + packed descriptors, copy within the allocation,
        // unlock before publishing; the receiver owns and frees the resulting STGMEDIUM.
        unsafe {
            let handle = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, size)?;
            let data = GlobalLock(handle).cast::<u8>();
            if data.is_null() {
                let _ = GlobalFree(Some(handle));
                return Err(Error::from_win32());
            }
            data.cast::<u32>().write_unaligned(count);
            for (index, item) in self.export.items.iter().enumerate() {
                let mut descriptor = FILEDESCRIPTORW {
                    dwFlags: (FD_ATTRIBUTES.0
                        | FD_UNICODE.0
                        | FD_PROGRESSUI.0
                        | if item.is_dir { 0 } else { FD_FILESIZE.0 })
                        as u32,
                    dwFileAttributes: if item.is_dir {
                        FILE_ATTRIBUTE_DIRECTORY.0
                    } else {
                        FILE_ATTRIBUTE_NORMAL.0
                    },
                    nFileSizeHigh: (item.size >> 32) as u32,
                    nFileSizeLow: item.size as u32,
                    ..Default::default()
                };
                let mut name = [0u16; 260];
                for (slot, value) in name
                    .iter_mut()
                    .zip(item.name.replace('/', "\\").encode_utf16())
                {
                    *slot = value;
                }
                descriptor.cFileName = name;
                data.add(size_of::<u32>() + index * size_of::<FILEDESCRIPTORW>())
                    .cast::<FILEDESCRIPTORW>()
                    .write_unaligned(descriptor);
            }
            let _ = GlobalUnlock(handle);
            Ok(STGMEDIUM {
                tymed: TYMED_HGLOBAL.0 as u32,
                u: STGMEDIUM_0 { hGlobal: handle },
                pUnkForRelease: ManuallyDrop::new(None),
            })
        }
    }
}
#[allow(non_snake_case)]
impl IDataObject_Impl for VirtualFiles_Impl {
    fn GetData(&self, format: *const FORMATETC) -> Result<STGMEDIUM> {
        // SAFETY: COM passes a borrowed FORMATETC; reject null before reading.
        let format = unsafe { format.as_ref() }.ok_or_else(|| Error::from(E_POINTER))?;
        self.query(format).ok()?;
        if format.cfFormat == self.descriptor {
            return self.descriptors();
        }
        if format.lindex < 0 {
            return Err(Error::from(DV_E_LINDEX));
        }
        if !self.export.is_dropped() {
            return Err(Error::from(E_PENDING));
        }
        let path = self
            .export
            .file_path(format.lindex as usize)
            .map_err(|e| Error::new(E_FAIL, e))?;
        let path = HSTRING::from(path.as_os_str());
        // SAFETY: path is a live, terminated string. The returned stream owns its file
        // handle; retain the staging directory in the app until receivers finish.
        let stream = unsafe {
            SHCreateStreamOnFileEx(&path, (STGM_READ | STGM_SHARE_DENY_WRITE).0, 0, false, None)
        }?;
        Ok(STGMEDIUM {
            tymed: TYMED_ISTREAM.0 as u32,
            u: STGMEDIUM_0 {
                pstm: ManuallyDrop::new(Some(stream)),
            },
            pUnkForRelease: ManuallyDrop::new(None),
        })
    }
    fn GetDataHere(&self, _: *const FORMATETC, _: *mut STGMEDIUM) -> Result<()> {
        Err(Error::from(E_NOTIMPL))
    }
    fn QueryGetData(&self, format: *const FORMATETC) -> HRESULT {
        // SAFETY: Only borrow a nonnull format for the duration of this call.
        unsafe { format.as_ref() }.map_or(E_POINTER, |format| self.query(format))
    }
    fn GetCanonicalFormatEtc(&self, _: *const FORMATETC, output: *mut FORMATETC) -> HRESULT {
        // SAFETY: Clear the optional caller-owned output target-device pointer.
        if let Some(output) = unsafe { output.as_mut() } {
            output.ptd = std::ptr::null_mut();
        }
        DATA_S_SAMEFORMATETC
    }
    fn SetData(&self, _: *const FORMATETC, _: *const STGMEDIUM, _: BOOL) -> Result<()> {
        Err(Error::from(E_NOTIMPL))
    }
    fn EnumFormatEtc(&self, direction: u32) -> Result<IEnumFORMATETC> {
        if direction != DATADIR_GET.0 as u32 {
            return Err(Error::from(E_NOTIMPL));
        }
        // SAFETY: shell copies the stack-owned descriptors into the enumerator.
        unsafe { SHCreateStdEnumFmtEtc(&self.formats()) }
    }
    fn DAdvise(&self, _: *const FORMATETC, _: u32, _: Ref<'_, IAdviseSink>) -> Result<u32> {
        Err(Error::from(OLE_E_ADVISENOTSUPPORTED))
    }
    fn DUnadvise(&self, _: u32) -> Result<()> {
        Err(Error::from(OLE_E_ADVISENOTSUPPORTED))
    }
    fn EnumDAdvise(&self) -> Result<IEnumSTATDATA> {
        Err(Error::from(OLE_E_ADVISENOTSUPPORTED))
    }
}

#[implement(IDropSource)]
struct Source {
    export: Arc<DeferredExport>,
    cancel: Arc<AtomicBool>,
}
#[allow(non_snake_case)]
impl IDropSource_Impl for Source_Impl {
    fn QueryContinueDrag(&self, escape: BOOL, keys: MODIFIERKEYS_FLAGS) -> HRESULT {
        if escape.as_bool() || self.cancel.load(Ordering::Relaxed) {
            return DRAGDROP_S_CANCEL;
        }
        if keys & MK_LBUTTON == MODIFIERKEYS_FLAGS(0) {
            self.export.confirm_drop();
            return DRAGDROP_S_DROP;
        }
        S_OK
    }
    fn GiveFeedback(&self, _: DROPEFFECT) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}
pub(super) fn start(
    export: Arc<DeferredExport>,
    finished: impl Fn(std::result::Result<bool, String>) + Send + 'static,
    cancel: Arc<AtomicBool>,
) -> std::result::Result<(), String> {
    let files: IDataObject = VirtualFiles::new(export.clone())
        .map_err(|e| e.to_string())?
        .into();
    let source: IDropSource = Source { export, cancel }.into();
    // SAFETY: Called on eframe's Windows UI thread. Every successful OleInitialize
    // (including S_FALSE) is balanced, and COM objects outlive the modal drag loop.
    unsafe {
        OleInitialize(None).map_err(|e| e.to_string())?;
        let mut effect = DROPEFFECT_NONE;
        let result = DoDragDrop(&files, &source, DROPEFFECT_COPY, &mut effect);
        OleUninitialize();
        finished(if result.is_err() {
            Err(result.to_string())
        } else {
            Ok(result == DRAGDROP_S_DROP && effect == DROPEFFECT_COPY)
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests;
