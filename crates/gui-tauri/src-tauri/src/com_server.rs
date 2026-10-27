//! COM LocalServer (`-Embedding`) for Windows shell context-menu verbs.
//!
//! Phase C — full DelegateExecute integration with shell_menu.rs and NSIS.
//!
//! ## Architecture
//!
//! ```text
//! Explorer
//!   │ CoCreateInstance(CLSID)
//!   ▼
//! geezipx-gui.exe -Embedding
//!   │ CoRegisterClassObject / STA message pump
//!   │ IObjectWithSelection::SetSelection(IShellItemArray)
//!   │ IExecuteCommand::Execute()
//!   ├─ write_action_file(action, paths)   → %LOCALAPPDATA%\GeeZipX\ShellActions
//!   ├─ geezipx-gui.exe --shell-action-file <path>
//!   └─ PostThreadMessage(WM_QUIT) when idle
//! ```
//!
//! ## CLSIDs
//!
//! Two new stable CLSIDs (not from the old MSIX PoC):
//! - `CLSID_COMPRESS`     → `ShellActionFileAction::Compress`
//! - `CLSID_COMPRESS_ZIP` → `ShellActionFileAction::CompressZip`
//!
//! These constants (both GUID and string forms) are `pub(crate)` so Phase C
//! (`shell_menu.rs`) can reuse them for DelegateExecute registration and
//! CLSID key path construction.

#[cfg(target_os = "windows")]
use crate::shell_action_file;

// ===========================================================================
// Embedding detection (platform-independent, testable)
// ===========================================================================

/// Returns `true` if any argument is `-Embedding` or `/Embedding`
/// (case-insensitive).
///
/// COM prepends this flag when launching LocalServer32 EXEs; the detection
/// is path-independent so it works for bare process names ("geezipx-gui.exe
/// -Embedding"), full paths, and `/Embedding` variants.
pub fn is_embedding_arg(args: &[String]) -> bool {
    args.iter()
        .any(|a| a.eq_ignore_ascii_case("-Embedding") || a.eq_ignore_ascii_case("/Embedding"))
}

// ===========================================================================
// CLSID definitions — string form (platform-independent, testable everywhere)
// ===========================================================================

/// CLSID for Compress verb as a registry-format string `{...}`.
///
/// Used by `shell_menu.rs` for DelegateExecute and CLSID key path construction.
/// Must match the GUID constant below and the NSIS `!define`.
pub(crate) const CLSID_COMPRESS_STR: &str = "{C1E5F6A0-8F6A-4F9E-B5C2-1C0A9B8F7E6D}";

/// CLSID for CompressZip verb as a registry-format string `{...}`.
pub(crate) const CLSID_COMPRESS_ZIP_STR: &str = "{D2F6A7B1-9A7B-4A0F-C6D3-2D1B0C9A8F7E}";

// ===========================================================================
// CLSID definitions — GUID form (Windows-only)
// ===========================================================================

/// CLSID for the Compress shell verb (DelegateExecute COM handler).
///
/// Maps to [`ShellActionFileAction::Compress`].
#[cfg(target_os = "windows")]
pub const CLSID_COMPRESS: windows::core::GUID = windows::core::GUID::from_values(
    0xC1E5F6A0,
    0x8F6A,
    0x4F9E,
    [0xB5, 0xC2, 0x1C, 0x0A, 0x9B, 0x8F, 0x7E, 0x6D],
);

/// CLSID for the CompressZip shell verb (DelegateExecute COM handler).
///
/// Maps to [`ShellActionFileAction::CompressZip`].
#[cfg(target_os = "windows")]
pub const CLSID_COMPRESS_ZIP: windows::core::GUID = windows::core::GUID::from_values(
    0xD2F6A7B1,
    0x9A7B,
    0x4A0F,
    [0xC6, 0xD3, 0x2D, 0x1B, 0x0C, 0x9A, 0x8F, 0x7E],
);

/// Map a CLSID to the corresponding shell action.
///
/// Returns `None` for unrecognised CLSIDs (should not happen in practice).
#[cfg(target_os = "windows")]
pub fn action_for_clsid(
    clsid: &windows::core::GUID,
) -> Option<shell_action_file::ShellActionFileAction> {
    if *clsid == CLSID_COMPRESS {
        Some(shell_action_file::ShellActionFileAction::Compress)
    } else if *clsid == CLSID_COMPRESS_ZIP {
        Some(shell_action_file::ShellActionFileAction::CompressZip)
    } else {
        None
    }
}

