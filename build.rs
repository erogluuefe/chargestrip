// Windows'ta .exe'ye gömülenler (Windows SDK'daki rc.exe ile):
// - Uygulama bildirimi (src/win/app.manifest): Windows 10 uyumluluğu olmadan taskbar'ın içine
//   katmanlı (layered) alt pencere konamaz; ayrıca monitör başına DPI ve modern denetimler.
// - Simge (assets/icon.ico) ve sürüm bilgisi (Cargo.toml'dan): Başlat menüsü, Uygulamalar listesi
//   ve dosya özelliklerinde görünür.
fn main() {
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        let mut res = winresource::WindowsResource::new();
        res.set_manifest_file("src/win/app.manifest")
            .set_icon("assets/icon.ico")
            .set("FileDescription", "Chargestrip")
            .set("ProductName", "Chargestrip");
        res.compile().expect("Windows kaynakları gömülemedi");
    }
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/win/app.manifest");
    println!("cargo:rerun-if-changed=assets/icon.ico");
}
