//! Messung (Walk-Survey): Live-Anzeige, Kennzahlen, Verlauf mit Roams/Markierungen, Ereignisliste.

use eframe::egui::{self, Align2, Color32, RichText, Ui};
use egui_plot::{HLine, Line, LineStyle, Plot, PlotPoint, PlotPoints, Points, Text, VLine};

use crate::app::WifiApp;
use crate::measure::EventKind;
use crate::model::*;
use crate::theme::pal;
use crate::widgets::{card, fmt_mmss, kpi, page_header, primary_button, signal_gauge};
use crate::{t, tf};

impl WifiApp {
    /// Messungs-Seite: `plan_mode = false` Rundgang (Verlauf), `true` Grundriss-Messung (Heatmap)
    pub(crate) fn page_measure(&mut self, ui: &mut Ui, plan_mode: bool) {
        let p = pal(ui);
        let t_now = self.now_t();
        // Vorbereitung der Grundriss-Messung gehört nicht auf die Rundgang-Seite: Pläne merken
        if !plan_mode && self.meas.as_ref().is_some_and(|m| m.draft) {
            if let Some(m) = self.meas.take() {
                if !m.plans.is_empty() {
                    self.last_plans = m.plans;
                }
            }
            self.plan_scale = None;
        }
        // Es ist immer nur eine Messung offen – gehört sie zur anderen Seite, dorthin verweisen
        if let Some(other) = self.meas.as_ref().filter(|m| m.plan_mode != plan_mode) {
            let other_running = other.running;
            let other_name = other.ssid.clone();
            self.other_survey_notice(ui, plan_mode, other_running, &other_name);
            return;
        }
        // Grundriss-Messung: geführter Ablauf in eigenen Schritten
        if plan_mode {
            self.page_plan_survey(ui);
            return;
        }
        let running = self.meas.as_ref().is_some_and(|m| m.running);
        let choices = self.ssid_choices();
        if self.meas_ssid.is_none() {
            // Vorauswahl: verbundene SSID, sonst die stärkste
            self.meas_ssid = self.conn.as_ref().map(|c| c.ssid.clone()).or_else(|| choices.first().map(|c| c.0.clone()));
        }
        let ssid = self.meas.as_ref().map(|m| m.ssid.clone()).or_else(|| self.meas_ssid.clone()).unwrap_or_default();
        let connected_ssid = self.conn.as_ref().map(|c| c.ssid.clone());
        // Grundriss-Messung ohne offene Messung: Vorbereitung (Pläne laden, Maßstab setzen)
        if plan_mode && self.meas.is_none() {
            self.meas = Some(crate::measure::Measurement::draft(ssid.clone(), self.last_plans.clone()));
        }
        let draft = self.meas.as_ref().is_some_and(|m| m.draft);

        // Enter = Messpunkt erfassen (solange nicht gerade in ein Feld getippt wird)
        if running && self.meas_point.is_none() && self.plan_sel.is_none() && !ui.ctx().wants_keyboard_input() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            self.open_point_dialog();
        }

