//! Messpunkte während einer Messung (Enter -> Raum/Ort eingeben), Liste der Messpunkte,
//! Messungen speichern/öffnen (automatisch in „Dokumente\WiFi Analyzer“) und Export
//! als CSV (Excel) oder als Bericht für Kunden (HTML, im Browser als PDF druckbar).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant, SystemTime};

use eframe::egui::{self, Align2, Color32, Margin, RichText, Ui};

use crate::app::{Sev, WifiApp};
use crate::measure::{MeasPoint, Measurement, Quality, ScanContext};
use crate::nettest::NetTest;
use crate::model::fmt_mac;
use crate::theme::{pal, Palette};
use crate::widgets::{badge, fmt_mmss, primary_button};
use crate::{t, tf};

/// Dateiendung gespeicherter Messungen
pub(super) const EXT: &str = "wifisurvey";

/// Logo für den HTML-Bericht (wie website/logo.svg, ohne Animation)
const REPORT_LOGO: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" aria-hidden="true"><defs><linearGradient id="lb" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#0f172a"/><stop offset="1" stop-color="#1e1b4b"/></linearGradient><linearGradient id="li" gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="1024" y2="1024"><stop offset=".18" stop-color="#22d3ee"/><stop offset=".5" stop-color="#6366f1"/><stop offset=".82" stop-color="#d946ef"/></linearGradient></defs><rect width="1024" height="1024" rx="240" fill="url(#lb)"/><g fill="none" stroke="url(#li)" stroke-linecap="round" stroke-linejoin="round"><polyline points="196,318 346,716 512,452 678,716 828,318" stroke-width="118"/><path d="M427 304.8A170 170 0 0 1 597 304.8" stroke-width="54"/><path d="M381 225.1A262 262 0 0 1 643 225.1" stroke-width="54"/></g></svg>"##;

/// Eingabe im Dialog „Messpunkt erfassen“
pub(crate) struct PointDraft {
    name: String,
    note: String,
    /// Programmzeit beim Öffnen – ab dann wird gemittelt
    since_t: f64,
    focus: bool,
    /// erster Frame: das Enter, das den Dialog geöffnet hat, noch ignorieren
    fresh: bool,
    /// Position im Grundriss (optional, per Klick im Dialog)
    pos: Option<[f32; 2]>,
    /// läuft schon während der Eingabe: Latenz/Durchsatz an dieser Stelle
    net_rx: Option<Receiver<NetTest>>,
    net: Option<NetTest>,
}

/// Eintrag der Liste „Gespeicherte Messungen“
#[derive(Clone)]
pub(crate) struct RecentSurvey {
    pub path: PathBuf,
    pub modified: SystemTime,
    /// Grundriss-Messung (sonst Rundgang)
    pub plan: bool,
}

/// Art einer gespeicherten Messung anhand der ersten Bytes erkennen (ohne den Grundriss zu lesen)
fn is_plan_survey(path: &Path) -> bool {
    use std::io::Read;
    let mut buf = [0u8; 512];
    let n = std::fs::File::open(path).and_then(|mut f| f.read(&mut buf)).unwrap_or(0);
    let head = String::from_utf8_lossy(&buf[..n]);
    head.contains("\"plan_mode\": true") || head.contains("\"plan_mode\":true")
}

pub(super) fn quality_color(p: &Palette, q: Quality) -> Color32 {
    match q {
        Quality::Excellent | Quality::Good => p.success,
        Quality::Fair => p.warning,
        Quality::Weak => p.danger,
    }
}

/// Ordner für Messungen: „Dokumente\WiFi Analyzer“ (auch wenn „Dokumente“ z. B. in OneDrive liegt)
pub(super) fn survey_dir() -> PathBuf {
    if crate::demo::enabled() {
        // Demo-Messungen nicht zwischen die echten legen
        return std::env::temp_dir().join("WiFi Analyzer Demo");
    }
    documents_dir().join("WiFi Analyzer")
}

fn documents_dir() -> PathBuf {
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{FOLDERID_Documents, SHGetKnownFolderPath};
    unsafe {
        let mut p: *mut u16 = std::ptr::null_mut();
        if SHGetKnownFolderPath(&FOLDERID_Documents, 0, std::ptr::null_mut(), &mut p) == 0 && !p.is_null() {
            let len = (0..).take_while(|&i| *p.add(i) != 0).count();
            let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, len));
            CoTaskMemFree(p as *const _);
            return PathBuf::from(s);
        }
        if !p.is_null() {
            CoTaskMemFree(p as *const _);
        }
    }
    std::env::var_os("USERPROFILE").map(|u| PathBuf::from(u).join("Documents")).unwrap_or_else(|| PathBuf::from("."))
}

/// Für Dateinamen ungültige Zeichen ersetzen
fn safe_name(s: &str) -> String {
    let out: String = s.chars().map(|c| if c.is_control() || r#"<>:"/\|?*"#.contains(c) { '_' } else { c }).collect();
    let out = out.trim().trim_end_matches('.').to_string();
    if out.is_empty() {
        "WLAN".into()
    } else {
        out
    }
}

/// „2026-11-06 14-30 Firmen-WLAN“
pub(super) fn base_name(m: &Measurement) -> String {
    format!("{} {}", m.started_wall.format("%Y-%m-%d %H-%M"), safe_name(&m.ssid))
}

/// Freier Dateiname: „name.ext“, sonst „name (2).ext“, „name (3).ext“ …
fn unique_path(dir: &Path, base: &str, ext: &str) -> PathBuf {
    let first = dir.join(format!("{base}.{ext}"));
    if !first.exists() {
        return first;
    }
    (2..).map(|n| dir.join(format!("{base} ({n}).{ext}"))).find(|p| !p.exists()).unwrap_or(first)
}

fn recent_surveys() -> Vec<RecentSurvey> {
    let Ok(rd) = std::fs::read_dir(survey_dir()) else { return Vec::new() };
    let mut v: Vec<RecentSurvey> = rd
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case(EXT)))
        .filter_map(|e| Some(RecentSurvey { plan: is_plan_survey(&e.path()), path: e.path(), modified: e.metadata().ok()?.modified().ok()? }))
        .collect();
    v.sort_by(|a, b| b.modified.cmp(&a.modified));
    v.truncate(40);
    v
}

/// Datei/Ordner mit dem Standardprogramm öffnen (Bericht im Browser, Ordner im Explorer)
pub(super) fn open_path(path: &Path) {
    let _ = std::process::Command::new("explorer").arg(path).spawn();
}

/// Hinweise für den Bericht (HTML und PDF): (Raum fett, Text)
pub(super) fn report_notes(m: &Measurement, s: &crate::measure::Stats) -> Vec<(Option<String>, String)> {
    let pts = &m.points;
    let mut out: Vec<(Option<String>, String)> = Vec::new();
    let weak: Vec<&MeasPoint> = pts.iter().filter(|p| p.quality() == Some(Quality::Weak)).collect();
    let fair: Vec<&MeasPoint> = pts.iter().filter(|p| p.quality() == Some(Quality::Fair)).collect();
    if pts.is_empty() {
        out.push((None, t!("Für eine raumweise Bewertung bei der nächsten Messung Messpunkte setzen.", "Add measurement points next time for a room-by-room assessment.").into()));
    } else if weak.is_empty() && fair.is_empty() {
        out.push((None, t!("An allen Messpunkten ist der Empfang gut – keine Maßnahmen nötig.", "Reception is good at all points – no action needed.").into()));
    }
    for p in &weak {
        out.push((
            Some(p.name.clone()),
            tf!(
                "schwacher Empfang ({:.0} dBm). Empfohlen: zusätzlicher Access Point oder bessere Position des nächstgelegenen APs.",
                "weak reception ({:.0} dBm). Recommended: an additional access point or a better position for the nearest AP.",
                p.rssi.unwrap_or_default()
            ),
        ));
    }
    for p in &fair {
        out.push((
            Some(p.name.clone()),
            tf!("ausreichend ({:.0} dBm) – für Telefonie und Video ggf. verbessern.", "fair ({:.0} dBm) – consider improving for voice and video.", p.rssi.unwrap_or_default()),
        ));
    }
    // Latenz/Paketverlust zum Router: Hinweis auf Störungen oder Überlast
    for p in pts {
        let Some(g) = p.net.as_ref().and_then(|n| n.gw.as_ref()) else { continue };
        if g.loss > 0.0 {
            out.push((Some(p.name.clone()), tf!("{:.0} % Paketverlust zum Router – Funkstörungen oder Überlast prüfen.", "{:.0} % packet loss to the router – check for interference or overload.", g.loss * 100.0)));
        } else if let Some(ms) = g.avg_ms.filter(|ms| *ms >= 30.0) {
            out.push((Some(p.name.clone()), tf!("hohe Latenz zum Router ({ms:.0} ms) – Kanalauslastung und Störungen prüfen.", "high latency to the router ({ms:.0} ms) – check channel load and interference.")));
        }
    }
    if s.disconnects > 0 {
        out.push((None, tf!("Während der Messung gab es {} Verbindungsabbrüche.", "There were {} disconnects during the survey.", s.disconnects)));
    }
    if s.sticky > 0 {
        out.push((
            None,
            t!(
                "Das Gerät blieb zeitweise an einem schwächeren AP („Sticky Client“). 802.11k/v und eine Mindest-Signalstärke am AP können helfen.",
                "The device stayed on a weaker AP at times (\"sticky client\"). 802.11k/v and a minimum RSSI on the AP can help."
            )
            .into(),
        ));
    }
    out
}

