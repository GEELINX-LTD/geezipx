use std::{env, fs, path::PathBuf, process::Command};

fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
fn main() {
    println!("cargo:rerun-if-env-changed=GEEZIPX_SHELL_PUBLISHER");
    let publisher =
        env::var("GEEZIPX_SHELL_PUBLISHER").unwrap_or_else(|_| "CN=GeeZipX Native".into());
    assert!(
        !publisher.contains(['\r', '\n']),
        "Invalid package publisher"
    );
    println!("cargo:rustc-env=GEEZIPX_SHELL_PUBLISHER={publisher}");
    let target = env::var("TARGET").unwrap();
    if !target.contains("windows") {
        return;
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let manifest = out.join("shell.manifest");
    fs::write(&manifest, format!(r#"<?xml version="1.0" encoding="utf-8"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity version="1.0.0.0" name="GeeZipX.Native.Shell" type="win32"/>
  <msix xmlns="urn:schemas-microsoft-com:msix.v1" publisher="{}" packageName="GeeZipX.Native.Shell" applicationId="GeeZipXShell"/>
</assembly>
"#, xml(&publisher))).unwrap();
    if target.ends_with("msvc") {
        println!("cargo:rustc-cdylib-link-arg=/MANIFEST:EMBED,ID=2");
        println!(
            "cargo:rustc-cdylib-link-arg=/MANIFESTINPUT:{}",
            manifest.display()
        );
    } else {
        let resource = out.join("shell.rc");
        let object = out.join("shell-resource.o");
        fs::write(
            &resource,
            format!(
                "2 24 \"{}\"\n",
                manifest.to_string_lossy().replace('\\', "\\\\")
            ),
        )
        .unwrap();
        let tool = env::var("WINDRES").unwrap_or_else(|_| "x86_64-w64-mingw32-windres".into());
        let status = Command::new(tool)
            .arg(&resource)
            .arg(&object)
            .status()
            .expect("Windows resource compiler required");
        assert!(status.success(), "Could not embed Shell package identity");
        println!("cargo:rustc-cdylib-link-arg={}", object.display());
    }
}
