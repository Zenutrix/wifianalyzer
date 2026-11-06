"""Erzeugt die Grafiken für den Installer (installer/art/*.png).

Aufruf:  python tools/make_installer_art.py   (benötigt Pillow und assets/icon.png)

- side-*.png   Seitenbild auf Willkommens- und Abschlussseite (Seitenverhältnis 164:314)
- small-*.png  Logo oben rechts auf den übrigen Seiten (quadratisch)
- back.png     dezenter Hintergrund aller Seiten (Seitenverhältnis 497:360)

Farben wie in der App (dunkles Theme, theme.rs): Hintergrund #0d1117, Akzent #2f81f7.
"""
import math
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageFont

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "installer" / "art"
ICON = Image.open(ROOT / "assets" / "icon.png").convert("RGBA")

BG = (13, 17, 23)
ACCENT = (47, 129, 247)
WEAK = (140, 160, 190)


def font(size, bold=False):
    for name in (["segoeuib.ttf", "seguisb.ttf"] if bold else []) + ["segoeui.ttf", "arial.ttf"]:
        try:
            return ImageFont.truetype(name, size)
        except OSError:
            continue
    return ImageFont.load_default()


def glow(size, center, radius, color, alpha):
    """Weicher, kreisförmiger Lichtschein."""
    layer = Image.new("RGBA", size, (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    cx, cy = center
    d.ellipse([cx - radius, cy - radius, cx + radius, cy + radius], fill=color + (alpha,))
    return layer.filter(ImageFilter.GaussianBlur(radius * 0.45))


def side(w, h):
    s = 3  # in dreifacher Größe zeichnen, dann sauber verkleinern
    W, H = w * s, h * s
    img = Image.new("RGBA", (W, H), BG + (255,))
    # vertikaler Verlauf: oben leicht bläulich, unten Hintergrund
    grad = Image.new("RGBA", (W, H))
    gd = ImageDraw.Draw(grad)
    top = (18, 32, 58)
    for y in range(H):
        t = min(1.0, y / (H * 0.85))
        c = tuple(int(top[i] + (BG[i] - top[i]) * t) for i in range(3))
        gd.line([(0, y), (W, y)], fill=c + (255,))
    img = Image.alpha_composite(img, grad)

    cx, cy = W // 2, int(H * 0.36)
    img = Image.alpha_composite(img, glow((W, H), (cx, cy), int(W * 0.55), ACCENT, 70))

    # Funkwellen-Ringe wie im Startbildschirm
    rings = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    rd = ImageDraw.Draw(rings)
    base = W * 0.30
    for k in range(5):
        r = base + k * W * 0.13
        a = int(110 * (1 - k / 5))
        rd.ellipse([cx - r, cy - r, cx + r, cy + r], outline=ACCENT + (a,), width=max(2, s * 2))
    img = Image.alpha_composite(img, rings)

    # Icon
    isz = int(W * 0.52)
    icon = ICON.resize((isz, isz), Image.LANCZOS)
    shadow = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    shadow.paste((0, 0, 0, 150), (cx - isz // 2 + s * 4, cy - isz // 2 + s * 10), icon.split()[3])
    img = Image.alpha_composite(img, shadow.filter(ImageFilter.GaussianBlur(s * 10)))
    img.alpha_composite(icon, (cx - isz // 2, cy - isz // 2))

    # Schriftzug
    d = ImageDraw.Draw(img)
    f1 = font(int(W * 0.125), bold=True)
    f2 = font(int(W * 0.062))
    ty = int(H * 0.64)
    d.text((cx, ty), "WiFi Analyzer", font=f1, fill=(255, 255, 255, 255), anchor="mm")
    d.text((cx, ty + int(W * 0.12)), "WLAN-Analyse für Windows", font=f2, fill=WEAK + (255,), anchor="mm")
    # dezente Linie + Autor unten
    ly = int(H * 0.90)
    d.line([(cx - W * 0.18, ly), (cx + W * 0.18, ly)], fill=ACCENT + (120,), width=s)
    d.text((cx, ly + int(W * 0.06)), "Thomas Schöpf", font=font(int(W * 0.055)), fill=WEAK + (220,), anchor="mm")

    return img.resize((w, h), Image.LANCZOS).convert("RGB")


def small(n):
    img = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    pad = round(n * 0.06)
    img.alpha_composite(ICON.resize((n - 2 * pad, n - 2 * pad), Image.LANCZOS), (pad, pad))
    return img


def back(w, h):
    img = Image.new("RGBA", (w, h), BG + (255,))
    img = Image.alpha_composite(img, glow((w, h), (int(w * 0.95), int(-h * 0.05)), int(w * 0.32), ACCENT, 38))
    img = Image.alpha_composite(img, glow((w, h), (int(w * 0.02), int(h * 1.05)), int(w * 0.22), (120, 80, 220), 22))
    return img.convert("RGB")


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    for w, h in [(202, 386), (269, 515), (336, 643), (430, 824), (534, 1022)]:
        side(w, h).save(OUT / f"side-{w}.png", optimize=True)
    for n in (58, 77, 97, 124, 159):
        small(n).save(OUT / f"small-{n}.png", optimize=True)
    back(1272, 922).save(OUT / "back.png", optimize=True)
    print("ok:", ", ".join(sorted(p.name for p in OUT.iterdir())))


if __name__ == "__main__":
    main()
