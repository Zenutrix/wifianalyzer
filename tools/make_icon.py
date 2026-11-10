"""Erzeugt das App-Logo: assets/icon.ico, assets/icon.png, assets/icon_preview.png,
assets/logo_tile.png (nur die Kachel – darauf baut der Ladebildschirm das Logo auf)
sowie website/logo.svg und website/icon.png.

Aufruf:  python tools/make_icon.py

Motiv: ein „W“ als durchgehende Linie im Verlauf Cyan -> Indigo -> Magenta; die mittlere
Spitze ist die Antenne, aus der zwei WLAN-Wellen aufsteigen. Gezeichnet wird in 2048 px und
für jede Icon-Größe herunterskaliert; bis 32 px eine kräftigere Variante mit einer Welle.

Die Geometrie muss zum Ladebildschirm passen: src/app/logo.rs (Werte in 1/1024).
"""
import math
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter

S = 1024
SS = 2
ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "assets"
WEB = ROOT / "website"

# --- Geometrie (1/1024 der Kachel), identisch in src/app/logo.rs
W_PTS = [(196, 318), (346, 716), (512, 452), (678, 716), (828, 318)]
W_WIDTH = 118
APEX = (512, 452)
WAVES = [(170, 54), (262, 54)]  # (Radius, Linienbreite)
WAVE_SPAN = (240, 300)  # Grad, 270 = oben
SMALL = dict(w_width=150, waves=[(205, 74)])

TILE_STOPS = [(0.0, (15, 23, 42)), (1.0, (30, 27, 75))]
INK_STOPS = [(0.0, (34, 211, 238)), (0.5, (99, 102, 241)), (1.0, (217, 70, 239))]


def lerp(a, b, t):
    return tuple(int(round(a[i] + (b[i] - a[i]) * t)) for i in range(len(a)))


def stops_at(stops, t):
    t = min(1.0, max(0.0, t))
    for i in range(len(stops) - 1):
        p0, c0 = stops[i]
        p1, c1 = stops[i + 1]
        if p0 <= t <= p1:
            return lerp(c0, c1, (t - p0) / (p1 - p0))
    return stops[-1][1]


def gradient(n, stops, x0=0.0, x1=1.0):
    """Diagonaler Verlauf; t läuft von x0 bis x1 über die Kachel."""
    s = 256
    img = Image.new("RGB", (s, s))
    px = img.load()
    for y in range(s):
        for x in range(s):
            u = (x + y) / (2 * (s - 1))
            px[x, y] = stops_at(stops, (u - x0) / (x1 - x0))
    return img.resize((n, n), Image.BICUBIC).convert("RGBA")


def squircle_mask(n):
    m = Image.new("L", (n, n), 0)
    ImageDraw.Draw(m).rounded_rectangle([0, 0, n - 1, n - 1], radius=int(n * 0.235), fill=255)
    return m


def tile(n):
    mask = squircle_mask(n)
    img = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    img.paste(gradient(n, TILE_STOPS), (0, 0), mask)
    halo = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    k = n / S
    cx, cy, r = 512 * k, 470 * k, 380 * k
    ImageDraw.Draw(halo).ellipse([cx - r, cy - r, cx + r, cy + r], fill=(99, 102, 241, 70))
    halo = halo.filter(ImageFilter.GaussianBlur(n * 0.09))
    img = Image.alpha_composite(img, Image.composite(halo, Image.new("RGBA", (n, n)), mask))
    edge = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    ImageDraw.Draw(edge).rounded_rectangle([1, 1, n - 2, n - 2], radius=int(n * 0.235), outline=(255, 255, 255, 34), width=max(1, int(n * 0.005)))
    return Image.alpha_composite(img, edge)


def ink_mask(n, small=False):
    k = n / S
    w = (SMALL["w_width"] if small else W_WIDTH) * k
    m = Image.new("L", (n, n), 0)
    d = ImageDraw.Draw(m)
    pts = [(x * k, y * k) for x, y in W_PTS]
    d.line(pts, fill=255, width=int(w), joint="curve")
    for p in pts:
        d.ellipse([p[0] - w / 2, p[1] - w / 2, p[0] + w / 2, p[1] + w / 2], fill=255)
    ax, ay = APEX[0] * k, APEX[1] * k
    for r, ww in (SMALL["waves"] if small else WAVES):
        r, ww = r * k, ww * k
        a0, a1 = WAVE_SPAN
        d.arc([ax - r, ay - r, ax + r, ay + r], start=a0, end=a1, fill=255, width=int(ww))
        for a in (a0, a1):
            rr = r - ww / 2
            e = (ax + rr * math.cos(math.radians(a)), ay + rr * math.sin(math.radians(a)))
            d.ellipse([e[0] - ww / 2, e[1] - ww / 2, e[0] + ww / 2, e[1] + ww / 2], fill=255)
    return m


