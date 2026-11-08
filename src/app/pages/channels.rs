//! Kanäle (Spektrum-Diagramm) und Kanalbewertung mit Empfehlung.

use std::collections::HashSet;
use std::time::Instant;

use eframe::egui::{Align2, Color32, RichText, Stroke, Ui, Vec2b};
use egui_plot::{uniform_grid_spacer, AxisHints, Bar, BarChart, Plot, PlotPoint, PlotPoints, Polygon, Text};

use crate::app::WifiApp;
use crate::model::*;
use crate::theme::pal;
use crate::widgets::{card, load_color, page_header, segmented};
use crate::{t, tf};

/// Kurvenform eines APs (flaches Dach, steile Flanken)
fn ap_shape(center: f64, half: f64, rssi: f64) -> Vec<[f64; 2]> {
    let n = 48;
    (0..=n)
        .map(|i| {
            let u = -1.0 + 2.0 * i as f64 / n as f64;
            let s = (1.0 - u.powi(6)).max(0.0);
            [center + u * half, -100.0 + (rssi + 100.0) * s]
        })
        .collect()
}

/// Halbe Kanalbreite in Kanalnummern (5 MHz je Nummer; 2,4 GHz DSSS ≈ 22 MHz)
fn half_span(r: &ApRecord) -> f64 {
    let h = r.info.width as f64 / 10.0;
    if r.info.band == Band::B24 && r.info.width == 20 {
        h + 0.2
    } else {
        h
    }
}

const NON_DFS_5: [u32; 9] = [36, 40, 44, 48, 149, 153, 157, 161, 165];

fn band_channels(band: Band) -> Vec<u32> {
    match band {
        Band::B24 => (1..=13).collect(),
        Band::B5 => [36, 40, 44, 48, 52, 56, 60, 64].into_iter().chain((100..=144).step_by(4)).chain([149, 153, 157, 161, 165]).collect(),
        _ => (1..=233).step_by(4).collect(),
    }
}

impl WifiApp {
    /// Störleistung (mW, Anzahl APs) auf einem 20-MHz-Kanal
    fn channel_load(&self, band: Band, ch: u32, now: Instant) -> (f64, usize) {
        let (lo, hi) = (ch as f64 - 2.0, ch as f64 + 2.0);
        let mut mw = 0.0;
        let mut n = 0;
        for r in self.aps.values() {
            if r.info.band != band || self.is_stale(r, now) {
                continue;
            }
            let h = half_span(r);
            let ov = hi.min(r.info.center + h) - lo.max(r.info.center - h);
            if ov > 0.0 {
                mw += 10f64.powf(r.info.rssi as f64 / 10.0) * (ov / 4.0).min(1.0);
                n += 1;
            }
        }
        (mw, n)
    }

    fn mw_to_score(mw: f64) -> f64 {
        if mw <= 0.0 {
            0.0
        } else {
            (10.0 * mw.log10() + 100.0).max(0.0)
        }
    }

