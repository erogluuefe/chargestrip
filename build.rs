// Embedded into the Windows executable (compiled with rc.exe from the Windows SDK):
// - Application manifest (src/win/app.manifest): without Windows 10 compatibility a layered child
//   window cannot be placed inside the taskbar; also per-monitor DPI and modern common controls.
// - Icon (assets/icon.ico) and version info (from Cargo.toml): shown in the Start menu, the Apps
//   list and the file properties.
fn main() {
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        let mut res = winresource::WindowsResource::new();
        res.set_manifest_file("src/win/app.manifest")
            .set_icon("assets/icon.ico")
            .set("FileDescription", "Chargestrip")
            .set("ProductName", "Chargestrip");
        res.compile().expect("failed to embed Windows resources");
    }
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/win/app.manifest");
    println!("cargo:rerun-if-changed=assets/icon.ico");
}