def render(small=False):
    n = S * SS
    base = tile(n)
    mask = ink_mask(n, small)
    ink = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    ink.paste(gradient(n, INK_STOPS, 0.18, 0.82), (0, 0), mask)
    glow = ink.filter(ImageFilter.GaussianBlur(n * 0.03))
    glow.putalpha(ImageChops.multiply(glow.getchannel("A"), Image.new("L", (n, n), 150)))
    base = Image.alpha_composite(base, Image.composite(glow, Image.new("RGBA", (n, n)), squircle_mask(n)))
    base = Image.alpha_composite(base, ink)
    return base.resize((S, S), Image.LANCZOS)


def svg():
    """Logo als SVG (Website) mit Klassen für die Aufbau-Animation per CSS."""
    pts = " ".join(f"{x},{y}" for x, y in W_PTS)
    ax, ay = APEX
    waves = []
    for i, (r, w) in enumerate(WAVES):
        a0, a1 = (math.radians(a) for a in WAVE_SPAN)
        x0, y0 = ax + r * math.cos(a0), ay + r * math.sin(a0)
        x1, y1 = ax + r * math.cos(a1), ay + r * math.sin(a1)
        waves.append(f'<path class="lg-wave lg-wave{i + 1}" d="M{x0:.1f} {y0:.1f} A{r} {r} 0 0 1 {x1:.1f} {y1:.1f}" stroke-width="{w}" pathLength="1"/>')
    ink = "".join(f'<stop offset="{0.18 + p * 0.64:.2f}" stop-color="rgb{c}"/>' for p, c in INK_STOPS)
    bg = "".join(f'<stop offset="{p}" stop-color="rgb{c}"/>' for p, c in TILE_STOPS)
    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" role="img" aria-label="WiFi Analyzer">
<defs>
<linearGradient id="lg-bg" x1="0" y1="0" x2="1" y2="1">{bg}</linearGradient>
<linearGradient id="lg-ink" gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="1024" y2="1024">{ink}</linearGradient>
<radialGradient id="lg-halo" cx=".5" cy=".46" r=".42"><stop offset="0" stop-color="#6366f1" stop-opacity=".28"/><stop offset="1" stop-color="#6366f1" stop-opacity="0"/></radialGradient>
<filter id="lg-glow" x="-20%" y="-20%" width="140%" height="140%"><feGaussianBlur stdDeviation="28"/></filter>
</defs>
<rect class="lg-tile" width="1024" height="1024" rx="240" fill="url(#lg-bg)"/>
<rect class="lg-tile" width="1024" height="1024" rx="240" fill="url(#lg-halo)"/>
<g fill="none" stroke="url(#lg-ink)" stroke-linecap="round" stroke-linejoin="round">
<g class="lg-glow" filter="url(#lg-glow)" opacity=".55"><polyline points="{pts}" stroke-width="{W_WIDTH}"/>{"".join(waves).replace('class="lg-wave', 'class="lg-wglow')}</g>
<polyline class="lg-w" points="{pts}" stroke-width="{W_WIDTH}" pathLength="1"/>
{"".join(waves)}
</g>
</svg>
'''


def main():
    OUT.mkdir(exist_ok=True)
    big = render(False)
    small = render(True)
    sizes = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256]
    frames = [(small if s <= 32 else big).resize((s, s), Image.LANCZOS) for s in sizes]
    frames[-1].save(OUT / "icon.ico", format="ICO", sizes=[(s, s) for s in sizes], append_images=frames[:-1])
    big.resize((256, 256), Image.LANCZOS).save(OUT / "icon.png")
    big.resize((512, 512), Image.LANCZOS).save(OUT / "icon_preview.png")
    tile(512).resize((256, 256), Image.LANCZOS).save(OUT / "logo_tile.png")
    big.resize((256, 256), Image.LANCZOS).save(WEB / "icon.png")
    (WEB / "logo.svg").write_text(svg(), encoding="utf-8")
    strip = Image.new("RGBA", (16 + 24 + 32 + 48 + 64 + 6 * 12, 76), (13, 17, 23, 255))
    x = 12
    for s in (16, 24, 32, 48, 64):
        strip.alpha_composite(frames[sizes.index(s)], (x, 6))
        x += s + 12
    (ROOT / "target").mkdir(exist_ok=True)
    strip.save(ROOT / "target" / "icon_sizes.png")
    print("Logo geschrieben:", OUT / "icon.ico")


if __name__ == "__main__":
    main()
