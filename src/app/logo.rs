//! Logo im Ladebildschirm: baut sich aus Kachel, W-Linie und WLAN-Wellen auf.
//! Geometrie und Farben wie in tools/make_icon.py (Werte in 1/1024 der Kachel).

use eframe::egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, TextureId, Vec2};

const W_PTS: [(f32, f32); 5] = [(196.0, 318.0), (346.0, 716.0), (512.0, 452.0), (678.0, 716.0), (828.0, 318.0)];
const W_WIDTH: f32 = 118.0;
const APEX: (f32, f32) = (512.0, 452.0);
const WAVES: [(f32, f32); 2] = [(170.0, 54.0), (262.0, 54.0)];
/// Winkelbereich der Wellen (Grad, 270 = oben)
const SPAN: (f32, f32) = (240.0, 300.0);
const INK: [(f32, [u8; 3]); 3] = [(0.0, [34, 211, 238]), (0.5, [99, 102, 241]), (1.0, [217, 70, 239])];

/// Zeitpunkte des Aufbaus (Sekunden)
const T_LINE: (f32, f32) = (0.35, 0.8);
const T_WAVES: f32 = 1.15;
/// Ab hier ist das Logo vollständig (danach nur noch die pulsierenden Wellen)
pub const BUILT: f32 = 1.9;

fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

fn ease_out(x: f32) -> f32 {
    1.0 - (1.0 - clamp01(x)).powi(3)
}

fn ease_in_out(x: f32) -> f32 {
    let x = clamp01(x);
    if x < 0.5 {
        4.0 * x * x * x
    } else {
        1.0 - (-2.0 * x + 2.0).powi(3) / 2.0
    }
}

/// leichtes Überschwingen (für das „Aufploppen“)
fn back_out(x: f32) -> f32 {
    let x = clamp01(x) - 1.0;
    let c = 1.70158;
    1.0 + (c + 1.0) * x * x * x + c * x * x
}

/// Farbe des Verlaufs an einer Stelle der Kachel (diagonal, wie im Icon)
fn ink_at(x: f32, y: f32) -> Color32 {
    let t = clamp01(((x + y) / 2048.0 - 0.18) / 0.64);
    let (mut a, mut b, mut f) = (INK[0].1, INK[0].1, 0.0);
    for w in INK.windows(2) {
        if t >= w[0].0 && t <= w[1].0 {
            a = w[0].1;
            b = w[1].1;
            f = (t - w[0].0) / (w[1].0 - w[0].0);
        }
    }
    let m = |i: usize| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * f).round() as u8;
    Color32::from_rgb(m(0), m(1), m(2))
}

/// Linie mit Farbverlauf zeichnen (Stücke + runde Gelenke, deckend – keine Überlagerungen)
fn ink_path(p: &Painter, pts: &[(f32, f32)], width: f32, at: &impl Fn(f32, f32) -> Pos2, k: f32, alpha: f32) {
    let w = width * k;
    for pair in pts.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let c = ink_at((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0).gamma_multiply(alpha);
        p.line_segment([at(a.0, a.1), at(b.0, b.1)], Stroke::new(w, c));
        p.circle_filled(at(b.0, b.1), w / 2.0, c);
    }
    if let Some(f) = pts.first() {
        p.circle_filled(at(f.0, f.1), w / 2.0, ink_at(f.0, f.1).gamma_multiply(alpha));
    }
}

/// Punkte entlang des W bis zum Anteil `frac` (0…1) der Gesamtlänge
fn w_points(frac: f32) -> Vec<(f32, f32)> {
    let segs: Vec<f32> = W_PTS.windows(2).map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt()).collect();
    let total: f32 = segs.iter().sum();
    let mut left = total * clamp01(frac);
    let mut out = vec![W_PTS[0]];
    for (i, len) in segs.iter().enumerate() {
        let (a, b) = (W_PTS[i], W_PTS[i + 1]);
        let n = (len / 8.0).ceil() as usize;
        let take = left.min(*len);
        let steps = ((take / len) * n as f32).ceil() as usize;
        for s in 1..=steps {
            let f = (s as f32 / n as f32).min(take / len);
            out.push((a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f));
        }
        left -= take;
        if left <= 0.0 {
            break;
        }
    }
    out
}

