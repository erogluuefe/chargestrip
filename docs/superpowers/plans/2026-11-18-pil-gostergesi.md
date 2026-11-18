# Pil Göstergesi Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Arctis Nova 7P kulaklık ve Waizowl OGM Pro V2 farenin pil yüzdesini taskbar / menü çubuğunda rakamlı simgelerle gösteren, birkaç MB RAM kullanan Rust uygulaması.

**Architecture:** Arka plan iş parçacığı 60 sn'de bir (veya "Yenile" ile hemen) iki USB alıcıyı `hidapi` ile sorgular, sonucu ana iş parçacığına gönderir (Windows: düz Win32 mesaj döngüsü + `WM_APP`; macOS: `tao` olay döngüsü); ana iş parçacığı her cihaz için bir `tray-icon` simgesini yeniden çizer. Cihaz protokolleri, sunum mantığı ve simge çizimi saf fonksiyonlardır ve birim testlidir; donanım ve tepsi kısmı elle doğrulanır.

**Tech Stack:** Rust (stable, MSVC hedefi Windows'ta), `hidapi` (Windows'ta `windows-native`), `tray-icon`, `auto-launch`; Windows'ta `windows-sys`, macOS'ta `tao`.

**Spec:** `docs/tasarim.md`

## Global Constraints

- Uygulama cihazlara **sadece okuma** amaçlı paket gönderir; ayar değiştiren hiçbir komut yok. Fare için yalnızca Task 3'te kayıttan doğrulanmış istek gönderilir.
- Bildirim yok. Düşük pil = simge kırmızı (≤%15).
- Renkler: normal beyaz `#FFFFFF`, ≤%15 kırmızı `#E53935`, şarjda yeşil `#43A047`, bağlı değil / eski değer gri `#9E9E9E`.
- Sorgu aralığı 60 sn; HID okuma zaman aşımı 1000 ms; sorgu asla UI iş parçacığında yapılmaz.
- Bağımlılıklar yalnızca: `hidapi`, `tray-icon`, `auto-launch`, Windows'ta `windows-sys`, macOS'ta `tao` (yazı tipi/görüntü kütüphanesi yok).
- RAM önceliği (kullanıcı kararı 2026-11-18): Windows'ta tao/winit yok, düz Win32 mesaj döngüsü; hedef < 5 MB.
- Windows'ta konsol penceresi açılmaz (`#![windows_subsystem = "windows"]`).
- Release profili: `opt-level = "z"`, `lto = true`, `strip = true`, `panic = "abort"`, `codegen-units = 1`.
- Kod tek tabandan Windows 11 ve macOS'ta derlenir; platforma özel kod `#[cfg]` ile ve en aza indirilmiş.
- Proje kökü: `C:\Users\Administrator\pil-gostergesi\`.

## Review Focus

1. **Alıcı çıkarılıp sonra takılırsa** → simge gri "–" olur, uygulama yeniden başlatılmadan bir sonraki sorguda değer geri gelir. (Task 4: `poller` her turda cihazı yeniden açar; Task 1 testi `present_unavailable_is_gray_dash`.)
2. **Kulaklık kapalı / fare uykuda ama alıcı takılı** → %0 kırmızı DEĞİL; kulaklık gri "–", fare son bilinen değer gri. (Task 1 testleri `present_disconnected_with_last_shows_gray_last`, Task 2 testi `parse_offline_is_disconnected`.)
3. **SteelSeries GG veya Waizowl yazılımı aynı anda açık** → bizim okuma onları bozmaz, onlar meşgul tutarsa bizimki "–" gösterip devam eder, çökmez. (Task 4 elle doğrulama adımı.)
4. **Açık renk Windows taskbar'ı** → beyaz rakam görünmez kalmamalı: her glif 1 px koyu (`#000000`, alfa 200) dış çizgiyle çizilir. (Task 1 testi `glyph_has_dark_outline`.)
5. **"100" ve tek haneli değerler** → 32 px genişliğe sığar, kesilmez; tek hane ortalanır. (Task 1 testi `render_100_fits`.)

