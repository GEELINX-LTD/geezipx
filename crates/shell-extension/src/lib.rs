//! GeeZipX Windows Shell Extension — IExplorerCommand COM DLL (PoC).
//!
//! **Experimental — not part of the official release.**

// ── Shared pure helpers (testable on all platforms) ────────────────────────

pub fn all_verbs() -> [ShellVerb; 4] {
    [
        ShellVerb::Extract,
        ShellVerb::ExtractHere,
        ShellVerb::CompressZip,
        ShellVerb::Compress,
    ]
}

pub const ARCHIVE_EXTS: &[&str] = &[
    ".zip", ".zipx", ".tar", ".gz", ".bz2", ".br", ".lz4", ".zst", ".xz", ".lzma", ".lz", ".7z",
    ".rar", ".cab", ".asar", ".deb", ".cpio", ".iso", ".udf", ".lzh", ".lha", ".zpaq", ".wim",
    ".isz",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellVerb {
    Extract,
    ExtractHere,
    CompressZip,
    Compress,
}

impl ShellVerb {
    pub fn title(self) -> &'static str {
        match self {
            Self::Extract => "Extract to...",
            Self::ExtractHere => "Extract here",
            Self::CompressZip => "Compress as ZIP",
            Self::Compress => "Compress as...",
        }
    }
    pub fn cli_flag(self) -> &'static str {
        match self {
            Self::Extract => "/extract",
            Self::ExtractHere => "/extract-here",
            Self::CompressZip => "/compress-zip",
            Self::Compress => "/compress",
        }
    }
    pub fn key_suffix(self) -> &'static str {
        match self {
            Self::Extract => "Extract",
            Self::ExtractHere => "ExtractHere",
            Self::CompressZip => "CompressZip",
            Self::Compress => "Compress",
        }
    }
    pub fn reg_key_for_ext(self, ext: &str) -> String {
        format!(
            "Software\\Classes\\SystemFileAssociations\\{ext}\\shell\\GeeZipX.{suffix}",
            suffix = self.key_suffix()
        )
    }
    pub fn reg_key_for_any_file(self) -> String {
        format!(
            "Software\\Classes\\*\\shell\\GeeZipX.{suffix}",
            suffix = self.key_suffix()
        )
    }
    pub fn reg_key_for_dir(self) -> String {
        format!(
            "Software\\Classes\\Directory\\shell\\GeeZipX.{suffix}",
            suffix = self.key_suffix()
        )
    }
}

/// Windows argv quoting (CommandLineToArgvW / MS CRT rules).
pub fn argv_quote(arg: &str) -> String {
    if arg.is_empty() {
        return "\"\"".to_string();
    }
    let needs = arg.bytes().any(|b| b == b' ' || b == b'\t' || b == b'"');
    if !needs {
        return arg.to_string();
    }
    let mut r = String::with_capacity(arg.len() + 2);
    r.push('"');
    let b = arg.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let mut n = 0;
        while i < b.len() && b[i] == b'\\' {
            n += 1;
            i += 1;
        }
        if i == b.len() {
            // Trailing backslashes: double them to avoid escaping the closing quote.
            for _ in 0..n * 2 {
                r.push('\\');
            }
            break;
        }
        if b[i] == b'"' {
            // Backslashes before a quote: double them, then backslash-escape the quote.
            for _ in 0..n * 2 {
                r.push('\\');
            }
            r.push('\\');
            r.push('"');
        } else {
            for _ in 0..n {
                r.push('\\');
            }
            r.push(b[i] as char);
        }
        i += 1;
    }
    r.push('"');
    r
}

// ── Non-Windows stub ───────────────────────────────────────────────────────

#[cfg(not(target_os = "windows"))]
mod platform {
    #[no_mangle]
    pub extern "C" fn DllCanUnloadNow() -> i32 {
        1 // S_FALSE — cannot unload (stub)
    }
    #[no_mangle]
    pub extern "C" fn DllGetClassObject(
        _rclsid: *const std::ffi::c_void,
        _riid: *const std::ffi::c_void,
        _ppv: *mut *mut std::ffi::c_void,
    ) -> i32 {
        0x80004001u32 as i32 // E_NOTIMPL
    }
}

// ── Windows — full COM implementation ──────────────────────────────────────