/// Grundriss + Heatmap für den HTML-Bericht: ein SVG mit eingebettetem JPEG und nummerierten Punkten
fn plan_svg(m: &Measurement, floor: usize, layer: crate::measure::Layer) -> Option<String> {
    use base64::Engine;
    let plan = m.plans.get(floor)?;
    let img = crate::heatmap::decode_plan(&plan.data).ok()?;
    let pts = m.heat_points(floor, layer);
    let comp = crate::heatmap::composite(&img, &pts, 1600, plan.width_m);
    let (w, h) = comp.dimensions();
    let b64 = base64::engine::general_purpose::STANDARD.encode(crate::heatmap::to_jpeg(&comp));
    let mut s = format!(
        r#"<svg viewBox="0 0 {w} {h}" width="100%" xmlns="http://www.w3.org/2000/svg" style="border:1px solid #d1d9e0;border-radius:8px;background:#fff"><image href="data:image/jpeg;base64,{b64}" x="0" y="0" width="{w}" height="{h}"/>"#
    );
    let r = (w.max(h) as f32 / 70.0).clamp(10.0, 22.0);
    for (i, p) in m.points.iter().enumerate() {
        let Some(pos) = p.pos.filter(|_| p.floor == floor) else { continue };
        let [cr, cg, cb] = p.value_for(layer).map(crate::heatmap::color).unwrap_or([120, 120, 120]);
        let (x, y) = (pos[0] * w as f32, pos[1] * h as f32);
        let _ = write!(
            s,
            r##"<circle cx="{x:.1}" cy="{y:.1}" r="{r:.1}" fill="rgb({cr},{cg},{cb})" stroke="#fff" stroke-width="{:.1}"/><text x="{x:.1}" y="{:.1}" font-size="{:.1}" font-weight="700" fill="#fff" text-anchor="middle" font-family="Segoe UI, sans-serif">{}</text>"##,
            r * 0.22,
            y + r * 0.38,
            r * 1.05,
            i + 1
        );
    }
    s.push_str("</svg>");
    let missing = m.points.iter().filter(|p| p.pos.is_none()).count();
    if missing > 0 && floor == 0 && layer == crate::measure::Layer::Measured {
        let _ = write!(
            s,
            r#"<p class="weak" style="font-size:12px">{}</p>"#,
            tf!("{missing} Messpunkte ohne Position im Grundriss.", "{missing} points without a position on the floor plan.")
        );
    }
    Some(s)
}

/// Kurztext der Netzwerkwerte (Dialog)
pub(super) fn net_summary(n: &NetTest) -> String {
    let mut parts = Vec::new();
    if let Some(ms) = n.gw.as_ref().and_then(|g| g.avg_ms) {
        parts.push(tf!("Router {ms:.0} ms", "Router {ms:.0} ms"));
    }
    if let Some(ms) = n.inet.as_ref().and_then(|g| g.avg_ms) {
        parts.push(tf!("Internet {ms:.0} ms", "Internet {ms:.0} ms"));
    }
    if let Some(d) = n.down_mbps {
        parts.push(format!("Down {d:.0} Mbit/s"));
    }
    if let Some(u) = n.up_mbps {
        parts.push(format!("Up {u:.0} Mbit/s"));
    }
    if parts.is_empty() {
        return n.note.clone().unwrap_or_else(|| t!("keine Netzwerkwerte", "no network values").into());
    }
    format!("\u{E182} {}", parts.join(" · "))
}

/// Farbe für eine Latenz in ms
pub(super) fn latency_color(p: &Palette, ms: f64) -> Color32 {
    if ms < 15.0 {
        p.success
    } else if ms < 40.0 {
        p.warning
    } else {
        p.danger
    }
}

/// Zellen „Ping“ und „Down / Up“ der Messpunkt-Tabelle
fn net_cells(ui: &mut Ui, p: &Palette, net: Option<&NetTest>, pending: bool) {
    let Some(n) = net else {
        if pending {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(12.0));
            });
            ui.label(RichText::new(t!("misst …", "measuring …")).small().color(p.weak));
        } else {
            ui.label(RichText::new("–").color(p.weak));
            ui.label(RichText::new("–").color(p.weak));
        }
        return;
    };
    let gw = n.gw.as_ref();
    match gw.and_then(|g| g.avg_ms) {
        Some(ms) => {
            let mut tip = tf!("Router {}: Ø {ms:.1} ms", "Router {}: avg {ms:.1} ms", n.gateway.as_deref().unwrap_or("?"));
            if let Some(g) = gw {
                if let (Some(a), Some(b)) = (g.min_ms, g.max_ms) {
                    tip.push_str(&format!(" ({a:.0}–{b:.0})"));
                }
                if let Some(j) = g.jitter_ms {
                    tip.push_str(&tf!(", Jitter {j:.1} ms", ", jitter {j:.1} ms"));
                }
                tip.push_str(&tf!(", Verlust {:.0} %", ", loss {:.0} %", g.loss * 100.0));
            }
            if let Some(i) = n.inet.as_ref() {
                match i.avg_ms {
                    Some(v) => tip.push_str(&tf!("\nInternet (1.1.1.1): Ø {v:.0} ms, Verlust {:.0} %", "\nInternet (1.1.1.1): avg {v:.0} ms, loss {:.0} %", i.loss * 100.0)),
                    None => tip.push_str(t!("\nInternet: keine Antwort", "\nInternet: no reply")),
                }
            }
            let lossy = gw.is_some_and(|g| g.loss > 0.0);
            let txt = if lossy { format!("{ms:.0} ms \u{E4E0}") } else { format!("{ms:.0} ms") };
            ui.label(RichText::new(txt).monospace().color(latency_color(p, ms))).on_hover_text(tip);
        }
        None => {
            let tip = n.note.clone().unwrap_or_else(|| t!("Router antwortet nicht auf Ping", "Router does not answer ping").into());
            ui.label(RichText::new("–").color(p.weak)).on_hover_text(tip);
        }
    }
    match (n.down_mbps, n.up_mbps) {
        (None, None) => {
            let r = ui.label(RichText::new("–").color(p.weak));
            if let Some(note) = &n.note {
                r.on_hover_text(note);
            }
        }
        (d, u) => {
            let f = |v: Option<f64>| v.map(|x| format!("{x:.0}")).unwrap_or("–".into());
            ui.label(RichText::new(format!("{} / {}", f(d), f(u))).monospace()).on_hover_text(t!("Durchsatz Download / Upload in Mbit/s (Internet, speed.cloudflare.com)", "Throughput download / upload in Mbit/s (internet, speed.cloudflare.com)"));
        }
    }
}