---

## File Structure

| Dosya | Sorumluluk | Task |
|---|---|---|
| `Cargo.toml` | bağımlılıklar, release profili | 1 |
| `src/model.rs` | `Reading`, `Shown`, `present()` — cihaz sonucunu görünüme çevirir | 1 |
| `src/icon.rs` | 3×5 piksel rakam/harf yazı tipi, `render_icon()` → 32×32 RGBA | 1 |
| `src/devices/mod.rs` | `poll_all()` | 2 |
| `src/devices/nova7.rs` | Arctis Nova 7P isteği ve cevap ayrıştırma | 2 |
| `docs/fare-protokolu.md` | kayıttan çıkarılan OGM pil protokolü | 3 |
| `src/devices/ogm.rs` | OGM 8K isteği ve cevap ayrıştırma | 3 |
| `src/poller.rs` | arka plan döngüsü, son bilinen değer takibi | 4 |
| `src/autostart.rs` | açılışta başlatma aç/kapa | 4 |
| `src/main.rs` | giriş noktası | 4 |
| `src/tray.rs` | iki tepsi simgesi + menü (platformdan bağımsız) | 4 |
| `src/platform/windows.rs`, `src/platform/macos.rs` | olay döngüleri | 4 |
| `.github/workflows/build.yml` | Windows + macOS derlemesi | 5 |

---

### Task 1: Araçlar, iskelet, sunum mantığı ve simge çizimi

**Files:**
- Create: `Cargo.toml`, `src/main.rs` (geçici `fn main(){}`), `src/model.rs`, `src/icon.rs`, `.gitignore`

**Interfaces:**
- Produces:
  ```rust
  // model.rs
  pub enum Reading { Ok { percent: u8, charging: bool }, Disconnected, Unavailable }
  #[derive(Clone, Copy, PartialEq, Debug)] pub struct Rgb(pub u8, pub u8, pub u8);
  pub const WHITE: Rgb; pub const RED: Rgb; pub const GREEN: Rgb; pub const GRAY: Rgb;
  pub struct Shown { pub text: String, pub color: Rgb, pub tooltip: String }
  pub fn present(name: &str, r: &Reading, last: Option<u8>) -> Shown
  // icon.rs
  pub const SIZE: u32 = 32;
  pub fn render_icon(letter: char, text: &str, color: Rgb) -> Vec<u8> // SIZE*SIZE*4 RGBA
  ```
  `Disconnected` = alıcı var, cihaz kapalı/uykuda. `Unavailable` = alıcı yok veya okunamadı.

- [ ] **Step 1: Araçları kur ve iskeleti oluştur**

  `winget install Rustlang.Rustup` ve `winget install Microsoft.VisualStudio.2022.BuildTools --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"`; ardından `cargo init --name pil-gostergesi` (proje kökünde), `git init`. Global Constraints'teki release profilini ve dört bağımlılığı ekle.
  Run: `cargo build` → Expected: başarılı.

- [ ] **Step 2: `model.rs` testlerini yaz**

  ```rust
  #[test] fn present_ok_normal() { let s = present("Fare", &Reading::Ok{percent:82,charging:false}, None);
      assert_eq!((s.text.as_str(), s.color), ("82", WHITE)); assert_eq!(s.tooltip, "Fare: %82"); }
  #[test] fn present_low_is_red() { assert_eq!(present("Fare",&Reading::Ok{percent:15,charging:false},None).color, RED);
      assert_eq!(present("Fare",&Reading::Ok{percent:16,charging:false},None).color, WHITE); }
  #[test] fn present_charging_is_green_even_if_low() { let s = present("Kulaklık",&Reading::Ok{percent:5,charging:true},None);
      assert_eq!(s.color, GREEN); assert_eq!(s.tooltip, "Kulaklık: %5 · şarj oluyor"); }
  #[test] fn present_disconnected_with_last_shows_gray_last() { let s = present("Fare",&Reading::Disconnected,Some(40));
      assert_eq!((s.text.as_str(), s.color), ("40", GRAY)); assert_eq!(s.tooltip, "Fare: bağlı değil (son %40)"); }
  #[test] fn present_disconnected_without_last_is_dash() { let s = present("Kulaklık",&Reading::Disconnected,None);
      assert_eq!((s.text.as_str(), s.color), ("–", GRAY)); assert_eq!(s.tooltip, "Kulaklık: bağlı değil"); }
  #[test] fn present_unavailable_is_gray_dash() { let s = present("Fare",&Reading::Unavailable,Some(40));
      assert_eq!((s.text.as_str(), s.color), ("–", GRAY)); assert_eq!(s.tooltip, "Fare: alıcı bulunamadı"); }
  ```

