//! IExplorerCommand is hosted by the packaged COM surrogate, never by the archive engine.
use crate::{Action, SETTINGS_KEY};
use std::{
    cell::Cell,
    ffi::c_void,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use windows::{
    core::{implement, ComObject, IUnknown, Interface, Ref, GUID, HRESULT, PWSTR},
    Win32::{
        Foundation::{
            CLASS_E_CLASSNOTAVAILABLE, CLASS_E_NOAGGREGATION, E_FAIL, E_INVALIDARG, E_NOTIMPL,
            E_OUTOFMEMORY, E_POINTER, E_UNEXPECTED, HMODULE, S_FALSE, S_OK,
        },
        System::{
            Com::{CoTaskMemAlloc, CoTaskMemFree, IBindCtx, IClassFactory, IClassFactory_Impl},
            LibraryLoader::{
                GetModuleFileNameW, GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
                GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            },
            SystemServices::{SFGAO_FILESYSTEM, SFGAO_FOLDER, SFGAO_STREAM},
        },
        UI::Shell::{
            IEnumExplorerCommand, IEnumExplorerCommand_Impl, IExplorerCommand,
            IExplorerCommand_Impl, IShellItemArray, ECF_DEFAULT, ECF_HASSUBCOMMANDS, ECS_ENABLED,
            ECS_HIDDEN, SIATTRIBFLAGS_AND, SIGDN_FILESYSPATH,
        },
    },
};
const CLSID: GUID = GUID::from_u128(0x5ad97452_37f8_4738_a102_5b472df60c20);
static OBJECTS: AtomicUsize = AtomicUsize::new(0);
static LOCKS: AtomicUsize = AtomicUsize::new(0);
struct Lifetime;
impl Lifetime {
    fn new() -> Self {
        OBJECTS.fetch_add(1, Ordering::SeqCst);
        Self
    }
}
impl Drop for Lifetime {
    fn drop(&mut self) {
        OBJECTS.fetch_sub(1, Ordering::SeqCst);
    }
}
struct Settings {
    exe: PathBuf,
    titles: Vec<String>,
    extensions: Vec<String>,
    enabled: bool,
}
impl Settings {
    fn load() -> windows::core::Result<Arc<Self>> {
        let key = windows_registry::CURRENT_USER.open(SETTINGS_KEY).ok();
        let read = |name: &str| key.as_ref().and_then(|key| key.get_string(name).ok());
        let mut module = HMODULE::default();
        let mut path = vec![0u16; 32_768];
        // FROM_ADDRESS treats this pointer as a code address, not a string. The unchanged
        // reference count is safe because we are executing inside this loaded DLL.
        let length = unsafe {
            GetModuleHandleExW(
                GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS
                    | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                windows::core::PCWSTR(DllGetClassObject as *const () as *const u16),
                &mut module,
            )?;
            GetModuleFileNameW(Some(module), &mut path)
        } as usize;
        if length == 0 || length >= path.len() {
            return Err(E_FAIL.into());
        }
        use std::os::windows::ffi::OsStringExt;
        let dll = PathBuf::from(std::ffi::OsString::from_wide(&path[..length]));
        let exe = dll.parent().ok_or(E_FAIL)?.join("geezipx-egui.exe");
        let titles = Action::MENU
            .iter()
            .map(|a| {
                read(&format!("Title.{}", a.argument())).unwrap_or_else(|| {
                    match a {
                        Action::Compress => "Compress to…",
                        Action::CompressZip => "Compress as ZIP",
                        Action::Extract => "Extract to…",
                        Action::ExtractHere => "Extract here",
                        Action::ExtractFolder => "Extract to named folder",
                        Action::Open => unreachable!(),
                    }
                    .into()
                })
            })
            .collect();
        let extensions = read("ArchiveExtensions")
            .unwrap_or_default()
            .split(';')
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        Ok(Arc::new(Self {
            exe,
            titles,
            extensions,
            enabled: read("Enabled").as_deref() == Some("1"),
        }))
    }
}
fn task_string(text: &str) -> windows::core::Result<PWSTR> {
    let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    let pointer = unsafe { CoTaskMemAlloc(wide.len() * 2) }.cast::<u16>();
    if pointer.is_null() {
        return Err(E_OUTOFMEMORY.into());
    }
    // Shell owns and frees successful output strings with CoTaskMemFree.
    unsafe {
        std::ptr::copy_nonoverlapping(wide.as_ptr(), pointer, wide.len());
    }
    Ok(PWSTR(pointer))
}
fn item_path(items: &IShellItemArray, index: u32) -> windows::core::Result<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    let name = unsafe { items.GetItemAt(index)?.GetDisplayName(SIGDN_FILESYSPATH)? };
    if name.is_null() {
        return Err(E_FAIL.into());
    }
    let path = unsafe { std::ffi::OsString::from_wide(name.as_wide()) };
    unsafe {
        CoTaskMemFree(Some(name.as_ptr().cast()));
    }
    if path.is_empty() {
        return Err(E_INVALIDARG.into());
    }
    Ok(path.into())
}
#[implement(IExplorerCommand)]
struct Command {
    action: Option<Action>,
    settings: Arc<Settings>,
    _lifetime: Lifetime,
}
impl Command {
    fn create(action: Option<Action>, settings: Arc<Settings>) -> IExplorerCommand {
        ComObject::new(Self {
            action,
            settings,
            _lifetime: Lifetime::new(),
        })
        .into_interface()
    }
    fn state(&self, items: &IShellItemArray) -> windows::core::Result<u32> {
        if !self.settings.enabled {
            return Ok(ECS_HIDDEN.0 as u32);
        }
        let count = unsafe { items.GetCount()? };
        if count == 0 || count > 100_000 {
            return Ok(ECS_HIDDEN.0 as u32);
        }
        let attributes = unsafe {
            items.GetAttributes(
                SIATTRIBFLAGS_AND,
                SFGAO_FILESYSTEM | SFGAO_FOLDER | SFGAO_STREAM,
            )?
        };
        if attributes.0 & SFGAO_FILESYSTEM.0 == 0 {
            return Ok(ECS_HIDDEN.0 as u32);
        }
        if self.action.is_some_and(Action::is_extraction) {
            // Explorer's ZIP namespace reports FOLDER and STREAM for a regular ZIP file.
            let directory =
                attributes.0 & SFGAO_FOLDER.0 != 0 && attributes.0 & SFGAO_STREAM.0 == 0;
            if count != 1 || directory {
                return Ok(ECS_HIDDEN.0 as u32);
            }
            // Filename checks only: do not read or probe archives on Explorer's UI thread.
            let path = item_path(items, 0)?;
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_ascii_lowercase();
            if !self
                .settings
                .extensions
                .iter()
                .any(|ext| name.ends_with(&format!(".{ext}")))
            {
                return Ok(ECS_HIDDEN.0 as u32);
            }
        }
        Ok(ECS_ENABLED.0 as u32)
    }
}
impl IExplorerCommand_Impl for Command_Impl {
    fn GetTitle(&self, _: Ref<'_, IShellItemArray>) -> windows::core::Result<PWSTR> {
        let title = self
            .action
            .and_then(|a| Action::MENU.iter().position(|v| *v == a))
            .map(|index| self.settings.titles[index].as_str())
            .unwrap_or("GeeZipX");
        task_string(title)
    }
    fn GetIcon(&self, _: Ref<'_, IShellItemArray>) -> windows::core::Result<PWSTR> {
        task_string(&format!("{},0", self.settings.exe.display()))
    }
    fn GetToolTip(&self, _: Ref<'_, IShellItemArray>) -> windows::core::Result<PWSTR> {
        Err(E_NOTIMPL.into())
    }
    fn GetCanonicalName(&self) -> windows::core::Result<GUID> {
        let offset = self
            .action
            .and_then(|a| Action::MENU.iter().position(|v| *v == a))
            .map_or(0, |index| index + 1);
        Ok(GUID::from_u128(CLSID.to_u128() + offset as u128))
    }
    fn GetState(
        &self,
        items: Ref<'_, IShellItemArray>,
        _: windows::core::BOOL,
    ) -> windows::core::Result<u32> {
        match items.as_ref() {
            Some(items) => self.state(items),
            None => Ok(ECS_HIDDEN.0 as u32),
        }
    }
    fn Invoke(
        &self,
        items: Ref<'_, IShellItemArray>,
        _: Ref<'_, IBindCtx>,
    ) -> windows::core::Result<()> {
        let action = self.action.ok_or(E_NOTIMPL)?;
        let items = items.as_ref().ok_or(E_INVALIDARG)?;
        if self.state(items)? != ECS_ENABLED.0 as u32 {
            return Err(E_INVALIDARG.into());
        }
        let count = unsafe { items.GetCount()? };
        let paths = (0..count)
            .map(|index| item_path(items, index))
            .collect::<windows::core::Result<Vec<_>>>()?;
        crate::dispatch(&self.settings.exe, action, &paths)
            .map_err(|error| windows::core::Error::new(E_FAIL, error))
    }
    fn GetFlags(&self) -> windows::core::Result<u32> {
        Ok(if self.action.is_none() {
            ECF_HASSUBCOMMANDS.0
        } else {
            ECF_DEFAULT.0
        } as u32)
    }
    fn EnumSubCommands(&self) -> windows::core::Result<IEnumExplorerCommand> {
        if self.action.is_some() {
            return Err(E_NOTIMPL.into());
        }
        Ok(Commands::create(self.settings.clone(), 0))
    }
}
#[implement(IEnumExplorerCommand)]
struct Commands {
    settings: Arc<Settings>,
    index: Cell<usize>,
    _lifetime: Lifetime,
}
impl Commands {
    fn create(settings: Arc<Settings>, index: usize) -> IEnumExplorerCommand {
        ComObject::new(Self {
            settings,
            index: Cell::new(index),
            _lifetime: Lifetime::new(),
        })
        .into_interface()
    }
}
impl IEnumExplorerCommand_Impl for Commands_Impl {
    fn Next(
        &self,
        count: u32,
        output: *mut Option<IExplorerCommand>,
        fetched: *mut u32,
    ) -> HRESULT {
        if !fetched.is_null() {
            unsafe {
                *fetched = 0;
            }
        }
        if count == 0 {
            return S_OK;
        }
        if output.is_null() || (count != 1 && fetched.is_null()) {
            return E_POINTER;
        }
        let available = (Action::MENU.len() - self.index.get()).min(count as usize);
        for index in 0..count as usize {
            let command = (index < available).then(|| {
                Command::create(
                    Some(Action::MENU[self.index.get() + index]),
                    self.settings.clone(),
                )
            });
            // COM provided space for count interface pointers. Write, rather than drop, the old values.
            unsafe {
                output.add(index).write(command);
            }
        }
        self.index.set(self.index.get() + available);
        if !fetched.is_null() {
            unsafe {
                *fetched = available as u32;
            }
        }
        if available == count as usize {
            S_OK
        } else {
            S_FALSE
        }
    }
    fn Skip(&self, count: u32) -> windows::core::Result<()> {
        let available = Action::MENU.len() - self.index.get();
        self.index.set(
            self.index
                .get()
                .saturating_add(count as usize)
                .min(Action::MENU.len()),
        );
        // The projection uses Result for Skip; preserve COM's positive S_FALSE status.
        if count as usize > available {
            Err(windows::core::Error::from_hresult(S_FALSE))
        } else {
            Ok(())
        }
    }
    fn Reset(&self) -> windows::core::Result<()> {
        self.index.set(0);
        Ok(())
    }
    fn Clone(&self) -> windows::core::Result<IEnumExplorerCommand> {
        Ok(Commands::create(self.settings.clone(), self.index.get()))
    }
}
#[implement(IClassFactory)]
struct Factory {
    _lifetime: Lifetime,
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
        unsafe {
            *output = std::ptr::null_mut();
        }
        if iid.is_null() {
            return Err(E_POINTER.into());
        }
        if !outer.is_null() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        let command = Command::create(None, Settings::load()?);
        unsafe { command.query(iid, output).ok() }
    }
    fn LockServer(&self, lock: windows::core::BOOL) -> windows::core::Result<()> {
        if lock.as_bool() {
            LOCKS.fetch_add(1, Ordering::SeqCst);
        } else {
            LOCKS
                .try_update(Ordering::SeqCst, Ordering::SeqCst, |count| {
                    count.checked_sub(1)
                })
                .map_err(|_| windows::core::Error::from(E_UNEXPECTED))?;
        }
        Ok(())
    }
}
/// COM entry point. The packaged surrogate supplies valid pointers according to this ABI.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllGetClassObject(
    clsid: *const GUID,
    iid: *const GUID,
    output: *mut *mut c_void,
) -> HRESULT {
    if output.is_null() {
        return E_POINTER;
    }
    unsafe {
        *output = std::ptr::null_mut();
    }
    if clsid.is_null() || iid.is_null() {
        return E_POINTER;
    }
    if unsafe { *clsid } != CLSID {
        return CLASS_E_CLASSNOTAVAILABLE;
    }
    let factory: IClassFactory = ComObject::new(Factory {
        _lifetime: Lifetime::new(),
    })
    .into_interface();
    unsafe { factory.query(iid, output) }
}
#[unsafe(no_mangle)]
pub extern "system" fn DllCanUnloadNow() -> HRESULT {
    if OBJECTS.load(Ordering::SeqCst) == 0 && LOCKS.load(Ordering::SeqCst) == 0 {
        S_OK
    } else {
        S_FALSE
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn enumeration_clone_reset_titles_and_unload_follow_com_contract() {
        assert_eq!(DllCanUnloadNow(), S_OK);
        let settings = Arc::new(Settings {
            exe: PathBuf::from(r"C:\中文 path\geezipx-egui.exe"),
            titles: Action::MENU.iter().map(|a| a.argument().into()).collect(),
            extensions: vec!["zip".into()],
            enabled: true,
        });
        let root = Command::create(None, settings);
        assert_eq!(DllCanUnloadNow(), S_FALSE);
        unsafe {
            assert_eq!(root.GetFlags().unwrap(), ECF_HASSUBCOMMANDS.0 as u32);
            let iterator = root.EnumSubCommands().unwrap();
            let mut commands = vec![None; 3];
            let mut fetched = 0;
            assert_eq!(iterator.Next(&mut commands, Some(&mut fetched)), S_OK);
            assert_eq!(fetched, 3);
            let title = commands[0].as_ref().unwrap().GetTitle(None).unwrap();
            assert_eq!(title.to_string().unwrap(), "compress");
            CoTaskMemFree(Some(title.as_ptr().cast()));
            let clone = iterator.Clone().unwrap();
            iterator.Reset().unwrap();
            let mut rest = vec![None; 3];
            assert_eq!(clone.Next(&mut rest, Some(&mut fetched)), S_FALSE);
            assert_eq!(fetched, 2);
            assert!(rest[2].is_none());
            clone.Reset().unwrap();
            assert_eq!((clone.vtable().Skip)(clone.as_raw(), 6), S_FALSE);
            assert_eq!(
                (clone.vtable().Next)(
                    clone.as_raw(),
                    0,
                    std::ptr::null_mut(),
                    std::ptr::null_mut()
                ),
                S_OK
            );
            drop(rest);
            drop(clone);
            drop(commands);
            drop(iterator);
        }
        drop(root);
        assert_eq!(DllCanUnloadNow(), S_OK);
        let mut output = std::ptr::dangling_mut::<c_void>();
        assert_eq!(
            unsafe { DllGetClassObject(&GUID::zeroed(), &IClassFactory::IID, &mut output) },
            CLASS_E_CLASSNOTAVAILABLE
        );
        assert!(output.is_null());
    }
}
