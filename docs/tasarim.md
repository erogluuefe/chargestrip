# Pil Göstergesi — kısa tasarım

Tarih: 2026-11-18 · Durum: onaylandı (2026-11-18)

## Amaç
Kablosuz kulaklık ve farenin pil durumunu, taskbar'da (Mac'te menü çubuğunda)
sürekli görünen, olabildiğince az kaynak yiyen küçük bir uygulama ile göstermek.
Üretici yazılımlarını (SteelSeries GG, Waizowl Driver) arka planda açık tutmaya gerek kalmasın.

## Cihazlar (bu bilgisayarda tespit edildi)
| Cihaz | Bağlantı | USB kimliği | Pil okuma |
|---|---|---|---|
| SteelSeries Arctis Nova 7P | 2.4 GHz alıcı | 1038:220A | Biliniyor (Nova 7 ailesi protokolü), doğrulanacak |
| Waizowl OGM Pro V2 | 2.4 GHz alıcı "OGM 8K" (Nordic) | 1915:0728, üretici arayüzü UsagePage FF0A (MI_03) | **Bilinmiyor**, çözülecek |
| NuPhy Air75 HE (klavye) | kablolu | 19F5:6120 | kapsam dışı |

## Hedefler
- Cihaz başına bir taskbar simgesi, simgenin üstünde yüzde rakamı (🎧 82 · 🖱 45).
- Üzerine gelince açıklama: cihaz adı, yüzde, "şarj oluyor" / "bağlı değil".
- Düşük pil uyarısı sadece renkle: ≤%15 simge kırmızı. Bildirim yok (kullanıcı kararı).
- Sağ tık menüsü: Yenile, Windows açılışında başlat (aç/kapa), Çıkış.
- Windows 11 ve macOS'ta aynı koddan çalışma.
- Kaynak kullanımı: birkaç MB RAM, boştayken ~0 CPU (dakikada bir sorgu).

## Hedef dışı
- Klavye (kablolu), Bluetooth cihazlar, başka marka cihazlar (ileride eklenebilir ama şimdi değil).
- Cihaz ayarı değiştirme (DPI, ışık, EQ vb.). Uygulama **sadece okur**.
- Grafik/geçmiş ekranı, ayar penceresi, sistem bildirimleri.

## Anti-hedefler (olursa başarısızlık sayılır)
- Fareye/kulaklığa ayarlarını değiştirebilecek bir komut göndermek.
- Oyun sırasında takılma, gecikme veya farenin 8K hızını etkilemek.
- Üretici yazılımı açıkken onunla çakışıp onu bozmak.

## Yaklaşımlar
1. **Rust (seçildi)**: tek kod tabanı Windows + macOS; birkaç MB RAM; derleme araçları kurulumu gerekiyor.
2. C# / .NET Framework: hiçbir şey kurmadan en hızlı biter, ~15-20 MB RAM, ama sadece Windows.
3. C: taşınabilir dil ama tepsi/menü kısmı iki kez yazılır (Win32 + Cocoa); tek kişilik araç için değmez.
4. PowerShell betiği: 60-100 MB RAM, "hafif" isteğine ters — elendi.

**Seçim:** Rust, kullanıcı macOS'ta da çalışmasını istediği için.

## Kullanıcının kararları
- Dil: Rust. Platformlar: Windows ve macOS.
- Bildirim yok; düşük pil kırmızı renkle gösterilir.

## Benim kararlarım (itiraz edilebilir)
- **Kütüphaneler:** `hidapi` (cihaz erişimi, her iki sistemde olgun), `tray-icon` + `tao` (tepsi simgesi ve olay döngüsü).
- **Simge çizimi:** rakamlar küçük gömülü bir piksel yazı tipiyle doğrudan simgeye çizilir; yazı tipi dosyası/bağımlılığı yok. Renk: normal beyaz, ≤%15 kırmızı, şarjda yeşil, bağlı değilse gri "–".
- **Sorgu aralığı:** 60 sn; menüden "Yenile" ile anında.
- **Fare uyuyorsa:** son bilinen değer gri gösterilir, uyandırmak için komut gönderilmez.
- **Windows'ta derleme:** rustup + MSVC Build Tools (~3-4 GB; C:'de 906 GB boş).
- **macOS derlemesi:** GitHub Actions ile otomatik (ya da bir Mac'te `cargo build`). İmzasız → ilk açılışta sağ tık → Aç.
- **Konum:** `C:\Users\Administrator\pil-gostergesi\`.

## Fare protokolünü çözme (ilk iş)
Tahmine dayalı komut göndermek yok (anti-hedef). Sıra:
1. Waizowl Driver Software açıkken, onun alıcıyla konuşmasını kaydet
   (USBPcap + Wireshark, ya da programın HID çağrılarını izleyerek).
   Programda pil ekranı var; kayıtta "0xE2 … 电量值" (pil değeri) iz bıraktığı görüldü.
2. Pil istek/cevap paketini ayıkla, yüzdeyi birkaç farklı şarj seviyesinde doğrula
   (farenin CPI tuşuyla gösterdiği seviye ile karşılaştır).
3. Aynı paketi uygulamadan göndermenin sadece okuma olduğunu doğrula.

**Çözülemezse:** uygulama kulaklıkla çıkar, fare simgesi "?" gösterir; bu durum kullanıcıya bildirilir, gizlenmez.

## Builder'a kalanlar
- Nova 7P cevap biçiminin (yüzde mi 0-4 seviye mi, şarj baytı) gerçek cihazda doğrulanması.
- Açılışta başlatma: Windows'ta kayıt defteri Run anahtarı, Mac'te LaunchAgent.
- Cihaz takılıp çıkarılınca yeniden bağlanma.