/// CSV-Feld für Excel: Trennzeichen/Anführungszeichen maskieren und Formeln entschärfen
/// (SSIDs kann jeder in der Nähe frei wählen, z. B. „=HYPERLINK(…)“)
pub(super) fn esc_csv(s: &str) -> String {
    let s = if s.starts_with(['=', '+', '-', '@', '\t', '\r']) && s.replace(',', ".").parse::<f64>().is_err() { format!("'{s}") } else { s.to_string() };
    if s.contains([';', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s
    }
}

fn esc_html(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

impl WifiApp {
    // ---------------------------------------------------------------- Messpunkt erfassen

    /// Aktueller Scan-Stand der SSID (stärkster AP, Anzahl sichtbarer APs)
    fn survey_scan_ctx(&self, ssid: &str) -> ScanContext {
        let now = Instant::now();
        let aps: Vec<_> = self.aps.values().filter(|r| !r.info.hidden && r.info.ssid == ssid && !self.is_stale(r, now)).collect();
        let best = aps.iter().max_by_key(|r| r.info.rssi).map(|r| (r.info.bssid, r.info.rssi, r.info.channel));
        let band = |b: crate::model::Band| aps.iter().filter(|r| r.info.band == b).map(|r| r.info.rssi).max();
        let bands = crate::measure::BandValues { b24: band(crate::model::Band::B24), b5: band(crate::model::Band::B5), b6: band(crate::model::Band::B6) };
        ScanContext { best, aps_visible: aps.len(), bands }
    }

    pub(super) fn open_point_dialog(&mut self) {
        self.open_point_dialog_at(None);
    }

    /// Dialog öffnen; `pos` = bereits im Grundriss angeklickte Stelle
    pub(super) fn open_point_dialog_at(&mut self, pos: Option<[f32; 2]>) {
        if self.meas.as_ref().is_some_and(|m| m.running) && self.meas_point.is_none() {
            let net_rx = self.spawn_net_test();
            // frischer Scan, damit die Werte je Band (2,4/5/6 GHz) von genau dieser Stelle stammen
            self.scan_now();
            self.meas_point = Some(PointDraft {
                name: String::new(),
                note: String::new(),
                since_t: self.now_t(),
                focus: true,
                fresh: true,
                pos,
                net_rx,
                net: None,
            });
        }
    }

    pub(super) fn point_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.meas_point.take() else { return };
        if self.meas.as_ref().is_none_or(|m| !m.running) {
            return;
        }
        // Grundriss im Dialog nur bei der Grundriss-Messung
        let plan = if self.meas.as_ref().is_some_and(|m| m.plan_mode) { self.plan_texture(ctx) } else { None };
        if let Some(r) = d.net_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            d.net = Some(r);
            d.net_rx = None;
        }
        let Some(m) = self.meas.as_ref() else { return };
        let others: Vec<([f32; 2], Option<f64>, usize)> = m.points.iter().enumerate().filter(|(_, p)| p.floor == m.floor).filter_map(|(i, p)| Some((p.pos?, p.rssi, i + 1))).collect();
        let (ping_on, speed_on) = (self.settings.point_ping, self.settings.point_speed);
        let band_now = {
            let ctx_b = self.survey_scan_ctx(&m.ssid).bands;
            let ctx_b = match (crate::demo::enabled(), d.pos) {
                (true, Some(pos)) => crate::demo::bands_at(pos),
                _ => ctx_b,
            };
            ctx_b.summary()
        };
        let t_now = self.now_t();
        let ssid = m.ssid.clone();
        let connected = self.conn.as_ref().is_some_and(|c| c.ssid == ssid);
        let live = self.conn.as_ref().filter(|c| c.ssid == ssid).and_then(|c| c.rssi).or_else(|| self.best_ap_of(&ssid).map(|b| b.info.rssi));
        // bisher an dieser Stelle gemessene Werte
        let since = d.since_t - m.t0 - 1.0;
        let vals: Vec<i32> = if connected {
            m.conn.iter().filter(|c| c.t >= since).map(|c| c.rssi).collect()
        } else {
            m.best.iter().filter(|b| b.t >= since).map(|b| b.rssi).collect()
        };
        let avg = (!vals.is_empty()).then(|| vals.iter().map(|v| *v as f64).sum::<f64>() / vals.len() as f64);
        let shown = avg.or(live.map(|v| v as f64));
        let secs = (t_now - d.since_t).max(0.0) as u32;
        let next_no = m.points.len() + 1;

        let (mut save, mut cancel) = (false, false);
        egui::Window::new(RichText::new(t!("\u{E316} Messpunkt erfassen", "\u{E316} Add measurement point")).strong())
            .collapsible(false)
            .resizable(false)
            .min_width(470.0)
            .max_width(470.0)
            .anchor(Align2::CENTER_CENTER, [0.0, -40.0])
            .show(ctx, |ui| {
                let p = pal(ui);
                egui::Frame::none().fill(p.surface2).rounding(8.0).inner_margin(Margin::same(12.0)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| match shown {
                        Some(v) => {
                            let q = Quality::of(v);
                            let col = quality_color(&p, q);
                            ui.label(RichText::new(format!("{v:.0} dBm")).size(30.0).strong().color(col));
                            ui.add_space(8.0);
                            ui.vertical(|ui| {
                                badge(ui, q.label(), col);
                                ui.add(egui::Label::new(RichText::new(q.hint()).size(11.5).color(p.weak)).wrap());
                            });
                        }
                        None => {
                            ui.label(RichText::new(t!("Noch kein Messwert", "No value yet")).size(20.0).color(p.weak));
                        }
                    });
                    if !band_now.is_empty() {
                        ui.label(RichText::new(tf!("Je Band (Scan): {band_now}", "Per band (scan): {band_now}")).size(12.0).color(p.text));
                    }
                    let src = if connected { t!("Verbindung", "connection") } else { t!("Scan", "scan") };
                    ui.label(
                        RichText::new(tf!("An dieser Stelle seit {secs} s · {} Werte ({src})", "At this spot for {secs} s · {} values ({src})", vals.len()))
                            .size(11.5)
                            .color(p.weak),
                    );
                });
                ui.add_space(10.0);
                ui.label(RichText::new(t!("Raum / Ort", "Room / location")).strong());
                let r1 = ui.add(
                    egui::TextEdit::singleline(&mut d.name)
                        .hint_text(tf!("z. B. Büro 1.03 – leer: „Messpunkt {next_no}“", "e.g. office 1.03 – empty: \"Point {next_no}\""))
                        .desired_width(f32::INFINITY),
                );
                if d.focus {
                    r1.request_focus();
                    d.focus = false;
                }
                ui.add_space(6.0);
                ui.label(RichText::new(t!("Notiz (optional)", "Note (optional)")).strong());
                let r2 = ui.add(
                    egui::TextEdit::singleline(&mut d.note)
                        .hint_text(t!("z. B. Tür geschlossen, hinter Regal, 2. OG", "e.g. door closed, behind shelf, 2nd floor"))
                        .desired_width(f32::INFINITY),
                );
                let (enter, escape) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
                let typing = r1.has_focus() || r2.has_focus();
                if enter && (r1.lost_focus() || r2.lost_focus() || (!typing && !d.fresh)) {
                    save = true;
                }
                d.fresh = false;
                if escape {
                    cancel = true;
                }
                // Position im Grundriss
                if let Some((tex, size)) = plan {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(t!("Position im Grundriss", "Position on the floor plan")).strong());
                        ui.label(RichText::new(if d.pos.is_some() { t!("\u{E182} gesetzt", "\u{E182} set") } else { t!("– hineinklicken (optional)", "– click into it (optional)") }).color(if d.pos.is_some() { p.success } else { p.weak }));
                    });
                    crate::app::plan::plan_picker(ui, tex, size, &mut d.pos, &others, next_no);
                }
                // Netzwerktest
                if connected && (ping_on || speed_on) {
                    ui.add_space(8.0);
                    match (&d.net, d.net_rx.is_some()) {
                        (Some(n), _) => {
                            ui.label(RichText::new(net_summary(n)).color(p.text));
                        }
                        (None, true) => {
                            ui.horizontal(|ui| {
                                ui.add(egui::Spinner::new().size(14.0));
                                ui.label(
                                    RichText::new(if speed_on {
                                        t!("Messe Latenz und Durchsatz … (läuft auch nach dem Speichern weiter)", "Measuring latency and throughput … (continues after saving)")
                                    } else {
                                        t!("Messe Latenz …", "Measuring latency …")
                                    })
                                    .color(p.weak),
                                );
                            });
                            ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
                        }
                        (None, false) => {}
                    }
                } else if !connected && (ping_on || speed_on) {
                    ui.add_space(6.0);
                    ui.label(RichText::new(t!("Latenz/Durchsatz: nur wenn der Laptop mit dieser SSID verbunden ist", "Latency/throughput: only when connected to this SSID")).size(11.5).color(p.weak));
                }
                ui.add_space(8.0);
                ui.label(
                    RichText::new(t!(
                        "Tipp: kurz an der Stelle stehen bleiben – gemittelt wird, solange dieses Fenster offen ist.",
                        "Tip: stay at the spot for a moment – values are averaged while this window is open."
                    ))
                    .size(11.5)
                    .color(p.weak),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if primary_button(ui, t!("\u{E316} Speichern  (Enter)", "\u{E316} Save  (Enter)"), p.accent).clicked() {
                        save = true;
                    }
                    if ui.button(t!("Abbrechen  (Esc)", "Cancel  (Esc)")).clicked() {
                        cancel = true;
                    }
                });
            });

        if save {
            self.push_undo(t!("Messpunkt erfasst", "Point added"));
            let scan = self.survey_scan_ctx(&ssid);
            let has_plan = self.meas.as_ref().is_some_and(|m| !m.plans.is_empty());
            let res = self.meas.as_mut().map(|m| {
                let floor = m.floor;
                let p = m.add_point(t_now, d.since_t, &d.name, &d.note, d.pos, &scan);
                if let (true, Some(pos)) = (crate::demo::enabled(), d.pos) {
                    // Demo: realistische Ausleuchtung je nach Position im Plan (höhere Stockwerke schwächer)
                    let v = crate::demo::rssi_at(pos) as f64 - 7.0 * floor as f64;
                    let id = p.id;
                    if let Some(pt) = m.point_mut(id) {
                        pt.rssi = Some(v);
                        pt.min = Some(v as i32 - 2);
                        pt.max = Some(v as i32 + 2);
                        pt.bands = crate::demo::bands_at(pos);
                    }
                }
                let p = m.points.last().expect("eben gespeichert");
                let msg = match (p.rssi, p.quality()) {
                    (Some(v), Some(q)) => tf!("„{}“ gespeichert: {v:.0} dBm – {}", "\"{}\" saved: {v:.0} dBm – {}", p.name, q.label()),
                    _ => tf!("„{}“ gespeichert (kein Messwert)", "\"{}\" saved (no value)", p.name),
                };
                (p.id, msg)
            });
            if let Some((id, msg)) = res {
                self.toast(Sev::Info, msg);
                // Netzwerkwerte: schon da -> übernehmen, sonst nachreichen
                match (d.net.take(), d.net_rx.take()) {
                    (Some(n), _) => {
                        if let Some(pt) = self.meas.as_mut().and_then(|m| m.point_mut(id)) {
                            pt.net = Some(n);
                        }
                    }
                    (None, Some(rx)) => self.net_jobs.push(crate::app::plan::NetJob { id, rx }),
                    _ => {}
                }
                if has_plan && d.pos.is_none() {
                    self.plan_sel = Some(id);
                }
            }
            self.survey_changed();
        } else if !cancel {
            self.meas_point = Some(d);
        }
    }

    // ---------------------------------------------------------------- Liste der Messpunkte

    pub(super) fn points_table(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        let Some(m) = &self.meas else { return };
        let running = m.running;
        ui.horizontal(|ui| {
            ui.label(RichText::new(tf!("Messpunkte ({})", "Measurement points ({})", m.points.len())).strong().color(p.text));
            if running {
                let hint = if m.plan_mode { t!("· Klick in den Plan = neuer Messpunkt", "· click into the plan = new point") } else { t!("· Enter = neuer Messpunkt", "· Enter = new point") };
                ui.label(RichText::new(hint).color(p.weak));
            }
        });
        if m.points.is_empty() {
            ui.add_space(6.0);
            ui.label(
                RichText::new(if running && m.plan_mode {
                    t!(
                        "Noch keine Messpunkte. An deiner Position in den Grundriss klicken, Raum eingeben (optional), Enter.",
                        "No points yet. Click your position on the floor plan, enter the room (optional), Enter."
                    )
                } else if running {
                    t!(
                        "Noch keine Messpunkte. In jedem Raum Enter drücken, Namen eingeben, Enter – der Wert dieser Stelle landet in der Liste.",
                        "No points yet. In each room press Enter, type the name, Enter – the value of that spot is added to the list."
                    )
                } else {
                    t!("Diese Messung hat keine Messpunkte.", "This survey has no measurement points.")
                })
                .color(p.weak),
            );
            return;
        }
        // Hersteller vorab bestimmen (auch für geöffnete Messungen, deren APs nicht mehr in der Liste sind)
        let vendors: Vec<String> = m.points.iter().map(|pt| pt.bssid.map(|b| self.vendor_of_mac(&b)).unwrap_or_default()).collect();
        let started = m.started_wall;
        let mut remove = None;
        let mut changed = false;
        let mut renamed: Vec<usize> = Vec::new();
        let pending: Vec<u64> = self.net_jobs.iter().map(|j| j.id).collect();
        let has_plan = m.plan_mode && !m.plans.is_empty();
        let floor_names: Vec<String> = if m.plans.len() > 1 { m.plans.iter().map(|f| f.title().to_string()).collect() } else { Vec::new() };
        let show_net = !pending.is_empty() || m.points.iter().any(|pt| pt.net.is_some());
        let plan_sel = self.plan_sel;
        let mut pick_for_plan: Option<(u64, Option<usize>)> = None;
        let Some(m) = self.meas.as_mut() else { return };
        egui::ScrollArea::vertical().id_salt("meas_points").auto_shrink(false).show(ui, |ui| {
            let cols = 9 + if show_net { 2 } else { 0 } + if has_plan { 2 } else { 0 };
            egui::Grid::new("meas_points_grid").num_columns(cols).striped(true).spacing([12.0, 5.0]).show(ui, |ui| {
                let mut heads = vec!["#", t!("Raum / Ort", "Room / location"), t!("Signal", "Signal"), t!("Bewertung", "Rating"), "AP", t!("Kanal", "Channel"), t!("Rate", "Rate")];
                if show_net {
                    heads.extend([t!("Ping", "Ping"), "Down / Up"]);
                }
                if has_plan {
                    heads.push("2,4 / 5 / 6 GHz");
                    heads.push(t!("Plan", "Plan"));
                }
                heads.extend([t!("Notiz", "Note"), ""]);
                for h in heads {
                    ui.label(RichText::new(h).small().strong().color(p.weak));
                }
                ui.end_row();
                for (i, pt) in m.points.iter_mut().enumerate() {
                    let wall = (started + chrono::Duration::milliseconds((pt.t * 1000.0) as i64)).format("%H:%M").to_string();
                    let selected = plan_sel == Some(pt.id);
                    ui.label(RichText::new(format!("{}", i + 1)).monospace().color(if selected { p.accent } else { p.weak })).on_hover_text(wall);
                    if ui.add_sized([170.0, 20.0], egui::TextEdit::singleline(&mut pt.name).frame(false)).changed() {
                        changed = true;
                        renamed.push(i);
                    }
                    match (pt.rssi, pt.quality()) {
                        (Some(v), Some(q)) => {
                            let col = quality_color(&p, q);
                            let range = match (pt.min, pt.max) {
                                (Some(a), Some(b)) if a != b => tf!("{a} … {b} dBm, {} Werte", "{a} … {b} dBm, {} values", pt.samples),
                                _ => tf!("{} Werte", "{} values", pt.samples),
                            };
                            ui.label(RichText::new(format!("{v:.0} dBm")).strong().color(col)).on_hover_text(range);
                            ui.horizontal(|ui| badge(ui, q.label(), col)).response.on_hover_text(q.hint());
                        }
                        _ => {
                            ui.label(RichText::new("–").color(p.weak));
                            ui.label(RichText::new(t!("kein Wert", "no value")).color(p.weak));
                        }
                    }
                    let ap = pt.bssid.map(|b| {
                        let short = fmt_mac(&b)[9..].to_string();
                        if vendors[i].is_empty() {
                            short
                        } else {
                            format!("{} · {short}", vendors[i])
                        }
                    });
                    let ap_resp = ui.label(RichText::new(ap.unwrap_or("–".into())).color(p.text));
                    let mut tip = pt.bssid.map(|b| fmt_mac(&b)).unwrap_or_default();
                    if pt.from_scan {
                        tip.push_str(t!("\nnicht verbunden – Wert aus dem Scan (stärkster AP)", "\nnot connected – value from scan (strongest AP)"));
                    }
                    if let (Some(bb), Some(br)) = (pt.best_bssid, pt.best_rssi) {
                        tip.push_str(&tf!("\nstärkster AP: {} ({br} dBm)", "\nstrongest AP: {} ({br} dBm)", fmt_mac(&bb)));
                    }
                    tip.push_str(&tf!("\n{} APs der SSID sichtbar", "\n{} APs of the SSID visible", pt.aps_visible));
                    ap_resp.on_hover_text(tip);
                    ui.label(if pt.channel > 0 { pt.channel.to_string() } else { "–".into() });
                    ui.label(if pt.tx_mbps > 0.0 { format!("{:.0} Mbit/s", pt.tx_mbps) } else { "–".into() });
                    if show_net {
                        net_cells(ui, &p, pt.net.as_ref(), pending.contains(&pt.id));
                    }
                    if has_plan {
                        let f = |v: Option<i32>| v.map(|x| x.to_string()).unwrap_or("–".into());
                        ui.label(RichText::new(format!("{} / {} / {}", f(pt.bands.b24), f(pt.bands.b5), f(pt.bands.b6))).monospace().color(p.text))
                            .on_hover_text(t!("Stärkster AP der SSID je Band an dieser Stelle (dBm)", "Strongest AP of the SSID per band at this spot (dBm)"));
                    }
                    if has_plan {
                        let (txt, tip) = match (pt.pos.is_some(), floor_names.get(pt.floor)) {
                            (true, Some(f)) => (format!("\u{E316} {f}"), t!("Position gesetzt – klicken, um das Stockwerk zu zeigen und den Punkt neu zu setzen", "Position set – click to show its floor and place it again")),
                            (true, None) => ("\u{E316}".to_string(), t!("Position gesetzt – klicken, um sie im Grundriss neu zu setzen", "Position set – click to place it again on the floor plan")),
                            _ => (t!("setzen", "place").to_string(), t!("Im Grundriss platzieren", "Place on the floor plan")),
                        };
                        let b = egui::Button::new(RichText::new(txt).small().color(if selected { Color32::WHITE } else { p.text })).fill(if selected { p.accent } else { p.surface2 });
                        if ui.add(b).on_hover_text(tip).clicked() {
                            pick_for_plan = Some((pt.id, pt.pos.map(|_| pt.floor)));
                        }
                    }
                    changed |= ui
                        .add_sized([180.0, 20.0], egui::TextEdit::singleline(&mut pt.note).frame(false).hint_text(RichText::new("…").color(p.weak)))
                        .changed();
                    if ui.small_button("\u{E4A6}").on_hover_text(t!("Messpunkt löschen", "Delete point")).clicked() {
                        remove = Some(i);
                    }
                    ui.end_row();
                }
            });
        });
        for i in renamed {
            m.sync_point_marker(i);
        }
        if let Some((id, floor)) = pick_for_plan {
            self.plan_sel = Some(id);
            if let (Some(f), Some(m)) = (floor, self.meas.as_mut()) {
                if m.floor != f {
                    m.floor = f;
                    self.heat_tex = None;
                    self.plan_scale = None;
                }
            }
        }
        if remove.is_some() {
            self.push_undo(t!("Messpunkt gelöscht", "Point deleted"));
        }
        let Some(m) = self.meas.as_mut() else { return };
        if let Some(i) = remove {
            m.remove_point(i);
            changed = true;
        }
        if changed {
            self.survey_changed();
        }
    }

    /// Herstellername zu einer BSSID (aus der AP-Liste oder direkt aus der OUI-Datenbank)
    pub(super) fn vendor_of_mac(&self, b: &[u8; 6]) -> String {
        if let Some(r) = self.aps.get(b) {
            if !r.vendor.is_empty() {
                return r.vendor.clone();
            }
        }
        self.oui.lookup(b).map(|v| v.0).unwrap_or_default()
    }

    // ---------------------------------------------------------------- Speichern / Öffnen

    pub(super) fn survey_changed(&mut self) {
        self.meas_dirty_since.get_or_insert_with(Instant::now);
    }

    /// Automatisch speichern: kurz nach Änderungen, während der Messung jede Minute, `force` sofort
    pub(super) fn autosave_survey(&mut self, force: bool) {
        let Some(m) = &self.meas else { return };
        if m.conn.is_empty() && m.best.is_empty() && m.points.is_empty() {
            return;
        }
        let due = force
            || self.meas_dirty_since.is_some_and(|t| t.elapsed() > Duration::from_millis(1500))
            || (m.running && self.meas_last_save.map_or(true, |t| t.elapsed() > Duration::from_secs(60)));
        if !due {
            return;
        }
        let path = self.meas_file.clone().unwrap_or_else(|| unique_path(&survey_dir(), &base_name(m), EXT));
        let text = m.to_file(self.now_t());
        let res = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|_| std::fs::write(&path, text));
        self.meas_last_save = Some(Instant::now());
        match res {
            Ok(()) => {
                self.meas_file = Some(path);
                self.meas_dirty_since = None;
                self.meas_save_error = None;
                self.survey_recent = None;
            }
            Err(e) => {
                if self.meas_save_error.is_none() {
                    self.toast(Sev::Warning, tf!("Messung konnte nicht gespeichert werden: {e}", "Could not save the survey: {e}"));
                }
                self.meas_save_error = Some(e.to_string());
            }
        }
    }

    pub(super) fn save_survey_as(&mut self) {
        let Some(m) = &self.meas else { return };
        let dir = survey_dir();
        let _ = std::fs::create_dir_all(&dir);
        let Some(path) = rfd::FileDialog::new()
            .set_directory(&dir)
            .set_file_name(format!("{}.{EXT}", base_name(m)))
            .add_filter(t!("WiFi-Analyzer-Messung", "WiFi Analyzer survey"), &[EXT])
            .save_file()
        else {
            return;
        };
        match std::fs::write(&path, m.to_file(self.now_t())) {
            Ok(()) => {
                self.toast(Sev::Info, tf!("Messung gespeichert: {}", "Survey saved: {}", path.display()));
                self.meas_file = Some(path);
                self.meas_dirty_since = None;
                self.survey_recent = None;
            }
            Err(e) => self.toast(Sev::Critical, tf!("Speichern fehlgeschlagen: {e}", "Saving failed: {e}")),
        }
    }

    pub(super) fn open_survey_dialog(&mut self) {
        let dir = survey_dir();
        let _ = std::fs::create_dir_all(&dir);
        if let Some(path) = rfd::FileDialog::new()
            .set_directory(&dir)
            .add_filter(t!("WiFi-Analyzer-Messung", "WiFi Analyzer survey"), &[EXT])
            .pick_file()
        {
            self.load_survey(&path);
        }
    }

    pub(super) fn load_survey(&mut self, path: &Path) {
        if self.meas.as_ref().is_some_and(|m| m.running) {
            self.toast(Sev::Warning, t!("Bitte zuerst die laufende Messung stoppen.", "Please stop the running survey first.").into());
            return;
        }
        // ungespeicherte Änderungen der aktuellen Messung sichern
        self.autosave_survey(true);
        let res = std::fs::read_to_string(path).map_err(|e| e.to_string()).and_then(|s| Measurement::from_file(&s));
        match res {
            Ok(m) => {
                let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                self.toast(Sev::Info, tf!("Messung geöffnet: {name} ({} Messpunkte)", "Survey opened: {name} ({} points)", m.points.len()));
                self.meas_ssid = Some(m.ssid.clone());
                if !m.plans.is_empty() {
                    self.last_plans = m.plans.clone();
                }
                self.page = if m.plan_mode { super::Page::PlanSurvey } else { super::Page::Measure };
                self.meas = Some(m);
                self.meas_file = Some(path.to_path_buf());
                self.meas_dirty_since = None;
                self.meas_point = None;

            }
            Err(e) => self.toast(Sev::Critical, tf!("Datei kann nicht geöffnet werden: {e}", "Cannot open file: {e}")),
        }
    }

    /// Gespeicherte Messungen (zwischengespeichert, max. alle 3 s neu eingelesen)
    pub(super) fn recent_surveys(&mut self) -> Vec<RecentSurvey> {
        let fresh = self.survey_recent.as_ref().is_some_and(|(t, _)| t.elapsed() < Duration::from_secs(3));
        if !fresh {
            self.survey_recent = Some((Instant::now(), recent_surveys()));
        }
        self.survey_recent.as_ref().map(|r| r.1.clone()).unwrap_or_default()
    }

    /// Liste „Gespeicherte Messungen“ (auf der leeren Messungs-Seite)
    pub(super) fn recent_surveys_ui(&mut self, ui: &mut Ui, plan_mode: bool) {
        let p = pal(ui);
        let list: Vec<RecentSurvey> = self.recent_surveys().into_iter().filter(|r| r.plan == plan_mode).take(12).collect();
        let mut open = None;
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(if plan_mode { t!("Gespeicherte Grundriss-Messungen", "Saved floor plan surveys") } else { t!("Gespeicherte Messungen", "Saved surveys") })
                    .strong()
                    .color(p.text),
            );
            if ui.small_button(t!("\u{E256} Ordner", "\u{E256} Folder")).on_hover_text(survey_dir().display().to_string()).clicked() {
                let _ = std::fs::create_dir_all(survey_dir());
                open_path(&survey_dir());
            }
        });
        if list.is_empty() {
            ui.label(
                RichText::new(t!(
                    "Noch keine. Jede Messung wird automatisch in „Dokumente\\WiFi Analyzer“ gespeichert.",
                    "None yet. Every survey is saved automatically to \"Documents\\WiFi Analyzer\"."
                ))
                .color(p.weak),
            );
            return;
        }
        for r in &list {
            let name = r.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let when: chrono::DateTime<chrono::Local> = r.modified.into();
            ui.horizontal(|ui| {
                if ui.link(RichText::new(format!("\u{E702} {name}"))).clicked() {
                    open = Some(r.path.clone());
                }
                ui.label(RichText::new(tf!("geändert {}", "modified {}", when.format("%d.%m.%Y %H:%M"))).small().color(p.weak));
            });
        }
        if let Some(path) = open {
            self.load_survey(&path);
        }
    }

    // ---------------------------------------------------------------- Export

    pub(super) fn export_dialog(&self, default_name: &str, filter: &str, ext: &str) -> Option<PathBuf> {
        let dir = survey_dir();
        let _ = std::fs::create_dir_all(&dir);
        rfd::FileDialog::new().set_directory(&dir).set_file_name(default_name).add_filter(filter, &[ext]).save_file()
    }

    pub(super) fn export_points_csv(&mut self) {
        let Some(m) = &self.meas else { return };
        let name = format!("{} {}.csv", base_name(m), t!("Messpunkte", "points"));
        let csv = self.points_csv(m);
        let Some(path) = self.export_dialog(&name, "CSV (Excel)", "csv") else { return };
        match std::fs::write(&path, csv) {
            Ok(_) => self.toast(Sev::Info, tf!("Messpunkte exportiert: {}", "Points exported: {}", path.display())),
            Err(e) => self.toast(Sev::Critical, tf!("Export fehlgeschlagen: {e}", "Export failed: {e}")),
        }
    }

    fn points_csv(&self, m: &Measurement) -> String {
        let header = t!(
            "Nr;Raum / Ort;Uhrzeit;Signal Ø (dBm);Bewertung;Min (dBm);Max (dBm);Messwerte;Quelle;AP (BSSID);Hersteller;Kanal;Datenrate (Mbit/s);Stärkster AP;Stärkster AP (dBm);APs sichtbar;Ping Router Ø (ms);Verlust Router (%);Jitter Router (ms);Ping Internet Ø (ms);Download (Mbit/s);Upload (Mbit/s);2,4 GHz (dBm);5 GHz (dBm);6 GHz (dBm);Stockwerk;Position X (%);Position Y (%);Notiz",
            "No;Room / location;Time;Signal avg (dBm);Rating;Min (dBm);Max (dBm);Samples;Source;AP (BSSID);Vendor;Channel;Data rate (Mbit/s);Strongest AP;Strongest AP (dBm);APs visible;Ping router avg (ms);Loss router (%);Jitter router (ms);Ping internet avg (ms);Download (Mbit/s);Upload (Mbit/s);2.4 GHz (dBm);5 GHz (dBm);6 GHz (dBm);Floor;Position X (%);Position Y (%);Note"
        );
        let mut s = tf!(
            "\u{feff}# WLAN-Messung SSID \"{}\" vom {} (WiFi Analyzer {})\n{header}\n",
            "\u{feff}# Wi-Fi survey SSID \"{}\" on {} (WiFi Analyzer {})\n{header}\n",
            m.ssid,
            m.started_wall.format("%Y-%m-%d %H:%M"),
            env!("CARGO_PKG_VERSION")
        );
        for (i, pt) in m.points.iter().enumerate() {
            let wall = (m.started_wall + chrono::Duration::milliseconds((pt.t * 1000.0) as i64)).format("%H:%M:%S").to_string();
            let opt = |v: Option<i32>| v.map(|x| x.to_string()).unwrap_or_default();
            let dec = |v: Option<f64>| v.map(|x| format!("{x:.1}").replace('.', ",")).unwrap_or_default();
            let gw = pt.net.as_ref().and_then(|n| n.gw.as_ref());
            let inet = pt.net.as_ref().and_then(|n| n.inet.as_ref());
            let row = [
                (i + 1).to_string(),
                pt.name.clone(),
                wall,
                pt.rssi.map(|v| format!("{v:.1}").replace('.', ",")).unwrap_or_default(),
                pt.quality().map(|q| q.label().to_string()).unwrap_or_default(),
                opt(pt.min),
                opt(pt.max),
                pt.samples.to_string(),
                if pt.from_scan { t!("Scan", "Scan") } else { t!("Verbindung", "Connection") }.to_string(),
                pt.bssid.map(|b| fmt_mac(&b)).unwrap_or_default(),
                pt.bssid.map(|b| self.vendor_of_mac(&b)).unwrap_or_default(),
                if pt.channel > 0 { pt.channel.to_string() } else { String::new() },
                if pt.tx_mbps > 0.0 { format!("{:.0}", pt.tx_mbps) } else { String::new() },
                pt.best_bssid.map(|b| fmt_mac(&b)).unwrap_or_default(),
                opt(pt.best_rssi),
                pt.aps_visible.to_string(),
                dec(gw.and_then(|g| g.avg_ms)),
                gw.map(|g| format!("{:.0}", g.loss * 100.0)).unwrap_or_default(),
                dec(gw.and_then(|g| g.jitter_ms)),
                dec(inet.and_then(|g| g.avg_ms)),
                dec(pt.net.as_ref().and_then(|n| n.down_mbps)),
                dec(pt.net.as_ref().and_then(|n| n.up_mbps)),
                opt(pt.bands.b24),
                opt(pt.bands.b5),
                opt(pt.bands.b6),
                m.plans.get(pt.floor).filter(|_| pt.pos.is_some()).map(|f| f.title().to_string()).unwrap_or_default(),
                dec(pt.pos.map(|p| p[0] as f64 * 100.0)),
                dec(pt.pos.map(|p| p[1] as f64 * 100.0)),
                pt.note.clone(),
            ];
            s.push_str(&row.iter().map(|c| esc_csv(c)).collect::<Vec<_>>().join(";"));
            s.push('\n');
        }
        s
    }

    pub(super) fn export_report(&mut self) {
        let Some(m) = &self.meas else { return };
        let name = format!("{} {}.html", base_name(m), t!("Messbericht", "survey report"));
        let html = self.report_html(m);
        let Some(path) = self.export_dialog(&name, t!("Bericht (HTML)", "Report (HTML)"), "html") else { return };
        match std::fs::write(&path, html) {
            Ok(_) => {
                self.toast(Sev::Info, tf!("Bericht erstellt – im Browser mit Strg+P als PDF speichern", "Report created – press Ctrl+P in the browser to save as PDF"));
                open_path(&path);
            }
            Err(e) => self.toast(Sev::Critical, tf!("Export fehlgeschlagen: {e}", "Export failed: {e}")),
        }
    }

    /// Bericht für Kunden: Zusammenfassung, Verlauf, Tabelle der Messpunkte, Hinweise
    fn report_html(&self, m: &Measurement) -> String {
        let s = m.stats(self.now_t());
        let lang = if crate::i18n::is_en() { "en" } else { "de" };
        let pts = &m.points;
        let rated: Vec<Quality> = pts.iter().filter_map(|p| p.quality()).collect();
        let ok = rated.iter().filter(|q| matches!(q, Quality::Excellent | Quality::Good)).count();
        let weak: Vec<&MeasPoint> = pts.iter().filter(|p| p.quality() == Some(Quality::Weak)).collect();
        let avg_pts = {
            let v: Vec<f64> = pts.iter().filter_map(|p| p.rssi).collect();
            (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64)
        };
        let hex = |q: Quality| {
            let (r, g, b) = q.rgb();
            format!("#{r:02x}{g:02x}{b:02x}")
        };

        let mut h = String::new();
        let _ = write!(
            h,
            r#"<!doctype html><html lang="{lang}"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>{title} – {ssid}</title>
<style>
:root{{--fg:#1f2328;--weak:#59636e;--line:#d1d9e0;--bg:#f6f8fa;--accent:#4f46e5;--grad:linear-gradient(90deg,#22d3ee,#6366f1,#d946ef)}}
*{{box-sizing:border-box}} body{{font:14px/1.5 "Segoe UI",system-ui,sans-serif;color:var(--fg);margin:0;background:#fff}}
.page{{max-width:1000px;margin:0 auto;padding:32px 28px}}
header{{display:flex;justify-content:space-between;align-items:flex-start;gap:16px;border-bottom:3px solid;border-image:var(--grad) 1;padding-bottom:14px}}
.brand{{display:flex;align-items:center;gap:12px}} .brand svg{{width:40px;height:40px;flex:none}}
h1{{font-size:26px;margin:0}} h2{{font-size:17px;margin:28px 0 10px}} .weak{{color:var(--weak)}}
.meta{{text-align:right;font-size:13px;color:var(--weak)}}
.kpis{{display:grid;grid-template-columns:repeat(4,1fr);gap:10px;margin-top:18px}}
.kpi{{background:var(--bg);border:1px solid var(--line);border-radius:8px;padding:10px 12px}}
.kpi b{{display:block;font-size:22px}} .kpi span{{font-size:12px;color:var(--weak)}}
table{{width:100%;border-collapse:collapse;font-size:13px}} th{{text-align:left;color:var(--weak);font-weight:600;border-bottom:2px solid var(--line);padding:6px 8px}}
td{{border-bottom:1px solid var(--line);padding:7px 8px;vertical-align:top}} tr:nth-child(even) td{{background:#fafbfc}}
.pill{{display:inline-block;border-radius:999px;padding:1px 10px;color:#fff;font-weight:600;font-size:12px;white-space:nowrap}}
.bar{{height:6px;border-radius:3px;background:#e6eaef;margin-top:4px;width:110px}} .bar i{{display:block;height:6px;border-radius:3px}}
.legend{{display:grid;grid-template-columns:repeat(4,1fr);gap:10px;font-size:12px}} .legend div{{border-left:4px solid;padding:2px 8px}}
ul.rec li{{margin:4px 0}} svg text{{font-family:"Segoe UI",sans-serif}}
footer{{margin-top:32px;padding-top:10px;border-top:1px solid var(--line);font-size:12px;color:var(--weak);display:flex;justify-content:space-between}}
.print{{position:fixed;right:18px;bottom:18px;background:var(--grad);color:#fff;border:0;border-radius:8px;padding:10px 16px;font-size:14px;cursor:pointer}}
.tablewrap{{overflow-x:auto}}
@media (max-width:720px){{.page{{padding:20px 14px}} header{{flex-direction:column}} .meta{{text-align:left}} .kpis,.legend{{grid-template-columns:repeat(2,1fr)}} table{{min-width:640px}}}}
@media print{{.print{{display:none}} .page{{padding:0}} h2{{break-after:avoid}} tr{{break-inside:avoid}} @page{{size:A4;margin:14mm}}}}
</style></head><body><div class="page">
<header><div class="brand">{logo}<div><h1>{title}</h1><div class="weak">{net} <b>{ssid}</b></div></div></div>
<div class="meta">{date_l}: {date}<br>{dur_l}: {dur}<br>{pts_l}: {npts}</div></header>
"#,
            title = t!("WLAN-Messbericht", "Wi-Fi survey report"),
            logo = REPORT_LOGO,
            ssid = esc_html(&m.ssid),
            net = t!("Netzwerk (SSID):", "Network (SSID):"),
            date_l = t!("Datum", "Date"),
            date = m.started_wall.format("%d.%m.%Y, %H:%M"),
            dur_l = t!("Dauer", "Duration"),
            dur = fmt_mmss(s.duration),
            pts_l = t!("Messpunkte", "Measurement points"),
            npts = pts.len(),
        );

        // Kennzahlen
        let avg_txt = avg_pts.or(s.avg).map(|v| format!("{v:.0} dBm")).unwrap_or("–".into());
        let ok_txt = if rated.is_empty() { "–".into() } else { format!("{ok} / {}", rated.len()) };
        let _ = write!(
            h,
            r#"<div class="kpis"><div class="kpi"><b>{avg_txt}</b><span>{}</span></div><div class="kpi"><b>{ok_txt}</b><span>{}</span></div><div class="kpi"><b>{}</b><span>{}</span></div><div class="kpi"><b>{} / {}</b><span>{}</span></div></div>"#,
            t!("Ø Signal an den Messpunkten", "Avg signal at the points"),
            t!("Messpunkte mit gutem Empfang", "Points with good reception"),
            weak.len(),
            t!("Stellen mit schwachem Empfang", "Spots with weak reception"),
            s.roams,
            s.disconnects,
            t!("Roams / Verbindungsabbrüche", "Roams / disconnects"),
        );

        // Verlauf und Grundriss (Grundriss-Messung: Plan zuerst)
        let chart = format!("<h2>{}</h2>{}", t!("Signalverlauf", "Signal over time"), self.report_svg(m));
        let mut plan = String::new();
        let multi = m.plans.len() > 1;
        for (fi, fp) in m.plans.iter().enumerate() {
            for layer in m.layers_with_data(fi) {
                if let Some(svg) = plan_svg(m, fi, layer) {
                    let mut title = match layer {
                        crate::measure::Layer::Measured => t!("Ausleuchtung im Grundriss", "Coverage on the floor plan").to_string(),
                        l => tf!("Ausleuchtung {}", "Coverage {}", l.label()),
                    };
                    if multi {
                        title = format!("{} – {title}", esc_html(fp.title()));
                    }
                    let _ = write!(plan, "<h2>{title}</h2>{svg}");
                }
            }
        }
        if !plan.is_empty() {
            let scale = crate::heatmap::scale();
            let _ = write!(plan, "<div style=\"display:flex;flex-wrap:wrap;gap:6px 14px;font-size:12px;margin-top:8px\">");
            for (d, c) in &scale {
                let _ = write!(plan, "<span><i style=\"display:inline-block;width:12px;height:12px;border-radius:3px;vertical-align:-2px;background:#{:02x}{:02x}{:02x}\"></i> ≥ {d:.0} dBm</span>", c[0], c[1], c[2]);
            }
            if let Some(c) = crate::heatmap::no_signal() {
                let _ = write!(plan, "<span><i style=\"display:inline-block;width:12px;height:12px;border-radius:3px;vertical-align:-2px;background:#{:02x}{:02x}{:02x}\"></i> {}</span>", c[0], c[1], c[2], t!("kein Signal", "no signal"));
            }
            let _ = write!(plan, "</div>");
        }
        if m.plan_mode {
            h.push_str(&plan);
            h.push_str(&chart);
        } else {
            h.push_str(&chart);
            h.push_str(&plan);
        }

        // Tabelle
        let show_net = pts.iter().any(|p| p.net.as_ref().is_some_and(|n| !n.is_empty()));
        let net_heads = if show_net { format!("<th>{}</th><th>Down / Up</th>", t!("Latenz (Router)", "Latency (router)")) } else { String::new() };
        let _ = write!(
            h,
            "<h2>{}</h2><div class=\"tablewrap\"><table><thead><tr><th>#</th><th>{}</th><th>{}</th><th>{}</th><th>{}</th><th>{}</th><th>{}</th>{net_heads}<th>{}</th></tr></thead><tbody>",
            t!("Ergebnisse je Messpunkt", "Results per point"),
            t!("Raum / Ort", "Room / location"),
            t!("Signal", "Signal"),
            t!("Bewertung", "Rating"),
            t!("Zugangspunkt (AP)", "Access point"),
            t!("Kanal", "Channel"),
            t!("Datenrate", "Data rate"),
            t!("Notiz", "Note"),
        );
        if pts.is_empty() {
            let _ = write!(h, r#"<tr><td colspan="10" class="weak">{}</td></tr>"#, t!("Keine Messpunkte erfasst.", "No measurement points recorded."));
        }
        for (i, p) in pts.iter().enumerate() {
            let (sig, pill) = match (p.rssi, p.quality()) {
                (Some(v), Some(q)) => {
                    let pct = ((v + 100.0) / 70.0).clamp(0.03, 1.0) * 100.0;
                    (
                        format!(r#"<b>{v:.0} dBm</b><div class="bar"><i style="width:{pct:.0}%;background:{}"></i></div>"#, hex(q)),
                        format!(r#"<span class="pill" style="background:{}">{}</span>"#, hex(q), q.label()),
                    )
                }
                _ => ("–".into(), String::new()),
            };
            let ap = p
                .bssid
                .map(|b| {
                    let v = self.vendor_of_mac(&b);
                    format!("{}<br><span class=\"weak\">{}</span>", if v.is_empty() { "–".into() } else { esc_html(&v) }, fmt_mac(&b))
                })
                .unwrap_or("–".into());
            let src = if p.from_scan { format!("<br><span class=\"weak\">{}</span>", t!("(nicht verbunden, Scan)", "(not connected, scan)")) } else { String::new() };
            let net = if show_net {
                let n = p.net.as_ref();
                let lat = n.and_then(|n| n.gw.as_ref()).and_then(|g| g.avg_ms.map(|ms| (ms, g.loss)));
                let lat = match lat {
                    Some((ms, loss)) if loss > 0.0 => tf!("{ms:.0} ms<br><span class=\"weak\">{:.0} % Verlust</span>", "{ms:.0} ms<br><span class=\"weak\">{:.0} % loss</span>", loss * 100.0),
                    Some((ms, _)) => format!("{ms:.0} ms"),
                    None => "–".into(),
                };
                let f = |v: Option<f64>| v.map(|x| format!("{x:.0}")).unwrap_or("–".into());
                let du = match n.map(|n| (n.down_mbps, n.up_mbps)) {
                    Some((None, None)) | None => "–".into(),
                    Some((d, u)) => format!("{} / {} Mbit/s", f(d), f(u)),
                };
                format!("<td>{lat}</td><td>{du}</td>")
            } else {
                String::new()
            };
            let _ = write!(
                h,
                "<tr><td>{}</td><td><b>{}</b></td><td>{sig}</td><td>{pill}</td><td>{ap}{src}</td><td>{}</td><td>{}</td>{net}<td>{}</td></tr>",
                i + 1,
                esc_html(&p.name),
                if p.channel > 0 { p.channel.to_string() } else { "–".into() },
                if p.tx_mbps > 0.0 { format!("{:.0} Mbit/s", p.tx_mbps) } else { "–".into() },
                esc_html(&p.note),
            );
        }
        h.push_str("</tbody></table></div>");

        // Legende
        let _ = write!(h, "<h2>{}</h2><div class=\"legend\">", t!("So sind die Werte zu lesen", "How to read the values"));
        for (q, range) in [(Quality::Excellent, "≥ -60 dBm"), (Quality::Good, "-67 … -61 dBm"), (Quality::Fair, "-75 … -68 dBm"), (Quality::Weak, "< -75 dBm")] {
            let _ = write!(h, r#"<div style="border-color:{}"><b>{}</b> · {range}<br><span class="weak">{}</span></div>"#, hex(q), q.label(), q.hint());
        }
        h.push_str("</div>");

        // Hinweise
        let _ = write!(h, "<h2>{}</h2><ul class=\"rec\">", t!("Hinweise", "Notes"));
        for (name, text) in report_notes(m, &s) {
            match name {
                Some(n) => {
                    let _ = write!(h, "<li><b>{}</b>: {}</li>", esc_html(&n), esc_html(&text));
                }
                None => {
                    let _ = write!(h, "<li>{}</li>", esc_html(&text));
                }
            }
        }
        h.push_str("</ul>");

        let _ = write!(
            h,
            r#"<footer><span>{} {}</span><span>WiFi Analyzer {} · wifianalyzer.schoepf-tirol.at</span></footer></div>
<button class="print" onclick="window.print()">{}</button></body></html>"#,
            t!("Erstellt am", "Created on"),
            chrono::Local::now().format("%d.%m.%Y %H:%M"),
            env!("CARGO_PKG_VERSION"),
            t!("🖨 Drucken / als PDF speichern", "🖨 Print / save as PDF"),
        );
        h
    }

    /// Signalverlauf als SVG mit den Messpunkten
    fn report_svg(&self, m: &Measurement) -> String {
        let (w, hgt, l, r, top, bot) = (940.0, 230.0, 46.0, 12.0, 12.0, 28.0);
        let dur = m.elapsed(self.now_t()).max(1.0);
        let x = |t: f64| l + (t / dur).clamp(0.0, 1.0) * (w - l - r);
        let y = |v: f64| top + ((-30.0 - v) / 70.0).clamp(0.0, 1.0) * (hgt - top - bot);
        let mut s = format!(r#"<svg viewBox="0 0 {w} {hgt}" width="100%" xmlns="http://www.w3.org/2000/svg" style="background:#fafbfc;border:1px solid #d1d9e0;border-radius:8px">"#);
        for db in [-40.0, -50.0, -60.0, -70.0, -80.0, -90.0] {
            let _ = write!(s, r##"<line x1="{l}" x2="{}" y1="{y:.1}" y2="{y:.1}" stroke="#e6eaef"/><text x="{}" y="{:.1}" font-size="10" fill="#59636e" text-anchor="end">{db}</text>"##, w - r, l - 6.0, y(db) + 3.0, y = y(db));
        }
        for (db, col) in [(-67.0, "#2da44e"), (-75.0, "#cf222e")] {
            let _ = write!(s, r#"<line x1="{l}" x2="{}" y1="{y:.1}" y2="{y:.1}" stroke="{col}" stroke-dasharray="5 4" opacity=".7"/>"#, w - r, y = y(db));
        }
        // Zeitachse
        let step = [30.0, 60.0, 120.0, 300.0, 600.0, 900.0, 1800.0, 3600.0].into_iter().find(|st| dur / st <= 10.0).unwrap_or(3600.0);
        let mut t = 0.0;
        while t <= dur {
            let _ = write!(s, r##"<text x="{:.1}" y="{}" font-size="10" fill="#59636e" text-anchor="middle">{}</text>"##, x(t), hgt - 10.0, fmt_mmss(t));
            t += step;
        }
        // Linie: Verbindung, sonst Scan-Werte
        let vals: Vec<(f64, f64)> = if m.conn.is_empty() { m.best.iter().map(|b| (b.t, b.rssi as f64)).collect() } else { m.conn.iter().map(|c| (c.t, c.rssi as f64)).collect() };
        if !vals.is_empty() {
            let k = (vals.len() / 900).max(1);
            let pts: Vec<String> = vals.iter().step_by(k).map(|(t, v)| format!("{:.1},{:.1}", x(*t), y(*v))).collect();
            let _ = write!(s, r##"<polyline points="{}" fill="none" stroke="#0969da" stroke-width="1.8" stroke-linejoin="round"/>"##, pts.join(" "));
        }
        for (i, p) in m.points.iter().enumerate() {
            let col = p
                .quality()
                .map(|q| {
                    let (r, g, b) = q.rgb();
                    format!("#{r:02x}{g:02x}{b:02x}")
                })
                .unwrap_or("#59636e".into());
            let px = x(p.t);
            let py = y(p.rssi.unwrap_or(-95.0));
            let _ = write!(
                s,
                r##"<line x1="{px:.1}" x2="{px:.1}" y1="{top}" y2="{}" stroke="{col}" opacity=".35"/><circle cx="{px:.1}" cy="{py:.1}" r="9" fill="{col}"/><text x="{px:.1}" y="{:.1}" font-size="10" font-weight="700" fill="#fff" text-anchor="middle">{}</text>"##,
                hgt - bot,
                py + 3.5,
                i + 1
            );
        }
        s.push_str("</svg>");
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_escaping_defuses_formulas_but_keeps_numbers() {
        assert_eq!(esc_csv("Büro"), "Büro");
        assert_eq!(esc_csv("-58"), "-58");
        assert_eq!(esc_csv("-63,5"), "-63,5");
        assert_eq!(esc_csv("=HYPERLINK(\"x\")"), "\"'=HYPERLINK(\"\"x\"\")\"");
        assert_eq!(esc_csv("+49 123"), "'+49 123");
        assert_eq!(esc_csv("a;b"), "\"a;b\"");
    }

    #[test]
    fn unique_path_does_not_overwrite() {
        let dir = std::env::temp_dir().join(format!("wa-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = unique_path(&dir, "2026-11-08 10-00 Net", EXT);
        assert!(a.ends_with("2026-11-08 10-00 Net.wifisurvey"));
        std::fs::write(&a, "x").unwrap();
        let b = unique_path(&dir, "2026-11-08 10-00 Net", EXT);
        assert!(b.ends_with("2026-11-08 10-00 Net (2).wifisurvey"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
