fn main() {
    // Windows: our own application manifest (Tauri's default + "asInvoker",
    // i.e. never request administrator rights, + Windows 10/11 compatibility).
    // Version info (company, description, copyright) comes from tauri.conf.json.
    println!("cargo:rerun-if-changed=windows-app-manifest.xml");
    let windows = tauri_build::WindowsAttributes::new()
        .app_manifest(include_str!("windows-app-manifest.xml"));
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("failed to run tauri-build");
}
