//! Messpunkte während einer Messung (Enter -> Raum/Ort eingeben), Liste der Messpunkte,
//! Messungen speichern/öffnen (automatisch in „Dokumente\WiFi Analyzer“) und Export
//! als CSV (Excel) oder als Bericht für Kunden (HTML, im Browser als PDF druckbar).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use eframe::egui::{self, Align2, Color32, Margin, RichText, Ui};

use crate::app::{Sev, WifiApp};
use crate::measure::{MeasPoint, Measurement, Quality, ScanContext};
use crate::model::fmt_mac;
use crate::theme::{pal, Palette};
use crate::widgets::{badge, fmt_mmss, primary_button};
use crate::{t, tf};

/// Dateiendung gespeicherter Messungen
pub(super) const EXT: &str = "wifisurvey";

/// Eingabe im Dialog „Messpunkt erfassen“
pub(crate) struct PointDraft {
    name: String,
    note: String,
    /// Programmzeit beim Öffnen – ab dann wird gemittelt
    since_t: f64,
    focus: bool,
}

/// Eintrag der Liste „Gespeicherte Messungen“
#[derive(Clone)]
pub(crate) struct RecentSurvey {
    pub path: PathBuf,
    pub modified: SystemTime,
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
fn base_name(m: &Measurement) -> String {
    format!("{} {}", m.started_wall.format("%Y-%m-%d %H-%M"), safe_name(&m.ssid))
}

fn recent_surveys() -> Vec<RecentSurvey> {
    let Ok(rd) = std::fs::read_dir(survey_dir()) else { return Vec::new() };
    let mut v: Vec<RecentSurvey> = rd
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case(EXT)))
        .filter_map(|e| Some(RecentSurvey { path: e.path(), modified: e.metadata().ok()?.modified().ok()? }))
        .collect();
    v.sort_by(|a, b| b.modified.cmp(&a.modified));
    v.truncate(12);
    v
}

/// Datei/Ordner mit dem Standardprogramm öffnen (Bericht im Browser, Ordner im Explorer)
pub(super) fn open_path(path: &Path) {
    let _ = std::process::Command::new("explorer").arg(path).spawn();
}

