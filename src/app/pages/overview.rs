//! Übersicht (Dashboard): Kennzahlen, Verbindung, Bänder, Sicherheit, stärkste Netze, Befunde.

use std::collections::BTreeMap;
use std::time::Instant;

use eframe::egui::{self, RichText, Sense, Ui};

use crate::app::{Page, Sev, WifiApp};
use crate::ie::SecLevel;
use crate::model::*;
use crate::theme::pal;
use crate::widgets::{bar_row, card, card_full, donut, kpi_tile, page_header, sec_color, signal_bar, signal_gauge};
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

/// Gleich hohe Karten in einer Reihe
const ROW_H: f32 = 236.0;
/// erste Reihe: Platz für die Verbindungsdetails, damit alle drei Karten gleich hoch sind
const ROW1_H: f32 = 266.0;

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

        let sub = if self.scan_count == 0 {
            t!("Suche WLAN-Netze in der Umgebung …", "Searching for Wi-Fi networks nearby …").to_string()
        } else {
            tf!("{} Netzwerke und {} Access Points in Reichweite", "{} networks and {} access points in range", ssids.len(), vis.len())
        };
        page_header(ui, t!("Übersicht", "Overview"), &sub, |_| {});

        let mut go: Option<Page> = None;
        let mut pick: Option<[u8; 6]> = None;
        let mut measure: Option<String> = None;
        let mut band_pick: Option<Band> = None;

        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            // Breite bestimmt das Raster: schmal (z. B. mit offener Detailansicht) = weniger Spalten
            let avail = ui.available_width();
            // ---------- Kennzahlen ----------
            let bands = format!("{}: {} · 5: {} · 6: {}", t!("2,4", "2.4"), per_band(Band::B24), per_band(Band::B5), per_band(Band::B6));
            let (val, col, csub) = match &self.conn {
                Some(c) => (c.rssi.map(|r| format!("{r} dBm")).unwrap_or("–".into()), c.rssi.map(signal_color).unwrap_or(p.success), c.ssid.clone()),
                None => ("–".into(), p.weak, t!("nicht verbunden", "not connected").into()),
            };
            let (fv, fc) = if crit > 0 {
                (crit.to_string(), p.danger)
            } else if warn > 0 {
                (warn.to_string(), p.warning)
            } else {
                ("0".into(), p.success)
            };
            let ch = |c: Option<u32>| c.map(|c| tf!("Kanal {c}", "Channel {c}")).unwrap_or("–".into());
            // (Icon, Titel, Wert, Unterzeile, Farbe)
            let tiles: [(&str, &str, String, String, egui::Color32); 6] = [
                ("\u{E150}", t!("Netzwerke", "Networks"), ssids.len().to_string(), tf!("+ {hidden} versteckte APs", "+ {hidden} hidden APs"), p.accent),
                ("\u{E4EA}", "Access Points", vis.len().to_string(), bands, p.purple),
                ("\u{E2E2}", t!("Verbindung", "Connection"), val, csub, col),
                ("\u{E4E0}", t!("Befunde", "Findings"), fv, tf!("{crit} kritisch · {warn} Warnungen", "{crit} critical · {warn} warnings"), fc),
                ("\u{E67E}", t!("Empfehlung 2,4 GHz", "Best 2.4 GHz"), ch(best24), t!("aus 1 / 6 / 11", "of 1 / 6 / 11").into(), p.success),
                ("\u{E67E}", t!("Empfehlung 5 GHz", "Best 5 GHz"), ch(best5), t!("ohne DFS", "non-DFS").into(), p.success),
            ];
            // eine Kachel braucht ca. 175 px, sonst werden Werte abgeschnitten; Raster 6 / 3 / 2
            let fit = ((avail + 10.0) / 185.0).floor() as usize;
            let per_row = if fit >= 6 { 6 } else if fit >= 3 { 3 } else { 2 };
            let mut clicked_tile: Option<usize> = None;
            for (r, chunk) in tiles.chunks(per_row).enumerate() {
                if r > 0 {
                    ui.add_space(10.0);
                }
                ui.columns(per_row, |c| {
                    let w = (c[0].available_width() - 30.0).max(100.0);
                    for (i, (icon, title, value, sub, color)) in chunk.iter().enumerate() {
                        if kpi_tile(&mut c[i], icon, title, value, sub, *color, w).clicked() {
                            clicked_tile = Some(r * per_row + i);
                        }
                    }
                });
            }
            match clicked_tile {
                Some(0) => go = Some(Page::Networks),
                Some(1) => go = Some(Page::Channels),
                Some(2) => go = Some(Page::History),
                Some(3) => go = Some(Page::Findings),
                Some(4) => {
                    band_pick = Some(Band::B24);
                    go = Some(Page::Rating);
                }
                Some(5) => {
                    band_pick = Some(Band::B5);
                    go = Some(Page::Rating);
                }
                _ => {}
            }
            ui.add_space(12.0);

            // ---------- Verbindung · Bänder · Sicherheit ----------
            let mut conn_card = |ui: &mut Ui, h: f32| {
                card_full(ui, Some(t!("Aktuelle Verbindung", "Current connection")), h, |ui| {
                    match &self.conn {
                        Some(c) => {
                            ui.horizontal(|ui| {
                                signal_gauge(ui, c.rssi, false, 0.5);
                                ui.vertical(|ui| {
                                    ui.add_space(8.0);
                                    ui.label(RichText::new(&c.ssid).size(17.0).strong().color(p.text));
                                    let vendor = self.aps.get(&c.bssid).map(Self::vendor_label).unwrap_or_default();
                                    ui.label(RichText::new(vendor).color(p.weak));
                                    ui.label(RichText::new(fmt_mac(&c.bssid)).monospace().size(12.0).color(p.weak));
                                });
                            });
                            egui::Grid::new("ov_conn").num_columns(2).spacing([14.0, 4.0]).show(ui, |ui| {
                                let k = |ui: &mut Ui, a: &str, b: String| {
                                    ui.label(RichText::new(a).color(p.weak));
                                    ui.label(RichText::new(b).color(p.text));
                                    ui.end_row();
                                };
                                k(ui, t!("Kanal", "Channel"), c.channel.map(|v| v.to_string()).unwrap_or("–".into()));
                                k(ui, "Standard", c.phy.clone());
                                k(ui, t!("Datenrate", "Data rate"), format!("Rx {:.0} · Tx {:.0} Mbit/s", c.rx_mbps, c.tx_mbps));
                                k(ui, t!("Sicherheit", "Security"), format!("{} · {}", c.auth, c.cipher));
                            });
                            ui.add_space(4.0);
                            if ui.button(t!("\u{E6B8} Messung starten", "\u{E6B8} Start survey")).clicked() {
                                measure = Some(c.ssid.clone());
                            }
                        }
                        None => {
                            ui.add_space(40.0);
                            ui.vertical_centered(|ui| {
                                ui.label(RichText::new("\u{E4EA}").size(38.0).color(p.weak));
                                ui.label(RichText::new(t!("Nicht mit einem WLAN verbunden", "Not connected to a Wi-Fi network")).color(p.text));
                                ui.label(RichText::new(t!("Die Analyse der Umgebung funktioniert trotzdem.", "Analysing the environment still works.")).size(12.0).color(p.weak));
                            });
                        }
                    }
                });
            };
            let bands_card = |ui: &mut Ui, h: f32| {
                card_full(ui, Some(t!("Frequenzbänder", "Frequency bands")), h, |ui| {
                    ui.add_space(8.0);
                    let segs = vec![
                        (t!("2,4 GHz", "2.4 GHz").to_string(), per_band(Band::B24), p.accent),
                        ("5 GHz".to_string(), per_band(Band::B5), p.purple),
                        ("6 GHz".to_string(), per_band(Band::B6), p.success),
                    ];
                    donut(ui, &segs, &vis.len().to_string(), "APs");
                });
            };
            let sec_card = |ui: &mut Ui, h: f32| {
                card_full(ui, Some(t!("Sicherheit", "Security")), h, |ui| {
                    ui.add_space(8.0);
                    let lvl = |l: SecLevel| vis.iter().filter(|r| r.info.sec_level == l).count();
                    let segs = vec![
                        (t!("Stark (WPA3/OWE)", "Strong (WPA3/OWE)").to_string(), lvl(SecLevel::Strong), sec_color(ui, SecLevel::Strong)),
                        (t!("Gut (WPA2)", "Good (WPA2)").to_string(), lvl(SecLevel::Ok), p.accent),
                        (t!("Schwach (WEP/TKIP)", "Weak (WEP/TKIP)").to_string(), lvl(SecLevel::Weak), p.warning),
                        (t!("Offen", "Open").to_string(), lvl(SecLevel::Open), p.danger),
                    ];
                    let unsafe_n = lvl(SecLevel::Weak) + lvl(SecLevel::Open);
                    donut(ui, &segs, &unsafe_n.to_string(), t!("unsicher", "insecure"));
                });
            };
            if avail >= 1080.0 {
                ui.columns(3, |cols| {
                    conn_card(&mut cols[0], ROW1_H);
                    bands_card(&mut cols[1], ROW1_H);
                    sec_card(&mut cols[2], ROW1_H);
                });
            } else {
                // schmal: Verbindung über die ganze Breite, Diagramme darunter
                conn_card(ui, 0.0);
                ui.add_space(12.0);
                if avail >= 760.0 {
                    ui.columns(2, |cols| {
                        bands_card(&mut cols[0], 200.0);
                        sec_card(&mut cols[1], 200.0);
                    });
                } else {
                    bands_card(ui, 0.0);
                    ui.add_space(12.0);
                    sec_card(ui, 0.0);
                }
            }
            ui.add_space(12.0);

            // ---------- Stärkste Netze · Befunde ----------
            let two = avail >= 900.0;
            let mut strongest = |ui: &mut Ui, h: f32| {
                card_full(ui, Some(t!("Stärkste Netze", "Strongest networks")), h, |ui| {
                    if ssids.is_empty() {
                        ui.label(RichText::new(t!("Noch keine Netze gefunden", "No networks found yet")).color(p.weak));
                    }
                    for (s, _) in ssids.iter().take(6) {
                        let Some(best) = self.best_ap_of(s) else { continue };
                        let n = vis.iter().filter(|r| r.info.ssid == *s && !r.info.hidden).count();
                        let resp = ui
                            .horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.set_width((ui.available_width() - 190.0).max(160.0));
                                    ui.label(RichText::new(s).strong().color(p.text));
                                    let sc = sec_color(ui, best.info.sec_level);
                                    ui.label(
                                        RichText::new(tf!("{n} AP · {} · {}", "{n} AP · {} · {}", best.info.band.label(), best.info.security))
                                            .size(11.5)
                                            .color(sc),
                                    );
                                });
                                signal_bar(ui, best.info.rssi, false);
                            })
                            .response
                            .interact(Sense::click())
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .on_hover_text(t!("Klicken: Details anzeigen", "Click: show details"));
                        if resp.clicked() {
                            pick = Some(best.info.bssid);
                        }
                        ui.add_space(2.0);
                    }
                });
            };
            let mut go_findings = false;
            let mut top_findings = |ui: &mut Ui, h: f32| {
                card_full(ui, Some(t!("Wichtigste Befunde", "Top findings")), h, |ui| {
                    if findings.is_empty() {
                        ui.add_space(30.0);
                        ui.vertical_centered(|ui| {
                            ui.label(RichText::new("\u{E182}").size(34.0).color(p.success));
                            ui.label(RichText::new(t!("Keine Auffälligkeiten", "Nothing unusual")).color(p.text));
                        });
                    }
                    for f in findings.iter().take(6) {
                        let col = match f.sev {
                            Sev::Critical => p.danger,
                            Sev::Warning => p.warning,
                            Sev::Info => p.accent,
                        };
                        ui.horizontal(|ui| {
                            crate::widgets::dot(ui, col, 4.5);
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&f.title).color(p.text));
                                ui.label(RichText::new(f.category).size(11.5).color(p.weak));
                            });
                        });
                    }
                    if !findings.is_empty() {
                        ui.add_space(4.0);
                        if ui.link(tf!("Alle {} Befunde mit Empfehlungen anzeigen »", "Show all {} findings with recommendations »", findings.len())).clicked() {
                            go_findings = true;
                        }
                    }
                });
            };
            if two {
                ui.columns(2, |cols| {
                    strongest(&mut cols[0], ROW_H);
                    top_findings(&mut cols[1], ROW_H);
                });
            } else {
                strongest(ui, 0.0);
                ui.add_space(12.0);
                top_findings(ui, 0.0);
            }
            if go_findings {
                go = Some(Page::Findings);
            }
            ui.add_space(12.0);

            // ---------- Standards · Hersteller ----------
            let standards = |ui: &mut Ui| {
                card_full(ui, Some(t!("WLAN-Standards", "Wi-Fi standards")), 0.0, |ui| {
                    for (k, n) in ranked(vis.iter().map(|r| r.info.standard.generation(r.info.band))) {
                        bar_row(ui, k, n, vis.len(), p.accent);
                    }
                    if vis.is_empty() {
                        ui.label(RichText::new("–").color(p.weak));
                    }
                });
            };
            let vendors = |ui: &mut Ui| {
                card_full(ui, Some(t!("Hersteller", "Vendors")), 0.0, |ui| {
                    let labels: Vec<String> = vis.iter().map(|r| Self::vendor_label(r)).collect();
                    for (k, n) in ranked(labels.iter().map(|s| s.as_str())).into_iter().take(6) {
                        bar_row(ui, k, n, vis.len(), p.purple);
                    }
                    if vis.is_empty() {
                        ui.label(RichText::new("–").color(p.weak));
                    }
                });
            };
            if two {
                ui.columns(2, |cols| {
                    standards(&mut cols[0]);
                    vendors(&mut cols[1]);
                });
            } else {
                standards(ui);
                ui.add_space(12.0);
                vendors(ui);
            }
            ui.add_space(12.0);

            card(ui, Some(t!("Letzte Ereignisse", "Recent events")), |ui| {
                ui.set_width(ui.available_width());
                if self.log.is_empty() {
                    ui.label(RichText::new(t!("Noch keine Ereignisse", "No events yet")).color(p.weak));
                }
                for e in self.log.iter().take(5) {
                    let col = match e.sev {
                        Sev::Critical => p.danger,
                        Sev::Warning => p.warning,
                        Sev::Info => p.weak,
                    };
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(e.time.format("%H:%M").to_string()).monospace().color(p.weak));
                        crate::widgets::dot(ui, col, 3.5);
                        ui.label(RichText::new(&e.text).color(p.text));
                    });
                }
                if self.log.len() > 5 && ui.link(t!("Alle Ereignisse »", "All events »")).clicked() {
                    go = Some(Page::Events);
                }
            });
        });

        if let Some(b) = band_pick {
            self.chan_band = b;
        }
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
