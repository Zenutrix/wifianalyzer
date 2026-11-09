//! Heatmap der Ausleuchtung auf dem Grundriss.
//!
//! Modell: Zwischen den Messpunkten wird per inverser Distanzgewichtung (IDW) interpoliert –
//! die Punkte beeinflussen sich gegenseitig. Entfernt man sich vom nächsten Messpunkt, nimmt das
//! Signal wie in der Funkausbreitung logarithmisch ab (Pfadverlust). Dadurch werden Bereiche
//! zwischen guten Punkten grün, Ränder und Lücken gelb/orange/rot. Unterhalb der untersten
//! Farbschwelle („kein Signal“) wird grau eingefärbt – Farbe einstellbar oder durchsichtig.
//! Messpunkte ganz ohne Wert (kein Empfang) zählen als `NO_SIGNAL_DBM`.
//!
//! Abstände rechnen in Metern, sobald der Maßstab gesetzt ist (sonst Annahme: Plan ist 40 m breit).

use std::sync::RwLock;

use image::{imageops, ImageFormat, Rgb, RgbImage, RgbaImage};

/// Deckkraft der Heatmap über dem Plan
const OPACITY: f32 = 0.62;
/// Annahme für die Planbreite ohne Maßstab (m)
pub const DEFAULT_WIDTH_M: f32 = 40.0;
/// Bis zu diesem Abstand (m) vom Messpunkt gilt der gemessene Wert ohne Abschlag
const TRUST_M: f32 = 2.5;
/// Pfadverlust-Exponent (Innenräume mit Wänden ≈ 3)
const PATH_EXP: f64 = 3.0;
/// Weiter als hier (m) vom nächsten Messpunkt wird nichts mehr eingefärbt
const MAX_RANGE_M: f32 = 30.0;

/// Wert für Messpunkte ohne jeden Empfang
pub const NO_SIGNAL_DBM: f64 = -100.0;
/// Standardfarbe für „kein Signal“
pub const DEFAULT_NO_SIGNAL: [u8; 3] = [150, 154, 162];

static NO_SIGNAL: RwLock<String> = RwLock::new(String::new());

/// Einstellung für „kein Signal“ setzen: leer = Grau, „none“ = durchsichtig, sonst rrggbb
pub fn set_no_signal(v: &str) {
    if let Ok(mut s) = NO_SIGNAL.write() {
        *s = v.trim().to_string();
    }
}

pub fn no_signal_setting() -> String {
    NO_SIGNAL.read().map(|s| s.clone()).unwrap_or_default()
}

/// Farbe für Bereiche ohne (ausreichendes) Signal; `None` = nicht einfärben
pub fn no_signal() -> Option<[u8; 3]> {
    let s = no_signal_setting();
    if s.eq_ignore_ascii_case("none") {
        return None;
    }
    if s.len() == 6 {
        if let Ok(v) = u32::from_str_radix(&s, 16) {
            return Some([(v >> 16) as u8, (v >> 8) as u8, v as u8]);
        }
    }
    Some(DEFAULT_NO_SIGNAL)
}

/// Farbschwelle: ab `dbm` (und stärker) diese Farbe
pub type Stop = (f64, [u8; 3]);

/// Standard-Farbskala (an gängige Survey-Tools angelehnt), stärkste zuerst
pub const DEFAULT_SCALE: [Stop; 7] = [
    (-45.0, [0, 176, 80]),
    (-55.0, [102, 204, 51]),
    (-62.0, [190, 225, 40]),
    (-67.0, [255, 230, 0]),
    (-72.0, [255, 160, 0]),
    (-78.0, [255, 80, 30]),
    (-85.0, [205, 20, 20]),
];

static SCALE: RwLock<Vec<Stop>> = RwLock::new(Vec::new());

/// Aktuelle Farbskala (absteigend nach dBm sortiert)
pub fn scale() -> Vec<Stop> {
    let s = SCALE.read().map(|s| s.clone()).unwrap_or_default();
    if s.len() < 2 {
        DEFAULT_SCALE.to_vec()
    } else {
        s
    }
}

