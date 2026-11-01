//! Explorer DelegateExecute server. It starts before the graphics runtime.
use super::{handoff, windows::CLSIDS};
use crate::launch::Action;
use std::{cell::RefCell, ffi::c_void, path::PathBuf};
use windows::{
    core::{implement, ComObject, IUnknown, Interface, Ref, GUID, PCWSTR},
    Win32::{
        Foundation::{
            CLASS_E_NOAGGREGATION, E_FAIL, E_INVALIDARG, E_NOINTERFACE, E_NOTIMPL, E_POINTER, POINT,
        },
        System::Com::{
            CoAddRefServerProcess, CoInitializeEx, CoRegisterClassObject, CoReleaseServerProcess,
            CoResumeClassObjects, CoRevokeClassObject, CoTaskMemFree, CoUninitialize,
            IClassFactory, IClassFactory_Impl, CLSCTX_LOCAL_SERVER, COINIT_APARTMENTTHREADED,
            REGCLS_MULTIPLEUSE, REGCLS_SUSPENDED,
        },
        UI::{
            Shell::{
                IExecuteCommand, IExecuteCommand_Impl, IObjectWithSelection,
                IObjectWithSelection_Impl, IShellItemArray, SIGDN_FILESYSPATH,
            },
            WindowsAndMessaging::{
                DispatchMessageW, GetMessageW, KillTimer, PeekMessageW, PostQuitMessage, SetTimer,
                TranslateMessage, MSG, PM_NOREMOVE, WM_TIMER,
            },
        },
    },
};
static LIVE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
fn retain() {
    LIVE.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    unsafe {
        CoAddRefServerProcess();
    }
}
fn release() {
    // SAFETY: every command and server lock owns one process reference on this STA.
    LIVE.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    if unsafe { CoReleaseServerProcess() } == 0 {
        unsafe { PostQuitMessage(0) };
    }
}
#[implement(IExecuteCommand, IObjectWithSelection)]
struct Command {
    action: Action,
    selection: RefCell<Option<IShellItemArray>>,
}
impl Drop for Command {
    fn drop(&mut self) {
        release();
    }
}
impl IObjectWithSelection_Impl for Command_Impl {
    fn SetSelection(&self, selection: Ref<'_, IShellItemArray>) -> windows::core::Result<()> {
        *self.selection.borrow_mut() = Some(
            selection
                .as_ref()
                .cloned()
                .ok_or_else(|| windows::core::Error::from(E_INVALIDARG))?,
        );
        Ok(())
    }
    fn GetSelection(
        &self,
        _iid: *const GUID,
        output: *mut *mut c_void,
    ) -> windows::core::Result<()> {
        if !output.is_null() {
            unsafe { *output = std::ptr::null_mut() };
        }
        Err(E_NOTIMPL.into())
    }
}
impl IExecuteCommand_Impl for Command_Impl {
    fn SetKeyState(&self, _: u32) -> windows::core::Result<()> {
        Ok(())
    }
    fn SetParameters(&self, _: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }
    fn SetPosition(&self, _: &POINT) -> windows::core::Result<()> {
        Ok(())
    }
    fn SetShowWindow(&self, _: i32) -> windows::core::Result<()> {
        Ok(())
    }
    fn SetNoShowUI(&self, _: windows::core::BOOL) -> windows::core::Result<()> {
        Ok(())
    }
    fn SetDirectory(&self, _: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }
    fn Execute(&self) -> windows::core::Result<()> {
        use std::os::windows::ffi::OsStringExt;
        let selection = self
            .selection
            .borrow_mut()
            .take()
            .ok_or_else(|| windows::core::Error::from(E_INVALIDARG))?;
        let mut paths = Vec::new();
        // SAFETY: the Shell item array remains owned by this STA. Copy and free each COM string.
        unsafe {
            let count = selection.GetCount()?;
            if count == 0 || count > 100_000 {
                return Err(E_INVALIDARG.into());
            }
            for index in 0..count {
                let name = selection
                    .GetItemAt(index)?
                    .GetDisplayName(SIGDN_FILESYSPATH)?;
                if name.is_null() {
                    return Err(E_FAIL.into());
                }
                let path = std::ffi::OsString::from_wide(name.as_wide());
                CoTaskMemFree(Some(name.as_ptr().cast()));
                if path.is_empty() {
                    return Err(E_INVALIDARG.into());
                }
                paths.push(PathBuf::from(path));
            }
        }
        let exe = super::executable().map_err(|e| windows::core::Error::new(E_FAIL, e))?;
        let file = handoff::write(self.action, &paths)
            .map_err(|e| windows::core::Error::new(E_FAIL, e))?;
        match std::process::Command::new(exe)
            .arg("--shell-action-file")
            .arg(&file)
            .spawn()
        {
            Ok(_) => Ok(()),
            Err(e) => {
                let _ = std::fs::remove_file(file);
                Err(windows::core::Error::new(E_FAIL, e.to_string()))
            }
        }
    }
}
#[implement(IClassFactory)]
struct Factory {
    action: Action,
}
impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(
        &self,
        outer: Ref<'_, IUnknown>,
        iid: *const GUID,
        output: *mut *mut c_void,
    ) -> windows::core::Result<()> {
        if output.is_null() {
            return Err(E_POINTER.into());
        }
        // SAFETY: COM supplied a writable out pointer, cleared on every failure path.
        unsafe {
            *output = std::ptr::null_mut();
        }
        if iid.is_null() {
            return Err(E_POINTER.into());
        }
        if !outer.is_null() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        let iid = unsafe { *iid };
        if ![
            IUnknown::IID,
            IExecuteCommand::IID,
            IObjectWithSelection::IID,
        ]
        .contains(&iid)
        {
            return Err(E_NOINTERFACE.into());
        }
        retain();
        let object = ComObject::new(Command {
            action: self.action,
            selection: RefCell::new(None),
        });
        if iid == IExecuteCommand::IID {
            let interface: IExecuteCommand = object.into_interface();
            unsafe {
                *output = interface.into_raw();
            }
        } else if iid == IObjectWithSelection::IID {
            let interface: IObjectWithSelection = object.into_interface();
            unsafe {
                *output = interface.into_raw();
            }
        } else {
            let interface: IUnknown = object.into_interface();
            unsafe {
                *output = interface.into_raw();
            }
        }
        Ok(())
    }
    fn LockServer(&self, lock: windows::core::BOOL) -> windows::core::Result<()> {
        if lock.as_bool() {
            retain();
        } else {
            release();
        }
        Ok(())
    }
}
pub fn run() -> Result<(), String> {
    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }
    struct Registrations(Vec<u32>);
    impl Drop for Registrations {
        fn drop(&mut self) {
            for cookie in &self.0 {
                let _ = unsafe { CoRevokeClassObject(*cookie) };
            }
        }
    }
    fn serve() -> windows::core::Result<()> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        }
        let _apartment = Apartment;
        let mut message = MSG::default();
        // Create the message queue before clients can release their server references.
        unsafe {
            let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
        }
        let mut registrations = Registrations(Vec::new());
        for (index, clsid) in CLSIDS.iter().enumerate() {
            let clsid = GUID::try_from(clsid.trim_start_matches('{').trim_end_matches('}'))?;
            let factory: IUnknown = ComObject::new(Factory {
                action: if index == 0 {
                    Action::Compress
                } else {
                    Action::CompressZip
                },
            })
            .into_interface();
            registrations.0.push(unsafe {
                CoRegisterClassObject(
                    &clsid,
                    &factory,
                    CLSCTX_LOCAL_SERVER,
                    REGCLS_MULTIPLEUSE | REGCLS_SUSPENDED,
                )?
            });
        }
        // A never-activated server must also terminate. Active command execution is synchronous on this STA.
        let timer = unsafe { SetTimer(None, 0, 30_000, None) };
        unsafe {
            CoResumeClassObjects()?;
        }
        loop {
            let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
            if result.0 == -1 {
                return Err(windows::core::Error::from_win32());
            }
            if result.0 == 0
                || (message.message == WM_TIMER
                    && message.wParam.0 == timer
                    && LIVE.load(std::sync::atomic::Ordering::SeqCst) == 0)
            {
                break;
            }
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        if timer != 0 {
            let _ = unsafe { KillTimer(None, timer) };
        }
        Ok(())
    }
    serve().map_err(|e| e.to_string())
}
