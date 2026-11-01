use super::*;
use windows_registry::CURRENT_USER;
const ROOT: &str = r"Software\Classes";
const CAPABILITIES: &str = r"Software\GeeZipXNative\Capabilities";
const MENU: &str = r"Software\Classes\AllFilesystemObjects\shell\GeeZipXNative";
pub(super) const CLSIDS: [&str; 2] = [
    "{5AD97452-37F8-4738-A102-5B472DF60C11}",
    "{5AD97452-37F8-4738-A102-5B472DF60C12}",
];
fn optional<T>(result: windows_registry::Result<T>) -> Result<Option<T>, String> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(e) if matches!(e.code().0 as u32, 0x80070002 | 0x80070003) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}
fn string(path: &str, name: &str) -> Result<Option<String>, String> {
    match optional(CURRENT_USER.open(path))? {
        Some(key) => optional(key.get_string(name)),
        None => Ok(None),
    }
}
fn set(path: &str, name: &str, value: &str) -> Result<(), String> {
    CURRENT_USER
        .create(path)
        .and_then(|key| key.set_string(name, value))
        .map_err(|e| e.to_string())
}
fn remove_tree(path: &str) -> Result<(), String> {
    optional(CURRENT_USER.remove_tree(path)).map(|_| ())
}
fn remove_value(path: &str, name: &str) -> Result<(), String> {
    if let Some(key) = optional(CURRENT_USER.options().read().write().open(path))? {
        optional(key.remove_value(name))?;
    }
    Ok(())
}
fn progid(ext: &str) -> String {
    format!("GeeZipX.Native.{ext}")
}
fn open_command(exe: &std::path::Path) -> Result<String, String> {
    let exe = exe
        .to_str()
        .ok_or("The executable path is not valid UTF-16 text")?;
    if exe.contains(['"', '\r', '\n']) {
        return Err("Invalid executable path".into());
    }
    Ok(format!("\"{exe}\" -- \"%1\""))
}
fn extension(ext: &str, enabled: bool, exe: &std::path::Path) -> Result<(), String> {
    let id = progid(ext);
    let open_with = format!(r"{ROOT}\.{ext}\OpenWithProgids");
    let associations = format!(r"{CAPABILITIES}\FileAssociations");
    if enabled {
        set(&format!(r"{ROOT}\{id}"), "", "GeeZipX Native Archive")?;
        set(
            &format!(r"{ROOT}\{id}\DefaultIcon"),
            "",
            &format!("\"{}\",0", exe.display()),
        )?;
        set(
            &format!(r"{ROOT}\{id}\shell\open\command"),
            "",
            &open_command(exe)?,
        )?;
        set(&open_with, &id, "")?;
        set(&associations, &format!(".{ext}"), &id)?;
    } else {
        remove_value(&open_with, &id)?;
        remove_value(&associations, &format!(".{ext}"))?;
        remove_tree(&format!(r"{ROOT}\{id}"))?;
    }
    Ok(())
}
fn notify() {
    use ::windows::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};
    // SAFETY: ASSOCCHANGED with IDLIST accepts null item pointers.
    unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None) };
}
fn menu(enabled: bool, language: Language, exe: &std::path::Path) -> Result<(), String> {
    let settings = geezipx_shell::SETTINGS_KEY;
    if enabled {
        let extensions = FORMATS
            .iter()
            .filter(|f| f.available())
            .flat_map(|f| f.extensions.iter().copied())
            .collect::<Vec<_>>()
            .join(";");
        set(settings, "ArchiveExtensions", &extensions)?;
        for action in Action::MENU {
            set(
                settings,
                &format!("Title.{}", action.argument()),
                action_label(action, language),
            )?;
        }
        set(settings, "Enabled", "1")?;
        if let Err(error) = super::modern::register(exe) {
            // A failed identity registration must not leave a newly activated modern command.
            if !super::modern::registered(exe).unwrap_or(false) {
                remove_value(settings, "Enabled")?;
            }
            return Err(error);
        }
    } else {
        super::modern::remove()?;
        remove_tree(settings)?;
    }
    remove_tree(MENU)?;
    for clsid in CLSIDS {
        remove_tree(&format!(r"{ROOT}\CLSID\{clsid}"))?;
    }
    if !enabled {
        return Ok(());
    }
    set(MENU, "MUIVerb", "GeeZipX")?;
    set(MENU, "Icon", &format!("\"{}\",0", exe.display()))?;
    set(
        MENU,
        "ExtendedSubCommandsKey",
        MENU.strip_prefix(r"Software\Classes\").unwrap(),
    )?;
    set(MENU, "MultiSelectModel", "Player")?;
    let applies = FORMATS
        .iter()
        .filter(|f| f.available())
        .flat_map(|f| f.extensions)
        .filter(|ext| !ext.contains('.'))
        .map(|ext| format!("System.FileExtension:=\".{ext}\""))
        .collect::<Vec<_>>()
        .join(" OR ");
    for action in Action::MENU {
        let path = format!(r"{MENU}\shell\{}", action.argument());
        set(&path, "MUIVerb", action_label(action, language))?;
        let command = format!(r"{path}\command");
        match action {
            Action::Compress | Action::CompressZip => {
                set(&path, "MultiSelectModel", "Player")?;
                let clsid = CLSIDS[usize::from(action == Action::CompressZip)];
                set(&command, "DelegateExecute", clsid)?;
                let server = format!(r"{ROOT}\CLSID\{clsid}\LocalServer32");
                // COM appends -Embedding when activating LocalServer32.
                set(&server, "", &format!("\"{}\"", exe.display()))?;
                set(
                    &server,
                    "ServerExecutable",
                    exe.to_str().ok_or("Invalid executable path")?,
                )?;
            }
            _ => {
                set(&path, "MultiSelectModel", "Single")?;
                set(&path, "AppliesTo", &applies)?;
                set(
                    &command,
                    "",
                    &format!(
                        "\"{}\" --shell-action {} -- \"%1\"",
                        exe.display(),
                        action.argument()
                    ),
                )?;
            }
        }
    }
    Ok(())
}
pub fn inspect() -> Result<Snapshot, String> {
    let associations = FORMATS.iter().map(|format| {
        let mut registered = format.available();
        let mut is_default = format.available();
        for ext in format.extensions {
            let id = progid(ext);
            registered &= string(&format!(r"{ROOT}\.{ext}\OpenWithProgids"), &id)?.is_some();
            let chosen = string(&format!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.{ext}\UserChoice"), "ProgId")?
                .or(string(&format!(r"{ROOT}\.{ext}"), "")?);
            // Explorer uses the final suffix for compound extensions.
            if !ext.contains('.') { is_default &= chosen.as_deref() == Some(&id); }
        }
        Ok(Association { registered, is_default })
    }).collect::<Result<_, String>>()?;
    Ok(Snapshot {
        associations,
        menu: string(MENU, "MUIVerb")?.is_some(),
    })
}
pub fn apply(selected: &[bool], enabled: bool, language: Language) -> Result<(), String> {
    let exe = executable()?;
    open_command(&exe)?;
    set(CAPABILITIES, "ApplicationName", "GeeZipX Native")?;
    set(
        CAPABILITIES,
        "ApplicationDescription",
        "GeeZipX native archive manager",
    )?;
    set(
        r"Software\RegisteredApplications",
        "GeeZipX Native",
        CAPABILITIES,
    )?;
    for (format, selected) in FORMATS.iter().zip(selected) {
        for ext in format.extensions {
            extension(ext, *selected && format.available(), &exe)?;
        }
    }
    let result = menu(enabled, language, &exe);
    notify();
    result
}
pub fn remove() -> Result<(), String> {
    let exe = executable()?;
    for format in FORMATS {
        for ext in format.extensions {
            extension(ext, false, &exe)?;
        }
    }
    menu(false, Language::English, &exe)?;
    remove_value(r"Software\RegisteredApplications", "GeeZipX Native")?;
    remove_tree(r"Software\GeeZipXNative\Capabilities")?;
    notify();
    Ok(())
}
pub fn open_defaults() -> Result<(), String> {
    open::that("ms-settings:defaultapps?registeredAppUser=GeeZipX%20Native")
        .map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn removal_opens_shared_values_with_write_access() {
        let path = format!(r"Software\GeeZipXNative\Tests\{}", std::process::id());
        set(&path, "ours", "value").unwrap();
        set(&path, "other", "preserve").unwrap();
        remove_value(&path, "ours").unwrap();
        assert!(string(&path, "ours").unwrap().is_none());
        assert_eq!(string(&path, "other").unwrap().as_deref(), Some("preserve"));
        remove_value(&path, "ours").unwrap();
        remove_tree(&path).unwrap();
    }
}