/// Farbskala setzen (wird sortiert); leer = Standard
pub fn set_scale(mut stops: Vec<Stop>) {
    stops.sort_by(|a, b| b.0.total_cmp(&a.0));
    stops.dedup_by(|a, b| (a.0 - b.0).abs() < 0.01);
    if let Ok(mut s) = SCALE.write() {
        *s = stops;
    }
}

/// Skala als Text für die Einstellungen: "-45:00b050,-55:66cc33,…"
pub fn scale_to_string(stops: &[Stop]) -> String {
    stops.iter().map(|(d, c)| format!("{d:.0}:{:02x}{:02x}{:02x}", c[0], c[1], c[2])).collect::<Vec<_>>().join(",")
}

pub fn scale_from_string(s: &str) -> Vec<Stop> {
    s.split(',')
        .filter_map(|part| {
            let (d, c) = part.trim().split_once(':')?;
            let d: f64 = d.trim().parse().ok()?;
            let c = c.trim();
            if c.len() != 6 {
                return None;
            }
            let v = u32::from_str_radix(c, 16).ok()?;
            Some((d, [(v >> 16) as u8, (v >> 8) as u8, v as u8]))
        })
        .collect()
}

/// Farbe für einen Signalwert; `None` = schwächer als die unterste Schwelle (nicht einfärben)
pub fn color_opt(rssi: f64) -> Option<[u8; 3]> {
    let s = scale();
    if rssi >= s[0].0 {
        return Some(s[0].1);
    }
    for w in s.windows(2) {
        let ((a, ca), (b, cb)) = (w[0], w[1]);
        if rssi >= b {
            let t = (a - rssi) / (a - b);
            return Some([0, 1, 2].map(|i| (ca[i] as f64 + (cb[i] as f64 - ca[i] as f64) * t).round() as u8));
        }
    }
    None
}

/// Farbe für einen Signalwert (unterhalb der Skala: „kein Signal“-Farbe bzw. unterste Schwelle)
pub fn color(rssi: f64) -> [u8; 3] {
    color_opt(rssi).or_else(no_signal).unwrap_or_else(|| scale().last().map(|s| s.1).unwrap_or([200, 0, 0]))
}

/// Unterste Schwelle der Skala (dBm)
pub fn scale_min() -> f64 {
    scale().last().map(|s| s.0).unwrap_or(-85.0)
}

pub fn scale_max() -> f64 {
    scale().first().map(|s| s.0).unwrap_or(-45.0)
}

/// Geschätztes Signal an einer Stelle (Plan-Koordinaten 0..1). `w_m`/`h_m` = Planmaße in Metern.
/// Liefert (Wert, Abstand zum nächsten Messpunkt in m).
pub fn estimate(points: &[([f32; 2], f64)], px: f32, py: f32, w_m: f32, h_m: f32) -> Option<(f64, f32)> {
    if points.is_empty() {
        return None;
    }
    let (mut sw, mut sv, mut dmin) = (0.0f64, 0.0f64, f32::MAX);
    for (p, v) in points {
        let dx = (px - p[0]) * w_m;
        let dy = (py - p[1]) * h_m;
        let d2 = dx * dx + dy * dy;
        dmin = dmin.min(d2.sqrt());
        // gegenseitige Beeinflussung: nahe Punkte zählen stark, ferne schwach
        let wgt = 1.0 / (d2 as f64 + 0.25);
        sw += wgt;
        sv += wgt * v;
    }
    let idw = sv / sw;
    // jenseits des Vertrauensradius: Pfadverlust ab dem nächsten Messpunkt
    let extra = (dmin - TRUST_M).max(0.0) / TRUST_M;
    let loss = 10.0 * PATH_EXP * (1.0 + extra as f64).log10();
    Some((idw - loss, dmin))
}