#[cfg(target_os = "windows")]
mod platform {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStringExt;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicI32, AtomicPtr, Ordering};

    use paste::paste;
    use windows_core::Result as HResult;

    use super::*;
    use windows::Win32::Foundation::{
        CloseHandle, CLASS_E_CLASSNOTAVAILABLE, CLASS_E_NOAGGREGATION, E_NOTIMPL, E_UNEXPECTED,
        HINSTANCE, HMODULE,
    };
    use windows::Win32::System::Com::{CoTaskMemFree, IBindCtx, IClassFactory, IClassFactory_Impl};
    use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
    use windows::Win32::System::Threading::{
        CreateProcessW, CREATE_NO_WINDOW, PROCESS_INFORMATION, STARTUPINFOW,
    };
    use windows::Win32::UI::Shell::{
        IEnumExplorerCommand, IExplorerCommand, IExplorerCommand_Impl, IShellItemArray, SHStrDupW,
        ECF_DEFAULT, ECS_ENABLED, ECS_HIDDEN, SIGDN_FILESYSPATH,
    };
    use windows_core::{implement, Error, Interface, Ref, BOOL, GUID, HRESULT, PCWSTR, PWSTR};
    use windows_registry::CURRENT_USER;
    use windows_result::Error as WrError;

    // ── CLSIDs ────────────────────────────────────────────────────────────

    pub(crate) const CLSID_EXTRACT: GUID = GUID::from_values(
        0x8F3A1C60,
        0x4D2E,
        0x4B5A,
        [0xA1, 0xF8, 0x7E, 0x6D, 0x5C, 0x4B, 0x3A, 0x21],
    );
    pub(crate) const CLSID_EXTRACT_HERE: GUID = GUID::from_values(
        0x9E2B1D70,
        0x5C3F,
        0x4C6B,
        [0xB2, 0xE9, 0x8F, 0x7E, 0x6D, 0x5C, 0x4B, 0x32],
    );
    pub(crate) const CLSID_COMPRESS_ZIP: GUID = GUID::from_values(
        0xA0C3E4D0,
        0x6D4E,
        0x4D7C,
        [0xC3, 0xF0, 0x9A, 0x8F, 0x7E, 0x6D, 0x5C, 0x43],
    );
    pub(crate) const CLSID_COMPRESS: GUID = GUID::from_values(
        0xB1D4F5E0,
        0x7E5F,
        0x4E8D,
        [0xD4, 0xA1, 0x0B, 0x9A, 0x8F, 0x7E, 0x6D, 0x54],
    );

    impl ShellVerb {
        pub(crate) fn from_clsid(c: &GUID) -> Option<Self> {
            if *c == CLSID_EXTRACT {
                Some(Self::Extract)
            } else if *c == CLSID_EXTRACT_HERE {
                Some(Self::ExtractHere)
            } else if *c == CLSID_COMPRESS_ZIP {
                Some(Self::CompressZip)
            } else if *c == CLSID_COMPRESS {
                Some(Self::Compress)
            } else {
                None
            }
        }
        pub(crate) fn clsid(self) -> GUID {
            match self {
                Self::Extract => CLSID_EXTRACT,
                Self::ExtractHere => CLSID_EXTRACT_HERE,
                Self::CompressZip => CLSID_COMPRESS_ZIP,
                Self::Compress => CLSID_COMPRESS,
            }
        }
        /// Check whether this verb is enabled via the current-user registry keys
        /// that the GUI settings page manages.  On any registry error we treat
        /// the verb as *hidden* (not enabled) so a broken registry never
        /// surfaces a stale menu item.
        fn is_enabled(self) -> bool {
            match self {
                Self::Extract | Self::ExtractHere => {
                    for ext in ARCHIVE_EXTS {
                        match static_verb_exists(&self.reg_key_for_ext(ext)) {
                            Ok(true) => return true,
                            Ok(false) => continue,
                            Err(_) => return false, // error → hide
                        }
                    }
                    false
                }
                Self::CompressZip | Self::Compress => {
                    matches!(
                        (
                            static_verb_exists(&self.reg_key_for_any_file()),
                            static_verb_exists(&self.reg_key_for_dir())
                        ),
                        (Ok(true), _) | (_, Ok(true))
                    )
                }
            }
        }
    }

    // ── Global state ──────────────────────────────────────────────────────

    static DLL_HINSTANCE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
    static OBJECT_COUNT: AtomicI32 = AtomicI32::new(0);
    static LOCK_COUNT: AtomicI32 = AtomicI32::new(0);

    fn dll_dir() -> Option<PathBuf> {
        let p = DLL_HINSTANCE.load(Ordering::Acquire);
        if p.is_null() {
            return None;
        }
        let hmodule = HMODULE(p);
        let mut buf = vec![0u16; 32768];
        let len = unsafe { GetModuleFileNameW(Some(hmodule), buf.as_mut_slice()) };
        if len == 0 {
            return None;
        }
        if (len as usize) < buf.len() {
            let os = std::ffi::OsString::from_wide(&buf[..len as usize]);
            return Path::new(&os).parent().map(|q| q.to_path_buf());
        }
        let mut buf2 = vec![0u16; 65536];
        let len2 = unsafe { GetModuleFileNameW(Some(hmodule), buf2.as_mut_slice()) };
        if len2 == 0 || (len2 as usize) >= buf2.len() {
            return None;
        }
        let os = std::ffi::OsString::from_wide(&buf2[..len2 as usize]);
        Path::new(&os).parent().map(|q| q.to_path_buf())
    }

    fn gui_exe() -> Option<PathBuf> {
        dll_dir().map(|d| d.join("geezipx-gui.exe"))
    }

    // ── RAII helpers ──────────────────────────────────────────────────────

    /// Wraps a CoTaskMem-allocated `PWSTR` and frees it on drop.
    struct CoMem(PWSTR);
    impl CoMem {
        unsafe fn into_string(self) -> Result<String, Self> {
            unsafe { self.0.to_string() }.map_err(|_| self)
        }
    }
    impl Drop for CoMem {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { CoTaskMemFree(Some(self.0.as_ptr() as _)) };
            }
        }
    }

    /// Guard that increments `OBJECT_COUNT` on creation and decrements on
    /// drop.  Every COM object (command or class factory) owns one guard so
    /// `DllCanUnloadNow` can reliably track live objects.
    struct ObjGuard;
    impl ObjGuard {
        fn new() -> Self {
            OBJECT_COUNT.fetch_add(1, Ordering::SeqCst);
            Self
        }
    }
    impl Drop for ObjGuard {
        fn drop(&mut self) {
            OBJECT_COUNT.fetch_sub(1, Ordering::SeqCst);
        }
    }

    // ── Registry helpers ──────────────────────────────────────────────────

    const HR_FILE_NOT_FOUND: i32 = 0x80070002u32 as i32;
    fn is_not_found(e: &WrError) -> bool {
        e.code().0 == HR_FILE_NOT_FOUND
    }
    fn static_verb_exists(path: &str) -> Result<bool, String> {
        match CURRENT_USER.open(path) {
            Ok(_) => Ok(true),
            Err(e) if is_not_found(&e) => Ok(false),
            Err(e) => Err(format!("registry: {e}")),
        }
    }

    // ── String allocation ─────────────────────────────────────────────────

    fn alloc_shell_str(s: &str) -> HResult<PWSTR> {
        let w: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe { SHStrDupW(PCWSTR::from_raw(w.as_ptr())) }
    }

    // ── IShellItemArray → paths ───────────────────────────────────────────

    fn resolve_shell_items(psia: &IShellItemArray) -> HResult<Vec<String>> {
        let n = unsafe { psia.GetCount()? };
        let mut v = Vec::with_capacity(n as usize);
        for i in 0..n {
            let item = unsafe { psia.GetItemAt(i)? };
            let pw = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH)? };
            let g = CoMem(pw);
            match unsafe { g.into_string() } {
                Ok(s) if !s.is_empty() => v.push(s),
                _ => continue,
            }
        }
        if v.is_empty() {
            Err(Error::from_hresult(HRESULT(0x80070057u32 as i32)))
        } else {
            Ok(v)
        }
    }

    // ── IExplorerCommand implementations (4 verbs) ────────────────────────

    /// Helper to build the command line and invoke the GUI.
    fn invoke_verb(verb: ShellVerb, psia: &IShellItemArray) -> HResult<()> {
        let paths = resolve_shell_items(psia)?;
        let exe = gui_exe().ok_or_else(|| Error::from_hresult(HRESULT(0x80070002u32 as i32)))?;
        let exe_s = exe.to_string_lossy();
        let mut cmd = String::new();
        cmd.push('"');
        cmd.push_str(&exe_s);
        cmd.push('"');
        cmd.push(' ');
        cmd.push_str(verb.cli_flag());
        for p in &paths {
            cmd.push(' ');
            cmd.push_str(&super::argv_quote(p));
        }
        let exe_wide: Vec<u16> = exe_s.encode_utf16().chain(std::iter::once(0)).collect();
        let mut cmd_wide: Vec<u16> = cmd.encode_utf16().chain(std::iter::once(0)).collect();
        let si = STARTUPINFOW {
            cb: std::mem::size_of::<STARTUPINFOW>() as u32,
            ..Default::default()
        };
        let mut pi = PROCESS_INFORMATION::default();
        unsafe {
            CreateProcessW(
                PCWSTR::from_raw(exe_wide.as_ptr()),
                Some(PWSTR::from_raw(cmd_wide.as_mut_ptr())),
                None,
                None,
                false,
                CREATE_NO_WINDOW,
                None,
                PCWSTR::null(),
                &si,
                &mut pi,
            )
        }?;
        unsafe {
            let _ = CloseHandle(pi.hProcess);
            let _ = CloseHandle(pi.hThread);
        }
        Ok(())
    }

    /// Determine the state flags for `GetState`.  Registry errors cause the
    /// verb to be hidden — never surface a stale item.
    fn verb_state(verb: ShellVerb) -> u32 {
        if verb.is_enabled() {
            ECS_ENABLED.0 as u32
        } else {
            ECS_HIDDEN.0 as u32
        }
    }

    /// Generate one `#[implement(IExplorerCommand)]` struct with all trait
    /// methods wired to `invoke_verb` / `alloc_shell_str` / `verb_state`.
    macro_rules! impl_iexplorer_command {
        ($struct_name:ident, $verb:expr) => {
            paste! {
                #[implement(IExplorerCommand)]
                struct $struct_name {
                    _guard: ObjGuard,
                }

                impl IExplorerCommand_Impl for [<$struct_name _Impl>] {
                    fn GetTitle(
                        &self,
                        _: Ref<'_, IShellItemArray>,
                    ) -> HResult<PWSTR> {
                        alloc_shell_str($verb.title())
                    }
                    fn GetIcon(
                        &self,
                        _: Ref<'_, IShellItemArray>,
                    ) -> HResult<PWSTR> {
                        let exe = gui_exe()
                            .ok_or_else(|| Error::from_hresult(HRESULT(0x80070002u32 as i32)))?;
                        alloc_shell_str(&format!("{},0", exe.display()))
                    }
                    fn GetToolTip(
                        &self,
                        _: Ref<'_, IShellItemArray>,
                    ) -> HResult<PWSTR> {
                        alloc_shell_str(&format!(
                            "GeeZipX — {} ({})",
                            $verb.title(),
                            $verb.cli_flag()
                        ))
                    }
                    fn GetCanonicalName(&self) -> HResult<GUID> {
                        Ok($verb.clsid())
                    }
                    fn GetState(
                        &self,
                        _: Ref<'_, IShellItemArray>,
                        _: BOOL,
                    ) -> HResult<u32> {
                        Ok(verb_state($verb))
                    }
                    fn GetFlags(&self) -> HResult<u32> {
                        Ok(ECF_DEFAULT.0 as u32)
                    }
                    fn EnumSubCommands(
                        &self,
                    ) -> HResult<IEnumExplorerCommand> {
                        Err(Error::from_hresult(E_NOTIMPL))
                    }
                    fn Invoke(
                        &self,
                        psia: Ref<'_, IShellItemArray>,
                        _: Ref<'_, IBindCtx>,
                    ) -> HResult<()> {
                        if psia.is_null() {
                            return Err(Error::from_hresult(HRESULT(0x80070057u32 as i32)));
                        }
                        invoke_verb($verb, psia.as_ref().unwrap())
                    }
                }
            }
        };
    }

    impl_iexplorer_command!(ExtractCmd, ShellVerb::Extract);
    impl_iexplorer_command!(ExtractHereCmd, ShellVerb::ExtractHere);
    impl_iexplorer_command!(CompressZipCmd, ShellVerb::CompressZip);
    impl_iexplorer_command!(CompressCmd, ShellVerb::Compress);

    // ── IClassFactory ─────────────────────────────────────────────────────

    #[implement(IClassFactory)]
    struct ClassFactory {
        verb: ShellVerb,
        _guard: ObjGuard,
    }

    impl IClassFactory_Impl for ClassFactory_Impl {
        fn CreateInstance(
            &self,
            outer: Ref<'_, windows_core::IUnknown>,
            riid: *const GUID,
            ppv: *mut *mut c_void,
        ) -> HResult<()> {
            if !outer.is_null() {
                return Err(Error::from_hresult(CLASS_E_NOAGGREGATION));
            }
            if riid.is_null() || ppv.is_null() {
                return Err(Error::from_hresult(HRESULT(0x80070057u32 as i32)));
            }
            // ObjGuard inside the command struct handles OBJECT_COUNT tracking.
            let cmd: IExplorerCommand = match self.verb {
                ShellVerb::Extract => ExtractCmd {
                    _guard: ObjGuard::new(),
                }
                .into(),
                ShellVerb::ExtractHere => ExtractHereCmd {
                    _guard: ObjGuard::new(),
                }
                .into(),
                ShellVerb::CompressZip => CompressZipCmd {
                    _guard: ObjGuard::new(),
                }
                .into(),
                ShellVerb::Compress => CompressCmd {
                    _guard: ObjGuard::new(),
                }
                .into(),
            };
            unsafe { cmd.query(riid, ppv).ok() }
        }
        fn LockServer(&self, f: BOOL) -> HResult<()> {
            if f.as_bool() {
                LOCK_COUNT.fetch_add(1, Ordering::SeqCst);
            } else {
                // Saturating decrement — never go negative.
                let _ = LOCK_COUNT.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |v| {
                    if v > 0 {
                        Some(v - 1)
                    } else {
                        None
                    }
                });
            }
            Ok(())
        }
    }

    // ── DLL exports ───────────────────────────────────────────────────────

    #[no_mangle]
    extern "system" fn DllMain(dll: HINSTANCE, reason: u32, _reserved: *mut c_void) -> BOOL {
        if reason == 1 {
            // DLL_PROCESS_ATTACH — store instance handle for later path resolution.
            DLL_HINSTANCE.store(dll.0, Ordering::Release);
        }
        BOOL::from(true)
    }

    #[no_mangle]
    extern "system" fn DllCanUnloadNow() -> HRESULT {
        let o = OBJECT_COUNT.load(Ordering::SeqCst);
        let l = LOCK_COUNT.load(Ordering::SeqCst);
        if o == 0 && l == 0 {
            HRESULT(0) // S_OK
        } else {
            HRESULT(1) // S_FALSE
        }
    }

    #[no_mangle]
    extern "system" fn DllGetClassObject(
        r: *const GUID,
        i: *const GUID,
        p: *mut *mut c_void,
    ) -> HRESULT {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if r.is_null() || i.is_null() || p.is_null() {
                return Err(Error::from_hresult(HRESULT(0x80070057u32 as i32)));
            }
            let verb = ShellVerb::from_clsid(unsafe { &*r })
                .ok_or_else(|| Error::from_hresult(CLASS_E_CLASSNOTAVAILABLE))?;
            let f: IClassFactory = ClassFactory {
                verb,
                _guard: ObjGuard::new(),
            }
            .into();
            unsafe { f.query(i, p).ok() }
        }));
        match result {
            Ok(Ok(())) => HRESULT(0),
            Ok(Err(e)) => e.into(),
            Err(_) => E_UNEXPECTED,
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Verb enumeration ──────────────────────────────────────────────────

    #[test]
    fn verbs_count() {
        assert_eq!(all_verbs().len(), 4);
    }

    #[test]
    fn verb_titles() {
        assert_eq!(ShellVerb::Extract.title(), "Extract to...");
        assert_eq!(ShellVerb::ExtractHere.title(), "Extract here");
        assert_eq!(ShellVerb::CompressZip.title(), "Compress as ZIP");
        assert_eq!(ShellVerb::Compress.title(), "Compress as...");
    }

    #[test]
    fn verb_flags() {
        assert_eq!(ShellVerb::Extract.cli_flag(), "/extract");
        assert_eq!(ShellVerb::ExtractHere.cli_flag(), "/extract-here");
        assert_eq!(ShellVerb::CompressZip.cli_flag(), "/compress-zip");
        assert_eq!(ShellVerb::Compress.cli_flag(), "/compress");
    }

    #[test]
    fn verb_suffixes() {
        assert_eq!(ShellVerb::Extract.key_suffix(), "Extract");
        assert_eq!(ShellVerb::ExtractHere.key_suffix(), "ExtractHere");
        assert_eq!(ShellVerb::CompressZip.key_suffix(), "CompressZip");
        assert_eq!(ShellVerb::Compress.key_suffix(), "Compress");
    }

    // ── Registry key paths ────────────────────────────────────────────────

    #[test]
    fn reg_key_ext() {
        assert_eq!(
            ShellVerb::Extract.reg_key_for_ext(".zip"),
            "Software\\Classes\\SystemFileAssociations\\.zip\\shell\\GeeZipX.Extract"
        );
    }

    #[test]
    fn reg_key_file() {
        assert_eq!(
            ShellVerb::CompressZip.reg_key_for_any_file(),
            "Software\\Classes\\*\\shell\\GeeZipX.CompressZip"
        );
    }

    #[test]
    fn reg_key_dir() {
        assert_eq!(
            ShellVerb::Compress.reg_key_for_dir(),
            "Software\\Classes\\Directory\\shell\\GeeZipX.Compress"
        );
    }

    #[test]
    fn hkcu_rel() {
        for v in all_verbs() {
            for e in ARCHIVE_EXTS {
                let p = v.reg_key_for_ext(e);
                assert!(!p.to_lowercase().starts_with("hkcu"));
                assert!(p.starts_with("Software\\Classes\\"));
            }
            assert!(!v.reg_key_for_any_file().to_lowercase().starts_with("hkcu"));
            assert!(!v.reg_key_for_dir().to_lowercase().starts_with("hkcu"));
        }
    }

    #[test]
    fn paths_distinct() {
        assert_ne!(
            ShellVerb::Extract.reg_key_for_ext(".zip"),
            ShellVerb::ExtractHere.reg_key_for_ext(".zip")
        );
        assert_ne!(
            ShellVerb::CompressZip.reg_key_for_any_file(),
            ShellVerb::Compress.reg_key_for_any_file()
        );
    }

    // ── Archive extensions ────────────────────────────────────────────────

    #[test]
    fn ext_count() {
        assert_eq!(ARCHIVE_EXTS.len(), 24);
        for e in ARCHIVE_EXTS {
            assert!(e.starts_with('.'), "{e:?}");
        }
    }

    // ── argv_quote ────────────────────────────────────────────────────────

    #[test]
    fn argv_plain() {
        assert_eq!(argv_quote("hello"), "hello");
    }
    #[test]
    fn argv_empty() {
        assert_eq!(argv_quote(""), "\"\"");
    }
    #[test]
    fn argv_sp() {
        assert_eq!(argv_quote("a b"), "\"a b\"");
    }
    #[test]
    fn argv_qt() {
        assert_eq!(argv_quote("\"x\""), "\"\\\"x\\\"\"");
    }
    #[test]
    fn argv_bsq() {
        assert_eq!(argv_quote("a\\\"b"), "\"a\\\\\\\"b\"");
    }
    #[test]
    fn argv_bs() {
        assert_eq!(argv_quote("a\\\\b"), "a\\\\b");
    }
    #[test]
    fn argv_dr() {
        assert_eq!(argv_quote("C:\\"), "C:\\");
    }
    #[test]
    fn argv_tb() {
        assert_eq!(argv_quote("a\tb"), "\"a\tb\"");
    }
    #[test]
    fn argv_trailing_bs_quoted() {
        // When quoting is required (space), trailing backslashes must be doubled.
        assert_eq!(argv_quote("C:\\a b\\"), "\"C:\\a b\\\\\"");
    }
    #[test]
    fn argv_trailing_bs_only() {
        assert_eq!(argv_quote("foo bar\\\\"), "\"foo bar\\\\\\\\\"");
    }
    #[test]
    fn argv_no_special_chars() {
        assert_eq!(argv_quote("C:\\no_special"), "C:\\no_special");
    }

    // ── Manifest consistency (tested against the template) ────────────────

    #[test]
    fn manifest_contains_four_clsids() {
        let m = include_str!("../package/AppxManifest.xml.in");
        for id in &[
            "{8F3A1C60-4D2E-4B5A-A1F8-7E6D5C4B3A21}",
            "{9E2B1D70-5C3F-4C6B-B2E9-8F7E6D5C4B32}",
            "{A0C3E4D0-6D4E-4D7C-C3F0-9A8F7E6D5C43}",
            "{B1D4F5E0-7E5F-4E8D-D4A1-0B9A8F7E6D54}",
        ] {
            assert!(m.contains(id), "missing CLSID {id}");
        }
    }

    #[test]
    fn manifest_no_dll_path_token() {
        let m = include_str!("../package/AppxManifest.xml.in");
        assert!(
            !m.contains("@DLL_PATH@"),
            "manifest must use relative DLL path, not @DLL_PATH@"
        );
    }

    #[test]
    fn manifest_dll_relative_path() {
        let m = include_str!("../package/AppxManifest.xml.in");
        assert!(
            m.contains("Path=\"geezipx_shell_extension.dll\""),
            "manifest must reference DLL via relative Path"
        );
    }

    #[test]
    fn manifest_publisher() {
        let m = include_str!("../package/AppxManifest.xml.in");
        // Publisher attribute uses @PUBLISHER_DN@ token (replaced by build script).
        assert!(m.contains("@PUBLISHER_DN@"), "missing @PUBLISHER_DN@");
        // PublisherDisplayName is a human-readable label.
        assert!(
            m.contains("GeeZipX Development"),
            "missing PublisherDisplayName"
        );
    }

    #[test]
    fn manifest_archive_ext_token() {
        let m = include_str!("../package/AppxManifest.xml.in");
        assert!(
            m.contains("@ARCHIVE_EXT_MENUS@"),
            "manifest must have @ARCHIVE_EXT_MENUS@ placeholder"
        );
    }

    #[test]
    fn manifest_version_token() {
        let m = include_str!("../package/AppxManifest.xml.in");
        assert!(m.contains("@VERSION@"), "missing @VERSION@");
        assert!(m.contains("@ARCH@"), "missing @ARCH@");
        assert!(m.contains("@PUBLISHER_DN@"), "missing @PUBLISHER_DN@");
    }

    #[test]
    fn manifest_min_version_22000() {
        let m = include_str!("../package/AppxManifest.xml.in");
        assert!(m.contains("MinVersion=\"10.0.22000.0\""));
    }

    #[test]
    fn manifest_allow_external_content() {
        let m = include_str!("../package/AppxManifest.xml.in");
        assert!(m.contains("allowExternalContent"));
    }

    #[test]
    fn manifest_surrogate_server_valid() {
        let m = include_str!("../package/AppxManifest.xml.in");
        assert!(m.contains("<com:SurrogateServer"));
        assert!(m.contains("DisplayName=\"GeeZipX Shell Extension\""));
    }

    // ── Platform-specific tests ───────────────────────────────────────────

    #[cfg(target_os = "windows")]
    #[test]
    fn clsids_unique() {
        use platform::*;
        let ids = [
            CLSID_EXTRACT,
            CLSID_EXTRACT_HERE,
            CLSID_COMPRESS_ZIP,
            CLSID_COMPRESS,
        ];
        for i in 0..ids.len() {
            for j in i + 1..ids.len() {
                assert_ne!(ids[i], ids[j]);
            }
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn clsid_roundtrip() {
        use platform::*;
        for (c, v) in [
            (CLSID_EXTRACT, ShellVerb::Extract),
            (CLSID_EXTRACT_HERE, ShellVerb::ExtractHere),
            (CLSID_COMPRESS_ZIP, ShellVerb::CompressZip),
            (CLSID_COMPRESS, ShellVerb::Compress),
        ] {
            assert_eq!(ShellVerb::from_clsid(&c), Some(v));
            assert_eq!(v.clsid(), c);
        }
        assert_eq!(
            ShellVerb::from_clsid(&GUID::from_values(0, 0, 0, [0, 0, 0, 0, 0, 0, 0, 0])),
            None
        );
    }

    #[test]
    fn panic_ok() {
        let r = std::panic::catch_unwind(|| 42);
        assert!(r.is_ok());
        assert_eq!(r.unwrap(), 42);
    }
}
