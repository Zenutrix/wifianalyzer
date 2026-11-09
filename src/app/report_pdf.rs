//! Messbericht direkt als PDF (A4): Kopf, Kennzahlen, Signalverlauf, Grundriss mit Heatmap,
//! Tabelle je Messpunkt (inkl. Latenz/Durchsatz), Legende, Hinweise, Seitenzahlen.

use crate::app::survey::{base_name, open_path, report_notes};
use crate::app::{Sev, WifiApp};
use crate::heatmap;
use crate::measure::{Layer, Measurement, Quality};
use crate::model::fmt_mac;
use crate::pdf::{fit, text_width, wrap, Pdf, Rgb, PAGE_H, PAGE_W};
use crate::widgets::fmt_mmss;
use crate::{t, tf};

const M: f32 = 40.0; // Seitenrand
const CW: f32 = PAGE_W - 2.0 * M; // Inhaltsbreite
const BOTTOM: f32 = PAGE_H - 52.0; // unterer Rand für Inhalt (darunter Fußzeile)

const TEXT: Rgb = [31, 35, 40];
const WEAK: Rgb = [89, 99, 110];
const ACCENT: Rgb = [9, 105, 218];
const BORDER: Rgb = [208, 215, 222];
const LIGHT: Rgb = [246, 248, 250];
const WHITE: Rgb = [255, 255, 255];

fn q_rgb(q: Quality) -> Rgb {
    let (r, g, b) = q.rgb();
    [r, g, b]
}

/// Platz prüfen, sonst neue Seite
fn need(pdf: &mut Pdf, y: &mut f32, h: f32) {
    if *y + h > BOTTOM {
        pdf.new_page();
        *y = M + 10.0;
    }
}

fn heading(pdf: &mut Pdf, y: &mut f32, text: &str) {
    need(pdf, y, 40.0);
    *y += 22.0;
    pdf.text(M, *y, 13.0, true, TEXT, text);
    *y += 10.0;
}

impl WifiApp {
    pub(super) fn export_pdf(&mut self) {
        let Some(m) = &self.meas else { return };
        let name = format!("{} {}.pdf", base_name(m), t!("Messbericht", "survey report"));
        let bytes = self.report_pdf(m);
        let Some(path) = self.export_dialog(&name, "PDF", "pdf") else { return };
        match std::fs::write(&path, bytes) {
            Ok(_) => {
                self.toast(Sev::Info, tf!("PDF-Bericht erstellt: {}", "PDF report created: {}", path.display()));
                open_path(&path);
            }
            Err(e) => self.toast(Sev::Critical, tf!("Export fehlgeschlagen: {e}", "Export failed: {e}")),
        }
    }

