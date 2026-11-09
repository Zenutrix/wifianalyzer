//! Minimaler PDF-Erzeuger für den Messbericht – ohne externe Bibliothek.
//!
//! Unterstützt: Text in Helvetica / Helvetica-Bold (WinAnsi, also inkl. ä ö ü ß € …), Linien,
//! Rechtecke, Kreise, Polylinien und JPEG-Bilder. Koordinaten in Punkt (1/72 Zoll) mit Ursprung
//! **oben links** (y nach unten) – das macht das Layout einfacher; intern wird umgerechnet.

use std::fmt::Write as _;

pub type Rgb = [u8; 3];

/// A4 hoch, in Punkt
pub const PAGE_W: f32 = 595.28;
pub const PAGE_H: f32 = 841.89;

struct Image {
    w: u32,
    h: u32,
    jpeg: Vec<u8>,
}

pub struct Pdf {
    pages: Vec<Vec<u8>>,
    cur: usize,
    images: Vec<Image>,
    title: String,
}

/// Zeichenbreiten von Helvetica (1/1000 em) für ASCII 32..126
const HELV: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, // ' ' .. '/'
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, // 0-9
    278, 278, 584, 584, 584, 556, 1015, // : ; < = > ? @
    667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, // A-Z
    278, 278, 278, 469, 556, 333, // [ \ ] ^ _ `
    556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, // a-z
    334, 260, 334, 584, // { | } ~
];

/// Unicode -> Windows-1252 (WinAnsiEncoding); nicht darstellbare Zeichen werden ersetzt
fn encode(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for c in s.chars() {
        let b: &[u8] = match c {
            ' '..='~' => {
                out.push(c as u8);
                continue;
            }
            '\u{a0}'..='\u{ff}' => {
                out.push(c as u32 as u8);
                continue;
            }
            '€' => &[0x80],
            '‚' => &[0x82],
            '„' => &[0x84],
            '…' => &[0x85],
            '‘' => &[0x91],
            '’' => &[0x92],
            '“' => &[0x93],
            '”' => &[0x94],
            '•' => &[0x95],
            '–' => &[0x96],
            '—' => &[0x97],
            '™' => &[0x99],
            '≥' => b">=",
            '≤' => b"<=",
            '→' | '›' => b">",
            '‹' => b"<",
            '\t' => b" ",
            _ => b"?",
        };
        out.extend_from_slice(b);
    }
    out
}

/// Breite eines Textes in Punkt
pub fn text_width(s: &str, size: f32, bold: bool) -> f32 {
    let units: f32 = s
        .chars()
        .map(|c| match c {
            ' '..='~' => HELV[c as usize - 32] as f32,
            'ı' => 222.0,
            'ä' | 'ö' | 'ü' | 'á' | 'é' | 'ó' | 'à' | 'è' => 556.0,
            'Ä' | 'Ö' | 'Ü' => 722.0,
            'ß' => 611.0,
            '…' => 1000.0,
            '–' => 556.0,
            '·' => 278.0,
            _ => 600.0,
        })
        .sum();
    units / 1000.0 * size * if bold { 1.07 } else { 1.0 }
}

/// Text auf eine Breite kürzen (mit „…“)
pub fn fit(s: &str, size: f32, bold: bool, max_w: f32) -> String {
    if text_width(s, size, bold) <= max_w {
        return s.to_string();
    }
    let mut out: String = s.to_string();
    while !out.is_empty() && text_width(&format!("{out}…"), size, bold) > max_w {
        out.pop();
    }
    format!("{}…", out.trim_end())
}