fn esc_csv(s: &str) -> String {
    if s.contains(';') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
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
        ScanContext { best, aps_visible: aps.len() }
    }

    pub(super) fn open_point_dialog(&mut self) {
        if self.meas.as_ref().is_some_and(|m| m.running) && self.meas_point.is_none() {
            self.meas_point = Some(PointDraft { name: String::new(), note: String::new(), since_t: self.now_t(), focus: true });
        }
    }

    pub(super) fn point_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.meas_point.take() else { return };
        let Some(m) = self.meas.as_ref().filter(|m| m.running) else { return };
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
        egui::Window::new(RichText::new(t!("📍 Messpunkt erfassen", "📍 Add measurement point")).strong())
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
                if enter && (r1.lost_focus() || r2.lost_focus()) {
                    save = true;
                }
                if escape {
                    cancel = true;
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
                    if primary_button(ui, t!("📍 Speichern  (Enter)", "📍 Save  (Enter)"), p.accent).clicked() {
                        save = true;
                    }
                    if ui.button(t!("Abbrechen  (Esc)", "Cancel  (Esc)")).clicked() {
                        cancel = true;
                    }
                });
            });

        if save {
            let scan = self.survey_scan_ctx(&ssid);
            let msg = self.meas.as_mut().map(|m| {
                let p = m.add_point(t_now, d.since_t, &d.name, &d.note, &scan);
                match (p.rssi, p.quality()) {
                    (Some(v), Some(q)) => tf!("„{}“ gespeichert: {v:.0} dBm – {}", "\"{}\" saved: {v:.0} dBm – {}", p.name, q.label()),
                    _ => tf!("„{}“ gespeichert (kein Messwert)", "\"{}\" saved (no value)", p.name),
                }
            });
            if let Some(msg) = msg {
                self.toast(Sev::Info, msg);
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
                ui.label(RichText::new(t!("· Enter = neuer Messpunkt", "· Enter = new point")).color(p.weak));
            }
        });
        if m.points.is_empty() {
            ui.add_space(6.0);
            ui.label(
                RichText::new(if running {
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
        let Some(m) = self.meas.as_mut() else { return };
        egui::ScrollArea::vertical().id_salt("meas_points").auto_shrink(false).show(ui, |ui| {
            egui::Grid::new("meas_points_grid").num_columns(9).striped(true).spacing([12.0, 5.0]).show(ui, |ui| {
                for h in ["#", t!("Raum / Ort", "Room / location"), t!("Signal", "Signal"), t!("Bewertung", "Rating"), "AP", t!("Kanal", "Channel"), t!("Rate", "Rate"), t!("Notiz", "Note"), ""] {
                    ui.label(RichText::new(h).small().strong().color(p.weak));
                }
                ui.end_row();
                for (i, pt) in m.points.iter_mut().enumerate() {
                    let wall = (started + chrono::Duration::milliseconds((pt.t * 1000.0) as i64)).format("%H:%M").to_string();
                    ui.label(RichText::new(format!("{}", i + 1)).monospace().color(p.weak)).on_hover_text(wall);
                    changed |= ui.add_sized([170.0, 20.0], egui::TextEdit::singleline(&mut pt.name).frame(false)).changed();
                    match (pt.rssi, pt.quality()) {
                        (Some(v), Some(q)) => {
                            let col = quality_color(&p, q);
                            let range = match (pt.min, pt.max) {
                                (Some(a), Some(b)) if a != b => tf!("{a} … {b} dBm, {} Werte", "{a} … {b} dBm, {} values", pt.samples),
                                _ => tf!("{} Werte", "{} values", pt.samples),
                            };
                            ui.label(RichText::new(format!("{v:.0} dBm")).strong().monospace().color(col)).on_hover_text(range);
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
                    changed |= ui
                        .add_sized([180.0, 20.0], egui::TextEdit::singleline(&mut pt.note).frame(false).hint_text(RichText::new("…").color(p.weak)))
                        .changed();
                    if ui.small_button("🗑").on_hover_text(t!("Messpunkt löschen", "Delete point")).clicked() {
                        remove = Some(i);
                    }
                    ui.end_row();
                }
            });
        });
        if let Some(i) = remove {
            m.remove_point(i);
            changed = true;
        }
        if changed {
            self.survey_changed();
        }
    }

    /// Herstellername zu einer BSSID (aus der AP-Liste oder direkt aus der OUI-Datenbank)
    fn vendor_of_mac(&self, b: &[u8; 6]) -> String {
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
        let path = self.meas_file.clone().unwrap_or_else(|| survey_dir().join(format!("{}.{EXT}", base_name(m))));
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
                self.meas = Some(m);
                self.meas_file = Some(path.to_path_buf());
                self.meas_dirty_since = None;
                self.meas_point = None;
                self.page = super::Page::Measure;
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
    pub(super) fn recent_surveys_ui(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        let list = self.recent_surveys();
        let mut open = None;
        ui.horizontal(|ui| {
            ui.label(RichText::new(t!("Gespeicherte Messungen", "Saved surveys")).strong().color(p.text));
            if ui.small_button(t!("📂 Ordner", "📂 Folder")).on_hover_text(survey_dir().display().to_string()).clicked() {
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
                if ui.link(RichText::new(format!("📄 {name}"))).clicked() {
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

    fn export_dialog(&self, default_name: &str, filter: &str, ext: &str) -> Option<PathBuf> {
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
            "Nr;Raum / Ort;Uhrzeit;Signal Ø (dBm);Bewertung;Min (dBm);Max (dBm);Messwerte;Quelle;AP (BSSID);Hersteller;Kanal;Datenrate (Mbit/s);Stärkster AP;Stärkster AP (dBm);APs sichtbar;Notiz",
            "No;Room / location;Time;Signal avg (dBm);Rating;Min (dBm);Max (dBm);Samples;Source;AP (BSSID);Vendor;Channel;Data rate (Mbit/s);Strongest AP;Strongest AP (dBm);APs visible;Note"
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
        let fair: Vec<&MeasPoint> = pts.iter().filter(|p| p.quality() == Some(Quality::Fair)).collect();
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
:root{{--fg:#1f2328;--weak:#59636e;--line:#d1d9e0;--bg:#f6f8fa;--accent:#0969da}}
*{{box-sizing:border-box}} body{{font:14px/1.5 "Segoe UI",system-ui,sans-serif;color:var(--fg);margin:0;background:#fff}}
.page{{max-width:1000px;margin:0 auto;padding:32px 28px}}
header{{display:flex;justify-content:space-between;align-items:flex-start;gap:16px;border-bottom:3px solid var(--accent);padding-bottom:14px}}
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
.print{{position:fixed;right:18px;bottom:18px;background:var(--accent);color:#fff;border:0;border-radius:8px;padding:10px 16px;font-size:14px;cursor:pointer}}
.tablewrap{{overflow-x:auto}}
@media (max-width:720px){{.page{{padding:20px 14px}} header{{flex-direction:column}} .meta{{text-align:left}} .kpis,.legend{{grid-template-columns:repeat(2,1fr)}} table{{min-width:640px}}}}
@media print{{.print{{display:none}} .page{{padding:0}} h2{{break-after:avoid}} tr{{break-inside:avoid}} @page{{size:A4;margin:14mm}}}}
</style></head><body><div class="page">
<header><div><h1>{title}</h1><div class="weak">{net} <b>{ssid}</b></div></div>
<div class="meta">{date_l}: {date}<br>{dur_l}: {dur}<br>{pts_l}: {npts}</div></header>
"#,
            title = t!("WLAN-Messbericht", "Wi-Fi survey report"),
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

        // Verlauf
        h.push_str(&format!("<h2>{}</h2>", t!("Signalverlauf", "Signal over time")));
        h.push_str(&self.report_svg(m));

        // Tabelle
        let _ = write!(
            h,
            "<h2>{}</h2><div class=\"tablewrap\"><table><thead><tr><th>#</th><th>{}</th><th>{}</th><th>{}</th><th>{}</th><th>{}</th><th>{}</th><th>{}</th></tr></thead><tbody>",
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
            let _ = write!(h, r#"<tr><td colspan="8" class="weak">{}</td></tr>"#, t!("Keine Messpunkte erfasst.", "No measurement points recorded."));
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
            let _ = write!(
                h,
                "<tr><td>{}</td><td><b>{}</b></td><td>{sig}</td><td>{pill}</td><td>{ap}{src}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
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
        if pts.is_empty() {
            let _ = write!(h, "<li>{}</li>", t!("Für eine raumweise Bewertung bei der nächsten Messung Messpunkte setzen.", "Add measurement points next time for a room-by-room assessment."));
        } else if weak.is_empty() && fair.is_empty() {
            let _ = write!(h, "<li>{}</li>", t!("An allen Messpunkten ist der Empfang gut – keine Maßnahmen nötig.", "Reception is good at all points – no action needed."));
        }
        for p in &weak {
            let _ = write!(
                h,
                "<li>{}</li>",
                tf!(
                    "<b>{}</b>: schwacher Empfang ({:.0} dBm). Empfohlen: zusätzlicher Access Point oder bessere Position des nächstgelegenen APs.",
                    "<b>{}</b>: weak reception ({:.0} dBm). Recommended: an additional access point or a better position for the nearest AP.",
                    esc_html(&p.name),
                    p.rssi.unwrap_or_default()
                )
            );
        }
        for p in &fair {
            let _ = write!(
                h,
                "<li>{}</li>",
                tf!(
                    "<b>{}</b>: ausreichend ({:.0} dBm) – für Telefonie und Video ggf. verbessern.",
                    "<b>{}</b>: fair ({:.0} dBm) – consider improving for voice and video.",
                    esc_html(&p.name),
                    p.rssi.unwrap_or_default()
                )
            );
        }
        if s.disconnects > 0 {
            let _ = write!(
                h,
                "<li>{}</li>",
                tf!(
                    "Während der Messung gab es {} Verbindungsabbrüche.",
                    "There were {} disconnects during the survey.",
                    s.disconnects
                )
            );
        }
        if s.sticky > 0 {
            let _ = write!(
                h,
                "<li>{}</li>",
                t!(
                    "Das Gerät blieb zeitweise an einem schwächeren AP („Sticky Client“). 802.11k/v und eine Mindest-Signalstärke am AP können helfen.",
                    "The device stayed on a weaker AP at times (\"sticky client\"). 802.11k/v and a minimum RSSI on the AP can help."
                )
            );
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
