//! Übersicht (Dashboard): Kennzahlen, Verbindung, stärkste Netze, Befunde, Verteilungen.

use std::collections::BTreeMap;
use std::time::Instant;

use eframe::egui::{self, RichText, Sense, Ui};

use crate::app::{Page, Sev, WifiApp};
use crate::model::*;
use crate::theme::pal;
use crate::widgets::{bar_row, card, kpi, page_header, sec_color, signal_bar, signal_gauge};
use crate::{t, tf};

/// Zählen und stabil sortieren (Anzahl absteigend, dann Name)
fn ranked<K: Ord + Clone>(items: impl Iterator<Item = K>) -> Vec<(K, usize)> {
    let mut m: BTreeMap<K, usize> = BTreeMap::new();
    for k in items {
        *m.entry(k).or_default() += 1;
    }
    let mut v: Vec<_> = m.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v
}

impl WifiApp {
    pub(crate) fn page_overview(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        let now = Instant::now();
        let vis: Vec<&ApRecord> = self.visible_records().into_iter().filter(|r| !self.is_stale(r, now)).collect();
        let ssids = self.ssid_choices();
        let hidden = vis.iter().filter(|r| r.info.hidden).count();
        let per_band = |b: Band| vis.iter().filter(|r| r.info.band == b).count();
        let findings = self.findings();
        let crit = findings.iter().filter(|f| f.sev == Sev::Critical).count();
        let warn = findings.iter().filter(|f| f.sev == Sev::Warning).count();
        let (best24, best5) = self.best_channels();

        let sub = match self.last_scan {
            Some(t) => tf!("Stand: Scan #{} · {}", "As of scan #{} · {}", self.scan_count, fmt_ago(t.elapsed().as_secs_f32())),
            None => t!("Warte auf den ersten Scan …", "Waiting for the first scan …").into(),
        };
        page_header(ui, t!("Übersicht", "Overview"), &sub, |_| {});

        let mut go: Option<Page> = None;
        let mut pick: Option<[u8; 6]> = None;
        let mut measure: Option<String> = None;

        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            // ---------- Kennzahlen ----------
            ui.horizontal_wrapped(|ui| {
                // sechs Kacheln über die volle Breite (abzgl. Abstand und Innenrand)
                let w = ((ui.available_width() - 5.0 * 8.0) / 6.0 - 30.0).max(118.0);
                if kpi(ui, t!("Netzwerke (SSIDs)", "Networks (SSIDs)"), &ssids.len().to_string(), &tf!("+ {hidden} versteckte APs", "+ {hidden} hidden APs"), p.text, w)
                    .interact(Sense::click())
                    .clicked()
                {
                    go = Some(Page::Networks);
                }
                kpi(
                    ui,
                    "Access Points",
                    &vis.len().to_string(),
                    &format!("{}: {} · 5: {} · 6: {}", t!("2,4", "2.4"), per_band(Band::B24), per_band(Band::B5), per_band(Band::B6)),
                    p.text,
                    w,
                );
                let (val, col) = match self.conn.as_ref().and_then(|c| c.rssi) {
                    Some(r) => (format!("{r} dBm"), signal_color(r)),
                    None => ("–".into(), p.weak),
                };
                kpi(ui, t!("Verbindung", "Connection"), &val, self.conn.as_ref().map(|c| c.ssid.as_str()).unwrap_or(t!("nicht verbunden", "not connected")), col, w);
                let (fv, fc) = if crit > 0 {
                    (crit.to_string(), p.danger)
                } else if warn > 0 {
                    (warn.to_string(), p.warning)
                } else {
                    ("0".into(), p.success)
                };
                let fsub = tf!("{crit} kritisch · {warn} Warnungen", "{crit} critical · {warn} warnings");
                if kpi(ui, t!("Befunde", "Findings"), &fv, &fsub, fc, w).interact(Sense::click()).clicked() {
                    go = Some(Page::Findings);
                }
                let ch = |c: Option<u32>| c.map(|c| c.to_string()).unwrap_or("–".into());
                if kpi(ui, t!("Empfohlen 2,4 GHz", "Best 2.4 GHz"), &ch(best24), t!("Kanal 1 / 6 / 11", "channel 1 / 6 / 11"), p.accent, w)
                    .interact(Sense::click())
                    .clicked()
                {
                    go = Some(Page::Rating);
                }
                if kpi(ui, t!("Empfohlen 5 GHz", "Best 5 GHz"), &ch(best5), t!("ohne DFS", "non-DFS"), p.accent, w).interact(Sense::click()).clicked() {
                    go = Some(Page::Rating);
                }
            });
            ui.add_space(10.0);

            // ---------- Verbindung · stärkste Netze · Befunde ----------
            ui.columns(3, |cols| {
                card(&mut cols[0], Some(t!("Aktuelle Verbindung", "Current connection")), |ui| {
                    ui.set_min_height(250.0);
                    match &self.conn {
                        Some(c) => {
                            ui.vertical_centered(|ui| signal_gauge(ui, c.rssi, false, 0.55));
                            ui.label(RichText::new(&c.ssid).size(16.0).strong().color(p.text));
                            let ap = self.aps.get(&c.bssid);
                            ui.label(
                                RichText::new(format!("{} · {}", fmt_mac(&c.bssid), ap.map(Self::vendor_label).unwrap_or_default()))
                                    .monospace()
                                    .size(12.0)
                                    .color(p.weak),
                            );
                            egui::Grid::new("ov_conn").num_columns(2).spacing([12.0, 3.0]).show(ui, |ui| {
                                let k = |ui: &mut Ui, a: &str, b: String| {
                                    ui.label(RichText::new(a).color(p.weak));
                                    ui.label(b);
                                    ui.end_row();
                                };
                                k(ui, t!("Kanal", "Channel"), c.channel.map(|v| v.to_string()).unwrap_or("–".into()));
                                k(ui, "Standard", c.phy.clone());
                                k(ui, t!("Datenrate", "Data rate"), format!("↓ {:.0} / ↑ {:.0} Mbit/s", c.rx_mbps, c.tx_mbps));
                                k(ui, t!("Sicherheit", "Security"), format!("{} · {}", c.auth, c.cipher));
                            });
                            if ui.button(t!("📏 Messung für diese SSID starten", "📏 Start a survey for this SSID")).clicked() {
                                measure = Some(c.ssid.clone());
                            }
                        }
                        None => {
                            ui.add_space(30.0);
                            ui.vertical_centered(|ui| {
                                ui.label(RichText::new("📡").size(36.0).color(p.weak));
                                ui.label(RichText::new(t!("Nicht mit einem WLAN verbunden", "Not connected to a Wi-Fi network")).color(p.weak));
                                ui.label(
                                    RichText::new(t!("Der Scan der Umgebung funktioniert trotzdem.", "Scanning the environment still works."))
                                        .size(12.0)
                                        .color(p.weak),
                                );
                            });
                        }
                    }
                });

                card(&mut cols[1], Some(t!("Stärkste Netze", "Strongest networks")), |ui| {
                    ui.set_min_height(250.0);
                    if ssids.is_empty() {
                        ui.label(RichText::new(t!("Noch keine Netze gefunden", "No networks found yet")).color(p.weak));
                    }
                    for (s, _) in ssids.iter().take(7) {
                        let Some(best) = self.best_ap_of(s) else { continue };
                        let n = vis.iter().filter(|r| r.info.ssid == *s && !r.info.hidden).count();
                        let resp = ui
                            .horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.set_width(ui.available_width() * 0.55);
                                    ui.label(RichText::new(s).strong().color(p.text));
                                    ui.label(
                                        RichText::new(tf!("{n} AP · {} · {}", "{n} AP · {} · {}", best.info.band.label(), best.info.security))
                                            .size(11.5)
                                            .color(sec_color(ui, best.info.sec_level).gamma_multiply(0.9)),
                                    );
                                });
                                signal_bar(ui, best.info.rssi, false);
                            })
                            .response
                            .interact(Sense::click())
                            .on_hover_text(t!("Klicken: Details anzeigen", "Click: show details"));
                        if resp.clicked() {
                            pick = Some(best.info.bssid);
                        }
                    }
                });

                card(&mut cols[2], Some(t!("Wichtigste Befunde", "Top findings")), |ui| {
                    ui.set_min_height(250.0);
                    if findings.is_empty() {
                        ui.label(RichText::new(t!("✔ Keine Auffälligkeiten", "✔ Nothing unusual")).color(p.success));
                    }
                    for f in findings.iter().take(6) {
                        let col = match f.sev {
                            Sev::Critical => p.danger,
                            Sev::Warning => p.warning,
                            Sev::Info => p.accent,
                        };
                        ui.horizontal(|ui| {
                            crate::widgets::dot(ui, col, 4.0);
                            ui.label(RichText::new(&f.title).color(p.text));
                        });
                    }
                    if findings.len() > 6 {
                        ui.add_space(4.0);
                    }
                    if !findings.is_empty() && ui.link(tf!("Alle {} Befunde anzeigen »", "Show all {} findings »", findings.len())).clicked() {
                        go = Some(Page::Findings);
                    }
                });
            });
            ui.add_space(10.0);

            // ---------- Verteilungen ----------
            ui.columns(3, |cols| {
                card(&mut cols[0], Some(t!("Standards", "Standards")), |ui| {
                    for (k, n) in ranked(vis.iter().map(|r| r.info.standard.generation(r.info.band))) {
                        bar_row(ui, k, n, vis.len(), p.accent);
                    }
                });
                card(&mut cols[1], Some(t!("Sicherheit", "Security")), |ui| {
                    for ((l, k), n) in ranked(vis.iter().map(|r| (r.info.sec_level, r.info.security.as_str()))) {
                        let c = sec_color(ui, l);
                        bar_row(ui, k, n, vis.len(), c);
                    }
                });
                card(&mut cols[2], Some(t!("Hersteller", "Vendors")), |ui| {
                    let labels: Vec<String> = vis.iter().map(|r| Self::vendor_label(r)).collect();
                    for (k, n) in ranked(labels.iter().map(|s| s.as_str())).into_iter().take(6) {
                        bar_row(ui, k, n, vis.len(), p.purple);
                    }
                });
            });
            ui.add_space(10.0);

            card(ui, Some(t!("Letzte Ereignisse", "Recent events")), |ui| {
                ui.set_width(ui.available_width());
                if self.log.is_empty() {
                    ui.label(RichText::new(t!("Noch keine Ereignisse", "No events yet")).color(p.weak));
                }
                for e in self.log.iter().take(6) {
                    let col = match e.sev {
                        Sev::Critical => p.danger,
                        Sev::Warning => p.warning,
                        Sev::Info => p.weak,
                    };
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(e.time.format("%H:%M:%S").to_string()).monospace().color(p.weak));
                        crate::widgets::dot(ui, col, 3.5);
                        ui.label(RichText::new(&e.text).color(p.text));
                    });
                }
                if self.log.len() > 6 && ui.link(t!("Alle Ereignisse »", "All events »")).clicked() {
                    go = Some(Page::Events);
                }
            });
        });

        if let Some(g) = go {
            self.page = g;
        }
        if pick.is_some() {
            self.selected = pick;
        }
        if let Some(s) = measure {
            self.start_measurement(s);
        }
    }
}
