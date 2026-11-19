# Chargestrip

Kablosuz kulaklık, fare vb. cihazların pil durumunu gösterir:

- **Windows 10/11:** taskbar'da, `^` okunun hemen solunda: `🎧 75%  🖱 64%`
- **macOS:** menü çubuğunda tek bir öğe (CI'da derleniyor, gerçek Mac'te henüz denenmedi)

Renkler (Windows): ≤%15 kırmızı, şarjda yeşil, cihaz kapalı/uykuda ise gri (varsa son değerle).
Sağ tık: Yenile · Açılışta başlat · Çıkış. Sorgu aralığı 60 sn.

Hafiflik: tek dosya ~200 KB, ~1,5 MB özel bellek, 7 bağımlılık (Windows). Arka planda servis yok.

## Desteklenen cihazlar

| Cihaz | Alıcı | Yöntem |
|---|---|---|
| SteelSeries Arctis Nova 7P | `1038:220A`, `22A7`, `2298` | Durum isteği `B0` |
| Waizowl OGM Pro V2 | `1915:0728` ("OGM 8K") | Kendi kaydımız: [docs/fare-protokolu.md](docs/fare-protokolu.md) |

## Derleme

```
cargo build --release          # target/release/chargestrip.exe
cargo test                     # birim testleri
cargo test probe -- --ignored --nocapture   # gerçek cihazlarla deneme
```

Windows'ta Rust (MSVC) ve Visual Studio C++ Build Tools gerekir.

## Sürüm çıkarma

1. `Cargo.toml` içindeki `version` değerini artır (ör. `0.3.0`) ve commit'le.
2. Aynı sürümle etiket gönder:
   ```
   git tag v0.3.0
   git push origin v0.3.0
   ```
3. GitHub Actions ([.github/workflows/release.yml](.github/workflows/release.yml)) testleri çalıştırır,
   Windows kurulum dosyasını (`Chargestrip-Setup-0.3.0.exe`) ve macOS paketini üretip
   Releases sayfasına koyar. Etiket `Cargo.toml` sürümüyle uyuşmazsa durur.

Kurulum ([installer/chargestrip.iss](installer/chargestrip.iss), Inno Setup) yönetici izni istemez,
`%LOCALAPPDATA%\Programs\Chargestrip` altına kurar, Başlat menüsüne kısayol ve isteğe bağlı açılışta
başlatma ekler; "Uygulamalar" listesinden kaldırılabilir. Dosyalar imzasızdır: Windows ilk açılışta
"Ek bilgi → Yine de çalıştır", macOS "sağ tık → Aç" ister.

Yerelde kurulum dosyası: `cargo build --release` ardından
`ISCC.exe /DAppVersion=0.3.0 installer\chargestrip.iss` → `dist\`.

## Yapı

```
src/
  main.rs            giriş; platform seçimi
  model.rs           Kind, Battery, Tone, Shown — gösterim kararları (platformdan bağımsız)
  poller.rs          arka plan sorgu iş parçacığı, son bilinen değerler
  devices/
    mod.rs           Driver tanımı, DRIVERS listesi, poll(), exchange() yardımcısı
    arctis_nova7.rs  her cihaz modeli = bir dosya
    waizowl_ogm.rs
  win/
    mod.rs           Win32 mesaj döngüsü
    strip.rs         taskbar şeridi (çizim, konum, sağ tık menüsü)
    autostart.rs     HKCU\…\Run
  mac.rs             menü çubuğu öğesi + LaunchAgent
build.rs             Windows uygulama bildirimi (katmanlı alt pencere ve DPI için)
installer/           Inno Setup kurulum tanımı
.github/workflows/   sürüm etiketiyle otomatik derleme ve Release
tools/hidspy/        protokol çözmek için HID trafik dinleyicisi
```

## Yeni cihaz ekleme

1. **Alıcıyı bul.** Cihazı takıp `cargo test probe -- --ignored --nocapture` çalıştır. Üreticiye özel
   (usage page ≥ `FF00`) arayüzler VID:PID ve usage page ile listelenir.
2. **Protokolü bul.**
   - Önce aynı cihazı destekleyen açık kaynak projelere bak.
   - Yoksa üreticinin yazılımını dinle (sadece dinler, cihaza bir şey göndermez):
     `pip install frida` → `python tools/hidspy/run.py "<yazılım.exe>" <vid> kayit.log`.
     Yazılım açıkken pil yüzdesini not et, cihazı kapat/aç, şarj kablosunu tak/çıkar; kayıtta
     değişen baytları eşleştir. Yalnızca yazılımın her bağlantıda kendiliğinden gönderdiği
     **salt-okuma** isteklerini kullan.
3. **Sürücüyü yaz.** `src/devices/<marka_model>.rs`:
   ```rust
   pub const DRIVER: Driver = Driver {
       kind: Kind::Mouse,          // yeni bir tür gerekiyorsa model.rs'e ekle (+ ikon eşlemeleri)
       vid: 0x1234,
       pids: &[0x5678],
       usage_page: 0xFF00,
       read,
   };

   fn read(dev: &HidDevice, _pid: u16) -> Option<Battery> {
       exchange(dev, &REQUEST, |report| /* bu isteğin cevabıysa çöz, değilse None */)
   }
   ```
   Kayıttan alınan ham çerçevelerle birim testleri ekle.
4. **Kaydet.** `src/devices/mod.rs` içinde `mod <marka_model>;` ve `DRIVERS` listesine
   `<marka_model>::DRIVER` ekle. Arayüz başka bir değişiklik gerektirmez.