/// Punkte eines Wellenbogens um die Spitze, Winkel in Grad
fn arc_points(r: f32, a0: f32, a1: f32) -> Vec<(f32, f32)> {
    let n = (((a1 - a0).abs() / 2.0).ceil() as usize).max(2);
    (0..=n)
        .map(|i| {
            let a = (a0 + (a1 - a0) * i as f32 / n as f32).to_radians();
            (APEX.0 + r * a.cos(), APEX.1 + r * a.sin())
        })
        .collect()
}

/// Logo in `rect` (quadratisch) zum Zeitpunkt `t` (Sekunden seit Start) zeichnen
pub fn draw(p: &Painter, rect: Rect, t: f32, tile: Option<TextureId>) {
    let appear = ease_out(t / 0.45);
    if appear <= 0.0 {
        return;
    }
    // 1. Kachel ploppt auf
    let scale = 0.82 + 0.18 * back_out(t / 0.55);
    let tile_rect = Rect::from_center_size(rect.center(), rect.size() * scale);
    let k = tile_rect.width() / 1024.0;
    let at = |x: f32, y: f32| tile_rect.min + Vec2::new(x * k, y * k);
    if let Some(tex) = tile {
        p.image(tex, tile_rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE.gamma_multiply(appear));
    }
    let clip = p.with_clip_rect(tile_rect.shrink(k * 6.0));

    // 2. das W zeichnet sich als Linie (mit leuchtender Spitze)
    let line = ease_in_out((t - T_LINE.0) / T_LINE.1);
    if line > 0.0 {
        let pts = w_points(line);
        // weicher Schein darunter
        let glow: Vec<Pos2> = pts.iter().map(|q| at(q.0, q.1)).collect();
        clip.add(Shape::line(glow, Stroke::new(W_WIDTH * k * 1.7, Color32::from_rgb(99, 102, 241).gamma_multiply(0.16))));
        ink_path(&clip, &pts, W_WIDTH, &at, k, 1.0);
        if line < 1.0 {
            if let Some(tip) = pts.last() {
                clip.circle_filled(at(tip.0, tip.1), W_WIDTH * k * 0.32, Color32::from_white_alpha(200));
            }
        }
    }

    // 3. Aufblitzen an der Spitze, dann die Wellen nacheinander
    let flash = (t - (T_LINE.0 + T_LINE.1)) / 0.5;
    if (0.0..1.0).contains(&flash) {
        let r = (60.0 + 220.0 * ease_out(flash)) * k;
        clip.circle_stroke(at(APEX.0, APEX.1), r, Stroke::new(10.0 * k, Color32::from_rgb(165, 180, 252).gamma_multiply((1.0 - flash) * 0.6)));
    }
    for (i, (r, w)) in WAVES.iter().enumerate() {
        let f = back_out((t - T_WAVES - 0.2 * i as f32) / 0.4);
        if f <= 0.0 {
            continue;
        }
        let half = (SPAN.1 - SPAN.0) / 2.0 * f.min(1.08);
        ink_path(&clip, &arc_points(*r, 270.0 - half, 270.0 + half), *w, &at, k, clamp01(f * 1.5));
    }

    // 4. danach: Wellen laufen weiter nach außen (Funksignal)
    if t > BUILT {
        for j in 0..2 {
            let ph = ((t - BUILT) / 1.8 + j as f32 * 0.5) % 1.0;
            let r = WAVES[1].0 + 40.0 + ph * 220.0;
            let a = (1.0 - ph) * 0.5 * ease_out((t - BUILT) / 0.6);
            let pts: Vec<Pos2> = arc_points(r, SPAN.0 + 4.0, SPAN.1 - 4.0).iter().map(|q| at(q.0, q.1)).collect();
            clip.add(Shape::line(pts, Stroke::new(WAVES[1].1 * k * (1.0 - ph * 0.6), Color32::from_rgb(129, 140, 248).gamma_multiply(a))));
        }
    }
}
