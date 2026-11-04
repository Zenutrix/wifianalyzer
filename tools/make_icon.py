"""Erzeugt das App-Icon (assets/icon.ico + assets/icon.png).

Aufruf:  python tools/make_icon.py
Gezeichnet wird in 1024 px und dann für jede Icon-Größe sauber herunterskaliert.
Kleine Größen (<= 32 px) bekommen eine vereinfachte Variante ohne feine Details.
"""
import math
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter

S = 1024
ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "assets"


def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(len(a)))


def gradient(size, stops):
    """Diagonaler Farbverlauf (oben links -> unten rechts)."""
    img = Image.new("RGBA", (size, size))
    px = img.load()
    for y in range(size):
        for x in range(size):
            t = (x + y) / (2 * (size - 1))
            for i in range(len(stops) - 1):
                p0, c0 = stops[i]
                p1, c1 = stops[i + 1]
                if p0 <= t <= p1:
                    px[x, y] = lerp(c0, c1, (t - p0) / (p1 - p0)) + (255,)
                    break
    return img


def squircle_mask(size, radius_frac=0.235, inset=0.0):
    m = Image.new("L", (size, size), 0)
    d = ImageDraw.Draw(m)
    i = int(size * inset)
    d.rounded_rectangle([i, i, size - 1 - i, size - 1 - i], radius=int(size * radius_frac), fill=255)
    return m


def wifi_layer(size, color, cx, cy, radii, width, dot_r):
    layer = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    for r in radii:
        box = [cx - r, cy - r, cx + r, cy + r]
        d.arc(box, start=225, end=315, fill=color, width=width)
        # runde Enden
        for ang in (225, 315):
            a = math.radians(ang)
            rr = r - width / 2
            ex, ey = cx + rr * math.cos(a), cy + rr * math.sin(a)
            d.ellipse([ex - width / 2, ey - width / 2, ex + width / 2, ey + width / 2], fill=color)
    d.ellipse([cx - dot_r, cy - dot_r, cx + dot_r, cy + dot_r], fill=color)
    return layer


def render(detailed=True):
    bg = gradient(S, [(0.0, (14, 14, 52)), (0.5, (38, 62, 190)), (1.0, (0, 200, 235))])
    mask = squircle_mask(S)
    icon = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    icon.paste(bg, (0, 0), mask)

    cx, cy = S * 0.5, S * 0.70

    if detailed:
        # Radar-Ringe
        rings = Image.new("RGBA", (S, S), (0, 0, 0, 0))
        d = ImageDraw.Draw(rings)
        for k, r in enumerate([0.50, 0.64, 0.78]):
            rr = S * r
            d.ellipse([cx - rr, cy - rr, cx + rr, cy + rr], outline=(160, 225, 255, 70 - k * 18), width=int(S * 0.006))
        # Radar-Sweep (Kegel mit auslaufender Transparenz)
        sweep = Image.new("RGBA", (S, S), (0, 0, 0, 0))
        sd = ImageDraw.Draw(sweep)
        R = S * 0.95
        steps = 60
        for i in range(steps):
            a0 = 250 + i * (60 / steps)
            alpha = int(110 * (i / steps) ** 2)
            sd.pieslice([cx - R, cy - R, cx + R, cy + R], start=a0, end=a0 + 60 / steps + 0.5, fill=(140, 240, 255, alpha))
        rings = Image.alpha_composite(rings, sweep)
        icon = Image.alpha_composite(icon, Image.composite(rings, Image.new("RGBA", (S, S)), mask))

        # weicher Glanz oben
        gloss = Image.new("RGBA", (S, S), (255, 255, 255, 0))
        ga = Image.linear_gradient("L").resize((S, S)).point(lambda v: int(max(0, 255 - v * 2.2) * 0.22))
        gloss.putalpha(ga)
        icon = Image.alpha_composite(icon, Image.composite(gloss, Image.new("RGBA", (S, S)), mask))

    radii = [S * 0.155, S * 0.285, S * 0.415] if detailed else [S * 0.17, S * 0.33]
    width = int(S * (0.078 if detailed else 0.115))
    dot_r = S * (0.065 if detailed else 0.085)

    # Glow unter den Bögen
    glow = wifi_layer(S, (120, 230, 255, 255), cx, cy, radii, width, dot_r).filter(ImageFilter.GaussianBlur(S * 0.03))
    glow.putalpha(ImageChops.multiply(glow.getchannel("A"), Image.new("L", (S, S), 230)))
    icon = Image.alpha_composite(icon, Image.composite(glow, Image.new("RGBA", (S, S)), mask))

    # leichter Schatten + weiße Bögen
    shadow = wifi_layer(S, (10, 20, 60, 110), cx, cy + S * 0.012, radii, width, dot_r).filter(ImageFilter.GaussianBlur(S * 0.008))
    icon = Image.alpha_composite(icon, shadow)
    icon = Image.alpha_composite(icon, wifi_layer(S, (255, 255, 255, 255), cx, cy, radii, width, dot_r))

    if detailed:
        # grüner "Live"-Punkt oben rechts
        bx, by, br = S * 0.79, S * 0.21, S * 0.055
        halo = Image.new("RGBA", (S, S), (0, 0, 0, 0))
        ImageDraw.Draw(halo).ellipse([bx - br * 2.2, by - br * 2.2, bx + br * 2.2, by + br * 2.2], fill=(70, 255, 150, 120))
        icon = Image.alpha_composite(icon, halo.filter(ImageFilter.GaussianBlur(S * 0.03)))
        d = ImageDraw.Draw(icon)
        d.ellipse([bx - br, by - br, bx + br, by + br], fill=(80, 240, 140, 255))
        d.ellipse([bx - br * 0.45, by - br * 0.6, bx + br * 0.15, by - br * 0.05], fill=(220, 255, 230, 200))

    # feine Kante
    edge = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    ImageDraw.Draw(edge).rounded_rectangle([2, 2, S - 3, S - 3], radius=int(S * 0.235), outline=(255, 255, 255, 50), width=int(S * 0.006))
    icon = Image.alpha_composite(icon, edge)
    return icon


def main():
    OUT.mkdir(exist_ok=True)
    big = render(True)
    small = render(False)
    sizes = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256]
    frames = [(small if s <= 32 else big).resize((s, s), Image.LANCZOS) for s in sizes]
    frames[-1].save(OUT / "icon.ico", format="ICO", sizes=[(s, s) for s in sizes], append_images=frames[:-1])
    big.resize((256, 256), Image.LANCZOS).save(OUT / "icon.png")
    big.resize((512, 512), Image.LANCZOS).save(OUT / "icon_preview.png")
    print("Icon geschrieben:", OUT / "icon.ico")


if __name__ == "__main__":
    main()