    pub(super) fn report_pdf(&self, m: &Measurement) -> Vec<u8> {
        let s = m.stats(self.now_t());
        let title = t!("WLAN-Messbericht", "Wi-Fi survey report");
        let mut pdf = Pdf::new(&format!("{title} – {}", m.ssid));
        let pts = &m.points;
        let mut y = M;

        // ---------- Kopf
        pdf.rect(M, y, CW, 3.0, ACCENT);
        y += 30.0;
        pdf.text(M, y, 22.0, true, TEXT, title);
        let meta = [
            tf!("Datum: {}", "Date: {}", m.started_wall.format("%d.%m.%Y, %H:%M")),
            tf!("Dauer: {}", "Duration: {}", fmt_mmss(s.duration)),
            tf!("Messpunkte: {}", "Points: {}", pts.len()),
        ];
        for (i, line) in meta.iter().enumerate() {
            pdf.text_right(M + CW, y - 12.0 + i as f32 * 12.0, 9.0, false, WEAK, line);
        }
        y += 18.0;
        let lbl = t!("Netzwerk (SSID): ", "Network (SSID): ");
        pdf.text(M, y, 10.5, false, WEAK, lbl);
        pdf.text(M + text_width(lbl, 10.5, false), y, 10.5, true, TEXT, &fit(&m.ssid, 10.5, true, 280.0));
        y += 14.0;

        // ---------- Kennzahlen
        let rated: Vec<Quality> = pts.iter().filter_map(|p| p.quality()).collect();
        let ok = rated.iter().filter(|q| matches!(q, Quality::Excellent | Quality::Good)).count();
        let weak = rated.iter().filter(|q| **q == Quality::Weak).count();
        let avg = {
            let v: Vec<f64> = pts.iter().filter_map(|p| p.rssi).collect();
            (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64).or(s.avg)
        };
        let mut boxes: Vec<(String, String)> = vec![
            (avg.map(|v| format!("{v:.0} dBm")).unwrap_or("–".into()), t!("Ø Signal an den Messpunkten", "Avg signal at the points").into()),
            (if rated.is_empty() { "–".into() } else { format!("{ok} / {}", rated.len()) }, t!("Messpunkte mit gutem Empfang", "Points with good reception").into()),
            (weak.to_string(), t!("Stellen mit schwachem Empfang", "Spots with weak reception").into()),
            (format!("{} / {}", s.roams, s.disconnects), t!("Roams / Verbindungsabbrüche", "Roams / disconnects").into()),
        ];
        let nets: Vec<&crate::nettest::NetTest> = pts.iter().filter_map(|p| p.net.as_ref()).collect();
        let avg_of = |f: &dyn Fn(&crate::nettest::NetTest) -> Option<f64>| {
            let v: Vec<f64> = nets.iter().filter_map(|n| f(n)).collect();
            (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64)
        };
        let gw = avg_of(&|n| n.gw.as_ref().and_then(|g| g.avg_ms));
        let inet = avg_of(&|n| n.inet.as_ref().and_then(|g| g.avg_ms));
        let down = avg_of(&|n| n.down_mbps);
        let up = avg_of(&|n| n.up_mbps);
        if gw.is_some() || down.is_some() {
            let f = |v: Option<f64>, unit: &str| v.map(|x| format!("{x:.0} {unit}")).unwrap_or("–".into());
            boxes.push((f(gw, "ms"), t!("Ø Latenz zum Router", "Avg latency to the router").into()));
            boxes.push((f(inet, "ms"), t!("Ø Latenz ins Internet", "Avg latency to the internet").into()));
            boxes.push((f(down, "Mbit/s"), t!("Ø Download", "Avg download").into()));
            boxes.push((f(up, "Mbit/s"), t!("Ø Upload", "Avg upload").into()));
        }
        let bw = (CW - 3.0 * 10.0) / 4.0;
        for (i, (val, label)) in boxes.iter().enumerate() {
            let (col, row) = (i % 4, i / 4);
            let (bx, by) = (M + col as f32 * (bw + 10.0), y + 8.0 + row as f32 * 58.0);
            pdf.round_rect(bx, by, bw, 50.0, 6.0, LIGHT, Some(BORDER));
            pdf.text(bx + 10.0, by + 21.0, 15.0, true, TEXT, &fit(val, 15.0, true, bw - 20.0));
            for (j, line) in wrap(label, 7.8, false, bw - 20.0).iter().take(2).enumerate() {
                pdf.text(bx + 10.0, by + 33.0 + j as f32 * 9.0, 7.8, false, WEAK, line);
            }
        }
        y += 8.0 + boxes.len().div_ceil(4) as f32 * 58.0;

        if m.plan_mode {
        // ---------- Grundriss (je Ebene: Messwert, 2,4 / 5 / 6 GHz)
        self.pdf_plan_maps(&mut pdf, m, &mut y);

        // ---------- Signalverlauf
        heading(&mut pdf, &mut y, t!("Signalverlauf", "Signal over time"));
        need(&mut pdf, &mut y, 170.0);
        self.pdf_chart(&mut pdf, m, M, y, CW, 150.0);
        y += 162.0;

        } else {
        // ---------- Signalverlauf
        heading(&mut pdf, &mut y, t!("Signalverlauf", "Signal over time"));
        need(&mut pdf, &mut y, 170.0);
        self.pdf_chart(&mut pdf, m, M, y, CW, 150.0);
        y += 162.0;

        // ---------- Grundriss (je Ebene: Messwert, 2,4 / 5 / 6 GHz)
        self.pdf_plan_maps(&mut pdf, m, &mut y);

        }

        // ---------- Tabelle
        heading(&mut pdf, &mut y, t!("Ergebnisse je Messpunkt", "Results per point"));
        let show_net = pts.iter().any(|p| p.net.as_ref().is_some_and(|n| !n.is_empty()));
        // Spalten: (Überschrift, Breite)
        let mut cols: Vec<(&str, f32)> = if show_net {
            vec![("#", 16.0), (t!("Raum / Ort", "Room / location"), 112.0), (t!("Signal", "Signal"), 46.0), (t!("Bewertung", "Rating"), 56.0), ("AP", 96.0), (t!("Kanal", "Ch."), 30.0), (t!("Rate", "Rate"), 46.0), (t!("Latenz", "Latency"), 50.0), ("Down / Up", 63.0)]
        } else {
            vec![("#", 16.0), (t!("Raum / Ort", "Room / location"), 140.0), (t!("Signal", "Signal"), 50.0), (t!("Bewertung", "Rating"), 60.0), ("AP", 128.0), (t!("Kanal", "Channel"), 40.0), (t!("Datenrate", "Data rate"), 81.0)]
        };
        let total: f32 = cols.iter().map(|c| c.1).sum();
        if let Some(last) = cols.last_mut() {
            last.1 += CW - total;
        }
        let header = |pdf: &mut Pdf, y: &mut f32| {
            pdf.rect(M, *y, CW, 18.0, LIGHT);
            let mut x = M;
            for (h, w) in &cols {
                pdf.text(x + 4.0, *y + 12.0, 7.8, true, WEAK, h);
                x += w;
            }
            *y += 18.0;
        };
        need(&mut pdf, &mut y, 60.0);
        header(&mut pdf, &mut y);
        if pts.is_empty() {
            pdf.text(M + 4.0, y + 14.0, 9.0, false, WEAK, t!("Keine Messpunkte erfasst.", "No measurement points recorded."));
            y += 22.0;
        }
        for (i, p) in pts.iter().enumerate() {
            let row_h = 28.0;
            if y + row_h > BOTTOM {
                pdf.new_page();
                y = M + 10.0;
                header(&mut pdf, &mut y);
            }
            let mut x = M;
            let base = y + 12.0;
            let sub = y + 22.0;
            let mut col = 0;
            let mut next = |x: &mut f32| {
                *x += cols[col].1;
                col += 1;
            };
            pdf.text(x + 4.0, base, 8.5, false, WEAK, &(i + 1).to_string());
            next(&mut x);
            pdf.text(x + 4.0, base, 8.8, true, TEXT, &fit(&p.name, 8.8, true, cols[1].1 - 8.0));
            let subline = match (p.bands.any(), p.note.is_empty()) {
                (true, true) => format!("{} GHz", p.bands.summary()).replace(" · ", " | "),
                (true, false) => format!("{} | {}", p.bands.summary(), p.note),
                (false, false) => p.note.clone(),
                (false, true) => String::new(),
            };
            if !subline.is_empty() {
                pdf.text(x + 4.0, sub, 7.0, false, WEAK, &fit(&subline, 7.0, false, cols[1].1 - 8.0));
            }
            next(&mut x);
            match (p.rssi, p.quality()) {
                (Some(v), Some(q)) => {
                    pdf.text(x + 4.0, base, 8.8, true, TEXT, &format!("{v:.0} dBm"));
                    let frac = ((v + 100.0) / 70.0).clamp(0.03, 1.0) as f32;
                    let bw = cols[2].1 - 12.0;
                    pdf.rect(x + 4.0, sub - 4.0, bw, 3.0, [230, 234, 239]);
                    pdf.rect(x + 4.0, sub - 4.0, bw * frac, 3.0, q_rgb(q));
                    next(&mut x);
                    let label = q.label();
                    let pw = text_width(label, 7.2, true) + 12.0;
                    pdf.round_rect(x + 4.0, base - 9.0, pw, 13.0, 6.5, q_rgb(q), None);
                    pdf.text(x + 10.0, base, 7.2, true, WHITE, label);
                }
                _ => {
                    pdf.text(x + 4.0, base, 8.5, false, WEAK, "–");
                    next(&mut x);
                }
            }
            next(&mut x);
            match p.bssid {
                Some(b) => {
                    let v = self.vendor_of_mac(&b);
                    pdf.text(x + 4.0, base, 8.2, false, TEXT, &fit(if v.is_empty() { "–" } else { &v }, 8.2, false, cols[4].1 - 8.0));
                    let mut mac = fmt_mac(&b);
                    if p.from_scan {
                        mac.push_str(t!(" (Scan)", " (scan)"));
                    }
                    pdf.text(x + 4.0, sub, 7.0, false, WEAK, &mac);
                }
                None => pdf.text(x + 4.0, base, 8.5, false, WEAK, "–"),
            }
            next(&mut x);
            pdf.text(x + 4.0, base, 8.5, false, TEXT, &if p.channel > 0 { p.channel.to_string() } else { "–".into() });
            next(&mut x);
            pdf.text(x + 4.0, base, 8.5, false, TEXT, &if p.tx_mbps > 0.0 { format!("{:.0} Mbit/s", p.tx_mbps) } else { "–".into() });
            next(&mut x);
            if show_net {
                let n = p.net.as_ref();
                let g = n.and_then(|n| n.gw.as_ref());
                match g.and_then(|g| g.avg_ms) {
                    Some(ms) => {
                        let c = if ms < 15.0 { q_rgb(Quality::Good) } else if ms < 40.0 { q_rgb(Quality::Fair) } else { q_rgb(Quality::Weak) };
                        pdf.text(x + 4.0, base, 8.5, true, c, &format!("{ms:.0} ms"));
                        if let Some(g) = g.filter(|g| g.loss > 0.0) {
                            pdf.text(x + 4.0, sub, 7.0, false, q_rgb(Quality::Weak), &tf!("{:.0} % Verlust", "{:.0} % loss", g.loss * 100.0));
                        } else if let Some(i) = n.and_then(|n| n.inet.as_ref()).and_then(|i| i.avg_ms) {
                            pdf.text(x + 4.0, sub, 7.0, false, WEAK, &tf!("Internet {i:.0} ms", "Internet {i:.0} ms"));
                        }
                    }
                    None => pdf.text(x + 4.0, base, 8.5, false, WEAK, "–"),
                }
                next(&mut x);
                let f = |v: Option<f64>| v.map(|x| format!("{x:.0}")).unwrap_or("–".into());
                match n.map(|n| (n.down_mbps, n.up_mbps)) {
                    Some((None, None)) | None => pdf.text(x + 4.0, base, 8.5, false, WEAK, "–"),
                    Some((d, u)) => {
                        pdf.text(x + 4.0, base, 8.5, false, TEXT, &format!("{} / {}", f(d), f(u)));
                        pdf.text(x + 4.0, sub, 7.0, false, WEAK, "Mbit/s");
                    }
                }
            }
            y += row_h;
            pdf.line(M, y, M + CW, y, [230, 234, 239], 0.6, false);
        }
        y += 6.0;

        // ---------- Legende
        heading(&mut pdf, &mut y, t!("So sind die Werte zu lesen", "How to read the values"));
        need(&mut pdf, &mut y, 60.0);
        let lw = (CW - 3.0 * 10.0) / 4.0;
        for (i, (q, range)) in [(Quality::Excellent, "≥ -60 dBm"), (Quality::Good, "-67 … -61 dBm"), (Quality::Fair, "-75 … -68 dBm"), (Quality::Weak, "< -75 dBm")].iter().enumerate() {
            let lx = M + i as f32 * (lw + 10.0);
            pdf.rect(lx, y, 3.0, 44.0, q_rgb(*q));
            pdf.text(lx + 9.0, y + 10.0, 8.5, true, TEXT, q.label());
            pdf.text(lx + 9.0 + text_width(q.label(), 8.5, true) + 4.0, y + 10.0, 7.8, false, WEAK, range);
            for (j, line) in wrap(q.hint(), 7.2, false, lw - 12.0).iter().take(3).enumerate() {
                pdf.text(lx + 9.0, y + 21.0 + j as f32 * 8.6, 7.2, false, WEAK, line);
            }
        }
        y += 50.0;

        // ---------- Hinweise
        heading(&mut pdf, &mut y, t!("Hinweise", "Notes"));
        for (name, text) in report_notes(m, &s) {
            let full = match &name {
                Some(n) => format!("{n}: {text}"),
                None => text,
            };
            let lines = wrap(&full, 9.0, false, CW - 14.0);
            need(&mut pdf, &mut y, lines.len() as f32 * 12.0 + 4.0);
            pdf.circle(M + 3.0, y + 8.5, 1.8, ACCENT, None);
            for (j, line) in lines.iter().enumerate() {
                y += 12.0;
                // Raumname fett hervorheben
                if j == 0 {
                    if let Some(n) = &name {
                        let head = format!("{n}:");
                        if let Some(rest) = line.strip_prefix(&head) {
                            pdf.text(M + 12.0, y, 9.0, true, TEXT, &head);
                            pdf.text(M + 12.0 + text_width(&head, 9.0, true), y, 9.0, false, TEXT, rest);
                            continue;
                        }
                    }
                }
                pdf.text(M + 12.0, y, 9.0, false, TEXT, line);
            }
            y += 4.0;
        }

        // ---------- Fußzeile auf allen Seiten
        let n = pdf.page_count();
        let created = tf!("Erstellt am {}", "Created on {}", chrono::Local::now().format("%d.%m.%Y %H:%M"));
        for i in 0..n {
            pdf.select_page(i);
            let fy = PAGE_H - 30.0;
            pdf.line(M, fy - 10.0, M + CW, fy - 10.0, BORDER, 0.6, false);
            pdf.text(M, fy, 7.5, false, WEAK, &format!("WiFi Analyzer {} · wifianalyzer.schoepf-tirol.at", env!("CARGO_PKG_VERSION")));
            pdf.text_center(PAGE_W / 2.0 + 40.0, fy, 7.5, false, WEAK, &created);
            pdf.text_right(M + CW, fy, 7.5, false, WEAK, &tf!("Seite {} / {n}", "Page {} / {n}", i + 1));
        }
        pdf.finish()
    }

