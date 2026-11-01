use super::*;
use std::{
    collections::BTreeMap,
    ffi::{c_char, c_void, CString},
    fs,
    path::Path,
    process::Command,
};
type CF = *const c_void;
const UTF8: u32 = 0x08000100;
const VIEWER: u32 = 2;
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFStringCreateWithCString(allocator: CF, value: *const c_char, encoding: u32) -> CF;
    fn CFStringGetCString(value: CF, buffer: *mut c_char, length: isize, encoding: u32) -> u8;
    fn CFRelease(value: CF);
    fn CFURLCreateFromFileSystemRepresentation(
        allocator: CF,
        path: *const u8,
        len: isize,
        directory: u8,
    ) -> CF;
}
#[link(name = "CoreServices", kind = "framework")]
unsafe extern "C" {
    fn UTTypeCreatePreferredIdentifierForTag(tag_class: CF, tag: CF, conforms: CF) -> CF;
    fn LSCopyDefaultRoleHandlerForContentType(content_type: CF, roles: u32) -> CF;
    fn LSSetDefaultRoleHandlerForContentType(content_type: CF, roles: u32, handler: CF) -> i32;
    fn LSRegisterURL(url: CF, update: u8) -> i32;
}
struct Owned(CF);
impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CFRelease(self.0) };
        }
    }
}
fn cf(text: &str) -> Result<Owned, String> {
    let text = CString::new(text).map_err(|e| e.to_string())?;
    let value = unsafe { CFStringCreateWithCString(std::ptr::null(), text.as_ptr(), UTF8) };
    if value.is_null() {
        Err("Could not create a CoreFoundation string".into())
    } else {
        Ok(Owned(value))
    }
}
fn content_type(ext: &str) -> Result<Owned, String> {
    let class = cf("public.filename-extension")?;
    let ext = cf(ext)?;
    let value = unsafe { UTTypeCreatePreferredIdentifierForTag(class.0, ext.0, std::ptr::null()) };
    if value.is_null() {
        Err("Could not resolve archive type".into())
    } else {
        Ok(Owned(value))
    }
}
fn default(ext: &str) -> Result<String, String> {
    let uti = content_type(ext)?;
    let value = Owned(unsafe { LSCopyDefaultRoleHandlerForContentType(uti.0, VIEWER) });
    if value.0.is_null() {
        return Ok(String::new());
    }
    let mut buffer = vec![0u8; 4096];
    if unsafe {
        CFStringGetCString(
            value.0,
            buffer.as_mut_ptr().cast(),
            buffer.len() as isize,
            UTF8,
        )
    } == 0
    {
        return Err("Default handler identifier is too long".into());
    }
    let len = buffer.iter().position(|b| *b == 0).unwrap_or(buffer.len());
    String::from_utf8(buffer[..len].to_vec()).map_err(|e| e.to_string())
}
fn set_default(ext: &str, handler: &str) -> Result<(), String> {
    let uti = content_type(ext)?;
    let handler = cf(handler)?;
    let status = unsafe { LSSetDefaultRoleHandlerForContentType(uti.0, VIEWER, handler.0) };
    if status == 0 {
        Ok(())
    } else {
        Err(format!("LaunchServices: {status}"))
    }
}
fn home() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("HOME is unset".into())
}
fn bundle() -> Result<PathBuf, String> {
    Ok(home()?.join("Applications/GeeZipX Native Launcher.app"))
}
fn backups() -> Result<PathBuf, String> {
    Ok(home()?.join("Library/Application Support/GeeZipXNative/previous-defaults"))
}
fn service(action: Action) -> Result<PathBuf, String> {
    Ok(home()?.join(format!(
        "Library/Services/GeeZipX Native {}.workflow",
        action.argument()
    )))
}
fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn applescript(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}
fn write(path: &Path, text: &str) -> Result<(), String> {
    fs::create_dir_all(path.parent().ok_or("Missing parent directory")?)
        .map_err(|e| e.to_string())?;
    fs::write(path, text).map_err(|e| e.to_string())
}
fn previous() -> Result<BTreeMap<String, String>, String> {
    match fs::read_to_string(backups()?) {
        Ok(text) => Ok(text
            .lines()
            .filter_map(|l| l.split_once('='))
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(e.to_string()),
    }
}
fn save_previous(values: &BTreeMap<String, String>) -> Result<(), String> {
    write(
        &backups()?,
        &values
            .iter()
            .map(|(k, v)| format!("{k}={v}\n"))
            .collect::<String>(),
    )
}
fn remove_dir(path: &Path) -> Result<(), String> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}
fn launcher(exe: &Path) -> Result<(), String> {
    use std::os::unix::ffi::OsStrExt;
    let exe = exe
        .to_str()
        .ok_or("Launcher requires a UTF-8 executable path")?;
    let prefix = applescript(&format!("{} -- ", shell_quote(exe)));
    let script = format!("on run\n do shell script {prefix} & \" >/dev/null 2>&1 &\"\nend run\non open selectedItems\n set cmd to {prefix}\n repeat with selectedItem in selectedItems\n set cmd to cmd & quoted form of (POSIX path of selectedItem) & \" \"\n end repeat\n do shell script cmd & \" >/dev/null 2>&1 &\"\nend open\n");
    let temporary = tempfile::tempdir().map_err(|e| e.to_string())?;
    let source = temporary.path().join("launcher.applescript");
    write(&source, &script)?;
    let compiled = temporary.path().join("GeeZipX.app");
    command(
        Command::new("osacompile")
            .arg("-o")
            .arg(&compiled)
            .arg(&source),
    )?;
    let extensions = FORMATS
        .iter()
        .filter(|f| f.available())
        .flat_map(|f| f.extensions)
        .map(|e| format!("<string>{e}</string>"))
        .collect::<String>();
    let info = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\"><plist version=\"1.0\"><dict><key>CFBundleIdentifier</key><string>{APP_ID}</string><key>CFBundleName</key><string>GeeZipX Native</string><key>CFBundleExecutable</key><string>applet</string><key>CFBundlePackageType</key><string>APPL</string><key>CFBundleVersion</key><string>1</string><key>NSAppleScriptEnabled</key><true/><key>CFBundleDocumentTypes</key><array><dict><key>CFBundleTypeName</key><string>GeeZipX Archive</string><key>CFBundleTypeRole</key><string>Viewer</string><key>LSHandlerRank</key><string>Alternate</string><key>CFBundleTypeExtensions</key><array>{extensions}</array></dict></array></dict></plist>");
    write(&compiled.join("Contents/Info.plist"), &info)?;
    // Re-sign after changing the generated applet's plist.
    command(
        Command::new("codesign")
            .args(["--force", "--sign", "-"])
            .arg(&compiled),
    )?;
    let target = bundle()?;
    fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
    // ditto preserves the generated executable, resources and signature.
    remove_dir(&target)?;
    command(Command::new("ditto").arg(&compiled).arg(&target))?;
    let bytes = target.as_os_str().as_bytes();
    let url = Owned(unsafe {
        CFURLCreateFromFileSystemRepresentation(
            std::ptr::null(),
            bytes.as_ptr(),
            bytes.len() as isize,
            1,
        )
    });
    if url.0.is_null() {
        return Err("Could not create launcher URL".into());
    }
    let status = unsafe { LSRegisterURL(url.0, 1) };
    if status == 0 {
        Ok(())
    } else {
        Err(format!("Register launcher: {status}"))
    }
}
fn install_services(exe: &Path, enabled: bool, language: Language) -> Result<(), String> {
    for action in Action::MENU {
        let path = service(action)?;
        remove_dir(&path)?;
        if !enabled {
            continue;
        }
        let exe = exe
            .to_str()
            .ok_or("Services require a UTF-8 executable path")?;
        let shell = xml(&format!(
            "{} --shell-action {} -- \"$@\" >/dev/null 2>&1 &",
            shell_quote(exe),
            action.argument()
        ));
        let name = xml(action_label(action, language));
        let info = format!("<?xml version=\"1.0\"?><plist version=\"1.0\"><dict><key>NSServices</key><array><dict><key>NSMenuItem</key><dict><key>default</key><string>GeeZipX — {name}</string></dict><key>NSMessage</key><string>runWorkflowAsService</string><key>NSRequiredContext</key><dict><key>NSApplicationIdentifier</key><string>com.apple.finder</string></dict><key>NSSendFileTypes</key><array><string>public.item</string></array></dict></array></dict></plist>");
        let document = format!("<?xml version=\"1.0\"?><plist version=\"1.0\"><dict><key>AMApplicationBuild</key><string>523</string><key>AMApplicationVersion</key><string>2.10</string><key>AMDocumentVersion</key><string>2</string><key>actions</key><array><dict><key>action</key><dict><key>ActionBundlePath</key><string>/System/Library/Automator/Run Shell Script.action</string><key>ActionName</key><string>Run Shell Script</string><key>ActionParameters</key><dict><key>COMMAND_STRING</key><string>{shell}</string><key>CheckedForUserDefaultShell</key><true/><key>inputMethod</key><integer>1</integer><key>shell</key><string>/bin/sh</string></dict><key>AMAccepts</key><dict><key>Types</key><array><string>com.apple.cocoa.path</string></array><key>Container</key><string>List</string></dict><key>AMProvides</key><dict><key>Types</key><array><string>com.apple.cocoa.string</string></array><key>Container</key><string>List</string></dict><key>BundleIdentifier</key><string>com.apple.RunShellScript</string><key>CFBundleVersion</key><string>2.0</string><key>Class Name</key><string>RunShellScriptAction</string><key>InputUUID</key><string>3DAB9A9D-85ED-45AD-A035-FAB824FA3A91</string><key>OutputUUID</key><string>429FDBE3-C952-4A40-B197-C0ED03EC93DA</string><key>UUID</key><string>D851F02B-C2D3-45D2-AE55-DDBF17723356</string></dict><key>isViewVisible</key><integer>0</integer></dict></array><key>connectors</key><dict/><key>workflowMetaData</key><dict><key>serviceInputTypeIdentifier</key><string>com.apple.Automator.fileSystemObject</string><key>serviceProcessesInput</key><integer>0</integer><key>serviceApplicationBundleID</key><string>com.apple.finder</string><key>serviceApplicationPath</key><string>/System/Library/CoreServices/Finder.app</string><key>serviceOutputTypeIdentifier</key><string>com.apple.Automator.nothing</string><key>workflowTypeIdentifier</key><string>com.apple.Automator.servicesMenu</string></dict></dict></plist>");
        write(&path.join("Contents/Info.plist"), &info)?;
        write(&path.join("Contents/document.wflow"), &document)?;
    }
    // Ask LaunchServices to rescan Services without restarting Finder.
    command(Command::new("/System/Library/CoreServices/pbs").arg("-update"))?;
    Ok(())
}
pub fn inspect() -> Result<Snapshot, String> {
    let registered = bundle()?.join("Contents/MacOS/applet").is_file();
    let associations = FORMATS
        .iter()
        .map(|f| {
            let mut is_default = f.available();
            for ext in f.extensions {
                is_default &= default(ext)?.eq_ignore_ascii_case(APP_ID);
            }
            Ok(Association {
                registered: registered && f.available(),
                is_default,
            })
        })
        .collect::<Result<_, String>>()?;
    let menu = Action::MENU
        .iter()
        .map(|a| service(*a))
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .all(|p| p.join("Contents/document.wflow").is_file());
    Ok(Snapshot { associations, menu })
}
pub fn apply(selected: &[bool], enabled: bool, language: Language) -> Result<(), String> {
    let exe = executable()?;
    launcher(&exe)?;
    let mut backups = previous()?;
    for (f, selected) in FORMATS.iter().zip(selected) {
        for ext in f.extensions {
            let current = default(ext)?;
            if *selected && f.available() {
                if !current.eq_ignore_ascii_case(APP_ID) {
                    backups.insert((*ext).to_owned(), current);
                    save_previous(&backups)?;
                    set_default(ext, APP_ID)?;
                }
            } else if current.eq_ignore_ascii_case(APP_ID) {
                set_default(
                    ext,
                    backups
                        .get(*ext)
                        .filter(|v| !v.is_empty())
                        .map_or("com.apple.archiveutility", String::as_str),
                )?;
            }
        }
    }
    install_services(&exe, enabled, language)
}
pub fn remove() -> Result<(), String> {
    let previous = previous()?;
    for f in FORMATS {
        for ext in f.extensions {
            if default(ext)?.eq_ignore_ascii_case(APP_ID) {
                set_default(
                    ext,
                    previous
                        .get(*ext)
                        .filter(|v| !v.is_empty())
                        .map_or("com.apple.archiveutility", String::as_str),
                )?;
            }
        }
    }
    install_services(&executable()?, false, Language::English)?;
    remove_dir(&bundle()?)?;
    match fs::remove_file(backups()?) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}
pub fn open_defaults() -> Result<(), String> {
    open::that(bundle()?).map_err(|e| e.to_string())
}