- [ ] **Step 3: Testlerin başarısız olduğunu gör** — Run: `cargo test model` → Expected: derleme hatası / FAIL.

- [ ] **Step 4: `present()` uygula** — renk önceliği: şarj → yeşil, ≤15 → kırmızı, değilse beyaz.

- [ ] **Step 5: Testler geçsin** — Run: `cargo test model` → Expected: 6 passed.

- [ ] **Step 6: `icon.rs` testlerini yaz**

  ```rust
  fn opaque_cols(buf:&[u8]) -> (u32,u32) /* en sol/en sağ alfa>0 sütun */;
  #[test] fn render_size() { assert_eq!(render_icon('F',"82",WHITE).len(), (SIZE*SIZE*4) as usize); }
  #[test] fn render_100_fits() { let (l,r) = opaque_cols(&render_icon('F',"100",WHITE)); assert!(l >= 0 && r <= SIZE-1);
      /* ve tam 3 rakam çizilmiş: rakam satırlarında en az 3 ayrı opak sütun grubu */ }
  #[test] fn single_digit_centered() { let (l,r) = opaque_cols(&render_icon('F',"5",WHITE)); assert!((l as i32 - (SIZE-1-r) as i32).abs() <= 1); }
  #[test] fn glyph_has_dark_outline() { /* render_icon('K',"8",WHITE) içinde beyaz pikselin 4-komşularından biri RGB(0,0,0), alfa 200 */ }
  #[test] fn color_applied() { /* render_icon('F',"8",RED) içinde (0xE5,0x39,0x35,255) piksel var */ }
  #[test] fn dash_renders() { assert!(render_icon('F',"–",GRAY).chunks(4).any(|p| p[3]==255)); }
  ```

- [ ] **Step 7: Başarısızlığı gör** — Run: `cargo test icon` → Expected: FAIL.

- [ ] **Step 8: `render_icon()` uygula** — 0-9, "–", "K", "F" için 3×5 bit tablosu. Harf üstte ölçek 1 sol üst köşede; rakamlar altta yatay ortalı, sığan en büyük ölçekte (1-2 hane ölçek 4, 3 hane ölçek 3: 9+1+9+1+9 = 29 px + 2 px dış çizgi = 31 px). Her dolu pikselin boş 4-komşularına dış çizgi.

- [ ] **Step 9: Testler geçsin** — Run: `cargo test` → Expected: tümü passed.

- [ ] **Step 10: Commit** — `git add -A && git commit -m "feat: sunum mantığı ve simge çizimi"`

---

### Task 2: Arctis Nova 7P sorgusu

**Files:**
- Create: `src/devices/mod.rs`, `src/devices/nova7.rs`

**Interfaces:**
- Consumes: `model::Reading`
- Produces:
  ```rust
  // nova7.rs
  pub const VID: u16 = 0x1038; pub const PID: u16 = 0x220A;
  pub fn parse(buf: &[u8]) -> Reading
  pub fn query(api: &hidapi::HidApi) -> Reading
  // mod.rs
  pub fn poll_all(api: &mut hidapi::HidApi) -> (Reading /*kulaklık*/, Reading /*fare*/) // her çağrıda api.refresh_devices()
  ```