    /// Grundriss mit Heatmap – je Ebene mit Daten eine Karte (Messwert, 2,4 / 5 / 6 GHz)
    fn pdf_plan_maps(&self, pdf: &mut Pdf, m: &Measurement, y: &mut f32) {
        let multi = m.plans.len() > 1;
        let mut any = false;
        for fi in 0..m.plans.len() {
            any |= self.pdf_floor_maps(pdf, m, fi, multi, y);
        }
        if any && m.plans.iter().any(|f| f.width_m.is_none()) {
            pdf.text(M, *y, 7.2, false, WEAK, t!("Ohne Maßstab: Heatmap mit angenommener Planbreite von 40 m berechnet.", "Without scale: heatmap computed assuming a plan width of 40 m."));
            *y += 10.0;
        }
    }

    /// Karten eines Stockwerks; true, wenn etwas gezeichnet wurde
    fn pdf_floor_maps(&self, pdf: &mut Pdf, m: &Measurement, fi: usize, multi: bool, y: &mut f32) -> bool {
        let Some(fp) = m.plans.get(fi) else { return false };
        let Ok(plan) = heatmap::decode_plan(&fp.data) else { return false };
        let layers = m.layers_with_data(fi);
        for (li, layer) in layers.iter().enumerate() {
            let placed = m.heat_points(fi, *layer);
            let comp = heatmap::composite(&plan, &placed, 1600, fp.width_m);
            let (iw, ih) = comp.dimensions();
            let mut w = CW;
            let mut h = w * ih as f32 / iw as f32;
            // erste Karte der Grundriss-Messung so groß wie möglich, weitere kompakter
            let max_h = if li == 0 && fi == 0 && m.plan_mode { (BOTTOM - *y - 60.0).clamp(260.0, 470.0) } else if li == 0 { 430.0 } else { 330.0 };
            if h > max_h {
                h = max_h;
                w = h * iw as f32 / ih as f32;
            }
            if *y + 32.0 + h + 24.0 > BOTTOM {
                pdf.new_page();
                *y = M;
            }
            let mut title = match layer {
                Layer::Measured => t!("Ausleuchtung im Grundriss", "Coverage on the floor plan").to_string(),
                l => tf!("Ausleuchtung {}", "Coverage {}", l.label()),
            };
            if multi {
                title = format!("{} – {title}", fp.title());
            }
            heading(pdf, y, &title);
            let idx = pdf.add_jpeg(heatmap::to_jpeg(&comp), iw, ih);
            let x0 = M + (CW - w) / 2.0;
            pdf.image(idx, x0, *y, w, h);
            pdf.rect_stroke(x0, *y, w, h, BORDER, 0.6);
            for (i, p) in m.points.iter().enumerate() {
                let (Some(pos), Some(v)) = (p.pos.filter(|_| p.floor == fi), p.value_for(*layer)) else { continue };
                let (cx, cy) = (x0 + pos[0] * w, *y + pos[1] * h);
                pdf.circle(cx, cy, 7.5, heatmap::color(v), Some((WHITE, 1.4)));
                pdf.text_center(cx, cy + 2.8, 7.5, true, WHITE, &(i + 1).to_string());
            }
            // Maßstabsleiste
            if let Some(wm) = fp.width_m {
                let len = heatmap::nice_length(wm * 0.18);
                let px = len / wm * w;
                let (bx, by) = (x0 + 10.0, *y + h - 10.0);
                pdf.round_rect(bx - 5.0, by - 15.0, px + 10.0, 20.0, 3.0, WHITE, None);
                pdf.line(bx, by, bx + px, by, TEXT, 1.2, false);
                pdf.line(bx, by - 3.0, bx, by + 2.0, TEXT, 1.2, false);
                pdf.line(bx + px, by - 3.0, bx + px, by + 2.0, TEXT, 1.2, false);
                let label = if len < 1.0 { format!("{:.0} cm", len * 100.0) } else { format!("{len:.0} m") };
                pdf.text_center(bx + px / 2.0, by - 5.0, 7.0, true, TEXT, &label);
            }
            *y += h + 10.0;
            // Farblegende (Schwellen der Farbskala)
            let scale = heatmap::scale();
            let mut lx = M;
            for (d, c) in scale.iter().rev() {
                let label = format!(">= {d:.0}");
                pdf.rect(lx, *y, 9.0, 7.0, *c);
                pdf.text(lx + 12.0, *y + 6.5, 7.2, false, WEAK, &label);
                lx += 18.0 + text_width(&label, 7.2, false);
            }
            pdf.text(lx, *y + 6.5, 7.2, false, WEAK, "dBm");
            if let Some(c) = heatmap::no_signal() {
                let lx = lx + text_width("dBm", 7.2, false) + 14.0;
                pdf.rect(lx, *y, 9.0, 7.0, c);
                pdf.text(lx + 12.0, *y + 6.5, 7.2, false, WEAK, t!("kein Signal", "no signal"));
            }
            let missing = m.points.iter().filter(|p| p.pos.is_none()).count();
            if missing > 0 && li == 0 && fi == 0 {
                pdf.text_right(M + CW, *y + 6.5, 7.5, false, WEAK, &tf!("{missing} Messpunkte ohne Position", "{missing} points without position"));
            }
            *y += 16.0;
        }
        !layers.is_empty()
    }

