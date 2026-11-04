//! Signal-Finder: einen Access Point über die Signalstärke orten (optional mit Piepton).

use std::time::Instant;

use eframe::egui::{self, RichText, Ui};
use egui_plot::{Line, Plot, PlotPoints};

use crate::app::WifiApp;
use crate::model::*;
use crate::theme::pal;
use crate::widgets::{card, dbm_axis, page_header, signal_gauge};
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
            t!("Signal-Finder", "Signal finder"),
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
        Plot::new("finder_plot")
            .height(ui.available_height().max(120.0))
            .include_y(-100.0)
            .include_y(-20.0)
            .include_x(-180.0)
            .include_x(0.0)
            .allow_zoom(false)
            .allow_drag(false)
            .allow_scroll(false)
            .y_axis_formatter(dbm_axis)
            .x_axis_formatter(|m, _| if m.value <= 0.5 { format!("{:.0} s", m.value) } else { String::new() })
            .show(ui, |pui| {
                pui.line(Line::new(PlotPoints::from(pts)).color(col).width(2.5f32).fill(-100.0f32));
            });
    }
}
