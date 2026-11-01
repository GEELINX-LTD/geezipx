//! Application-owned Windows drag extraction. No FILECONTENTS/Explorer copy job.
//! Only destination metadata is queried after release; all payload I/O uses Job.
use super::DropOutcome;
use geezipx_egui::export::DeferredExport;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use windows::{
    core::*,
    Win32::{
        Foundation::{HWND, POINT},
        System::{
            Com::*,
            SystemServices::{SFGAO_FILESYSTEM, SFGAO_FOLDER},
            Variant::VARIANT,
        },
        UI::{
            Accessibility::*,
            Input::KeyboardAndMouse::{GetAsyncKeyState, VK_ESCAPE, VK_LBUTTON},
            Shell::*,
            WindowsAndMessaging::{
                GetAncestor, GetCursorPos, IsChild, IsWindowVisible, WindowFromPoint, GA_ROOT,
            },
        },
    },
};

pub(super) fn start(
    frame: &eframe::Frame,
    export: Arc<DeferredExport>,
    finished: impl Fn(std::result::Result<DropOutcome, String>) + Send + 'static,
    cancel: Arc<AtomicBool>,
) -> std::result::Result<(), String> {
    let source = match frame.window_handle().map_err(|e| e.to_string())?.as_raw() {
        RawWindowHandle::Win32(window) => window.hwnd.get() as usize,
        _ => return Err("Windows window handle unavailable".into()),
    };
    std::thread::Builder::new()
        .name("geezipx-drop-target".into())
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                track_drop(source, &export, &cancel)
            }))
            .unwrap_or_else(|_| Err("Windows drop target lookup failed".into()));
            finished(result);
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn track_drop(
    source: usize,
    export: &DeferredExport,
    cancel: &AtomicBool,
) -> std::result::Result<DropOutcome, String> {
    // SAFETY: Key-state queries use fixed virtual-key constants. No COM or data
    // reads occur while the button is down, and Escape never opens a chooser.
    while unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) } < 0 {
        if cancel.load(Ordering::Relaxed)
            || export.control.is_cancelled()
            || unsafe { GetAsyncKeyState(VK_ESCAPE.0 as i32) } < 0
        {
            return Ok(DropOutcome::Cancelled);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    if cancel.load(Ordering::Relaxed)
        || export.control.is_cancelled()
        || unsafe { GetAsyncKeyState(VK_ESCAPE.0 as i32) } < 0
    {
        return Ok(DropOutcome::Cancelled);
    }
    let mut point = POINT::default();
    // SAFETY: point is writable; HWNDs are borrowed for read-only hit testing.
    unsafe {
        GetCursorPos(&mut point).map_err(|e| e.to_string())?;
        if GetAncestor(WindowFromPoint(point), GA_ROOT).0 as usize == source {
            return Ok(DropOutcome::Cancelled);
        }
    }
    let target = destination(point).ok().flatten();
    if cancel.load(Ordering::Relaxed) || export.control.is_cancelled() {
        return Ok(DropOutcome::Cancelled);
    }
    // A virtual/unknown destination is explicitly chosen in GeeZipX's dialog.
    // Never guess the active Explorer path or silently delegate a shell copy.
    let target = target.or_else(|| rfd::FileDialog::new().pick_folder());
    if cancel.load(Ordering::Relaxed) || export.control.is_cancelled() {
        return Ok(DropOutcome::Cancelled);
    }
    Ok(match target {
        Some(path) => {
            export.confirm_drop();
            DropOutcome::ExtractTo(path)
        }
        None => DropOutcome::Cancelled,
    })
}

struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        // SAFETY: Exactly balances this thread's successful CoInitializeEx.
        unsafe { CoUninitialize() };
    }
}

