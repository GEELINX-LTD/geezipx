fn main() {
    println!("cargo:rerun-if-changed=windows/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("windows/icon.ico")
            .set_language(0x0409)
            .set("ProductName", "GeeZipX")
            .set("FileDescription", "GeeZipX")
            .set("OriginalFilename", "geezipx-egui.exe")
            .set("LegalCopyright", "GeeZipX contributors")
            .compile()
            .expect("Could not embed GeeZipX Windows icon and version resources");
    }
}