/// Text in Zeilen umbrechen
pub fn wrap(s: &str, size: f32, bold: bool, max_w: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in s.split_whitespace() {
        let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
        if text_width(&candidate, size, bold) <= max_w || line.is_empty() {
            line = candidate;
        } else {
            lines.push(std::mem::take(&mut line));
            line = word.to_string();
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

fn rgb(c: Rgb) -> String {
    format!("{:.3} {:.3} {:.3}", c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0)
}

impl Pdf {
    pub fn new(title: &str) -> Self {
        Self { pages: vec![Vec::new()], cur: 0, images: Vec::new(), title: title.to_string() }
    }

    fn op(&mut self, s: &str) {
        let page = &mut self.pages[self.cur];
        page.extend_from_slice(s.as_bytes());
        page.push(b'\n');
    }

    pub fn new_page(&mut self) {
        self.pages.push(Vec::new());
        self.cur = self.pages.len() - 1;
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Auf eine vorhandene Seite wechseln (z. B. für Fußzeilen mit Seitenzahl)
    pub fn select_page(&mut self, i: usize) {
        self.cur = i.min(self.pages.len() - 1);
    }

    /// Text; `y` ist die Grundlinie (von oben)
    pub fn text(&mut self, x: f32, y: f32, size: f32, bold: bool, color: Rgb, s: &str) {
        let mut bytes = Vec::new();
        for b in encode(s) {
            if matches!(b, b'(' | b')' | b'\\') {
                bytes.push(b'\\');
            }
            bytes.push(b);
        }
        let head = format!("BT /{} {size:.2} Tf {} rg {x:.2} {:.2} Td (", if bold { "F2" } else { "F1" }, rgb(color), PAGE_H - y);
        let page = &mut self.pages[self.cur];
        page.extend_from_slice(head.as_bytes());
        page.extend_from_slice(&bytes);
        page.extend_from_slice(b") Tj ET\n");
    }

    pub fn text_right(&mut self, right: f32, y: f32, size: f32, bold: bool, color: Rgb, s: &str) {
        let w = text_width(s, size, bold);
        self.text(right - w, y, size, bold, color, s);
    }

    pub fn text_center(&mut self, cx: f32, y: f32, size: f32, bold: bool, color: Rgb, s: &str) {
        let w = text_width(s, size, bold);
        self.text(cx - w / 2.0, y, size, bold, color, s);
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, fill: Rgb) {
        self.op(&format!("{} rg {x:.2} {:.2} {w:.2} {h:.2} re f", rgb(fill), PAGE_H - y - h));
    }

    pub fn rect_stroke(&mut self, x: f32, y: f32, w: f32, h: f32, color: Rgb, lw: f32) {
        self.op(&format!("{} RG {lw:.2} w {x:.2} {:.2} {w:.2} {h:.2} re S", rgb(color), PAGE_H - y - h));
    }

    /// Rechteck mit runden Ecken (gefüllt, optional mit Rahmen)
    pub fn round_rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, fill: Rgb, stroke: Option<Rgb>) {
        let r = r.min(w / 2.0).min(h / 2.0);
        let k = 0.5523 * r;
        let (x0, x1) = (x, x + w);
        let (y0, y1) = (PAGE_H - y - h, PAGE_H - y); // unten, oben (PDF-Koordinaten)
        let mut p = String::new();
        let _ = write!(p, "{:.2} {:.2} m ", x0 + r, y0);
        let _ = write!(p, "{:.2} {:.2} l {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c ", x1 - r, y0, x1 - r + k, y0, x1, y0 + r - k, x1, y0 + r);
        let _ = write!(p, "{:.2} {:.2} l {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c ", x1, y1 - r, x1, y1 - r + k, x1 - r + k, y1, x1 - r, y1);
        let _ = write!(p, "{:.2} {:.2} l {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c ", x0 + r, y1, x0 + r - k, y1, x0, y1 - r + k, x0, y1 - r);
        let _ = write!(p, "{:.2} {:.2} l {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c h ", x0, y0 + r, x0, y0 + r - k, x0 + r - k, y0, x0 + r, y0);
        match stroke {
            Some(s) => self.op(&format!("{} rg {} RG 0.8 w {p}B", rgb(fill), rgb(s))),
            None => self.op(&format!("{} rg {p}f", rgb(fill))),
        }
    }

    pub fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, color: Rgb, lw: f32, dashed: bool) {
        let dash = if dashed { "[3 2] 0 d" } else { "[] 0 d" };
        self.op(&format!("{} RG {lw:.2} w {dash} {x1:.2} {:.2} m {x2:.2} {:.2} l S [] 0 d", rgb(color), PAGE_H - y1, PAGE_H - y2));
    }

    pub fn polyline(&mut self, pts: &[(f32, f32)], color: Rgb, lw: f32) {
        if pts.len() < 2 {
            return;
        }
        let mut p = format!("{} RG {lw:.2} w 1 j 1 J ", rgb(color));
        for (i, (x, y)) in pts.iter().enumerate() {
            let _ = write!(p, "{x:.2} {:.2} {} ", PAGE_H - y, if i == 0 { "m" } else { "l" });
        }
        p.push('S');
        self.op(&p);
    }

    pub fn circle(&mut self, cx: f32, cy: f32, r: f32, fill: Rgb, stroke: Option<(Rgb, f32)>) {
        let k = 0.5523 * r;
        let y = PAGE_H - cy;
        let mut p = String::new();
        let _ = write!(p, "{:.2} {:.2} m ", cx + r, y);
        let _ = write!(p, "{:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c ", cx + r, y + k, cx + k, y + r, cx, y + r);
        let _ = write!(p, "{:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c ", cx - k, y + r, cx - r, y + k, cx - r, y);
        let _ = write!(p, "{:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c ", cx - r, y - k, cx - k, y - r, cx, y - r);
        let _ = write!(p, "{:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c h ", cx + k, y - r, cx + r, y - k, cx + r, y);
        match stroke {
            Some((s, w)) => self.op(&format!("{} rg {} RG {w:.2} w {p}B", rgb(fill), rgb(s))),
            None => self.op(&format!("{} rg {p}f", rgb(fill))),
        }
    }

    /// JPEG einbinden; liefert die Nummer für [`Pdf::image`]
    pub fn add_jpeg(&mut self, jpeg: Vec<u8>, w: u32, h: u32) -> usize {
        self.images.push(Image { w, h, jpeg });
        self.images.len() - 1
    }

    pub fn image(&mut self, idx: usize, x: f32, y: f32, w: f32, h: f32) {
        self.op(&format!("q {w:.2} 0 0 {h:.2} {x:.2} {:.2} cm /Im{idx} Do Q", PAGE_H - y - h));
    }

    /// Fertiges PDF als Bytes
    pub fn finish(self) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::new();
        let mut offsets: Vec<usize> = Vec::new();
        out.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");
        let n_img = self.images.len();
        let n_pages = self.pages.len();
        // Objektnummern: 1 Katalog, 2 Seiten, 3/4 Schriften, 5 Info, dann Bilder, dann je Seite (Seite, Inhalt)
        let img_obj = |i: usize| 6 + i;
        let page_obj = |i: usize| 6 + n_img + i * 2;
        let content_obj = |i: usize| 7 + n_img + i * 2;
        let total = 5 + n_img + n_pages * 2;

        let mut obj = |out: &mut Vec<u8>, body: &[u8]| {
            offsets.push(out.len());
            let n = offsets.len();
            out.extend_from_slice(format!("{n} 0 obj\n").as_bytes());
            out.extend_from_slice(body);
            out.extend_from_slice(b"\nendobj\n");
        };
        obj(&mut out, b"<< /Type /Catalog /Pages 2 0 R >>");
        let kids: Vec<String> = (0..n_pages).map(|i| format!("{} 0 R", page_obj(i))).collect();
        obj(&mut out, format!("<< /Type /Pages /Count {n_pages} /Kids [{}] >>", kids.join(" ")).as_bytes());
        obj(&mut out, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>");
        obj(&mut out, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>");
        // Metadaten: UTF-16BE mit BOM als Hex-String (Textseiten nutzen WinAnsi, Metadaten nicht)
        let title_hex: String = std::iter::once(0xFEFFu16).chain(self.title.encode_utf16()).map(|u| format!("{u:04X}")).collect();
        let info = format!("<< /Producer (WiFi Analyzer) /Title <{title_hex}> >>");
        obj(&mut out, info.as_bytes());
        for im in &self.images {
            let mut body = format!(
                "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
                im.w,
                im.h,
                im.jpeg.len()
            )
            .into_bytes();
            body.extend_from_slice(&im.jpeg);
            body.extend_from_slice(b"\nendstream");
            obj(&mut out, &body);
        }
        let xobjects: String = (0..n_img).map(|i| format!("/Im{i} {} 0 R ", img_obj(i))).collect();
        for (i, content) in self.pages.iter().enumerate() {
            obj(
                &mut out,
                format!(
                    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_W} {PAGE_H}] /Resources << /Font << /F1 3 0 R /F2 4 0 R >> /XObject << {xobjects}>> >> /Contents {} 0 R >>",
                    content_obj(i)
                )
                .as_bytes(),
            );
            let mut body = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
            body.extend_from_slice(content);
            body.extend_from_slice(b"\nendstream");
            obj(&mut out, &body);
        }
        debug_assert_eq!(offsets.len(), total);
        let xref = out.len();
        let mut x = format!("xref\n0 {}\n0000000000 65535 f \n", total + 1);
        for o in &offsets {
            let _ = writeln!(x, "{o:010} 00000 n ");
        }
        let _ = write!(x, "trailer\n<< /Size {} /Root 1 0 R /Info 5 0 R >>\nstartxref\n{xref}\n%%EOF\n", total + 1);
        out.extend_from_slice(x.as_bytes());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_german_text_and_escapes() {
        assert_eq!(encode("Büro (EG) – 5 €"), b"B\xfcro (EG) \x96 5 \x80".to_vec());
        assert_eq!(encode("≥ 😀"), b">= ?".to_vec());
    }

    #[test]
    fn text_helpers() {
        assert!(text_width("WWW", 10.0, false) > text_width("iii", 10.0, false));
        let f = fit("Ein sehr langer Raumname im Erdgeschoss", 10.0, false, 60.0);
        assert!(f.ends_with('…') && text_width(&f, 10.0, false) <= 60.0);
        assert_eq!(wrap("eins zwei drei vier", 10.0, false, 45.0).len(), 2);
    }

    #[test]
    fn produces_valid_structure() {
        let mut p = Pdf::new("Test (1)");
        p.text(40.0, 60.0, 12.0, true, [0, 0, 0], "Grüße (Test)");
        p.rect(40.0, 80.0, 100.0, 20.0, [200, 0, 0]);
        p.circle(100.0, 200.0, 10.0, [0, 128, 0], Some(([255, 255, 255], 1.0)));
        p.new_page();
        p.polyline(&[(0.0, 0.0), (10.0, 10.0)], [0, 0, 255], 1.0);
        let bytes = p.finish();
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.starts_with("%PDF-1.4"));
        assert!(s.contains("/Count 2"));
        assert!(s.contains("(Gr\u{fffd}\u{fffd}e \\(Test\\)) Tj") || s.contains("\\(Test\\)) Tj"));
        assert!(s.trim_end().ends_with("%%EOF"));
        // startxref zeigt auf die xref-Tabelle, jeder Eintrag auf "n 0 obj" (auf den Bytes prüfen)
        let xref_pos: usize = s.rsplit("startxref\n").next().unwrap().lines().next().unwrap().parse().unwrap();
        assert!(bytes[xref_pos..].starts_with(b"xref"));
        let table = String::from_utf8_lossy(&bytes[xref_pos..]).to_string();
        for (n, line) in table.lines().skip(3).take_while(|l| l.ends_with(" n ")).enumerate() {
            let off: usize = line[..10].parse().unwrap();
            assert!(bytes[off..].starts_with(format!("{} 0 obj", n + 1).as_bytes()), "Objekt {}", n + 1);
        }
    }
}