/// Heatmap als RGBA-Raster (`w` × `h`). `points`: Position (0..1 relativ zum Plan) und dBm.
/// `aspect` = Planbreite / Planhöhe, `width_m` = reale Breite des Plans (Maßstab).
pub fn field(points: &[([f32; 2], f64)], w: usize, h: usize, aspect: f32, width_m: Option<f32>) -> Vec<[u8; 4]> {
    let mut out = vec![[0u8; 4]; w * h];
    if points.is_empty() || w == 0 || h == 0 {
        return out;
    }
    let w_m = width_m.filter(|v| *v > 0.5).unwrap_or(DEFAULT_WIDTH_M);
    let h_m = w_m / aspect.max(0.01);
    let min = scale_min();
    let nosig = no_signal();
    let lowest = scale().last().map(|s| s.1).unwrap_or([200, 0, 0]);
    for y in 0..h {
        let py = (y as f32 + 0.5) / h as f32;
        for x in 0..w {
            let px = (x as f32 + 0.5) / w as f32;
            let Some((v, dmin)) = estimate(points, px, py, w_m, h_m) else { continue };
            if dmin > MAX_RANGE_M {
                continue;
            }
            match (color_opt(v), nosig) {
                (Some([r, g, b]), Some(_)) => out[y * w + x] = [r, g, b, (OPACITY * 255.0) as u8],
                (Some([r, g, b]), None) => {
                    // durchsichtig: weich auslaufen in den letzten 3 dB über der untersten Schwelle
                    let edge = ((v - min) / 3.0).clamp(0.0, 1.0) as f32;
                    out[y * w + x] = [r, g, b, (OPACITY * (0.35 + 0.65 * edge) * 255.0) as u8];
                }
                (None, Some(grey)) => {
                    // weicher Übergang von der untersten Farbe zu „kein Signal“ über 4 dB
                    let t = ((min - v) / 4.0).clamp(0.0, 1.0);
                    let c = [0, 1, 2].map(|i| (lowest[i] as f64 + (grey[i] as f64 - lowest[i] as f64) * t).round() as u8);
                    let a = OPACITY * (1.0 - 0.1 * t as f32);
                    out[y * w + x] = [c[0], c[1], c[2], (a * 255.0) as u8];
                }
                (None, None) => {}
            }
        }
    }
    out
}

/// Grundriss dekodieren (PNG/JPEG/BMP …)
pub fn decode_plan(bytes: &[u8]) -> Result<RgbaImage, String> {
    image::load_from_memory(bytes).map(|i| i.to_rgba8()).map_err(|e| e.to_string())
}

/// Grundriss beim Laden verkleinern/umkodieren, damit Messungsdateien handlich bleiben
/// (max. 2400 px Kantenlänge; große Fotos als JPEG)
pub fn prepare_plan(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), String> {
    let img = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
    let (w, h) = (img.width(), img.height());
    if w < 50 || h < 50 {
        return Err(crate::t!("Bild ist zu klein", "Image is too small").into());
    }
    let max = 2400;
    if w <= max && h <= max && bytes.len() <= 3_000_000 {
        return Ok((bytes.to_vec(), w, h));
    }
    let img = if w > max || h > max { img.resize(max, max, imageops::FilterType::Lanczos3) } else { img };
    let mut out = Vec::new();
    img.to_rgb8()
        .write_to(&mut std::io::Cursor::new(&mut out), ImageFormat::Jpeg)
        .map_err(|e| e.to_string())?;
    Ok((out, img.width(), img.height()))
}

