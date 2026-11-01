use super::*;
use std::{collections::BTreeMap, fs, path::Path, process::Command};
const DESKTOP: &str = "com.geelinx.geezipx.native.desktop";
struct Paths {
    data: PathBuf,
    config: PathBuf,
}
impl Paths {
    fn current() -> Result<Self, String> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or("HOME is unset")?;
        Ok(Self {
            data: std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .unwrap_or_else(|| home.join(".local/share")),
            config: std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .unwrap_or_else(|| home.join(".config")),
        })
    }
    fn desktop(&self) -> PathBuf {
        self.data.join("applications").join(DESKTOP)
    }
    fn backups(&self) -> PathBuf {
        self.config.join(APP_ID).join("previous-defaults")
    }
    fn menu_files(&self) -> Vec<PathBuf> {
        let mut files = vec![self.data.join("kio/servicemenus/geezipx-native.desktop")];
        for action in Action::MENU {
            for manager in ["nautilus", "nemo"] {
                files.push(
                    self.data
                        .join(format!("{manager}/scripts/GeeZipX/{}", action.argument())),
                );
            }
        }
        files
    }
}
fn write(path: &Path, content: &str) -> Result<(), String> {
    use std::io::Write;
    let parent = path.parent().ok_or("Missing parent directory")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    file.write_all(content.as_bytes())
        .map_err(|e| e.to_string())?;
    file.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}
