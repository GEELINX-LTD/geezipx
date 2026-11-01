//! Windows 11 identity registration, outside Explorer and before any graphics initialization.
use geezipx_shell::{PACKAGE_NAME, PUBLISHER};
use std::path::Path;
use windows::{
    core::HSTRING,
    Foundation::Uri,
    Management::Deployment::{AddPackageOptions, DeploymentResult, PackageManager},
    Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
};

fn checked(result: DeploymentResult) -> windows::core::Result<()> {
    let code = result.ExtendedErrorCode()?;
    if code.is_err() {
        return Err(windows::core::Error::new(
            code,
            result.ErrorText()?.to_string(),
        ));
    }
    Ok(())
}
pub(super) fn supported() -> Result<bool, String> {
    let build = windows_registry::LOCAL_MACHINE
        .open(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")
        .and_then(|key| key.get_string("CurrentBuildNumber"))
        .map_err(|e| e.to_string())?;
    Ok(build.parse::<u32>().map_err(|e| e.to_string())? >= 22_000)
}
fn manager<T>(work: impl FnOnce(PackageManager) -> windows::core::Result<T>) -> Result<T, String> {
    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe { RoUninitialize() };
        }
    }
    unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.map_err(|e| e.to_string())?;
    let _apartment = Apartment;
    work(PackageManager::new().map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}
fn packages(
    manager: &PackageManager,
) -> windows::core::Result<Vec<windows::ApplicationModel::Package>> {
    Ok(manager
        .FindPackagesByUserSecurityIdNamePublisher(
            &HSTRING::new(),
            &HSTRING::from(PACKAGE_NAME),
            &HSTRING::from(PUBLISHER),
        )?
        .into_iter()
        .collect())
}
pub(super) fn registered(exe: &Path) -> Result<bool, String> {
    if !supported()? {
        return Ok(false);
    }
    let root = exe.parent().ok_or("Missing application directory")?;
    manager(|manager| {
        for package in packages(&manager)? {
            let location = package.UserExternalPath()?;
            let version = package.Id()?.Version()?;
            let current = env!("CARGO_PKG_VERSION")
                .split(['-', '+'])
                .next()
                .unwrap_or_default();
            if same_file::is_same_file(Path::new(&location.to_string()), root).unwrap_or(false)
                && format!("{}.{}.{}", version.Major, version.Minor, version.Build) == current
                && version.Revision == 0
            {
                return Ok(true);
            }
        }
        Ok(false)
    })
}
pub(super) fn register(exe: &Path) -> Result<(), String> {
    if !supported()? {
        return Ok(());
    }
    let root = exe.parent().ok_or("Missing application directory")?;
    let package = root.join("GeeZipXShell.msix");
    if !package.is_file() || !root.join("geezipx_shell.dll").is_file() {
        return Err("Windows 11 menus require GeeZipXShell.msix and geezipx_shell.dll beside the application. Install the Windows package or run scripts/build-egui-windows-shell.ps1.".into());
    }
    if registered(exe)? {
        return Ok(());
    }
    manager(|manager| {
        // External locations cannot be changed by reinstalling an equal-version identity.
        for package in packages(&manager)? {
            checked(manager.RemovePackageAsync(&package.Id()?.FullName()?)?.get()?)?;
        }
        let options = AddPackageOptions::new()?;
        options.SetExternalLocationUri(&Uri::CreateUri(&HSTRING::from(root.as_os_str()))?)?;
        let result = manager.AddPackageByUriAsync(&Uri::CreateUri(&HSTRING::from(package.as_os_str()))?, &options)?.get()?;
        checked(result)
    }).map_err(|e| format!("Could not register the Windows 11 menu identity: {e}. The MSIX signing certificate must be trusted on this computer."))
}
pub(super) fn remove() -> Result<(), String> {
    if !supported()? {
        return Ok(());
    }
    manager(|manager| {
        for package in packages(&manager)? {
            let result = manager
                .RemovePackageAsync(&package.Id()?.FullName()?)?
                .get()?;
            checked(result)?;
        }
        Ok(())
    })
}
