// Windows'ta .exe'ye gömülenler:
// - Uygulama bildirimi (manifest): Windows 8+ uyumluluğu olmadan katmanlı (layered) alt pencere
//   oluşturulamaz; ayrıca monitör başına DPI farkındalığı.
// - Simge (assets/icon.ico) ve sürüm bilgisi (Cargo.toml'dan): Başlat menüsü, Uygulamalar listesi
//   ve dosya özelliklerinde görünür. Windows SDK'daki rc.exe ile derlenir.
fn main() {
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        embed_manifest::embed_manifest(embed_manifest::new_manifest("Chargestrip"))
            .expect("manifest gömülemedi");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico").set("FileDescription", "Chargestrip").set("ProductName", "Chargestrip");
        res.compile().expect("simge gömülemedi");
    }
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/icon.ico");
}