fn destination(point: POINT) -> Result<Option<PathBuf>> {
    // SAFETY: All interfaces are created, used and dropped on this worker STA.
    // The guard is declared first so COM resources drop before uninitializing.
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        let _apartment = Apartment;
        let windows: IShellWindows = CoCreateInstance(&ShellWindows, None, CLSCTX_ALL)?;
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)?;
        if let Ok(options) = automation.cast::<IUIAutomation2>() {
            let _ = options.SetConnectionTimeout(1500);
            let _ = options.SetTransactionTimeout(1500);
        }
        let hit = WindowFromPoint(point);
        for index in 0..windows.Count()? {
            let Ok(dispatch) = windows.Item(&VARIANT::from(index)) else {
                continue;
            };
            if let Ok(Some(path)) = destination_in_view(&dispatch, &automation, point, hit) {
                return Ok(Some(path));
            }
        }
        // Desktop views are not included in every IShellWindows enumeration.
        let mut desktop = 0;
        if let Ok(dispatch) = windows.FindWindowSW(
            &VARIANT::default(),
            &VARIANT::default(),
            SWC_DESKTOP,
            &mut desktop,
            SWFO_NEEDDISPATCH,
        ) {
            return destination_in_view(&dispatch, &automation, point, hit);
        }
        Ok(None)
    }
}

unsafe fn destination_in_view(
    dispatch: &IDispatch,
    automation: &IUIAutomation,
    point: POINT,
    hit: HWND,
) -> Result<Option<PathBuf>> {
    // SAFETY: Called in the initialized apartment; all returned COM pointers and
    // allocated shell strings/PIDLs have bounded ownership in this function.
    unsafe {
        let provider: IServiceProvider = dispatch.cast()?;
        let browser: IShellBrowser = provider.QueryService(&SID_STopLevelBrowser)?;
        let view = browser.QueryActiveShellView()?;
        let window = view.GetWindow()?;
        if !IsWindowVisible(window).as_bool() || !(window == hit || IsChild(window, hit).as_bool())
        {
            return Ok(None);
        }
        let folder: IFolderView2 = view.cast()?;
        let mut element = automation.ElementFromPoint(point)?;
        let walker = automation.ControlViewWalker()?;
        let mut row = None;
        let mut reached_view = false;
        for _ in 0..32 {
            let kind = element.CurrentControlType()?;
            if kind == UIA_ListItemControlTypeId || kind == UIA_DataItemControlTypeId {
                row = Some(element.CurrentName()?.to_string());
                break;
            }
            if element.CurrentNativeWindowHandle()? == window {
                reached_view = true;
                break;
            }
            let Ok(parent) = walker.GetParentElement(&element) else {
                break;
            };
            element = parent;
        }
        if let Some(row) = row {
            let items: IShellItemArray = folder.Items(SVGIO_ALLVIEW)?;
            let mut target = None;
            for index in 0..items.GetCount()? {
                let item = items.GetItemAt(index)?;
                if item_name(&item, SIGDN_NORMALDISPLAY)? == row {
                    // Duplicate display names are ambiguous (e.g. hidden extensions).
                    if target.is_some() {
                        return Ok(None);
                    }
                    let attributes = item.GetAttributes(SFGAO_FOLDER | SFGAO_FILESYSTEM)?;
                    if attributes & (SFGAO_FOLDER | SFGAO_FILESYSTEM)
                        != SFGAO_FOLDER | SFGAO_FILESYSTEM
                    {
                        return Ok(None);
                    }
                    target = Some(PathBuf::from(item_name(&item, SIGDN_FILESYSPATH)?));
                }
            }
            return Ok(target.filter(|path| path.is_dir()));
        }
        if !reached_view {
            return Ok(None);
        }
        let persist: IPersistFolder2 = folder.GetFolder()?;
        let pidl = persist.GetCurFolder()?;
        let item: Result<IShellItem> = SHCreateItemFromIDList(pidl);
        CoTaskMemFree(Some(pidl.cast()));
        let path = PathBuf::from(item_name(&item?, SIGDN_FILESYSPATH)?);
        Ok(path.is_dir().then_some(path))
    }
}

unsafe fn item_name(item: &IShellItem, kind: SIGDN) -> Result<String> {
    // SAFETY: GetDisplayName returns a task-allocated UTF-16 string; free it even
    // when conversion fails, before returning the owned Rust string.
    unsafe {
        let name = item.GetDisplayName(kind)?;
        let value = name.to_string();
        CoTaskMemFree(Some(name.0.cast()));
        Ok(value?)
    }
}