    /// Signalverlauf als Vektorgrafik mit nummerierten Messpunkten
    fn pdf_chart(&self, pdf: &mut Pdf, m: &Measurement, x: f32, y: f32, w: f32, h: f32) {
        let (l, r, top, bot) = (34.0, 6.0, 6.0, 16.0);
        let (px, pw, py, ph) = (x + l, w - l - r, y + top, h - top - bot);
        pdf.round_rect(x, y, w, h, 6.0, [250, 251, 252], Some(BORDER));
        let dur = m.elapsed(self.now_t()).max(1.0);
        let sx = |t: f64| px + (t / dur).clamp(0.0, 1.0) as f32 * pw;
        let sy = |v: f64| py + ((-30.0 - v) / 70.0).clamp(0.0, 1.0) as f32 * ph;
        for db in [-40.0, -50.0, -60.0, -70.0, -80.0, -90.0] {
            pdf.line(px, sy(db), px + pw, sy(db), [230, 234, 239], 0.5, false);
            pdf.text_right(px - 4.0, sy(db) + 2.5, 6.8, false, WEAK, &format!("{db:.0}"));
        }
        pdf.line(px, sy(-67.0), px + pw, sy(-67.0), q_rgb(Quality::Good), 0.7, true);
        pdf.line(px, sy(-75.0), px + pw, sy(-75.0), q_rgb(Quality::Weak), 0.7, true);
        let step = [30.0, 60.0, 120.0, 300.0, 600.0, 900.0, 1800.0, 3600.0].into_iter().find(|s| dur / s <= 8.0).unwrap_or(3600.0);
        let mut t = 0.0;
        while t <= dur {
            pdf.text_center(sx(t), py + ph + 11.0, 6.8, false, WEAK, &fmt_mmss(t));
            t += step;
        }
        let vals: Vec<(f64, f64)> = if m.conn.is_empty() { m.best.iter().map(|b| (b.t, b.rssi as f64)).collect() } else { m.conn.iter().map(|c| (c.t, c.rssi as f64)).collect() };
        let k = (vals.len() / 600).max(1);
        let line: Vec<(f32, f32)> = vals.iter().step_by(k).map(|(t, v)| (sx(*t), sy(*v))).collect();
        pdf.polyline(&line, ACCENT, 1.2);
        for (i, p) in m.points.iter().enumerate() {
            let (cx, cy) = (sx(p.t), sy(p.rssi.unwrap_or(-95.0)));
            let c = p.quality().map(q_rgb).unwrap_or(WEAK);
            pdf.line(cx, py, cx, py + ph, [215, 220, 226], 0.5, false);
            pdf.circle(cx, cy, 6.0, c, Some((WHITE, 1.0)));
            pdf.text_center(cx, cy + 2.4, 6.5, true, WHITE, &(i + 1).to_string());
        }
    }
}
