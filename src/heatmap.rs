//! Heatmap der Ausleuchtung auf dem Grundriss.
//!
//! Zwischen den Messpunkten wird per inverser Distanzgewichtung (IDW) interpoliert. Weit weg von
//! jedem Messpunkt blendet die Farbe aus – dort wurde nicht gemessen, also wird auch nichts
//! behauptet.

use image::{imageops, ImageFormat, Rgb, RgbImage, RgbaImage};

/// Ab diesem Abstand (in Planhöhen) zum nächsten Messpunkt beginnt die Farbe auszublenden …
const FADE_START: f32 = 0.10;
/// … und ist hier ganz weg
const FADE_END: f32 = 0.32;
/// Deckkraft der Heatmap über dem Plan
const OPACITY: f32 = 0.58;

/// Farbe für einen Signalwert (dBm), stufenlos von Grün über Gelb nach Rot
pub fn color(rssi: f64) -> [u8; 3] {
    const STOPS: [(f64, [f64; 3]); 6] = [
        (-50.0, [22.0, 163.0, 74.0]),
        (-60.0, [63.0, 185.0, 80.0]),
        (-67.0, [163.0, 205.0, 57.0]),
        (-72.0, [234.0, 179.0, 8.0]),
        (-78.0, [245.0, 115.0, 22.0]),
        (-85.0, [220.0, 38.0, 38.0]),
    ];
    if rssi >= STOPS[0].0 {
        return STOPS[0].1.map(|c| c as u8);
    }
    for w in STOPS.windows(2) {
        let ((a, ca), (b, cb)) = (w[0], w[1]);
        if rssi >= b {
            let t = (a - rssi) / (a - b);
            return [0, 1, 2].map(|i| (ca[i] + (cb[i] - ca[i]) * t).round() as u8);
        }
    }
    STOPS[5].1.map(|c| c as u8)
}

/// Heatmap als RGBA-Raster (`w` × `h`). `points`: Position (0..1 relativ zum Plan) und dBm.
/// `aspect` = Planbreite / Planhöhe, damit Abstände in beiden Richtungen gleich zählen.
pub fn field(points: &[([f32; 2], f64)], w: usize, h: usize, aspect: f32) -> Vec<[u8; 4]> {
    let mut out = vec![[0u8; 4]; w * h];
    if points.is_empty() || w == 0 || h == 0 {
        return out;
    }
    for y in 0..h {
        let py = (y as f32 + 0.5) / h as f32;
        for x in 0..w {
            let px = (x as f32 + 0.5) / w as f32;
            let (mut sw, mut sv, mut dmin) = (0.0f64, 0.0f64, f32::MAX);
            let mut exact = None;
            for (p, v) in points {
                let dx = (px - p[0]) * aspect;
                let dy = py - p[1];
                let d2 = dx * dx + dy * dy;
                dmin = dmin.min(d2.sqrt());
                if d2 < 1e-8 {
                    exact = Some(*v);
                    break;
                }
                let wgt = 1.0 / d2 as f64;
                sw += wgt;
                sv += wgt * v;
            }
            let v = exact.unwrap_or(sv / sw);
            let fade = if dmin <= FADE_START {
                1.0
            } else if dmin >= FADE_END {
                0.0
            } else {
                let t = (dmin - FADE_START) / (FADE_END - FADE_START);
                1.0 - t * t * (3.0 - 2.0 * t) // weich ausblenden
            };
            let [r, g, b] = color(v);
            out[y * w + x] = [r, g, b, (fade * OPACITY * 255.0) as u8];
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
pub fn composite(plan: &RgbaImage, points: &[([f32; 2], f64)], max_w: u32) -> RgbImage {
    let (pw, ph) = plan.dimensions();
    let scale = (max_w as f32 / pw as f32).min(1.0);
    let (w, h) = (((pw as f32 * scale) as u32).max(1), ((ph as f32 * scale) as u32).max(1));
    let base = if scale < 1.0 { imageops::resize(plan, w, h, imageops::FilterType::Triangle) } else { plan.clone() };
    // Heatmap in reduzierter Auflösung rechnen und weich hochskalieren
    let (fw, fh) = ((w / 4).max(1), (h / 4).max(1));
    let f = field(points, fw as usize, fh as usize, w as f32 / h as f32);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_follow_quality() {
        assert_eq!(color(-40.0), [22, 163, 74]);
        assert_eq!(color(-95.0), [220, 38, 38]);
        let mid = color(-69.5);
        assert!(mid[0] > 163 && mid[0] < 234);
    }

    #[test]
    fn field_interpolates_and_fades() {
        let pts = [([0.1f32, 0.5f32], -45.0), ([0.9, 0.5], -85.0)];
        let f = field(&pts, 50, 10, 1.0);
        let left = f[5 * 50 + 5];
        let right = f[5 * 50 + 45];
        assert!(left[1] > right[1], "links grün, rechts rot");
        assert!(left[3] > 0 && right[3] > 0);
        // Mitte ist weit von beiden Punkten (0.4 > FADE_END) -> ausgeblendet
        assert_eq!(f[5 * 50 + 25][3], 0);
    }

    #[test]
    fn composite_keeps_size_and_tints() {
        let plan = RgbaImage::from_pixel(200, 100, image::Rgba([255, 255, 255, 255]));
        let img = composite(&plan, &[([0.5, 0.5], -45.0)], 1000);
        assert_eq!(img.dimensions(), (200, 100));
        let c = img.get_pixel(100, 50);
        assert!(c[1] > c[0], "Mitte grünlich");
        assert_eq!(*img.get_pixel(0, 0), Rgb([255, 255, 255]));
        assert!(!to_jpeg(&img).is_empty());
    }
}