    fn best_of(&self, band: Band, cands: &[u32], now: Instant) -> Option<(u32, f64, usize)> {
        cands
            .iter()
            .map(|&c| {
                let (mw, n) = self.channel_load(band, c, now);
                (c, mw, n)
            })
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(c, mw, n)| (c, Self::mw_to_score(mw), n))
    }

    /// Empfohlene Kanäle (2,4 GHz: 1/6/11, 5 GHz ohne DFS) – nur wenn schon gescannt wurde
    pub(crate) fn best_channels(&self) -> (Option<u32>, Option<u32>) {
        if self.scan_count == 0 {
            return (None, None);
        }
        let now = Instant::now();
        (
            self.best_of(Band::B24, &[1, 6, 11], now).map(|b| b.0),
            self.best_of(Band::B5, &NON_DFS_5, now).map(|b| b.0),
        )
    }

    // ------------------------------------------------------------------ Spektrum

    pub(crate) fn page_channels(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        let band = self.chan_band;
        let now = Instant::now();
        let mut recs: Vec<&ApRecord> = self.visible_records().into_iter().filter(|r| r.info.band == band).collect();
        recs.sort_by_key(|r| r.info.rssi); // schwache zuerst zeichnen, starke oben

        let mut band_sel = band;
        page_header(
            ui,
            t!("Kanäle", "Channels"),
            &tf!(
                "{} Access Points im {} · Klick auf eine Kurve zeigt Details",
                "{} access points in {} · click a curve for details",
                recs.len(),
                band.label()
            ),
            |ui| {
                segmented(ui, &mut band_sel, &[(Band::B6, "6 GHz"), (Band::B5, "5 GHz"), (Band::B24, t!("2,4 GHz", "2.4 GHz"))]);
            },
        );

        let group_ssid = self.settings.group_ssid;
        let palette = self.ssid_colors();
        let color_of = |r: &ApRecord| if group_ssid { palette.get(&Self::group_key(r)).copied().unwrap_or(p.accent) } else { ap_color(&r.info.bssid) };
        // Beschriftung: je Kanalmitte eine gestapelte Liste der SSIDs (stärkste zuerst)
        let mut label_stacks: Vec<(f64, f64, Vec<(String, Color32, [u8; 6])>)> = Vec::new();
        for r in recs.iter().rev() {
            let name = Self::display_ssid(r);
            let idx = label_stacks.iter().position(|(c, _, _)| (c - r.info.center).abs() < 0.01);
            let stack = match idx {
                Some(i) => &mut label_stacks[i],
                None => {
                    label_stacks.push((r.info.center, r.info.rssi as f64, Vec::new()));
                    label_stacks.last_mut().unwrap()
                }
            };
            if !stack.2.iter().any(|(n, _, _)| *n == name) {
                stack.2.push((name, color_of(r), r.info.bssid));
            }
        }

        let (xmin, xmax, steps) = match band {
            Band::B24 => (-1.0, 16.0, [1.0, 1.0, 1.0]),
            Band::B5 => (30.0, 180.0, [4.0, 4.0, 16.0]),
            _ => (-4.0, 238.0, [4.0, 16.0, 32.0]),
        };
        let sel = self.selected;
        let text_col = p.text;
        let h = ui.available_height() - 40.0;
        crate::widgets::plot_style(ui, true);
        let resp = Plot::new(("chan", band as u8))
            .height(h.max(220.0))
            .include_x(xmin)
            .include_x(xmax)
            .include_y(-100.0)
            .include_y(-20.0)
            .allow_zoom(Vec2b::new(band != Band::B24, false))
            .allow_drag(Vec2b::new(band != Band::B24, false))
            .allow_scroll(false)
            .allow_boxed_zoom(false)
            .x_grid_spacer(uniform_grid_spacer(move |_| steps))
            // eigene X-Achse: Kanalnummern auch bei hoher Bildschirmskalierung (Standard blendet < 60 pt aus)
            .custom_x_axes(vec![AxisHints::new_x()
                .label(t!("Kanal", "Channel"))
                .label_spacing(22.0..=30.0)
                .formatter(move |m, _| {
                    let max = if band == Band::B24 { 14.0 } else { 240.0 };
                    let v = m.value.round();
                    if (m.value - v).abs() < 1e-3 && v >= 1.0 && v <= max {
                        format!("{v}")
                    } else {
                        String::new()
                    }
                })])
            .custom_y_axes(vec![crate::widgets::dbm_y_axis()])
            .label_formatter(|_, v| crate::tf!("Kanal {:.0}\n{:.0} dBm", "Channel {:.0}\n{:.0} dBm", v.x, v.y))
            .show(ui, |pui| {
                if band == Band::B5 {
                    pui.polygon(
                        Polygon::new(PlotPoints::from(vec![[50.0, -22.5], [146.0, -22.5], [146.0, -20.5], [50.0, -20.5]]))
                            .fill_color(p.warning.gamma_multiply(0.5))
                            .stroke(Stroke::NONE),
                    );
                    pui.text(
                        Text::new(PlotPoint::new(98.0, -23.0), RichText::new(t!("DFS (Kanal 52–144, Radar-Erkennung)", "DFS (channels 52–144, radar detection)")).small().color(p.warning))
                            .anchor(Align2::CENTER_TOP),
                    );
                }
                for r in &recs {
                    let c = color_of(r);
                    let is_sel = sel == Some(r.info.bssid);
                    let dim = sel.is_some() && !is_sel;
                    let (fa, sw): (f32, f32) = if is_sel { (0.35, 3.0) } else if dim { (0.05, 1.0) } else { (0.16, 1.6) };
                    let stroke_c = if dim { c.gamma_multiply(0.4) } else { c };
                    pui.polygon(
                        Polygon::new(PlotPoints::from(ap_shape(r.info.center, half_span(r), r.info.rssi as f64)))
                            .fill_color(c.gamma_multiply(fa))
                            .stroke(Stroke::new(sw, stroke_c)),
                    );
                }
                for (center, top, names) in &label_stacks {
                    let shown = names.len().min(5);
                    for (n, (name, c, b)) in names.iter().take(shown).enumerate() {
                        let dim = sel.is_some() && sel != Some(*b);
                        let mut txt = RichText::new(name).size(11.5).color(if dim { text_col.gamma_multiply(0.35) } else { *c });
                        if sel == Some(*b) {
                            txt = txt.strong();
                        }
                        let y = top + 0.8 + (shown - 1 - n) as f64 * 3.0;
                        pui.text(Text::new(PlotPoint::new(*center, y), txt).anchor(Align2::CENTER_BOTTOM));
                    }
                    if names.len() > shown {
                        pui.text(
                            Text::new(
                                PlotPoint::new(*center, top + 0.8 + shown as f64 * 3.0),
                                RichText::new(crate::tf!("+{} weitere", "+{} more", names.len() - shown)).small().color(text_col.gamma_multiply(0.6)),
                            )
                            .anchor(Align2::CENTER_BOTTOM),
                        );
                    }
                }
                (pui.pointer_coordinate(), pui.response().clicked())
            });
        crate::widgets::plot_style(ui, false);
        let (ptr, clicked) = resp.inner;
        let hits: Vec<&&ApRecord> = match ptr {
            Some(pt) => {
                let mut h: Vec<&&ApRecord> = recs.iter().filter(|r| (pt.x - r.info.center).abs() <= half_span(r) && pt.y <= r.info.rssi as f64).collect();
                h.sort_by_key(|r| -r.info.rssi);
                h
            }
            None => vec![],
        };
        let pick = if clicked { Some(hits.first().map(|r| r.info.bssid)) } else { None };
        ui.horizontal_wrapped(|ui| {
            if hits.is_empty() {
                let hint = if band == Band::B24 {
                    t!("Maus über eine Kurve bewegen, um die Netze darunter zu sehen.", "Hover a curve to see the networks below the cursor.")
                } else {
                    t!(
                        "Maus über eine Kurve bewegen · Mausrad/Ziehen zoomt · Doppelklick setzt zurück.",
                        "Hover a curve · scroll/drag to zoom · double-click to reset."
                    )
                };
                ui.label(RichText::new(hint).size(12.5).color(p.weak));
            } else {
                ui.label(RichText::new(t!("Unter dem Cursor:", "Under the cursor:")).size(12.5).color(p.weak));
                for r in hits.iter().take(8) {
                    ui.label(
                        RichText::new(tf!("{} ({} dBm, Kanal {})", "{} ({} dBm, ch {})", Self::display_ssid(r), r.info.rssi, r.info.channel))
                            .size(12.5)
                            .color(color_of(r)),
                    );
                }
            }
        });
        let _ = now;
        self.chan_band = band_sel;
        if let Some(p) = pick {
            self.selected = p;
        }
    }

    // ------------------------------------------------------------------ Bewertung

    pub(crate) fn page_rating(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        let mut band = self.chan_band;
        let now = Instant::now();
        page_header(
            ui,
            t!("Kanalbewertung", "Channel rating"),
            t!(
                "Störpegel je 20-MHz-Kanal aus allen sichtbaren Access Points (niedriger ist besser)",
                "Interference per 20 MHz channel from all visible access points (lower is better)"
            ),
            |ui| {
                segmented(ui, &mut band, &[(Band::B6, "6 GHz"), (Band::B5, "5 GHz"), (Band::B24, t!("2,4 GHz", "2.4 GHz"))]);
            },
        );
        self.chan_band = band;
        let chans = band_channels(band);
        let loads: Vec<(u32, f64, usize)> = chans
            .iter()
            .map(|&c| {
                let (mw, n) = self.channel_load(band, c, now);
                (c, Self::mw_to_score(mw), n)
            })
            .collect();

        // ---------- Empfehlung ----------
        card(ui, Some(t!("💡 Empfehlung", "💡 Recommendation")), |ui| {
            ui.set_width(ui.available_width());
            let line = |ui: &mut Ui, title: &str, r: Option<(u32, f64, usize)>| {
                if let Some((c, s, n)) = r {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(title).color(p.weak));
                        ui.label(RichText::new(tf!("Kanal {c}", "Channel {c}")).strong().size(16.0).color(load_color(s)));
                        ui.label(RichText::new(tf!("· {n} überlappende APs · Störpegel {:.0} dB", "· {n} overlapping APs · interference {:.0} dB", s)).color(p.weak));
                    });
                }
            };
            match band {
                Band::B24 => {
                    line(ui, t!("Überlappungsfrei (1/6/11):", "Non-overlapping (1/6/11):"), self.best_of(band, &[1, 6, 11], now));
                    line(ui, t!("Europa (1/5/9/13):", "Europe (1/5/9/13):"), self.best_of(band, &[1, 5, 9, 13], now));
                }
                Band::B5 => {
                    line(ui, t!("20 MHz ohne DFS:", "20 MHz without DFS:"), self.best_of(band, &NON_DFS_5, now));
                    line(ui, t!("20 MHz inkl. DFS:", "20 MHz incl. DFS:"), self.best_of(band, &chans, now));
                    let blocks = [(36u32, 42u32), (52, 58), (100, 106), (116, 122), (132, 138), (149, 155)];
                    let best80 = blocks
                        .iter()
                        .map(|(s, c)| (*s, *c, (0..4).map(|k| self.channel_load(band, s + 4 * k, now).0).sum::<f64>()))
                        .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
                    if let Some((s, c, mw)) = best80 {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(t!("80-MHz-Block:", "80 MHz block:")).color(p.weak));
                            let dfs = if is_dfs(s) { " · DFS" } else { "" };
                            ui.label(
                                RichText::new(tf!("Kanal {}–{} (Mitte {c}){dfs}", "Channels {}–{} (center {c}){dfs}", s, s + 12))
                                    .strong()
                                    .size(16.0)
                                    .color(load_color(Self::mw_to_score(mw))),
                            );
                        });
                    }
                }
                _ => {
                    let psc: Vec<u32> = chans.iter().copied().filter(|c| is_psc(*c)).collect();
                    line(ui, t!("PSC-Kanal:", "PSC channel:"), self.best_of(band, &psc, now));
                }
            }
            if self.scan_count == 0 {
                ui.label(RichText::new(t!("Noch kein Scan – Werte folgen in Kürze.", "No scan yet – values will follow shortly.")).color(p.weak));
            }
        });
        ui.add_space(8.0);

        let used: HashSet<u32> = self.visible_records().iter().filter(|r| r.info.band == band).map(|r| r.info.channel).collect();
        let bars: Vec<Bar> = loads
            .iter()
            .map(|(c, s, n)| {
                let dfs = if band == Band::B5 && is_dfs(*c) { " (DFS)" } else { "" };
                let mine = if used.contains(c) { " ●" } else { "" };
                Bar::new(*c as f64, s.max(0.5))
                    .width(if band == Band::B24 { 0.7 } else { 2.8 })
                    .fill(load_color(*s))
                    .name(tf!("Kanal {c}{dfs}{mine} · {n} APs", "Channel {c}{dfs}{mine} · {n} APs"))
            })
            .collect();
        let steps = if band == Band::B24 { [1.0, 1.0, 1.0] } else { [4.0, 4.0, 16.0] };
        crate::widgets::plot_style(ui, true);
        Plot::new(("rating", band as u8))
            .height((ui.available_height() - 10.0).max(200.0))
            .include_y(0.0)
            .include_y(70.0)
            .allow_zoom(false)
            .allow_drag(false)
            .allow_scroll(false)
            .x_grid_spacer(uniform_grid_spacer(move |_| steps))
            .custom_x_axes(vec![AxisHints::new_x()
                .label(t!("Kanal", "Channel"))
                .label_spacing(22.0..=30.0)
                .formatter(|m, _| if (m.value - m.value.round()).abs() < 1e-3 && m.value > 0.5 { format!("{}", m.value.round()) } else { String::new() })])
            .y_axis_formatter(|m, _| if m.value >= 0.0 && (m.value % 10.0).abs() < 1e-6 { format!("{} dB", m.value) } else { String::new() })
            .show(ui, |pui| {
                pui.bar_chart(BarChart::new(bars).element_formatter(Box::new(|b, _| format!("{}\n{:.0} dB", b.name, b.value))));
            });
        crate::widgets::plot_style(ui, false);
    }
}
