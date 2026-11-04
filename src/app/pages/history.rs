//! Signalverlauf über die Zeit (markierte APs, Auswahl oder Top 30).

use std::time::Instant;

use eframe::egui::{RichText, Ui};
use egui_plot::{Corner, Legend, Line, LineStyle, Plot, PlotPoints};

use crate::app::WifiApp;
use crate::model::*;
use crate::theme::pal;
use crate::widgets::{dbm_axis, page_header, segmented};
use crate::{t, tf};

impl WifiApp {
    pub(crate) fn page_history(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        let mut all = self.hist_all;
        let mut win = self.hist_window;
        page_header(
            ui,
            t!("Signalverlauf", "Signal history"),
            t!("Signalstärke über die Zeit – markierte (☆) und ausgewählte Access Points", "Signal strength over time – pinned (☆) and selected access points"),
            |ui| {
                segmented(ui, &mut win, &[(3600.0, "60 min"), (900.0, "15 min"), (300.0, "5 min"), (60.0, "1 min")]);
                segmented(ui, &mut all, &[(true, t!("Top 30", "Top 30")), (false, t!("Markierte", "Pinned"))]);
            },
        );
        self.hist_all = all;
        self.hist_window = win;

        let now = Instant::now();
        let t_now = self.now_t();
        let mut recs: Vec<&ApRecord> = self
            .visible_records()
            .into_iter()
            .filter(|r| all || self.pinned.contains(&r.info.bssid) || self.selected == Some(r.info.bssid))
            .filter(|r| !self.is_stale(r, now) || self.pinned.contains(&r.info.bssid))
            .collect();
        recs.sort_by_key(|r| -r.info.rssi);
        recs.truncate(30);
        let show_conn = self.conn.is_some();
        if recs.is_empty() && !show_conn {
            ui.add_space(50.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("📈").size(40.0).color(p.weak));
                ui.label(RichText::new(t!("Noch nichts zum Anzeigen", "Nothing to show yet")).size(17.0).color(p.text));
                ui.label(
                    RichText::new(t!(
                        "Access Points in der Netzwerkliste mit ☆ markieren oder „Top 30“ wählen.",
                        "Pin access points with ☆ in the network list or choose \"Top 30\"."
                    ))
                    .color(p.weak),
                );
            });
            return;
        }
        if recs.is_empty() {
            ui.label(RichText::new(t!("Nur die aktuelle Verbindung – weitere APs mit ☆ markieren.", "Only the current connection – pin more APs with ☆.")).color(p.weak));
        }
        let sel = self.selected;
        Plot::new("history")
            .legend(Legend::default().position(Corner::LeftTop))
            .include_y(-100.0)
            .include_y(-20.0)
            .include_x(-win)
            .include_x(0.0)
            .allow_zoom(false)
            .allow_drag(false)
            .allow_scroll(false)
            .x_axis_label(t!("Zeit", "Time"))
            .y_axis_formatter(dbm_axis)
            .x_axis_formatter(|m, _| {
                if m.value > 0.5 {
                    return String::new();
                }
                let s = (-m.value).round().max(0.0) as u64;
                if s == 0 {
                    t!("jetzt", "now").into()
                } else {
                    format!("-{}:{:02}", s / 60, s % 60)
                }
            })
            .label_formatter(|name, v| {
                let s = (-v.x).max(0.0) as u64;
                tf!("{name}\nvor {}:{:02} · {:.0} dBm", "{name}\n{}:{:02} ago · {:.0} dBm", s / 60, s % 60, v.y)
            })
            .show(ui, |pui| {
                for r in &recs {
                    let pts: Vec<[f64; 2]> = r.history.iter().filter(|(t, _)| t_now - t <= win).map(|(t, v)| [t - t_now, *v]).collect();
                    let w: f32 = if sel == Some(r.info.bssid) { 3.0 } else { 1.8 };
                    pui.line(
                        Line::new(PlotPoints::from(pts))
                            .color(ap_color(&r.info.bssid))
                            .width(w)
                            .name(format!("{} [{}]", Self::display_ssid(r), &fmt_mac(&r.info.bssid)[9..])),
                    );
                }
                if show_conn {
                    let pts: Vec<[f64; 2]> = self.conn_hist.iter().filter(|(t, _)| t_now - t <= win).map(|(t, v)| [t - t_now, *v]).collect();
                    pui.line(
                        Line::new(PlotPoints::from(pts))
                            .color(p.text)
                            .style(LineStyle::Solid)
                            .width(2.0f32)
                            .name(t!("Aktuelle Verbindung", "Current connection")),
                    );
                }
            });
    }
}
