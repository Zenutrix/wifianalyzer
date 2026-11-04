//! Messung (Walk-Survey): Live-Anzeige, Kennzahlen, Verlauf mit Roams/Markierungen, Ereignisliste.

use eframe::egui::{self, Align2, Color32, RichText, Ui};
use egui_plot::{Corner, HLine, Legend, Line, LineStyle, Plot, PlotPoint, PlotPoints, Text, VLine};

use crate::app::{Sev, WifiApp};
use crate::measure::EventKind;
use crate::model::*;
use crate::theme::pal;
use crate::widgets::{card, dbm_axis, fmt_mmss, kpi, page_header, primary_button, signal_gauge};
use crate::{t, tf};

impl WifiApp {
    pub(crate) fn page_measure(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        let t_now = self.now_t();
        let running = self.meas.as_ref().is_some_and(|m| m.running);
        let choices = self.ssid_choices();
        if self.meas_ssid.is_none() {
            // Vorauswahl: verbundene SSID, sonst die stärkste
            self.meas_ssid = self.conn.as_ref().map(|c| c.ssid.clone()).or_else(|| choices.first().map(|c| c.0.clone()));
        }
        let ssid = self.meas.as_ref().map(|m| m.ssid.clone()).or_else(|| self.meas_ssid.clone()).unwrap_or_default();
        let connected_ssid = self.conn.as_ref().map(|c| c.ssid.clone());

        let (mut start, mut stop, mut discard, mut export, mut add_marker) = (false, false, false, false, false);
        let mut new_ssid = None;
        let mut follow = self.meas_follow;
        page_header(
            ui,
            t!("Messung", "Survey"),
            t!(
                "Walk-Survey: Signal der Verbindung jede Sekunde, Roams, Abbrüche und Markierungen",
                "Walk survey: connection signal every second, roams, disconnects and markers"
            ),
            |ui| {
                if self.meas.is_some() && ui.button(t!("💾 CSV", "💾 CSV")).on_hover_text(t!("Messwerte und Ereignisse exportieren", "Export samples and events")).clicked() {
                    export = true;
                }
                if !running && self.meas.is_some() && ui.button(t!("🗑 Verwerfen", "🗑 Discard")).clicked() {
                    discard = true;
                }
                if running {
                    if primary_button(ui, t!("⏹ Messung stoppen", "⏹ Stop survey"), p.danger).clicked() {
                        stop = true;
                    }
                } else if ui
                    .add_enabled(!ssid.is_empty(), egui::Button::new(RichText::new(t!("▶ Messung starten", "▶ Start survey")).color(Color32::WHITE).strong()).fill(p.success))
                    .clicked()
                {
                    start = true;
                }
                ui.add_enabled_ui(!running, |ui| {
                    egui::ComboBox::from_id_salt("meas_ssid")
                        .width(220.0)
                        .selected_text(if ssid.is_empty() { "–" } else { ssid.as_str() })
                        .show_ui(ui, |ui| {
                            for (s, r) in &choices {
                                let tag = if connected_ssid.as_deref() == Some(s.as_str()) { "  🔗" } else { "" };
                                if ui.selectable_label(*s == ssid, format!("{s}  ({r} dBm){tag}")).clicked() {
                                    new_ssid = Some(s.clone());
                                }
                            }
                        });
                });
                ui.label(RichText::new("SSID").color(p.weak));
            },
        );
        if let Some(s) = new_ssid {
            self.meas_ssid = Some(s);
        }

        // Markierungen
        ui.horizontal(|ui| {
            let te = ui.add_enabled(
                running,
                egui::TextEdit::singleline(&mut self.meas_marker).hint_text(t!("Ort / Notiz, z. B. Raum 1.03 – Enter setzt die Markierung", "Location / note, e.g. room 1.03 – Enter sets the marker")).desired_width(360.0),
            );
            let enter = te.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui.add_enabled(running, egui::Button::new(t!("📍 Markierung setzen", "📍 Set marker"))).clicked() || (running && enter) {
                add_marker = true;
            }
            ui.separator();
            ui.checkbox(&mut follow, t!("Nur letzte 5 min", "Last 5 min only"));
        });
        self.meas_follow = follow;

        if start {
            self.start_measurement(ssid.clone());
        }
        if stop {
            if let Some(m) = &mut self.meas {
                m.stop(t_now);
            }
            self.add_log(Sev::Info, tf!("Messung beendet: {ssid}", "Survey stopped: {ssid}"));
        }
        if discard {
            self.meas = None;
        }
        if export {
            self.export_measurement();
        }
        if add_marker {
            if let Some(m) = &mut self.meas {
                m.add_marker(t_now, &self.meas_marker);
            }
            self.meas_marker.clear();
        }
        ui.add_space(8.0);

        // ---------- Live-Anzeige + Kennzahlen ----------
        let conn_here = self.conn.as_ref().filter(|c| c.ssid == ssid);
        let best = self.best_ap_of(&ssid);
        let live_rssi = conn_here.and_then(|c| c.rssi).or(best.map(|b| b.info.rssi));
        ui.horizontal(|ui| {
            card(ui, None, |ui| {
                ui.set_width(300.0);
                ui.vertical_centered(|ui| signal_gauge(ui, live_rssi, false, 0.75));
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
                                ui.label(RichText::new(t!("✔ Verbunden mit dem stärksten AP", "✔ Connected to the strongest AP")).color(p.success));
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
                    ui.label(RichText::new(t!("⚠ Sticky Client: besserer AP verfügbar", "⚠ Sticky client: a better AP is available")).strong().color(p.danger));
                }
            });
            ui.add_space(6.0);
            match &self.meas {
                Some(m) => {
                    let s = m.stats(t_now);
                    let pct = |v: f64| format!("{:.0} %", v * 100.0);
                    let w = 128.0;
                    ui.vertical(|ui| {
                        ui.horizontal_wrapped(|ui| {
                            let state = if m.running { t!("läuft", "running") } else { t!("beendet", "stopped") };
                            kpi(ui, &tf!("Dauer ({state})", "Duration ({state})"), &fmt_mmss(s.duration), "", if m.running { p.success } else { p.text }, w);
                            let src = if s.from_scan { t!("aus dem Scan", "from scan") } else { "" };
                            kpi(ui, t!("Ø Signal", "Avg signal"), &s.avg.map(|v| format!("{v:.1} dBm")).unwrap_or("–".into()), src, s.avg.map(|v| signal_color(v as i32)).unwrap_or(p.text), w);
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
                            kpi(ui, t!("Markierungen", "Markers"), &m.marker_count().to_string(), "", p.text, w);
                        });
                    });
                }
                None => {
                    card(ui, Some(t!("So funktioniert die Messung", "How the survey works")), |ui| {
                        ui.set_max_width(640.0);
                        ui.label(t!("1.  SSID wählen – am besten die, mit der der Laptop verbunden ist (🔗)", "1.  Choose the SSID – ideally the one the laptop is connected to (🔗)"));
                        ui.label(t!("2.  „Messung starten“ und mit dem Laptop die Fläche ablaufen", "2.  Click \"Start survey\" and walk the area with the laptop"));
                        ui.label(t!("3.  An wichtigen Stellen eine Markierung setzen (Raum, Stockwerk …)", "3.  Set markers at important spots (room, floor …)"));
                        ui.label(t!("4.  Stoppen – Verlauf, Roams und Statistik bleiben erhalten, Export als CSV", "4.  Stop – history, roams and statistics are kept, export as CSV"));
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

        let Some(m) = &self.meas else { return };
        ui.add_space(8.0);

        // ---------- Verlauf ----------
        let end = m.elapsed(t_now).max(10.0);
        let x0 = if self.meas_follow { (end - 300.0).max(0.0) } else { 0.0 };
        let in_view = |t: f64| t >= x0 - 1.0;
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
        let best_pts: Vec<[f64; 2]> = m.best.iter().filter(|b| in_view(b.t)).map(|b| [b.t, b.rssi as f64]).collect();
        let no_conn = m.conn.is_empty();
        let events: Vec<(f64, String, Color32)> = m
            .events
            .iter()
            .filter(|e| in_view(e.t))
            .filter_map(|e| {
                let (label, col) = match &e.kind {
                    EventKind::Roam { to, .. } => (format!("Roam > {}", &fmt_mac(to)[9..]), p.accent),
                    EventKind::Disconnect { .. } => (t!("Abbruch", "Disconnect").to_string(), p.danger),
                    EventKind::Connect { .. } => (t!("Verbunden", "Connected").to_string(), p.success),
                    EventKind::Sticky { .. } => ("Sticky".to_string(), p.warning),
                    EventKind::Marker(t) => (format!("📍 {t}"), p.purple),
                    EventKind::Start | EventKind::Stop => return None,
                };
                Some((e.t, label, col))
            })
            .collect();
        let vendor_of = |b: &[u8; 6]| self.aps.get(b).map(Self::vendor_label).unwrap_or_default();
        let plot_h = (ui.available_height() * 0.62).max(180.0);
        Plot::new(("meas_plot", self.meas_follow))
            .height(plot_h)
            .legend(Legend::default().position(Corner::LeftBottom))
            .include_y(-100.0)
            .include_y(-20.0)
            .include_x(x0)
            .include_x(end)
            .allow_zoom(false)
            .allow_drag(false)
            .allow_scroll(false)
            .y_axis_formatter(dbm_axis)
            .x_axis_formatter(|m, _| if m.value >= 0.0 { fmt_mmss(m.value) } else { String::new() })
            .label_formatter(|name, v| format!("{name}\n{} · {:.0} dBm", fmt_mmss(v.x.max(0.0)), v.y))
            .show(ui, |pui| {
                pui.hline(HLine::new(-67.0).color(p.success.gamma_multiply(0.5)).style(LineStyle::dashed_dense()));
                pui.hline(HLine::new(-75.0).color(p.danger.gamma_multiply(0.5)).style(LineStyle::dashed_dense()));
                if !best_pts.is_empty() {
                    // ohne Verbindung ist der Scan-Wert die Hauptlinie, sonst nur Vergleich
                    let line = Line::new(PlotPoints::from(best_pts)).name(t!("Bester AP der SSID (Scan)", "Best AP of the SSID (scan)"));
                    pui.line(if no_conn {
                        line.color(p.accent).width(2.6f32)
                    } else {
                        line.color(p.weak).width(1.2f32).style(LineStyle::dashed_loose())
                    });
                }
                for (bssid, ch, pts) in segments {
                    let v = vendor_of(&bssid);
                    let name = tf!("AP {} · Kanal {ch}", "AP {} · channel {ch}", &fmt_mac(&bssid)[9..]);
                    pui.line(
                        Line::new(PlotPoints::from(pts))
                            .color(ap_color(&bssid))
                            .width(2.6f32)
                            .name(if v.is_empty() { name } else { format!("{name} · {v}") }),
                    );
                }
                for (t, label, col) in &events {
                    pui.vline(VLine::new(*t).color(col.gamma_multiply(0.8)).width(1.5f32));
                    pui.text(Text::new(PlotPoint::new(*t, -21.0), RichText::new(label).small().color(*col)).anchor(Align2::LEFT_TOP));
                }
            });

        // ---------- Ereignisliste ----------
        ui.add_space(6.0);
        ui.label(RichText::new(t!("Ereignisse der Messung", "Survey events")).strong().color(p.text));
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
