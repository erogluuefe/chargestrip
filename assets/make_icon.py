"""Chargestrip logosu: yeşil degrade yuvarlak kare, beyaz pil, koyu yeşil şimşek.

Çıktılar (bu klasöre):
  icon.png  1024 px, macOS paketi için (CI'da .icns'e çevrilir)
  icon.ico  16–256 px, Windows .exe ve kurulum programı için

Çalıştırma: pip install pillow && python assets/make_icon.py
"""
from pathlib import Path
from PIL import Image, ImageDraw

N, K = 1024, 4  # hedef boyut, kenar yumuşatma için büyütme
W = N * K
OUT = Path(__file__).parent


def s(*v):
    return [x * K for x in v]


def draw() -> Image.Image:
    img = Image.new("RGBA", (W, W), (0, 0, 0, 0))

    # Zemin: yukarıdan aşağı açık yeşilden koyu yeşile.
    grad = Image.new("RGBA", (W, W))
    gd = ImageDraw.Draw(grad)
    top, bottom = (76, 195, 110), (30, 130, 90)
    for y in range(W):
        t = y / W
        gd.line([(0, y), (W, y)], fill=tuple(int(a + (b - a) * t) for a, b in zip(top, bottom)) + (255,))
    mask = Image.new("L", (W, W), 0)
    ImageDraw.Draw(mask).rounded_rectangle(s(64, 64, 960, 960), radius=200 * K, fill=255)
    img.paste(grad, (0, 0), mask)

    d = ImageDraw.Draw(img)
    white = (255, 255, 255, 255)
    d.rounded_rectangle(s(190, 330, 790, 694), radius=90 * K, fill=white)  # pil gövdesi
    d.rounded_rectangle(s(800, 446, 856, 578), radius=22 * K, fill=white)  # pil ucu

    # Şimşek
    cx, cy, size = 490, 512, 300
    bolt = [(0.10, -0.50), (-0.28, 0.06), (-0.02, 0.06), (-0.10, 0.50), (0.28, -0.06), (0.02, -0.06)]
    d.polygon([((cx + x * size) * K, (cy + y * size) * K) for x, y in bolt], fill=(30, 120, 70, 255))

    return img.resize((N, N), Image.LANCZOS)


if __name__ == "__main__":
    icon = draw()
    icon.save(OUT / "icon.png")
    sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256]
    icon.save(OUT / "icon.ico", sizes=[(n, n) for n in sizes])
    print("icon.png, icon.ico yazıldı")