        let (mut start, mut stop, mut discard, mut add_point) = (false, false, false, false);
        let (mut export_all, mut export_points, mut export_report, mut save_as, mut open) = (false, false, false, false, false);
        let mut export_pdf = false;
        let mut new_ssid = None;
        let mut follow = self.meas_follow;
        let (mut ping_on, mut speed_on) = (self.settings.point_ping, self.settings.point_speed);
        let has_meas = self.meas.is_some() && !draft;
        let confirm = self.meas_discard_confirm;
        // Vollbild (F11): nur Messpunkt-Zeile und Plan
        let fullscreen = plan_mode && self.plan_fullscreen;
        let live_now: Option<i32> = self.conn.as_ref().filter(|c| c.ssid == ssid).and_then(|c| c.rssi).or(self.best_ap_of(&ssid).map(|b| b.info.rssi));
        let connected_now = self.conn.as_ref().is_some_and(|c| c.ssid == ssid);
        if !fullscreen {
        page_header(
            ui,
            if plan_mode { t!("Grundriss-Messung", "Floor plan survey") } else { t!("Messung", "Survey") },
            if plan_mode && draft {
                t!("Pläne laden, je Stockwerk den Maßstab setzen, Messung starten – dann in den Plan klicken, wo du stehst", "Load the plans, set the scale per floor, start – then click where you stand on the plan")
            } else if plan_mode {
                t!("In den Plan klicken, wo du stehst – die Heatmap entsteht live", "Click where you stand on the plan – the heatmap builds up live")
            } else {
                t!("Rundgang mit Verlauf, Roaming und Messpunkten pro Raum", "Walk survey with timeline, roaming and points per room")
            },
            |ui| {
                if has_meas {
                    ui.menu_button(t!("\u{E20C} Export", "\u{E20C} Export"), |ui| {
                        if ui.button(t!("\u{E702} Bericht für Kunden als PDF", "\u{E702} Customer report as PDF")).clicked() {
                            export_pdf = true;
                            ui.close_menu();
                        }
                        if ui.button(t!("\u{E288} Bericht als Webseite (HTML)", "\u{E288} Report as web page (HTML)")).clicked() {
                            export_report = true;
                            ui.close_menu();
                        }
                        if ui.button(t!("\u{E198} Messpunkte als Liste (CSV / Excel)", "\u{E198} Points as list (CSV / Excel)")).clicked() {
                            export_points = true;
                            ui.close_menu();
                        }
                        if ui.button(t!("\u{E154} Alle Messwerte (CSV / Excel)", "\u{E154} All samples (CSV / Excel)")).clicked() {
                            export_all = true;
                            ui.close_menu();
                        }
                    });
                    if ui.button(t!("\u{E248} Speichern unter …", "\u{E248} Save as …")).on_hover_text(t!("Wird ohnehin automatisch gespeichert", "Saved automatically anyway")).clicked() {
                        save_as = true;
                    }
                }
                if !running && ui.button(t!("\u{E256} Öffnen", "\u{E256} Open")).on_hover_text(t!("Gespeicherte Messung öffnen", "Open a saved survey")).clicked() {
                    open = true;
                }
                if !running && has_meas {
                    let label = if confirm { t!("\u{E4A6} Wirklich schließen?", "\u{E4A6} Really close?") } else { t!("\u{E4F6} Schließen", "\u{E4F6} Close") };
                    if ui.button(label).on_hover_text(t!("Die Messung bleibt gespeichert", "The survey stays saved")).clicked() {
                        discard = true;
                    }
                }
                if running {
                    if crate::widgets::outline_button(ui, t!("\u{E46C} Messung stoppen", "\u{E46C} Stop survey"), p.danger).clicked() {
                        stop = true;
                    }
                } else if ui.add_enabled_ui(!ssid.is_empty(), |ui| primary_button(ui, t!("\u{E3D0} Messung starten", "\u{E3D0} Start survey"), p.accent)).inner.clicked() {
                    start = true;
                }
                ui.add_enabled_ui(!running, |ui| {
                    egui::ComboBox::from_id_salt("meas_ssid")
                        .width(220.0)
                        .selected_text(if ssid.is_empty() { "–" } else { ssid.as_str() })
                        .show_ui(ui, |ui| {
                            for (s, r) in &choices {
                                let tag = if connected_ssid.as_deref() == Some(s.as_str()) { "  \u{E2E2}" } else { "" };
                                if ui.selectable_label(*s == ssid, format!("{s}  ({r} dBm){tag}")).clicked() {
                                    new_ssid = Some(s.clone());
                                }
                            }
                        });
                });
                ui.label(RichText::new("SSID").color(p.weak));
            },
        );
        }
        if let Some(s) = new_ssid {
            self.meas_ssid = Some(s);
        }

