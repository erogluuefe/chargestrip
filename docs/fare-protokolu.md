# Waizowl OGM Pro V2 — pil protokolü

Kaynak: Waizowl Driver Software 1.0.5.1 (`DeviceDriver.exe`) `tools/hidspy` (Frida) ile dinlendi, 2026-11-18.
Sadece dinlendi; kayıt sırasında bizim tarafımızdan cihaza hiçbir şey gönderilmedi.

## Cihaz
- Alıcı "OGM 8K", USB `1915:0728` (Nordic), arayüz 3, usage page `0xFF0A`, usage `0x01`.
- Fare şarj kablosuna takılıyken de trafik aynı alıcı üzerinden geldi.

## Çerçeve
Her istek ve cevap: rapor no `0x00` + 64 bayt.

| bayt | anlam |
|---|---|
| 0 | komut |
| 1 | 0x00 |
| 2 | istekte 0x81 (0x44'te 0xA5); cevapta 0x8C / 0x9A / 0xA5 |
| 3 | 0x01 (0x40/0x00 isteklerinde 0x00) |
| 4…62 | veri |
| 63 | sağlama: 64 baytın toplamı mod 256 = **0xA1** |

## Kullandığımız iki istek

| istek (rapor no hariç) | cevap |
|---|---|
| `01 00 81 01 00… 1E` — bağlantı durumu | `[4]` = `0x0A` bağlı, `0x02` fare kapalı/uykuda |
| `41 00 81 01 00… DE` — pil | `[5]` = `0x01` şarj kablosu takılı; `[6]` = yüzde (0–100) |

Kayıttan örnekler:

| durum | çerçeve başı | sağlama |
|---|---|---|
| bağlı | `01 00 8c 01 0a 15 19 28 07 06 01 f5 07` | `a9` |
| fare kapatıldı | `01 00 8c 01 02 15 19 28 07 06 01 f5 07` | `b1` |
| pil %63 (Waizowl ekranı: 63) | `41 00 8c 01 00 00 3f 00 01 05 06` | `88` |
| kablo takılı, %68 | `41 00 8c 01 00 01 44 00 01 05 06` | `82` |
| kablo çıkarıldı, %68 | `41 00 8c 01 00 00 44 00 01 05 06` | `83` |

Notlar:
- 0x41 cevabının `[8]` baytı CPI tuşuyla 1→2→3→4→0→1 değişti: DPI kademesi, kullanılmıyor.
- Kablo takılınca yüzde 62 → 68 zıpladı (gerilime dayalı ölçüm, şarjda yükselir).
- Fare pil/durum değişince alıcı aynı çerçeveleri kendiliğinden de yollar. Okurken komut baytı ve
  sağlama tutmayan çerçeveler atlanır.

## Neden salt-okuma
- Waizowl açılışta ve her yeniden bağlanmada (fare açılınca, kablo takılınca) kullanıcı hiçbir şey
  yapmadan aynı sırayı gönderiyor: `01`, `41`, `43`, `44`, `40`, `00`. Hepsi veri alanı boş istekler,
  cevaplar her seferinde mevcut durumu döndürüyor.
- Ayar yazan bir istek kayıtta yok; kullanıcı kayıt sırasında ayar değiştirmedi.
- Doğrulama: uygulamanın istekleri tekrar tekrar gönderilip `43`/`44` cevaplarının (ayar tabloları,
  ör. DPI 400/900/1600/3200/6400) değişmediği Waizowl yeniden dinlenerek kontrol edilir.

## Diğer istekler (kullanılmıyor)
- `43 00 81 01` → `a5` cevap, tuş/ayar tablosu gibi.
- `44 00 a5 01` → DPI kademeleri (`0x0190`=400 … `0x1900`=6400) ve diğer ayarlar.
- `40 00 81 00`, `00 00 81 00` → `9a` cevap, sürüm/kimlik bilgisi.