/// Plan + Heatmap als ein Bild (für Bericht und PDF), Breite höchstens `max_w`
pub fn composite(plan: &RgbaImage, points: &[([f32; 2], f64)], max_w: u32, width_m: Option<f32>) -> RgbImage {
    let (pw, ph) = plan.dimensions();
    let scale = (max_w as f32 / pw as f32).min(1.0);
    let (w, h) = (((pw as f32 * scale) as u32).max(1), ((ph as f32 * scale) as u32).max(1));
    let base = if scale < 1.0 { imageops::resize(plan, w, h, imageops::FilterType::Triangle) } else { plan.clone() };
    // Heatmap in reduzierter Auflösung rechnen und weich hochskalieren
    let (fw, fh) = ((w / 4).max(1), (h / 4).max(1));
    let f = field(points, fw as usize, fh as usize, w as f32 / h as f32, width_m);
    let mut small = RgbaImage::new(fw, fh);
    for (i, px) in f.iter().enumerate() {
        small.put_pixel(i as u32 % fw, i as u32 / fw, image::Rgba(*px));
    }
    let heat = imageops::resize(&small, w, h, imageops::FilterType::Triangle);
    let mut out = RgbImage::new(w, h);
    for (x, y, p) in base.enumerate_pixels() {
        // transparente Bereiche des Plans auf Weiß
        let a = p[3] as f32 / 255.0;
        let bg = [0, 1, 2].map(|i| p[i] as f32 * a + 255.0 * (1.0 - a));
        let hp = heat.get_pixel(x, y);
        let ha = hp[3] as f32 / 255.0;
        let c = [0, 1, 2].map(|i| (bg[i] * (1.0 - ha) + hp[i] as f32 * ha).round() as u8);
        out.put_pixel(x, y, Rgb(c));
    }
    out
}

/// JPEG-Kodierung (für PDF und HTML-Bericht)
pub fn to_jpeg(img: &RgbImage) -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 88);
    let _ = enc.encode_image(img);
    out
}

/// „Runde“ Länge für eine Maßstabsleiste (m), etwa `target` lang
pub fn nice_length(target: f32) -> f32 {
    [0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 25.0, 50.0, 100.0, 200.0].into_iter().rev().find(|v| *v <= target).unwrap_or(0.5)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_colors_and_cutoff() {
        set_scale(Vec::new());
        assert_eq!(color(-40.0), [0, 176, 80]);
        assert_eq!(color_opt(-95.0), None);
        let mid = color(-69.5);
        assert!(mid[1] > 160 && mid[1] < 230);
        let txt = scale_to_string(&DEFAULT_SCALE);
        assert_eq!(scale_from_string(&txt), DEFAULT_SCALE.to_vec());
    }

    #[test]
    fn points_influence_each_other_and_signal_decays() {
        set_scale(Vec::new());
        // zwei gute Punkte 8 m auseinander, Plan 40 m breit
        let pts = [([0.40f32, 0.5f32], -45.0), ([0.60, 0.5], -45.0)];
        let (mid, _) = estimate(&pts, 0.5, 0.5, 40.0, 20.0).unwrap();
        assert!(mid > -55.0, "zwischen zwei guten Punkten bleibt es gut: {mid}");
        let (far, _) = estimate(&pts, 0.95, 0.5, 40.0, 20.0).unwrap();
        assert!(far < -62.0, "weit weg schwächer: {far}");
        // guter und schlechter Punkt: Mitte liegt dazwischen
        let pts2 = [([0.45f32, 0.5f32], -45.0), ([0.55, 0.5], -80.0)];
        let (m2, _) = estimate(&pts2, 0.5, 0.5, 40.0, 20.0).unwrap();
        assert!(m2 < -50.0 && m2 > -80.0);
    }

    #[test]
    fn field_transparent_far_away() {
        set_scale(Vec::new());
        let f = field(&[([0.1f32, 0.5f32], -60.0)], 50, 10, 4.0, Some(40.0));
        assert!(f[5 * 50 + 5][3] > 0);
        assert_eq!(f[5 * 50 + 49][3], 0, "36 m entfernt: nicht eingefärbt");
    }

    #[test]
    fn composite_keeps_size_and_tints() {
        set_scale(Vec::new());
        let plan = RgbaImage::from_pixel(200, 100, image::Rgba([255, 255, 255, 255]));
        let img = composite(&plan, &[([0.5, 0.5], -45.0)], 1000, Some(20.0));
        assert_eq!(img.dimensions(), (200, 100));
        let c = img.get_pixel(100, 50);
        assert!(c[1] > c[0], "Mitte grünlich");
        assert!(!to_jpeg(&img).is_empty());
        assert_eq!(nice_length(7.0), 5.0);
    }
}
