//! AP-Finder: einen Access Point über die Signalstärke orten (optional mit Piepton).

use std::time::Instant;

use eframe::egui::{self, RichText, Ui};
use egui_plot::{Line, Plot, PlotPoints};

use crate::app::WifiApp;
use crate::model::*;
use crate::theme::pal;
use crate::widgets::{card, page_header, signal_gauge};
use crate::{t, tf};

impl WifiApp {
    pub(crate) fn page_finder(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        let now = Instant::now();
        let mut keys = self.sorted_keys();
        keys.sort_by_key(|k| -self.aps[k].info.rssi);
        let mut pick = None;
        let mut beep = self.beep_on;
        let cur = self
            .selected
            .and_then(|k| self.aps.get(&k))
            .map(|r| format!("{} – {}", Self::display_ssid(r), fmt_mac(&r.info.bssid)))
            .unwrap_or_else(|| t!("Access Point wählen …", "Choose an access point …").into());
        page_header(
            ui,
            t!("AP-Finder", "AP finder"),
            t!("Mit dem Laptop herumgehen: je stärker das Signal, desto näher der Access Point", "Walk around with the laptop: the stronger the signal, the closer the access point"),
            |ui| {
                ui.checkbox(&mut beep, t!("🔊 Piepton", "🔊 Beep")).on_hover_text(t!(
                    "Geigerzähler-Ton: höher und schneller bei stärkerem Signal",
                    "Geiger counter tone: higher and faster with a stronger signal"
                ));
                egui::ComboBox::from_id_salt("finder_pick").width(340.0).selected_text(cur).show_ui(ui, |ui| {
                    for k in &keys {
                        let r = &self.aps[k];
                        let label = tf!(
                            "{} – {} ({} dBm, Kanal {})",
                            "{} – {} ({} dBm, ch {})",
                            Self::display_ssid(r),
                            fmt_mac(k),
                            r.info.rssi,
                            r.info.channel
                        );
                        if ui.selectable_label(self.selected == Some(*k), label).clicked() {
                            pick = Some(*k);
                        }
                    }
                });
            },
        );
        self.beep_on = beep;
        if pick.is_some() {
            self.selected = pick;
        }

        let Some(r) = self.selected.and_then(|k| self.aps.get(&k)) else {
            self.beeper.set(false, -100);
            ui.add_space(60.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("🎯").size(40.0).color(p.weak));
                ui.label(RichText::new(t!("Wähle oben einen Access Point aus.", "Choose an access point above.")).size(17.0).color(p.weak));
                ui.label(
                    RichText::new(t!("Tipp: In der Netzwerkliste per Doppelklick direkt hierher.", "Tip: double-click a row in the network list to jump here."))
                        .color(p.weak),
                );
            });
            // Schnellauswahl: die stärksten Access Points als Karten
            let now = std::time::Instant::now();
            let mut top: Vec<_> = self.aps.values().filter(|r| !self.is_stale(r, now)).collect();
            top.sort_by(|a, b| b.info.rssi.cmp(&a.info.rssi));
            top.truncate(8);
            if !top.is_empty() {
                ui.add_space(28.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new(t!("Stärkste Access Points in der Nähe", "Strongest access points nearby")).strong().color(p.text));
                });
                ui.add_space(10.0);
                let mut pick = None;
                let w = 230.0f32;
                let cols = ((ui.available_width() + 10.0) / (w + 10.0)).floor().clamp(1.0, 4.0) as usize;
                let total_w = cols as f32 * (w + 10.0) - 10.0;
                ui.horizontal(|ui| {
                    ui.add_space(((ui.available_width() - total_w) / 2.0).max(0.0));
                    egui::Grid::new("finder_pick").num_columns(cols).spacing([10.0, 10.0]).show(ui, |ui| {
                        for (i, r) in top.iter().enumerate() {
                            let col = signal_color(r.info.rssi);
                            let resp = egui::Frame::none()
                                .fill(p.surface)
                                .stroke(egui::Stroke::new(1.0f32, p.border))
                                .rounding(10.0)
                                .inner_margin(egui::Margin::symmetric(12.0, 10.0))
                                .show(ui, |ui| {
                                    ui.set_width(w - 24.0);
                                    ui.horizontal(|ui| {
                                        ui.vertical(|ui| {
                                            ui.add(egui::Label::new(RichText::new(Self::display_ssid(r)).strong().color(p.text)).truncate());
                                            ui.label(RichText::new(format!("{} · {}", &fmt_mac(&r.info.bssid)[9..], Self::vendor_label(r))).small().color(p.weak));
                                        });
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            ui.label(RichText::new(format!("{}", r.info.rssi)).size(18.0).strong().color(col));
                                        });
                                    });
                                })
                                .response;
                            let resp = ui.interact(resp.rect, ui.id().with(("finder_pick", i)), egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                            if resp.hovered() {
                                ui.painter().rect_stroke(resp.rect, 10.0, egui::Stroke::new(1.0f32, p.accent));
                            }
                            if resp.clicked() {
                                pick = Some(r.info.bssid);
                            }
                            if (i + 1) % cols == 0 {
                                ui.end_row();
                            }
                        }
                    });
                });
                if pick.is_some() {
                    self.selected = pick;
                }
            }
            return;
        };
        let rssi = r.info.rssi;
        let stale = self.is_stale(r, now);
        self.beeper.set(self.beep_on && !stale, rssi);

        card(ui, None, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(Self::display_ssid(r)).size(22.0).strong().color(p.text));
                ui.label(
                    RichText::new(tf!(
                        "{} · {} · Kanal {} · {}",
                        "{} · {} · channel {} · {}",
                        fmt_mac(&r.info.bssid),
                        Self::vendor_label(r),
                        r.info.channel,
                        r.info.band.label()
                    ))
                    .color(p.weak),
                );
                signal_gauge(ui, Some(rssi), stale, 1.0);
                ui.label(RichText::new(if stale { t!("Signal verloren", "Signal lost") } else { signal_text(rssi) }).size(18.0).color(p.text));
                ui.label(RichText::new(tf!("min {}  ·  Ø {:.1}  ·  max {}", "min {}  ·  avg {:.1}  ·  max {}", r.rssi_min, r.rssi_avg(), r.rssi_max)).color(p.weak));
                ui.label(
                    RichText::new(t!("Tipp: Scan-Intervall auf 2 s stellen für schnellere Aktualisierung.", "Tip: set the scan interval to 2 s for faster updates."))
                        .size(12.0)
                        .color(p.weak),
                );
            });
        });
        ui.add_space(8.0);
        let t_now = self.now_t();
        let pts: Vec<[f64; 2]> = r.history.iter().filter(|(t, _)| t_now - t <= 180.0).map(|(t, v)| [t - t_now, *v]).collect();
        let col = ap_color(&r.info.bssid);
        crate::widgets::plot_style(ui, true);
        Plot::new("finder_plot")
            .height(ui.available_height().max(120.0))
            .include_y(-100.0)
            .include_y(-20.0)
            .include_x(-180.0)
            .include_x(0.0)
            .allow_zoom(false)
            .allow_drag(false)
            .allow_scroll(false)
            .custom_y_axes(vec![crate::widgets::dbm_y_axis()])
            .x_axis_formatter(|m, _| if m.value <= 0.5 { format!("{:.0} s", m.value) } else { String::new() })
            .show(ui, |pui| {
                pui.line(Line::new(PlotPoints::from(pts)).color(col).width(2.5f32).fill(-100.0f32));
            });
        crate::widgets::plot_style(ui, false);
    }
}