// ===========================================================================
// CLSID bytes for testing on non-Windows platforms
// ===========================================================================

/// Bytes of CLSID_COMPRESS (little-endian GUID layout).
#[cfg(not(target_os = "windows"))]
#[allow(dead_code)]
const CLSID_COMPRESS_BYTES: [u8; 16] = {
    let d1 = 0xC1E5F6A0u32.to_le_bytes();
    let d2 = 0x8F6Au16.to_le_bytes();
    let d3 = 0x4F9Eu16.to_le_bytes();
    [
        d1[0], d1[1], d1[2], d1[3], // Data1
        d2[0], d2[1], // Data2
        d3[0], d3[1], // Data3
        0xB5, 0xC2, 0x1C, 0x0A, 0x9B, 0x8F, 0x7E, 0x6D, // Data4
    ]
};

/// Bytes of CLSID_COMPRESS_ZIP (little-endian GUID layout).
#[cfg(not(target_os = "windows"))]
#[allow(dead_code)]
const CLSID_COMPRESS_ZIP_BYTES: [u8; 16] = {
    let d1 = 0xD2F6A7B1u32.to_le_bytes();
    let d2 = 0x9A7Bu16.to_le_bytes();
    let d3 = 0x4A0Fu16.to_le_bytes();
    [
        d1[0], d1[1], d1[2], d1[3], // Data1
        d2[0], d2[1], // Data2
        d3[0], d3[1], // Data3
        0xC6, 0xD3, 0x2D, 0x1B, 0x0C, 0x9A, 0x8F, 0x7E, // Data4
    ]
};

// ===========================================================================
// Windows-only COM implementation
// ===========================================================================

#[cfg(target_os = "windows")]
mod platform {
    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::{CLSID_COMPRESS, CLSID_COMPRESS_ZIP};
    use crate::shell_action_file::{self, ShellActionFileAction};
    use windows::core::{implement, IUnknown, Interface, Ref, GUID};
    use windows::Win32::Foundation::{
        CLASS_E_NOAGGREGATION, E_FAIL, E_INVALIDARG, E_NOINTERFACE, E_NOTIMPL, E_POINTER,
        E_UNEXPECTED, LPARAM, WPARAM,
    };
    use windows::Win32::System::Com::{
        CoInitializeEx, CoRegisterClassObject, CoRevokeClassObject, CoTaskMemFree, IClassFactory,
        IClassFactory_Impl, CLSCTX_LOCAL_SERVER, COINIT_APARTMENTTHREADED, REGCLS_MULTIPLEUSE,
    };
    use windows::Win32::System::Diagnostics::Debug::OutputDebugStringW;
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::Shell::{
        IExecuteCommand, IExecuteCommand_Impl, IObjectWithSelection, IObjectWithSelection_Impl,
        IShellItem, IShellItemArray, SIGDN_FILESYSPATH,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, PostThreadMessageW, TranslateMessage, MSG, WM_QUIT,
    };

    // ------------------------------------------------------------------
    // Global lifetime counters
    //
    // The server stays alive as long as any COM object exists OR any
    // client holds a server lock (via IClassFactory::LockServer).
    // When both counts reach zero the message pump posts WM_QUIT to
    // itself and the process exits.
    // ------------------------------------------------------------------

    /// Number of live ShellCommand instances.
    /// Class factories are NOT counted — they are held alive by COM
    /// registration cookies and released during shutdown.
    static OBJECT_COUNT: AtomicU32 = AtomicU32::new(0);

    /// Number of outstanding server locks (IClassFactory::LockServer).
    static LOCK_COUNT: AtomicU32 = AtomicU32::new(0);

