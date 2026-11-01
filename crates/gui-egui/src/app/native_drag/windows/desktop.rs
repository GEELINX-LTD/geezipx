//! The merged desktop view is virtual; its background writes to FOLDERID_Desktop.
use super::*;
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetDesktopWindow, GetShellWindow, GetWindowThreadProcessId,
};

pub(super) fn host(hit: HWND) -> Option<HWND> {
    // SAFETY: Borrowed window handles are only queried, never retained or changed.
    unsafe { host_for_shell(hit, GetShellWindow()) }
}

fn host_for_shell(hit: HWND, shell: HWND) -> Option<HWND> {
    // SAFETY: All queries tolerate stale/null handles and fail closed below.
    unsafe {
        let root = GetAncestor(hit, GA_ROOT);
        let mut shell_pid = 0;
        let mut root_pid = 0;
        GetWindowThreadProcessId(shell, Some(&mut shell_pid));
        GetWindowThreadProcessId(root, Some(&mut root_pid));
        if shell_pid == 0 || shell_pid != root_pid {
            return None;
        }
        matches!(window_class(root).as_str(), "Progman" | "WorkerW").then_some(root)
    }
}

fn window_class(window: HWND) -> String {
    let mut name = [0; 256];
    // SAFETY: The fixed-size buffer is writable for the entire API call.
    let length = unsafe { GetClassNameW(window, &mut name) }.max(0) as usize;
    String::from_utf16_lossy(&name[..length])
}

fn is_background_window(window: HWND, host: HWND) -> bool {
    if window.0.is_null() {
        return false;
    }
    // SAFETY: Only inspect the hierarchy of the already verified desktop host.
    unsafe {
        window == host
            || window == GetDesktopWindow()
            || (GetAncestor(window, GA_ROOT) == host
                && matches!(
                    window_class(window).as_str(),
                    "SHELLDLL_DefView" | "SysListView32"
                ))
    }
}

pub(super) unsafe fn destination(
    automation: &IUIAutomation,
    point: POINT,
    host: HWND,
) -> Result<Option<PathBuf>> {
    // SAFETY: The caller owns this worker's COM apartment; nothing crosses threads.
    unsafe {
        match view_hit(automation, point, |window| {
            is_background_window(window, host)
        })? {
            ViewHit::Background => {
                // A desktop PIDL represents a virtual merge of user/public/system
                // items. Query the real user folder, including OneDrive/redirection.
                let raw = SHGetKnownFolderPath(&FOLDERID_Desktop, KF_FLAG_DEFAULT, None)?;
                let path = raw.to_string();
                CoTaskMemFree(Some(raw.0.cast()));
                let path = PathBuf::from(path?);
                Ok(path.is_dir().then_some(path))
            }
            ViewHit::Item(row) => {
                // Icons can originate in either user or public Desktop. Resolve
                // the actual filesystem item instead of appending its display name.
                let windows: IShellWindows = CoCreateInstance(&ShellWindows, None, CLSCTX_ALL)?;
                let mut desktop = 0;
                let dispatch = windows.FindWindowSW(
                    &VARIANT::from(CSIDL_DESKTOP as i32),
                    &VARIANT::default(),
                    SWC_DESKTOP,
                    &mut desktop,
                    SWFO_NEEDDISPATCH,
                )?;
                let provider: IServiceProvider = dispatch.cast()?;
                let browser: IShellBrowser = provider.QueryService(&SID_STopLevelBrowser)?;
                let view = browser.QueryActiveShellView()?;
                directory_item(&view.cast()?, &row)
            }
            ViewHit::Unknown => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::{
        Foundation::{LPARAM, LRESULT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassW, WNDCLASSW, WS_CHILD,
            WS_OVERLAPPED,
        },
    };

    unsafe extern "system" fn window_proc(
        window: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        // SAFETY: Forward the native window procedure's unchanged parameters.
        unsafe { DefWindowProcW(window, message, wparam, lparam) }
    }

    struct Window(HWND);
    impl Window {
        fn new(class: PCWSTR, parent: Option<HWND>) -> Self {
            // SAFETY: Registered names are static, the procedure is a system
            // default, and this thread owns/destroys each returned test window.
            unsafe {
                let instance = GetModuleHandleW(None).unwrap();
                let _ = RegisterClassW(&WNDCLASSW {
                    hInstance: instance.into(),
                    lpfnWndProc: Some(window_proc),
                    lpszClassName: class,
                    ..Default::default()
                });
                Self(
                    CreateWindowExW(
                        Default::default(),
                        class,
                        w!("GeeZipX desktop target regression"),
                        if parent.is_some() {
                            WS_CHILD
                        } else {
                            WS_OVERLAPPED
                        },
                        0,
                        0,
                        20,
                        20,
                        parent,
                        None,
                        Some(instance.into()),
                        None,
                    )
                    .unwrap(),
                )
            }
        }
    }
    impl Drop for Window {
        fn drop(&mut self) {
            // SAFETY: The owning test thread releases its window exactly once.
            unsafe {
                let _ = DestroyWindow(self.0);
            }
        }
    }

    #[test]
    fn recognizes_desktop_hierarchy_without_accepting_explorer_or_unknown_windows() {
        let shell = Window::new(w!("Progman"), None);
        let worker = Window::new(w!("WorkerW"), None);
        let view = Window::new(w!("SHELLDLL_DefView"), Some(worker.0));
        let list = Window::new(w!("SysListView32"), Some(view.0));
        let explorer = Window::new(w!("CabinetWClass"), None);
        let explorer_view = Window::new(w!("SHELLDLL_DefView"), Some(explorer.0));
        assert_eq!(host_for_shell(list.0, shell.0), Some(worker.0));
        assert_eq!(host_for_shell(shell.0, shell.0), Some(shell.0));
        assert_eq!(host_for_shell(explorer_view.0, shell.0), None);
        assert_eq!(host_for_shell(list.0, HWND::default()), None);
        assert!(is_background_window(list.0, worker.0));
        assert!(is_background_window(view.0, worker.0));
        assert!(is_background_window(worker.0, worker.0));
        assert!(!is_background_window(explorer_view.0, worker.0));
        assert!(!is_background_window(HWND::default(), worker.0));
    }
}
