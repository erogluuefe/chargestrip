// Windows'ta uygulama bildirimi (manifest) gömülür: Windows 8+ uyumluluğu olmadan
// katmanlı (layered) alt pencere oluşturulamaz; ayrıca monitör başına DPI farkındalığı.
fn main() {
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        embed_manifest::embed_manifest(embed_manifest::new_manifest("PilGostergesi"))
            .expect("manifest gömülemedi");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