fn remove_file(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}
fn default(mime: &str) -> Result<String, String> {
    command(Command::new("xdg-mime").args(["query", "default", mime]))
}
// Exec values use desktop-entry quoting, independently from POSIX shell quoting.
fn desktop_executable(exe: &Path) -> Result<String, String> {
    let value = exe
        .to_str()
        .ok_or("Desktop integration requires a UTF-8 executable path")?;
    if value.contains(['\n', '\r']) {
        return Err("Executable path contains a line break".into());
    }
    let mut escaped = String::from("\"");
    for c in value.chars() {
        match c {
            '\\' => escaped.push_str("\\\\\\\\"),
            '"' | '`' | '$' => {
                escaped.push_str("\\\\");
                escaped.push(c);
            }
            '%' => escaped.push_str("%%"),
            _ => escaped.push(c),
        }
    }
    escaped.push('"');
    Ok(escaped)
}
fn desktop_entry(exe: &Path, menu: bool) -> Result<String, String> {
    let exe = desktop_executable(exe)?;
    let mimes = FORMATS
        .iter()
        .filter(|f| f.available())
        .map(|f| f.mime)
        .collect::<Vec<_>>()
        .join(";");
    let mut text = format!("[Desktop Entry]\nType=Application\nName=GeeZipX\nComment=Archive manager\nExec=/usr/bin/env {exe} -- %F\nIcon=package-x-generic\nTerminal=false\nCategories=Utility;Archiving;\nMimeType={mimes};\n");
    if menu {
        text.push_str("Actions=compress;compress-zip;extract;extract-here;extract-folder;\n");
        for action in Action::MENU {
            text.push_str(&format!("\n[Desktop Action {}]\nName={}\nName[zh_CN]={}\nExec=/usr/bin/env {exe} --shell-action {} -- %F\n", action.argument(), action_label(action, Language::English), action_label(action, Language::Chinese), action.argument()));
        }
    }
    Ok(text)
}
fn backup_map(paths: &Paths) -> Result<BTreeMap<String, String>, String> {
    match fs::read_to_string(paths.backups()) {
        Ok(text) => Ok(text
            .lines()
            .filter_map(|line| line.split_once('='))
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(e.to_string()),
    }
}
fn save_backups(paths: &Paths, values: &BTreeMap<String, String>) -> Result<(), String> {
    write(
        &paths.backups(),
        &values
            .iter()
            .map(|(k, v)| format!("{k}={v}\n"))
            .collect::<String>(),
    )
}
// Remove only our own handler from the relevant sections; retain other applications.
fn without_handler(text: &str, mime: &str) -> String {
    let mut relevant = false;
    let mut result = String::new();
    for line in text.lines() {
        if line.starts_with('[') {
            relevant = matches!(line, "[Default Applications]" | "[Added Associations]");
        }
        if let Some((key, value)) = line
            .split_once('=')
            .filter(|(key, _)| relevant && key.trim() == mime)
        {
            let kept = value
                .split(';')
                .filter(|v| !v.is_empty() && *v != DESKTOP)
                .collect::<Vec<_>>();
            if !kept.is_empty() {
                result.push_str(&format!("{key}={};\n", kept.join(";")));
            }
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }
    result
}
fn restore(paths: &Paths, mime: &str, backup: Option<&String>) -> Result<(), String> {
    if default(mime)? != DESKTOP {
        return Ok(());
    }
    // xdg-mime may write a desktop-specific file. Clean only this application's values.
    let mut files = vec![
        paths.config.join("mimeapps.list"),
        paths.data.join("applications/mimeapps.list"),
    ];
    if let Some(desktops) = std::env::var_os("XDG_CURRENT_DESKTOP") {
        for desktop in desktops
            .to_string_lossy()
            .split(':')
            .filter(|d| !d.is_empty() && !d.contains('/'))
        {
            files.push(
                paths
                    .config
                    .join(format!("{}-mimeapps.list", desktop.to_lowercase())),
            );
        }
    }
    for file in files {
        if file.exists() {
            let text = fs::read_to_string(&file).map_err(|e| e.to_string())?;
            write(&file, &without_handler(&text, mime))?;
        }
    }
    if let Some(previous) = backup.filter(|p| !p.is_empty() && *p != DESKTOP) {
        command(Command::new("xdg-mime").args(["default", previous, mime]))?;
    }
    Ok(())
}
fn install_menu(
    paths: &Paths,
    exe: &Path,
    enabled: bool,
    language: Language,
) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    for file in paths.menu_files() {
        remove_file(&file)?;
    }
    if !enabled {
        return Ok(());
    }
    let exe_text = exe.to_str().ok_or("Executable path is not UTF-8")?;
    for action in Action::MENU {
        for manager in ["nautilus", "nemo"] {
            let path = paths
                .data
                .join(format!("{manager}/scripts/GeeZipX/{}", action.argument()));
            write(&path, &format!("#!/bin/sh\n# Paths supplied as argv preserve spaces, newlines and the complete selection.\n[ \"$#\" -gt 0 ] || exit 1\nexec {} --shell-action {} -- \"$@\"\n", shell_quote(exe_text), action.argument()))?;
            fs::set_permissions(path, fs::Permissions::from_mode(0o755))
                .map_err(|e| e.to_string())?;
        }
    }
    let mut service = String::from("[Desktop Entry]\nType=Service\nMimeType=all/allfiles;inode/directory;\nX-KDE-ServiceTypes=KonqPopupMenu/Plugin\nX-KDE-Submenu=GeeZipX\nActions=compress;compress-zip;extract;extract-here;extract-folder;\n");
    for action in Action::MENU {
        service.push_str(&format!("\n[Desktop Action {}]\nName={}\nIcon=package-x-generic\nExec=/usr/bin/env {} --shell-action {} -- %F\n", action.argument(), action_label(action, language), desktop_executable(exe)?, action.argument()));
    }
    let service_path = paths.data.join("kio/servicemenus/geezipx-native.desktop");
    write(&service_path, &service)?;
    // Dolphin only trusts executable ServiceMenu files from a user directory.
    fs::set_permissions(service_path, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())
}
pub fn inspect() -> Result<Snapshot, String> {
    let paths = Paths::current()?;
    let registered = paths.desktop().is_file();
    let associations = FORMATS
        .iter()
        .map(|f| {
            let is_default = if f.available() {
                default(f.mime)? == DESKTOP
            } else {
                false
            };
            Ok(Association {
                registered: registered && f.available(),
                is_default,
            })
        })
        .collect::<Result<_, String>>()?;
    Ok(Snapshot {
        associations,
        menu: paths.menu_files().iter().all(|p| p.is_file()),
    })
}
pub fn apply(selected: &[bool], menu: bool, language: Language) -> Result<(), String> {
    let paths = Paths::current()?;
    let exe = executable()?;
    write(&paths.desktop(), &desktop_entry(&exe, menu)?)?;
    // Override filename detection only for formats the user chose to associate.
    // A higher glob weight keeps ZIPX and compound extensions distinct from system aliases.
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<mime-info xmlns=\"http://www.freedesktop.org/standards/shared-mime-info\">\n");
    for (f, enabled) in FORMATS.iter().zip(selected) {
        if !*enabled || !f.available() {
            continue;
        }
        xml.push_str(&format!(
            "<mime-type type=\"{}\"><comment>{}</comment>",
            f.mime, f.name
        ));
        for ext in f.extensions {
            xml.push_str(&format!("<glob pattern=\"*.{ext}\" weight=\"80\"/>"));
        }
        xml.push_str("</mime-type>\n");
    }
    xml.push_str("</mime-info>\n");
    write(&paths.data.join("mime/packages/geezipx-native.xml"), &xml)?;
    command(Command::new("update-mime-database").arg(paths.data.join("mime")))?;
    command(Command::new("update-desktop-database").arg(paths.data.join("applications")))?;
    let mut backups = backup_map(&paths)?;
    for (f, selected) in FORMATS.iter().zip(selected) {
        if *selected && f.available() {
            let current = default(f.mime)?;
            if current != DESKTOP {
                backups.insert(f.mime.to_owned(), current);
                save_backups(&paths, &backups)?;
                command(Command::new("xdg-mime").args(["default", DESKTOP, f.mime]))?;
            }
        } else {
            restore(&paths, f.mime, backups.get(f.mime))?;
        }
    }
    install_menu(&paths, &exe, menu, language)
}
pub fn remove() -> Result<(), String> {
    let paths = Paths::current()?;
    let backups = backup_map(&paths)?;
    for f in FORMATS {
        restore(&paths, f.mime, backups.get(f.mime))?;
    }
    for file in paths.menu_files().into_iter().chain([
        paths.desktop(),
        paths.data.join("mime/packages/geezipx-native.xml"),
        paths.backups(),
    ]) {
        remove_file(&file)?;
    }
    if paths.data.join("applications").exists() {
        command(Command::new("update-desktop-database").arg(paths.data.join("applications")))?;
    }
    if paths.data.join("mime").exists() {
        command(Command::new("update-mime-database").arg(paths.data.join("mime")))?;
    }
    Ok(())
}
pub fn open_defaults() -> Result<(), String> {
    // Each desktop exposes a different defaults panel. The application picker is portable.
    command(Command::new("xdg-open").arg(Paths::current()?.data.join("applications")))?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mime_cleanup_preserves_other_handlers_sections_and_comments() {
        let text = format!("# keep\n[Default Applications]\napplication/zip={DESKTOP};other.desktop;\ntext/plain=editor.desktop;\n[Added Associations]\napplication/zip={DESKTOP};\n[Removed Associations]\napplication/zip=keep.desktop;\n");
        assert_eq!(without_handler(&text, "application/zip"), "# keep\n[Default Applications]\napplication/zip=other.desktop;\ntext/plain=editor.desktop;\n[Added Associations]\n[Removed Associations]\napplication/zip=keep.desktop;\n");
    }
    #[test]
    fn generated_launchers_quote_executable_and_keep_whole_selection() {
        let entry = desktop_entry(Path::new("/tmp/中文 path/a%$`\"\\bin"), true).unwrap();
        assert!(entry.contains("%%"));
        assert!(entry.contains(" -- %F"));
        assert!(entry.contains("--shell-action compress-zip -- %F"));
        let root = tempfile::tempdir().unwrap();
        let paths = Paths {
            data: root.path().join("data"),
            config: root.path().join("config"),
        };
        install_menu(&paths, Path::new("/tmp/a'b"), true, Language::Chinese).unwrap();
        let script =
            fs::read_to_string(paths.data.join("nautilus/scripts/GeeZipX/compress")).unwrap();
        assert!(script.contains("'/tmp/a'\\''b'"));
        assert!(script.contains("-- \"$@\""));
        install_menu(&paths, Path::new("/tmp/a'b"), false, Language::Chinese).unwrap();
        assert!(paths.menu_files().iter().all(|p| !p.exists()));
    }
}
