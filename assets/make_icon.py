"""Chargestrip logo: green gradient rounded square, white battery, dark green lightning bolt.

Outputs (into this folder):
  icon.png  1024 px, for the macOS bundle (converted to .icns in CI)
  icon.ico  16–256 px, for the Windows .exe and the installer

Run: pip install pillow && python assets/make_icon.py
"""
from pathlib import Path
from PIL import Image, ImageDraw

N, K = 1024, 4  # target size, supersampling factor for smooth edges
W = N * K
OUT = Path(__file__).parent


def s(*v):
    return [x * K for x in v]


def draw() -> Image.Image:
    img = Image.new("RGBA", (W, W), (0, 0, 0, 0))

    # Background: light green at the top to dark green at the bottom.
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
    d.rounded_rectangle(s(190, 330, 790, 694), radius=90 * K, fill=white)  # battery body
    d.rounded_rectangle(s(800, 446, 856, 578), radius=22 * K, fill=white)  # battery tip

    # Lightning bolt
    cx, cy, size = 490, 512, 300
    bolt = [(0.10, -0.50), (-0.28, 0.06), (-0.02, 0.06), (-0.10, 0.50), (0.28, -0.06), (0.02, -0.06)]
    d.polygon([((cx + x * size) * K, (cy + y * size) * K) for x, y in bolt], fill=(30, 120, 70, 255))

    return img.resize((N, N), Image.LANCZOS)


if __name__ == "__main__":
    icon = draw()
    icon.save(OUT / "icon.png")
    sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256]
    icon.save(OUT / "icon.ico", sizes=[(n, n) for n in sizes])
    print("wrote icon.png and icon.ico")