    /// Try to unload: if both counts are zero, post WM_QUIT.
    fn try_unload() {
        if OBJECT_COUNT.load(Ordering::SeqCst) == 0 && LOCK_COUNT.load(Ordering::SeqCst) == 0 {
            // Post WM_QUIT to ourselves.  A spurious quit is acceptable
            // for a transient COM server.  Log failures via
            // OutputDebugStringW so they are visible in DebugView/WinDbg.
            let tid = unsafe { GetCurrentThreadId() };
            unsafe {
                if PostThreadMessageW(tid, WM_QUIT, WPARAM(0), LPARAM(0)).is_err() {
                    debug_output("GeeZipX COM: PostThreadMessageW(WM_QUIT) failed");
                }
            }
        }
    }

    /// Debug output sent to `OutputDebugStringW` (visible in DebugView / WinDbg).
    fn debug_output(msg: &str) {
        let wide: Vec<u16> = msg.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            OutputDebugStringW(windows::core::PCWSTR::from_raw(wide.as_ptr()));
        }
    }

    // ------------------------------------------------------------------
    // CoMem — RAII wrapper for CoTaskMem-allocated PWSTRs
    // ------------------------------------------------------------------

    /// Wraps a `CoTaskMemFree`-allocated `PWSTR` and frees it on drop.
    struct CoMem(windows::core::PWSTR);

    impl CoMem {
        /// Convert to an owned `String`, consuming the wrapper.
        ///
        /// The returned `String` is a **copy** of the wide string data —
        /// the original `CoTaskMem`-allocated buffer is still owned by
        /// `self` and freed when `self` is dropped (even on the error path,
        /// because `Drop` fires regardless of whether `into_string`
        /// succeeded or returned `Err(self)`).
        ///
        /// On failure returns `Err(self)` — the memory is NOT leaked; Drop
        /// will still free it.
        ///
        /// # Safety
        /// Caller must ensure `.0` points to a valid null-terminated wide
        /// string allocated by `CoTaskMemAlloc` (or returned by a Shell API
        /// that documents the caller must free with `CoTaskMemFree`).
        unsafe fn into_string(self) -> Result<String, Self> {
            // SAFETY: PWSTR::to_string() reads a null-terminated wide string
            // and copies it into an owned Rust String.
            unsafe { self.0.to_string() }.map_err(|_| self)
        }
    }

    impl Drop for CoMem {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: The pointer was allocated by a COM/Shell function
                // that documents CoTaskMemFree as the required deallocator
                // (e.g. IShellItem::GetDisplayName).
                unsafe { CoTaskMemFree(Some(self.0.as_ptr() as _)) };
            }
        }
    }

    // ------------------------------------------------------------------
    // ShellCommand — implements IExecuteCommand + IObjectWithSelection
    // ------------------------------------------------------------------

    #[implement(IExecuteCommand, IObjectWithSelection)]
    struct ShellCommand {
        /// Which action to perform (Compress or CompressZip).
        action: ShellActionFileAction,
        /// Selection set by IObjectWithSelection::SetSelection.
        selection: RefCell<Option<IShellItemArray>>,
    }

    impl ShellCommand {
        /// Resolve the stored `IShellItemArray` to a `Vec<PathBuf>`.
        ///
        /// Each item must be a file-system object; non-file-system items
        /// cause the entire selection to be rejected (no partial success).
        fn resolve_paths(&self) -> windows::core::Result<Vec<PathBuf>> {
            // Clone the IShellItemArray out of the RefCell so the borrow is
            // released before we call any COM methods on the clone.
            // COM re-entrancy (e.g. nested message pumps) can otherwise
            // cause a double-borrow panic.
            let sia: IShellItemArray = {
                let mut opt = self.selection.borrow_mut();
                opt.take()
                    .ok_or_else(|| windows::core::Error::new(E_INVALIDARG, "no selection set"))?
            };

            // SAFETY: IShellItemArray methods are thin wrappers around COM
            // vtables.  The array is guaranteed to outlive this call.
            let count = unsafe { sia.GetCount() }.map_err(|e| {
                windows::core::Error::new(E_UNEXPECTED, format!("GetCount failed: {e}"))
            })?;

            if count == 0 {
                return Err(windows::core::Error::new(E_INVALIDARG, "empty selection"));
            }

            let mut paths = Vec::with_capacity(count as usize);
            for i in 0..count {
                // SAFETY: Index is within [0, count), COM method is
                // standard IShellItemArray semantics.
                let item: IShellItem = unsafe { sia.GetItemAt(i) }.map_err(|e| {
                    windows::core::Error::new(E_UNEXPECTED, format!("GetItemAt({i}) failed: {e}"))
                })?;

                // SAFETY: GetDisplayName with SIGDN_FILESYSPATH returns a
                // CoTaskMemAlloc-ed PWSTR the caller must free with
                // CoTaskMemFree.  CoMem below handles that.
                let pwstr = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }.map_err(|e| {
                    windows::core::Error::new(
                        E_FAIL,
                        format!("GetDisplayName({i}) failed (not a file-system object?): {e}"),
                    )
                })?;
                let cm = CoMem(pwstr);

                // SAFETY: The pointer came from GetDisplayName which
                // returns a valid null-terminated wide string.
                let s = unsafe { cm.into_string() }.map_err(|_| {
                    windows::core::Error::new(
                        E_FAIL,
                        format!("GetDisplayName({i}) returned invalid UTF-16"),
                    )
                })?;

                if s.is_empty() {
                    return Err(windows::core::Error::new(
                        E_FAIL,
                        format!("GetDisplayName({i}) returned empty string"),
                    ));
                }

                paths.push(PathBuf::from(s));
            }

            Ok(paths)
        }
    }

    // SAFETY: The `#[implement]` macro generates Drop for the outer
    // `ShellCommand_Impl` type that calls this inner Drop.
    //
    // Each ShellCommand instance increments OBJECT_COUNT on creation
    // (in IClassFactory::CreateInstance).  When the last command is
    // released AND no server locks are held, WM_QUIT is posted.
    impl Drop for ShellCommand {
        fn drop(&mut self) {
            OBJECT_COUNT.fetch_sub(1, Ordering::SeqCst);
            try_unload();
        }
    }

    // -- IObjectWithSelection_Impl -----------------------------------------

    impl IObjectWithSelection_Impl for ShellCommand_Impl {
        fn SetSelection(&self, psia: Ref<'_, IShellItemArray>) -> windows::core::Result<()> {
            // Clone (AddRef) the IShellItemArray to hold a reference.
            //
            // SAFETY: as_ref() checks for null pointer internally and returns
            // None if the pointer is null. We handle that case.
            let owned: IShellItemArray = psia
                .as_ref()
                .cloned()
                .ok_or_else(|| windows::core::Error::new(E_INVALIDARG, "null IShellItemArray"))?;
            *self.selection.borrow_mut() = Some(owned);
            Ok(())
        }

        fn GetSelection(
            &self,
            _riid: *const GUID,
            ppv: *mut *mut std::ffi::c_void,
        ) -> windows::core::Result<()> {
            // We don't support clients querying the selection back.
            // SAFETY: ppv is a valid out-pointer per COM contract;
            // we must null it even on error so the caller doesn't
            // dereference an uninitialised pointer.
            if !ppv.is_null() {
                unsafe {
                    *ppv = std::ptr::null_mut();
                }
            }
            Err(windows::core::Error::new(
                E_NOTIMPL,
                "GetSelection not implemented",
            ))
        }
    }

    // -- IExecuteCommand_Impl ----------------------------------------------

    impl IExecuteCommand_Impl for ShellCommand_Impl {
        fn SetKeyState(&self, _grfkeystate: u32) -> windows::core::Result<()> {
            Ok(())
        }
        fn SetParameters(
            &self,
            _pszparameters: &windows::core::PCWSTR,
        ) -> windows::core::Result<()> {
            Ok(())
        }
        fn SetPosition(
            &self,
            _pt: &windows::Win32::Foundation::POINT,
        ) -> windows::core::Result<()> {
            Ok(())
        }
        fn SetShowWindow(&self, _nshow: i32) -> windows::core::Result<()> {
            Ok(())
        }
        fn SetNoShowUI(&self, _fnoshowui: windows::core::BOOL) -> windows::core::Result<()> {
            Ok(())
        }
        fn SetDirectory(&self, _pszdirectory: &windows::core::PCWSTR) -> windows::core::Result<()> {
            Ok(())
        }

        fn Execute(&self) -> windows::core::Result<()> {
            debug_output("GeeZipX COM: Execute() called");

            // 1. Resolve paths
            let paths = self.resolve_paths().map_err(|e| {
                debug_output(&format!("GeeZipX COM: resolve_paths failed: {e}"));
                e
            })?;

            debug_output(&format!("GeeZipX COM: resolved {} path(s)", paths.len()));

            // 2. Write action file
            let action_file_path = shell_action_file::write_action_file(self.action, &paths)
                .map_err(|e| {
                    let msg = format!("write_action_file failed: {e}");
                    debug_output(&format!("GeeZipX COM: {msg}"));
                    windows::core::Error::new(E_FAIL, msg)
                })?;

            debug_output(&format!(
                "GeeZipX COM: action file written: {}",
                action_file_path.display()
            ));

            // 3. Get the current executable path
            let exe_path = std::env::current_exe().map_err(|e| {
                let msg = format!("current_exe() failed: {e}");
                debug_output(&format!("GeeZipX COM: {msg}"));
                windows::core::Error::new(E_FAIL, msg)
            })?;

            // 4. Launch the normal GUI with --shell-action-file
            let shell_value = action_file_path.to_string_lossy().to_string();

            let status = std::process::Command::new(&exe_path)
                .arg("--shell-action-file")
                .arg(&shell_value)
                .spawn();

            match status {
                Ok(_child) => {
                    debug_output("GeeZipX COM: GUI process launched successfully");
                    Ok(())
                }
                Err(e) => {
                    // Best-effort: delete the action file on failure
                    let _ = std::fs::remove_file(&action_file_path);
                    let msg = format!("failed to launch GUI: {e}");
                    debug_output(&format!("GeeZipX COM: {msg}"));
                    Err(windows::core::Error::new(E_FAIL, msg))
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // ClassFactory — one per CLSID
    // ------------------------------------------------------------------

    #[implement(IClassFactory)]
    struct ClassFactory {
        /// The shell action to embed in created ShellCommands.
        action: ShellActionFileAction,
    }

    // ClassFactory does NOT contribute to OBJECT_COUNT.  Only actual
    // ShellCommand instances do.  The factories are held alive by COM
    // registration cookies and released during CoRevokeClassObject +
    // CoUninitialize.  There is no custom Drop — the generated Drop
    // (which releases the IClassFactory vtable) is sufficient.

    impl IClassFactory_Impl for ClassFactory_Impl {
        fn CreateInstance(
            &self,
            outer: Ref<'_, windows::core::IUnknown>,
            riid: *const GUID,
            ppvobject: *mut *mut std::ffi::c_void,
        ) -> windows::core::Result<()> {
            // COM contract: riid and ppvobject must be non-null.
            if riid.is_null() || ppvobject.is_null() {
                return Err(windows::core::Error::new(
                    E_POINTER,
                    "riid and ppvobject must not be null",
                ));
            }

            // Aggregation not supported.
            if !outer.is_null() {
                // SAFETY: ppvobject is non-null; null it on error.
                unsafe { *ppvobject = std::ptr::null_mut() };
                return Err(windows::core::Error::from(CLASS_E_NOAGGREGATION));
            }

            // SAFETY: `riid` is a valid non-null pointer per COM contract.
            let riid_ref = unsafe { &*riid };

            // Supported interfaces: IUnknown, IExecuteCommand, IObjectWithSelection.
            let want_iunknown = *riid_ref == windows::core::IUnknown::IID;
            let want_exec = *riid_ref == IExecuteCommand::IID;
            let want_sel = *riid_ref == IObjectWithSelection::IID;

            if !want_iunknown && !want_exec && !want_sel {
                unsafe { *ppvobject = std::ptr::null_mut() };
                return Err(windows::core::Error::from(E_NOINTERFACE));
            }

            // Create the command object (adds to OBJECT_COUNT).
            OBJECT_COUNT.fetch_add(1, Ordering::SeqCst);
            let cmd: windows::core::ComObject<ShellCommand> =
                windows::core::ComObject::new(ShellCommand {
                    action: self.action,
                    selection: RefCell::new(None),
                });

            // Query the requested interface.
            //
            // SAFETY: `ppvobject` is a valid non-null out pointer per COM
            // contract.  The ComObject guarantees the interface is
            // implemented.  We transfer ownership by forgetting the
            // interface — the caller will Release() when done.
            if want_exec {
                let iface: IExecuteCommand = cmd.into_interface();
                unsafe {
                    *ppvobject = iface.as_raw() as *mut std::ffi::c_void;
                }
                std::mem::forget(iface);
            } else if want_sel {
                let iface: IObjectWithSelection = cmd.into_interface();
                unsafe {
                    *ppvobject = iface.as_raw() as *mut std::ffi::c_void;
                }
                std::mem::forget(iface);
            } else {
                // IUnknown — return canonical controlling IUnknown.
                // COM identity requires that QueryInterface(IID_IUnknown)
                // returns the same pointer for every interface on the object.
                // Returning an arbitrary IExecuteCommand pointer breaks this.
                let unk: IUnknown = cmd.into_interface();
                unsafe {
                    *ppvobject = unk.as_raw() as *mut std::ffi::c_void;
                }
                std::mem::forget(unk);
            }

            Ok(())
        }

        fn LockServer(&self, flock: windows::core::BOOL) -> windows::core::Result<()> {
            if flock.as_bool() {
                LOCK_COUNT.fetch_add(1, Ordering::SeqCst);
            } else {
                LOCK_COUNT.fetch_sub(1, Ordering::SeqCst);
                try_unload();
            }
            Ok(())
        }
    }

    // ------------------------------------------------------------------
    // COM server entry point
    // ------------------------------------------------------------------

    /// Run the COM LocalServer message pump.  Does not return.
    ///
    /// Registers class factories for `CLSID_COMPRESS` and `CLSID_COMPRESS_ZIP`,
    /// enters an STA message pump, and only exits when both the object count
    /// and server lock count reach zero.
    pub fn run_com_server() -> ! {
        debug_output("GeeZipX COM: server starting");

        // SAFETY: CoInitializeEx must be called once per thread before
        // COM APIs.  Passing nullptr as reserved is documented.
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if hr.is_err() {
            debug_output(&format!("GeeZipX COM: CoInitializeEx failed: {hr:?}"));
            std::process::exit(1);
        }

        // Create class factories.  Factories themselves are NOT counted in
        // OBJECT_COUNT — they are held alive by COM registration cookies
        // and released during CoRevokeClassObject + CoUninitialize.
        let factory_compress: windows::core::ComObject<ClassFactory> =
            windows::core::ComObject::new(ClassFactory {
                action: ShellActionFileAction::Compress,
            });

        let factory_compress_zip: windows::core::ComObject<ClassFactory> =
            windows::core::ComObject::new(ClassFactory {
                action: ShellActionFileAction::CompressZip,
            });

        // Register class factories with COM.
        //
        // SAFETY: CoRegisterClassObject registers an IUnknown with COM.
        // CLSCTX_LOCAL_SERVER + REGCLS_MULTIPLEUSE allow multiple concurrent
        // instances.  Cookies are used for later revocation.
        let cookie_compress = {
            let iunknown: windows::core::IUnknown = factory_compress.into_interface();
            let hr = unsafe {
                CoRegisterClassObject(
                    &CLSID_COMPRESS,
                    &iunknown,
                    CLSCTX_LOCAL_SERVER,
                    REGCLS_MULTIPLEUSE,
                )
            };
            match hr {
                Ok(cookie) => {
                    debug_output("GeeZipX COM: registered CLSID_COMPRESS");
                    cookie
                }
                Err(e) => {
                    debug_output(&format!(
                        "GeeZipX COM: CoRegisterClassObject(Compress) failed: {e}"
                    ));
                    // --- Registration failure cleanup ------------------------
                    //
                    // CoInitializeEx succeeded, so COM is initialised on this
                    // thread.  We MUST call CoUninitialize before exiting.
                    //
                    // We use `process::exit(1)` rather than returning an error,
                    // because returning would unwind the stack through the
                    // `ComObject` Drop impls (ClassFactory, etc.) which call
                    // COM release methods — those are only safe while COM is
                    // still initialised.  After CoUninitialize those Drops
                    // would be UB.  `process::exit` stops execution immediately
                    // without running any destructors, so the order is:
                    //
                    //   1. CoUninitialize (COM clean-up)
                    //   2. process::exit (no Drop code runs)
                    //
                    // This is correct for an STA LocalServer where the process
                    // has no other work to do.
                    unsafe {
                        windows::Win32::System::Com::CoUninitialize();
                    }
                    std::process::exit(1);
                }
            }
        };

        let cookie_compress_zip = {
            let iunknown: windows::core::IUnknown = factory_compress_zip.into_interface();
            let hr = unsafe {
                CoRegisterClassObject(
                    &CLSID_COMPRESS_ZIP,
                    &iunknown,
                    CLSCTX_LOCAL_SERVER,
                    REGCLS_MULTIPLEUSE,
                )
            };
            match hr {
                Ok(cookie) => {
                    debug_output("GeeZipX COM: registered CLSID_COMPRESS_ZIP");
                    cookie
                }
                Err(e) => {
                    debug_output(&format!(
                        "GeeZipX COM: CoRegisterClassObject(CompressZip) failed: {e}"
                    ));
                    // --- Second registration failure cleanup ------------------
                    //
                    // At this point the first class factory IS registered with
                    // COM, so we must revoke it before uninitialising.
                    //
                    // Cleanup order (same process::exit rationale as above —
                    // COM Drop methods must not run after CoUninitialize):
                    //
                    //   1. CoRevokeClassObject(cookie_compress) — revoke first
                    //   2. CoUninitialize — COM clean-up
                    //   3. process::exit(1) — no Drop code runs
                    unsafe {
                        let _ = CoRevokeClassObject(cookie_compress);
                        windows::Win32::System::Com::CoUninitialize();
                    }
                    std::process::exit(1);
                }
            }
        };

        debug_output("GeeZipX COM: entering message pump");

        // STA message pump.
        //
        // SAFETY: GetMessageW, TranslateMessage, DispatchMessageW are
        // standard Win32 message pump functions.  The MSG struct is
        // stack-allocated and properly zeroed via Default.
        let mut msg: MSG = MSG::default();
        loop {
            // SAFETY: MSG is initialized; all pointer fields are zeroed.
            let ret = unsafe { GetMessageW(&mut msg, None, 0, 0) };
            if ret.0 == 0 || ret.0 == -1 {
                // WM_QUIT (0) or error (-1) — exit.
                break;
            }
            // SAFETY: msg contains a valid message retrieved by GetMessageW.
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        debug_output("GeeZipX COM: message pump exited, cleaning up");

        // Clean up: revoke class objects, then uninitialize COM.
        //
        // SAFETY: These cookies were returned by CoRegisterClassObject above.
        // CoUninitialize is called on the same thread as CoInitializeEx.
        unsafe {
            let _ = CoRevokeClassObject(cookie_compress);
            let _ = CoRevokeClassObject(cookie_compress_zip);
            windows::Win32::System::Com::CoUninitialize();
        }

        debug_output("GeeZipX COM: server shutdown complete");
        std::process::exit(0);
    }
}

#[cfg(target_os = "windows")]
pub use platform::run_com_server;

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // is_embedding_arg (platform-independent)
    // ------------------------------------------------------------------

    #[test]
    fn test_is_embedding_empty() {
        let args: &[String] = &[];
        assert!(!is_embedding_arg(args));
    }

    #[test]
    fn test_is_embedding_dash() {
        let args = &["geezipx-gui.exe".into(), "-Embedding".into()];
        assert!(is_embedding_arg(args));
    }

    #[test]
    fn test_is_embedding_slash() {
        let args = &["geezipx-gui.exe".into(), "/Embedding".into()];
        assert!(is_embedding_arg(args));
    }

    #[test]
    fn test_is_embedding_lowercase() {
        let args = &["geezipx-gui.exe".into(), "-embedding".into()];
        assert!(is_embedding_arg(args));
    }

    #[test]
    fn test_is_embedding_uppercase() {
        let args = &["geezipx-gui.exe".into(), "-EMBEDDING".into()];
        assert!(is_embedding_arg(args));
    }

    #[test]
    fn test_is_embedding_mixed_case() {
        let args = &["geezipx-gui.exe".into(), "-EmBeDdInG".into()];
        assert!(is_embedding_arg(args));
    }

    #[test]
    fn test_is_embedding_not_present() {
        let args = &[
            "geezipx-gui.exe".into(),
            "--shell-action-file".into(),
            "/tmp/test.gzsa".into(),
        ];
        assert!(!is_embedding_arg(args));
    }

    #[test]
    fn test_is_embedding_similar_but_not() {
        let args = &["geezipx-gui.exe".into(), "-Embedded".into()];
        assert!(!is_embedding_arg(args));
    }

    #[test]
    fn test_is_embedding_middle_position() {
        let args = &[
            "geezipx-gui.exe".into(),
            "/some-flag".into(),
            "-Embedding".into(),
            "extra".into(),
        ];
        assert!(is_embedding_arg(args));
    }

    #[test]
    fn test_is_embedding_with_backslash_path() {
        // COM may pass the full path before -Embedding
        let args = &[
            r"C:\Program Files\GeeZipX\geezipx-gui.exe".into(),
            "-Embedding".into(),
        ];
        assert!(is_embedding_arg(args));
    }

    // ------------------------------------------------------------------
    // CLSID ↔ action mapping (Windows-only)
    // ------------------------------------------------------------------

    #[cfg(target_os = "windows")]
    #[test]
    fn test_action_for_compress_clsid() {
        let action = action_for_clsid(&CLSID_COMPRESS);
        assert_eq!(
            action,
            Some(shell_action_file::ShellActionFileAction::Compress)
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_action_for_compress_zip_clsid() {
        let action = action_for_clsid(&CLSID_COMPRESS_ZIP);
        assert_eq!(
            action,
            Some(shell_action_file::ShellActionFileAction::CompressZip)
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_action_for_unknown_clsid() {
        let unknown = windows::core::GUID::from_values(0, 0, 0, [0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(action_for_clsid(&unknown), None);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_compress_and_compress_zip_clsids_are_different() {
        assert_ne!(CLSID_COMPRESS, CLSID_COMPRESS_ZIP);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_action_constants_consistent_with_action_file() {
        let a = action_for_clsid(&CLSID_COMPRESS).unwrap();
        assert_eq!(a.as_action_str(), "compress");

        let a = action_for_clsid(&CLSID_COMPRESS_ZIP).unwrap();
        assert_eq!(a.as_action_str(), "compress-zip");
    }

    // ------------------------------------------------------------------
    // CLSID string constants (platform-independent format checks)
    // ------------------------------------------------------------------

    #[test]
    fn test_clsid_string_format() {
        // GUID string format: {XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX} = 38 chars
        assert_eq!(CLSID_COMPRESS_STR.len(), 38);
        assert_eq!(CLSID_COMPRESS_ZIP_STR.len(), 38);
        assert!(CLSID_COMPRESS_STR.starts_with('{'));
        assert!(CLSID_COMPRESS_STR.ends_with('}'));
        assert!(CLSID_COMPRESS_ZIP_STR.starts_with('{'));
        assert!(CLSID_COMPRESS_ZIP_STR.ends_with('}'));
    }

    #[test]
    fn test_clsid_strings_are_different() {
        assert_ne!(CLSID_COMPRESS_STR, CLSID_COMPRESS_ZIP_STR);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_clsid_string_matches_guid() {
        // Verify the string constants match the GUID constants by comparing
        // the formatted GUID string.
        let guid_str = format!("{CLSID_COMPRESS:?}");
        // windows::core::GUID Debug format is lowercase with hyphens.
        // Our string constant uses uppercase.  Compare case-insensitively.
        assert!(guid_str.to_uppercase().contains("C1E5F6A0"));
        assert!(guid_str.to_uppercase().contains("8F6A"));
        assert!(guid_str.to_uppercase().contains("4F9E"));
        assert!(guid_str.to_uppercase().contains("B5C2"));
        assert!(guid_str.to_uppercase().contains("1C0A9B8F7E6D"));

        let guid_str2 = format!("{CLSID_COMPRESS_ZIP:?}");
        assert!(guid_str2.to_uppercase().contains("D2F6A7B1"));
        assert!(guid_str2.to_uppercase().contains("9A7B"));
        assert!(guid_str2.to_uppercase().contains("4A0F"));
        assert!(guid_str2.to_uppercase().contains("C6D3"));
        assert!(guid_str2.to_uppercase().contains("2D1B0C9A8F7E"));
    }
}