- [ ] **Step 1: Protokolü doğrula**

  Arctis Nova 7 protokolünü doğrula (Nova 7 ailesi; PID listesinde `0x220a` olduğunu teyit et). Beklenen (teyit edilecek): arayüz 3, usage page `0xFFC0`; istek `[0x00, 0xB0]` (64 bayta doldurulmuş); cevapta `buf[2]` seviye 0–4, `buf[3]` durum (`0x00` kapalı, `0x01` şarjda, `0x03` açık). Farklıysa aşağıdaki testlerdeki değerleri kaynağa göre düzelt ve `nova7.rs` başına kaynağın commit bağlantısını yorum olarak yaz.

- [ ] **Step 2: Testleri yaz**

  ```rust
  #[test] fn parse_online_full() { assert_eq!(parse(&[0xB0,0,4,3,0,0,0,0]), Reading::Ok{percent:100,charging:false}); }
  #[test] fn parse_online_half() { assert_eq!(parse(&[0xB0,0,2,3,0,0,0,0]), Reading::Ok{percent:50,charging:false}); }
  #[test] fn parse_charging() { assert_eq!(parse(&[0xB0,0,1,1,0,0,0,0]), Reading::Ok{percent:25,charging:true}); }
  #[test] fn parse_offline_is_disconnected() { assert_eq!(parse(&[0xB0,0,0,0,0,0,0,0]), Reading::Disconnected); }
  #[test] fn parse_short_or_garbage_is_unavailable() { assert_eq!(parse(&[0xB0]), Reading::Unavailable); assert_eq!(parse(&[]), Reading::Unavailable); }
  ```
  (`Reading`'e `PartialEq, Debug` türet.)

- [ ] **Step 3: Başarısızlığı gör** — Run: `cargo test nova7` → Expected: FAIL.

- [ ] **Step 4: `parse()` ve `query()` uygula** — `query`: VID/PID + usage page ile cihazı bul, yoksa `Unavailable`; `write` → `read_timeout(…, 1000)`; hata/zaman aşımı → `Unavailable`. Cihaz paylaşımlı açılır (hidapi varsayılanı), tutulmaz: her sorguda aç/kapa.

- [ ] **Step 5: Testler geçsin** — Run: `cargo test nova7` → Expected: 5 passed.

- [ ] **Step 6: Gerçek cihazda doğrula** — Geçici `examples/probe.rs`: `poll_all` sonucunu yazdırır. Run: `cargo run --example probe` → kulaklık açıkken `Ok{..}`, kapatınca `Disconnected`, alıcı çıkarılınca `Unavailable`. Fare şimdilik `Unavailable` döner.

- [ ] **Step 7: Commit** — `git commit -am "feat: Arctis Nova 7P pil sorgusu"` (yeni dosyaları `git add` ile ekle).

---

### Task 3: OGM fare protokolünü çıkar ve uygula

**Files:**
- Create: `docs/fare-protokolu.md`, `src/devices/ogm.rs`, `tests/fixtures/ogm/*.bin` (kayıttan alınan ham cevaplar)
- Modify: `src/devices/mod.rs` (fare sorgusu `ogm::query`)

**Interfaces:**
- Consumes: `model::Reading`
- Produces:
  ```rust
  pub const VID: u16 = 0x1915; pub const PID: u16 = 0x0728; pub const USAGE_PAGE: u16 = 0xFF0A;
  pub const BATTERY_REQUEST: &[u8]; // kayıttan birebir
  pub fn parse(buf: &[u8]) -> Reading
  pub fn query(api: &hidapi::HidApi) -> Reading
  ```

- [ ] **Step 1: Trafiği kaydet** — USBPcap + Wireshark kur (`winget install Wireshark` USBPcap seçeneğiyle). Kullanıcıdan: Waizowl Driver Software'i aç, ana sayfada pil görünene kadar bekle, kapat. OGM 8K (1915:0728) alıcısının bulunduğu USB kök hub'ını kaydet. Wireshark'ta `usb.idVendor == 0x1915` ve MI_03 uç noktası (UsagePage FF0A) paketlerini süz. Yalnızca dinlenir; bu adımda bizim tarafımızdan hiçbir paket gönderilmez.

- [ ] **Step 2: Pil isteğini ve cevabını belirle** — Waizowl'un gösterdiği yüzde ile cevaptaki bayt eşleşmeli. En az iki farklı şarj seviyesinde (ör. bugün ve fare biraz kullanıldıktan sonra) ve bir kez fare kapalıyken kaydı tekrarla; şarj kablosu takılıyken bir kez daha. Driver günlüğündeki `0xE2` (durum değişince pil okunmaz) ve `41 00 8C 01 ..` / `01 00 8C 01 .. 15` bildirimlerini de açıkla. Sonuçları `docs/fare-protokolu.md`'ye yaz: istek baytları, cevap düzeni, yüzde/şarj/bağlı değil eşlemesi, ve isteğin **ayar yazan bir komut olmadığının** gerekçesi (Waizowl'un açılışta salt okuma için gönderdiği paket olduğu). Ham cevapları `tests/fixtures/ogm/` altına kaydet.
  **Dur noktası:** istek salt-okuma olduğu kanıtlanamazsa uygulama, fare için `Unavailable` ("?") ile çıkar; kullanıcıya bildirilir, bu task orada biter.

- [ ] **Step 3: Testleri yaz** — her fixture için: `parse(include_bytes!("../../tests/fixtures/ogm/<ad>.bin")) == <dokümandaki beklenen Reading>`; ek olarak `parse(&[]) == Unavailable`. Fixture adları ve beklenen değerler `docs/fare-protokolu.md` tablosundan birebir.

- [ ] **Step 4: Başarısızlığı gör** — Run: `cargo test ogm` → Expected: FAIL.

- [ ] **Step 5: `parse()` ve `query()` uygula** — `query`: VID/PID/USAGE_PAGE ile aç, `BATTERY_REQUEST` gönder, 1000 ms bekle; kayıtta görülen sıra (feature report mu, output report mu) neyse aynısı.

- [ ] **Step 6: Testler geçsin** — Run: `cargo test` → Expected: tümü passed.

- [ ] **Step 7: Gerçek cihazda doğrula** — Run: `cargo run --example probe` → farenin yüzdesi Waizowl'un ve CPI tuşu göstergesinin verdiğiyle uyumlu; fare kapatılınca `Disconnected`. Probe'u 10 kez art arda çalıştırıp fare ayarlarının (DPI, polling) değişmediğini Waizowl'da kontrol et.

- [ ] **Step 8: Commit** — `git add -A && git commit -m "feat: OGM Pro V2 pil sorgusu ve protokol notları"`

---

### Task 4: Arka plan sorgusu ve tepsi uygulaması

**Files:**
- Create: `src/poller.rs`, `src/autostart.rs`
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: `devices::poll_all`, `model::{Reading, present, Shown}`, `icon::{render_icon, SIZE}`
- Produces:
  ```rust
  // poller.rs
  pub struct Memory { pub headset: Option<u8>, pub mouse: Option<u8> }
  pub fn remember(mem: &mut Memory, headset: &Reading, mouse: &Reading) // Ok ise son değeri günceller
  pub fn spawn(proxy: tao::event_loop::EventLoopProxy<AppEvent>) -> std::sync::mpsc::Sender<()> // () = hemen yenile
  pub enum AppEvent { Update { headset: Shown, mouse: Shown } }
  // autostart.rs
  pub fn is_enabled() -> bool; pub fn set_enabled(on: bool) -> Result<(), String>
  ```

- [ ] **Step 1: `remember` testlerini yaz**

  ```rust
  #[test] fn remember_keeps_last_on_disconnect() { let mut m = Memory{headset:None,mouse:None};
      remember(&mut m, &Reading::Ok{percent:70,charging:false}, &Reading::Ok{percent:40,charging:false});
      remember(&mut m, &Reading::Disconnected, &Reading::Disconnected);
      assert_eq!((m.headset, m.mouse), (Some(70), Some(40))); }
  #[test] fn remember_updates_on_ok() { /* 40 → 39 */ }
  ```

- [ ] **Step 2: Başarısızlığı gör, uygula, geçsin** — Run: `cargo test poller` → önce FAIL, sonra 2 passed.

- [ ] **Step 3: `spawn` ve `main.rs`'i uygula** — `spawn`: kendi iş parçacığında `HidApi` açar, döngü: `poll_all` → `remember` → `present("Kulaklık", …, mem.headset)` / `present("Fare", …, mem.mouse)` → `proxy.send_event`; sonra `rx.recv_timeout(60 s)` (Yenile mesajı gelirse hemen tekrar). `HidApi::new()` başarısızsa her turda yeniden dener, iki cihaz `Unavailable`. `main.rs`: `tao` olay döngüsü, `TrayIconBuilder` ile iki simge (kulaklık 'K', fare 'F'), ilk durumda "–" gri; `AppEvent::Update` gelince `set_icon(render_icon(...))` + `set_tooltip`. Menü (her iki simgede aynı): "Yenile", "Açılışta başlat" (işaretli öğe, `autostart::is_enabled()`), "Çıkış". macOS'ta Dock simgesini gizle (`ActivationPolicy::Accessory`).

- [ ] **Step 4: `autostart.rs`'i uygula** — `auto-launch` ile, uygulama adı `Pil Göstergesi`, yol `std::env::current_exe()`.

- [ ] **Step 5: Elle doğrula (Windows)** — Run: `cargo build --release` → `target\release\pil-gostergesi.exe` çalıştır. Kontrol listesi:
  - İki simge taskbar'da, rakamlar okunuyor; açık ve koyu taskbar temasında görünür.
  - Üzerine gelince açıklamalar Task 1'deki metinlerle aynı.
  - Kulaklığı kapat → "Yenile" → gri "–"; aç → değer geri.
  - Fare alıcısını çıkar → gri "–"; tak → bir sonraki Yenile'de değer geri, yeniden başlatma yok.
  - SteelSeries GG / Waizowl yazılımı açıkken çalışıyor, onlar da bozulmuyor.
  - "Açılışta başlat" aç/kapa kayıt defterine yansıyor (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`).
  - Görev Yöneticisi: RAM < 5 MB, boşta CPU %0; .exe boyutu not edilir.
  - Oyun içinde fare hissinde/gecikmesinde fark yok (kullanıcı onayı).

- [ ] **Step 6: Commit** — `git add -A && git commit -m "feat: tepsi uygulaması, arka plan sorgusu, açılışta başlat"`

---

### Task 5: macOS derlemesi

**Files:**
- Create: `.github/workflows/build.yml`, `README.md`

- [ ] **Step 1: İş akışını yaz** — `windows-latest` ve `macos-latest` üzerinde `cargo test` + `cargo build --release`; çıktıları artifact olarak yükle (macOS'ta `.app` paketlemesi: `Info.plist` içinde `LSUIElement = true`).

- [ ] **Step 2: README** — Türkçe: ne yapar, desteklenen cihazlar, Mac'te ilk açılış (sağ tık → Aç, gerekirse Giriş İzleme izni), açılışta başlatma.

- [ ] **Step 3: Doğrula** — Bu adım için GitHub deposu gerekir; kullanıcı onayı olmadan depo oluşturulmaz / push yapılmaz. Onay verilirse: push → iş akışı iki platformda yeşil. Onay yoksa: `cargo check` Windows'ta geçer, macOS doğrulaması "yapılmadı" olarak raporlanır.

- [ ] **Step 4: Commit** — `git add -A && git commit -m "ci: Windows ve macOS derlemesi"`