        // Messpunkte
        let undo_label = if draft { None } else { self.undo_label() };
        let mut undo = false;
        ui.horizontal(|ui| {
            // während der Messung ist „Messpunkt erfassen“ die Hauptaktion (in der Vorbereitung ausgeblendet)
            if !draft && ui.add_enabled_ui(running, |ui| primary_button(ui, t!("\u{E316} Messpunkt erfassen", "\u{E316} Add measurement point"), p.accent)).inner.clicked() {
                add_point = true;
            }
            if plan_mode && !draft {
                // Live-Wert und Kennzahlen direkt in dieser Zeile
                self.plan_live_inline(ui, live_now, connected_now);
            } else if draft {
                ui.label(RichText::new(t!("Pro Messpunkt zusätzlich messen:", "Also measure at each point:")).color(p.weak));
            } else {
            ui.label(
                RichText::new(if running {
                    t!("oder Enter drücken – Raum eingeben – Enter", "or press Enter – type the room – Enter")
                } else {
                    t!("Messpunkte lassen sich während einer laufenden Messung erfassen", "Points can be added while a survey is running")
                })
                .color(p.weak),
            );
            }
            if !draft {
                ui.separator();
            }
            if !plan_mode {
                ui.checkbox(&mut follow, t!("Nur letzte 5 min", "Last 5 min only"));
                ui.separator();
            }
            ui.checkbox(&mut ping_on, t!("Latenz", "Latency"))
                .on_hover_text(t!("Pro Messpunkt: Ping zum Router und ins Internet (1.1.1.1), ca. 3 s", "Per point: ping to the router and to the internet (1.1.1.1), approx. 3 s"));
            ui.checkbox(&mut speed_on, t!("Speedtest", "Speed test")).on_hover_text(t!(
                "Pro Messpunkt: Download/Upload über das Internet (speed.cloudflare.com), ca. 12 s und bis zu 70 MB – nur mit Verbindung zur gemessenen SSID",
                "Per point: download/upload via the internet (speed.cloudflare.com), approx. 12 s and up to 70 MB – only when connected to the surveyed SSID"
            ));
            if fullscreen && running && crate::widgets::outline_button(ui, t!("\u{E46C} Stoppen", "\u{E46C} Stop"), p.danger).clicked() {
                stop = true;
            }
            if let Some(what) = undo_label.as_deref() {
                ui.separator();
                if ui.button(t!("\u{E08A} Rückgängig", "\u{E08A} Undo")).on_hover_text(tf!("Strg+Z: {what}", "Ctrl+Z: {what}")).clicked() {
                    undo = true;
                }
            }
            if let Some(f) = &self.meas_file {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let name = f.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                    let (txt, col) = match (&self.meas_save_error, self.meas_dirty_since.is_some()) {
                        (Some(_), _) => (tf!("\u{E4E0} {name} – nicht gespeichert", "\u{E4E0} {name} – not saved"), p.danger),
                        (None, true) => (tf!("\u{E248} {name} …", "\u{E248} {name} …"), p.weak),
                        (None, false) => (tf!("\u{E182} {name}", "\u{E182} {name}"), p.weak),
                    };
                    if ui.link(RichText::new(txt).small().color(col)).on_hover_text(f.display().to_string()).clicked() {
                        if let Some(dir) = f.parent() {
                            crate::app::survey::open_path(dir);
                        }
                    }
                });
            }
        });
        if undo {
            self.undo_points();
        }
        self.meas_follow = follow;
        self.settings.point_ping = ping_on;
        self.settings.point_speed = speed_on;
        if !discard {
            self.meas_discard_confirm = false;
        }

        if start {
            // Vorbereitung: erst den Maßstab je Stockwerk setzen (sonst stimmt die Heatmap nicht)
            let unscaled = if draft { self.meas.as_ref().and_then(|m| m.plans.iter().position(|f| f.width_m.is_none())) } else { None };
            match unscaled {
                Some(i) => {
                    let name = self.meas.as_ref().map(|m| m.plans[i].title().to_string()).unwrap_or_default();
                    self.plan_start_scale(i);
                    self.toast(
                        crate::app::Sev::Warning,
                        tf!(
                            "Für „{name}“ fehlt noch der Maßstab – z. B. eine Tür abmessen. Oder „ohne Maßstab starten“.",
                            "\"{name}\" has no scale yet – e.g. measure a door. Or \"start without scale\"."
                        ),
                    );
                }
                None => self.start_survey(ssid.clone(), plan_mode),
            }
        }
        if stop {
            self.stop_measurement();
        }
        if discard {
            // ohne Messpunkte direkt schließen, sonst erst nach Bestätigung (gespeichert ist sie ohnehin)
            let has_points = self.meas.as_ref().is_some_and(|m| !m.points.is_empty());
            if confirm || !has_points {
                self.autosave_survey(true);
                self.meas = None;
                self.meas_file = None;
                self.meas_discard_confirm = false;
                self.apply_scan_interval();
            } else {
                self.meas_discard_confirm = true;
            }
        }
        if add_point {
            self.open_point_dialog();
        }
        if export_all {
            self.export_measurement();
        }
        if export_points {
            self.export_points_csv();
        }
        if export_pdf {
            self.export_pdf();
        }
        if export_report {
            self.export_report();
        }
        if save_as {
            self.save_survey_as();
        }
        if open {
            self.open_survey_dialog();
        }
        ui.add_space(8.0);

        // ---------- Live-Anzeige + Kennzahlen ----------
        let conn_here = self.conn.as_ref().filter(|c| c.ssid == ssid);
        let best = self.best_ap_of(&ssid);
        let live_rssi = conn_here.and_then(|c| c.rssi).or(best.map(|b| b.info.rssi));
        let mut start_cta = false;
        {
        ui.horizontal(|ui| {
            card(ui, None, |ui| {
                ui.set_width(270.0);
                ui.vertical_centered(|ui| signal_gauge(ui, live_rssi, false, 0.58));
                match conn_here {
                    Some(c) => {
                        let ap = self.aps.get(&c.bssid);
                        ui.label(RichText::new(tf!("Verbunden mit {}", "Connected to {}", fmt_mac(&c.bssid))).strong().color(ap_color(&c.bssid)));
                        ui.label(
                            RichText::new(tf!(
                                "{} · Kanal {} · {:.0} Mbit/s",
                                "{} · channel {} · {:.0} Mbit/s",
                                ap.map(Self::vendor_label).unwrap_or_default(),
                                c.channel.map(|v| v.to_string()).unwrap_or("?".into()),
                                c.tx_mbps
                            ))
                            .color(p.weak),
                        );
                        if let (Some(b), Some(cr)) = (best, c.rssi) {
                            if b.info.bssid != c.bssid {
                                let d = b.info.rssi - cr;
                                let col = if d >= 10 { p.danger } else if d >= 5 { p.warning } else { p.weak };
                                ui.label(RichText::new(tf!("Bester AP: {} {} dBm ({:+} dB)", "Best AP: {} {} dBm ({:+} dB)", fmt_mac(&b.info.bssid), b.info.rssi, d)).color(col));
                            } else {
                                ui.label(RichText::new(t!("\u{E182} Verbunden mit dem stärksten AP", "\u{E182} Connected to the strongest AP")).color(p.success));
                            }
                        }
                    }
                    None => {
                        ui.label(RichText::new(t!("Nicht mit dieser SSID verbunden", "Not connected to this SSID")).strong().color(p.warning));
                        ui.label(
                            RichText::new(t!(
                                "Angezeigt wird der stärkste AP aus dem Scan. Für die Roaming-Messung mit der SSID verbinden.",
                                "Showing the strongest AP from the scan. Connect to the SSID to measure roaming."
                            ))
                            .size(12.0)
                            .color(p.weak),
                        );
                    }
                }
                if self.meas.as_ref().is_some_and(|m| m.running && m.is_sticky()) {
                    ui.label(RichText::new(t!("\u{E4E0} Sticky Client: besserer AP verfügbar", "\u{E4E0} Sticky client: a better AP is available")).strong().color(p.danger));
                }
            });
            ui.add_space(6.0);
            match &self.meas {
                Some(m) => {
                    let s = m.stats(t_now);
                    let pct = |v: f64| format!("{:.0} %", v * 100.0);
                    // 6 Kacheln je Reihe in die verbleibende Breite einpassen (Rahmen + Abstand ≈ 38 pt)
                    let w = ((ui.available_width() - 6.0 * 38.0) / 6.0).clamp(78.0, 150.0);
                    ui.vertical(|ui| {
                        ui.horizontal_wrapped(|ui| {
                            let state = if m.running { t!("läuft", "running") } else { t!("beendet", "stopped") };
                            kpi(ui, &tf!("Dauer ({state})", "Duration ({state})"), &fmt_mmss(s.duration), "", if m.running { p.success } else { p.text }, w);
                            let src = if s.from_scan { t!("aus dem Scan", "from scan") } else { "" };
                            kpi(ui, t!("Ø Signal", "Avg signal"), &s.avg.map(|v| format!("{v:.0} dBm")).unwrap_or("–".into()), src, s.avg.map(|v| signal_color(v as i32)).unwrap_or(p.text), w);
                            let mm = match (s.min, s.max) {
                                (Some(a), Some(b)) => format!("{a} / {b}"),
                                _ => "–".into(),
                            };
                            kpi(ui, "Min / Max", &mm, "dBm", s.min.map(signal_color).unwrap_or(p.text), w);
                            kpi(ui, t!("Gut (≥ -67 dBm)", "Good (≥ -67 dBm)"), &pct(s.good), "", p.success, w);
                            kpi(ui, "-75 … -67 dBm", &pct(s.fair), "", p.warning, w);
                            kpi(ui, t!("Schwach (< -75)", "Weak (< -75)"), &pct(s.bad), "", p.danger, w);
                        });
                        ui.horizontal_wrapped(|ui| {
                            kpi(ui, "Roams", &s.roams.to_string(), "", p.accent, w);
                            kpi(ui, t!("Abbrüche", "Disconnects"), &s.disconnects.to_string(), "", if s.disconnects > 0 { p.danger } else { p.text }, w);
                            kpi(ui, t!("Sticky-Phasen", "Sticky phases"), &s.sticky.to_string(), "", if s.sticky > 0 { p.warning } else { p.text }, w);
                            kpi(ui, t!("APs genutzt", "APs used"), &s.aps_used.to_string(), "", p.text, w);
                            kpi(ui, t!("Messwerte", "Samples"), &s.samples.to_string(), "", p.text, w);
                            kpi(ui, t!("Messpunkte", "Points"), &m.points.len().to_string(), "", p.purple, w);
                        });
                    });
                }
                None if plan_mode => {}
                None => {
                    card(ui, Some(t!("So funktioniert die Messung", "How the survey works")), |ui| {
                        ui.set_max_width(640.0);
                        ui.label(t!("1.  SSID wählen – am besten die, mit der der Laptop verbunden ist (\u{E2E2})", "1.  Choose the SSID – ideally the one the laptop is connected to (\u{E2E2})"));
                        ui.label(t!("2.  „Messung starten“ und mit dem Laptop die Räume ablaufen", "2.  Click \"Start survey\" and walk through the rooms with the laptop"));
                        ui.label(t!("3.  In jedem Raum Enter drücken, Raum eingeben, Enter – das ist ein Messpunkt", "3.  In each room press Enter, type the room, Enter – that's a measurement point"));
                        ui.label(t!("4.  Stoppen – alles wird automatisch gespeichert; Export als Bericht (PDF) oder Liste (Excel)", "4.  Stop – everything is saved automatically; export as report (PDF) or list (Excel)"));
                        ui.add_space(8.0);
                        let label = if ssid.is_empty() { t!("\u{E3D0} Messung starten", "\u{E3D0} Start survey").to_string() } else { tf!("\u{E3D0} Messung für „{ssid}“ starten", "\u{E3D0} Start survey for \"{ssid}\"") };
                        if ui.add_enabled_ui(!ssid.is_empty(), |ui| primary_button(ui, &label, p.accent)).inner.clicked() {
                            start_cta = true;
                        }
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(t!(
                                "Die Verbindung wird jede Sekunde gemessen, unabhängig vom Scan-Intervall. Sticky Client = Laptop bleibt ≥ 10 dB unter dem besten AP, länger als 15 s.",
                                "The connection is measured every second, independent of the scan interval. Sticky client = laptop stays ≥ 10 dB below the best AP for more than 15 s."
                            ))
                            .size(12.0)
                            .color(p.weak),
                        );
                    });
                }
            }
        });
        }

        if start_cta {
            self.start_survey(ssid.clone(), plan_mode);
            return;
        }
        if self.meas.is_none() {
            ui.add_space(10.0);
            card(ui, None, |ui| {
                ui.set_max_width(640.0);
                self.recent_surveys_ui(ui, plan_mode);
            });
            return;
        }
        ui.add_space(6.0);
        self.meas_chart(ui);

        // ---------- Messpunkte | Ereignisse (Reiter) ----------
        ui.add_space(6.0);
        let (n_pts, n_ev) = self.meas.as_ref().map(|m| (m.points.len(), m.events.iter().filter(|e| !matches!(e.kind, EventKind::Start | EventKind::Stop)).count())).unwrap_or_default();
        let mut show_events = self.meas_show_events;
        let pts_label = tf!("\u{E316} Messpunkte ({n_pts})", "\u{E316} Points ({n_pts})");
        let ev_label = tf!("\u{E1A0} Ereignisse ({n_ev})", "\u{E1A0} Events ({n_ev})");
        crate::widgets::segmented(ui, &mut show_events, &[(false, pts_label.as_str()), (true, ev_label.as_str())]);
        self.meas_show_events = show_events;
        ui.add_space(4.0);
        if show_events {
            self.survey_events(ui);
        } else {
            self.points_table(ui);
        }
    }

    /// Signalverlauf der Messung (Diagramm mit Roams, Messpunkten, Live-Kopf)
    fn meas_chart(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        let t_now = self.now_t();
        let Some(m) = &self.meas else { return };

        // ---------- Verlauf ----------
        let elapsed = m.elapsed(t_now);
        let live = m.running;
        if live {
            // flüssiger Zeitverlauf, solange gemessen wird (sonst nur 1×/s)
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        }
        // Zeitfenster: gleitend die letzten 5 min oder bis zur nächsten "runden" Länge,
        // damit die Achse nicht jede Sekunde neu skaliert
        let (x0, x1) = if self.meas_follow {
            ((elapsed - 300.0).max(0.0), elapsed.max(300.0))
        } else {
            (0.0, nice_span(elapsed))
        };
        let in_view = |t: f64| t >= x0 - 5.0;

        // Verbindungslinie je AP zerlegen (Farbwechsel = Roam), Lücken > 5 s = Abbruch
        let mut segments: Vec<([u8; 6], u32, Vec<[f64; 2]>)> = Vec::new();
        let mut last_t = f64::NEG_INFINITY;
        for c in m.conn.iter().filter(|c| in_view(c.t)) {
            let new_seg = segments.last().map_or(true, |s| s.0 != c.bssid) || c.t - last_t > 5.0;
            if new_seg {
                let mut pts = Vec::new();
                if c.t - last_t <= 5.0 {
                    if let Some(pt) = segments.last().and_then(|s| s.2.last()) {
                        pts.push(*pt); // an den vorherigen Abschnitt anschließen
                    }
                }
                segments.push((c.bssid, c.channel, pts));
            }
            if let Some(s) = segments.last_mut() {
                s.2.push([c.t, c.rssi as f64]);
            }
            last_t = c.t;
        }
        // Live-Kopf: letzten Verbindungswert bis "jetzt" fortführen, solange die Verbindung besteht
        let conn_head = m.conn.last().filter(|c| live && elapsed - c.t < 3.0).map(|c| [elapsed, c.rssi as f64]);
        if let (Some(head), Some(s)) = (conn_head, segments.last_mut()) {
            s.2.push(head);
        }

        // Scan-Werte als Stufenlinie: ein Wert gilt bis zum nächsten Scan
        let mut best_pts: Vec<[f64; 2]> = Vec::new();
        for b in m.best.iter().filter(|b| in_view(b.t)) {
            if let Some(prev) = best_pts.last().copied() {
                best_pts.push([b.t, prev[1]]);
            }
            best_pts.push([b.t, b.rssi as f64]);
        }
        let best_head = m.best.last().filter(|b| live && elapsed - b.t < 15.0).map(|b| [elapsed, b.rssi as f64]);
        if let Some(h) = best_head {
            best_pts.push(h);
        }
        let no_conn = m.conn.is_empty();
        let waiting = m.conn.is_empty() && m.best.is_empty();

        let events: Vec<(f64, String, Color32)> = m
            .events
            .iter()
            .filter(|e| in_view(e.t))
            .filter_map(|e| {
                let (label, col) = match &e.kind {
                    EventKind::Roam { to, .. } => (format!("\u{E0A0} {}", &fmt_mac(to)[12..]), p.accent),
                    EventKind::Disconnect { .. } => (t!("Abbruch", "Disconnect").to_string(), p.danger),
                    EventKind::Connect { .. } => (t!("Verbunden", "Connected").to_string(), p.success),
                    EventKind::Sticky { .. } => ("Sticky".to_string(), p.warning),
                    EventKind::Marker(t) => (format!("\u{E316} {t}"), p.purple),
                    EventKind::Start | EventKind::Stop => return None,
                };
                Some((e.t, label, col))
            })
            .collect();
        let vendor_of = |b: &[u8; 6]| self.aps.get(b).map(Self::vendor_label).unwrap_or_default();
        let head = conn_head.or(if no_conn { best_head } else { None });
        let head_col = match (conn_head, m.conn.last()) {
            (Some(_), Some(c)) => ap_color(&c.bssid),
            _ => p.accent,
        };
        // Legende über dem Diagramm statt darin (verdeckt sonst Linien)
        let best_name = t!("Bester AP der SSID (Scan)", "Best AP of the SSID (scan)");
        let mut legend: Vec<(String, Color32, bool)> = Vec::new();
        if !best_pts.is_empty() {
            legend.push((best_name.to_string(), if no_conn { p.accent } else { p.weak }, !no_conn));
        }
        let seg_name = |bssid: &[u8; 6], ch: u32| {
            let v = vendor_of(bssid);
            let name = tf!("AP {} · Kanal {ch}", "AP {} · channel {ch}", &fmt_mac(bssid)[9..]);
            if v.is_empty() {
                name
            } else {
                format!("{name} · {v}")
            }
        };
        for (bssid, ch, _) in &segments {
            let n = seg_name(bssid, *ch);
            if !legend.iter().any(|l| l.0 == n) {
                legend.push((n, ap_color(bssid), false));
            }
        }
        if !legend.is_empty() {
            ui.horizontal_wrapped(|ui| {
                for (name, col, dashed) in &legend {
                    let (r, _) = ui.allocate_exact_size(egui::Vec2::new(18.0, 12.0), egui::Sense::hover());
                    let y = r.center().y;
                    if *dashed {
                        for k in 0..3 {
                            let x = r.min.x + k as f32 * 6.5;
                            ui.painter().line_segment([egui::pos2(x, y), egui::pos2(x + 4.0, y)], egui::Stroke::new(1.5f32, *col));
                        }
                    } else {
                        ui.painter().line_segment([egui::pos2(r.min.x, y), egui::pos2(r.max.x, y)], egui::Stroke::new(3.0f32, *col));
                    }
                    ui.label(RichText::new(name).small().color(p.text));
                    ui.add_space(10.0);
                }
            });
        }
        let avail_h = ui.available_height();
        // unten sollen noch einige Messpunkt-Zeilen sichtbar bleiben
        let plot_h = (avail_h - 210.0).min(avail_h * 0.55).max(180.0);
        // Ereignis-Beschriftungen in Zeilen staffeln, damit sie sich nicht überlappen
        let plot_w = (ui.available_width() - 70.0).max(200.0) as f64;
        let secs_per_px = (x1 - x0).max(1.0) / plot_w;
        let dbm_per_px = 80.0 / plot_h as f64;
        let mut row_end: Vec<f64> = Vec::new();
        let label_y: Vec<f64> = events
            .iter()
            .map(|(t, label, _)| {
                let w = (label.chars().count() as f64 * 6.6 + 12.0) * secs_per_px;
                let row = match row_end.iter().position(|e| *e <= *t) {
                    Some(r) => r,
                    None if row_end.len() < 4 => {
                        row_end.push(f64::MIN);
                        row_end.len() - 1
                    }
                    None => row_end.iter().enumerate().min_by(|a, b| a.1.total_cmp(b.1)).map(|(i, _)| i).unwrap_or(0),
                };
                row_end[row] = t + w;
                -21.0 - row as f64 * 15.0 * dbm_per_px
            })
            .collect();
        crate::widgets::plot_style(ui, true);
        Plot::new(("meas_plot", self.meas_follow))
            .height(plot_h)
            .include_y(-100.0)
            .include_y(-20.0)
            .include_x(x0)
            .include_x(x1)
            .allow_zoom(false)
            .allow_drag(false)
            .allow_scroll(false)
            .allow_double_click_reset(false)
            .custom_y_axes(vec![crate::widgets::dbm_y_axis()])
            .x_axis_formatter(|m, _| if m.value >= 0.0 { fmt_mmss(m.value) } else { String::new() })
            .label_formatter(|name, v| format!("{name}\n{} · {:.0} dBm", fmt_mmss(v.x.max(0.0)), v.y))
            .show(ui, |pui| {
                pui.hline(HLine::new(-67.0).color(p.success.gamma_multiply(0.5)).style(LineStyle::dashed_dense()));
                pui.hline(HLine::new(-75.0).color(p.danger.gamma_multiply(0.5)).style(LineStyle::dashed_dense()));
                if live {
                    // "Jetzt"-Linie
                    pui.vline(VLine::new(elapsed).color(p.weak.gamma_multiply(0.35)).width(1.0f32));
                }
                if !best_pts.is_empty() {
                    // ohne Verbindung ist der Scan-Wert die Hauptlinie, sonst nur Vergleich
                    let line = Line::new(PlotPoints::from(best_pts)).name(best_name);
                    pui.line(if no_conn {
                        line.color(p.accent).width(2.6f32)
                    } else {
                        line.color(p.weak).width(1.2f32).style(LineStyle::dashed_loose())
                    });
                }
                for (bssid, ch, pts) in segments {
                    pui.line(Line::new(PlotPoints::from(pts)).color(ap_color(&bssid)).width(2.6f32).name(seg_name(&bssid, ch)));
                }
                if let Some(h) = head {
                    pui.points(Points::new(vec![h]).radius(5.0f32).color(head_col).filled(true));
                }
                for ((t, label, col), y) in events.iter().zip(&label_y) {
                    pui.vline(VLine::new(*t).color(col.gamma_multiply(0.8)).width(1.5f32));
                    pui.text(Text::new(PlotPoint::new(*t, *y), RichText::new(label).small().color(*col)).anchor(Align2::LEFT_TOP));
                }
                if waiting {
                    pui.text(
                        Text::new(
                            PlotPoint::new((x0 + x1) / 2.0, -60.0),
                            RichText::new(t!("Warte auf den ersten Messwert …", "Waiting for the first sample …")).size(16.0).color(p.weak),
                        )
                        .anchor(Align2::CENTER_CENTER),
                    );
                }
            });
        crate::widgets::plot_style(ui, false);
    }

    fn survey_events(&self, ui: &mut Ui) {
        let p = pal(ui);
        let Some(m) = &self.meas else { return };
        let wall = |t: f64| (m.started_wall + chrono::Duration::milliseconds((t * 1000.0) as i64)).format("%H:%M:%S").to_string();
        egui::ScrollArea::vertical().id_salt("meas_events").auto_shrink(false).show(ui, |ui| {
            egui::Grid::new("meas_ev_grid").num_columns(4).striped(true).spacing([14.0, 4.0]).show(ui, |ui| {
                for e in m.events.iter().rev() {
                    let col = match e.kind {
                        EventKind::Roam { .. } => p.accent,
                        EventKind::Disconnect { .. } => p.danger,
                        EventKind::Sticky { .. } => p.warning,
                        EventKind::Marker(_) => p.purple,
                        EventKind::Connect { .. } => p.success,
                        _ => p.weak,
                    };
                    ui.label(RichText::new(fmt_mmss(e.t)).monospace());
                    ui.label(RichText::new(wall(e.t)).monospace().color(p.weak));
                    ui.label(RichText::new(e.title()).strong().color(col));
                    ui.label(e.details());
                    ui.end_row();
                }
            });
        });
    }
}

/// Länge der Zeitachse: nächste "runde" Dauer, damit das Diagramm nicht jede Sekunde neu skaliert
fn nice_span(elapsed: f64) -> f64 {
    let need = elapsed * 1.03 + 5.0;
    [60.0, 120.0, 300.0, 600.0, 900.0, 1200.0, 1800.0, 2700.0, 3600.0]
        .into_iter()
        .find(|s| *s >= need)
        .unwrap_or_else(|| (need / 1800.0).ceil() * 1800.0)
}
