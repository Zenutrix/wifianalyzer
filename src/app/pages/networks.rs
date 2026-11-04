//! Netzwerkliste: Tabelle aller Access Points, optional nach SSID gruppiert.

use std::collections::HashMap;
use std::time::Instant;

use eframe::egui::{self, Align, Color32, Layout, RichText, Sense, Ui, Vec2};
use egui_extras::{Column, TableBuilder};

use crate::app::{SortCol, WifiApp};
use crate::ie::SecLevel;
use crate::model::*;
use crate::theme::pal;
use crate::widgets::{load_color, page_header, sec_color, signal_bar};
use crate::{t, tf};

enum RowKind {
    Single,
    Group { key: String, n: usize, expanded: bool, chans: String, bands: String },
    Child,
}

/// Aktionen aus dem Kontextmenü / Zeilenklicks (werden nach dem Zeichnen ausgeführt)
enum RowAction {
    Select([u8; 6]),
    ToggleGroup(String, [u8; 6]),
    Pin([u8; 6]),
    Finder([u8; 6]),
    Measure(String),
}

fn headers() -> [(SortCol, &'static str, &'static str); 13] {
    [
        (SortCol::Pin, "★", t!("Markierte APs (☆ anklicken zum Markieren)", "Pinned APs (click ☆ to pin)")),
        (SortCol::Ssid, "SSID", t!("Netzwerkname", "Network name")),
        (SortCol::Bssid, "BSSID", t!("MAC-Adresse des Access Points", "MAC address of the access point")),
        (SortCol::Vendor, t!("Hersteller", "Vendor"), t!("Aus der OUI der BSSID bzw. WPS", "From the BSSID's OUI or WPS")),
        (SortCol::Signal, "Signal", t!("Empfangsstärke (RSSI)", "Received signal strength (RSSI)")),
        (SortCol::Channel, t!("Kanal", "Channel"), t!("Primärkanal und Band", "Primary channel and band")),
        (SortCol::Width, t!("Breite", "Width"), t!("Kanalbreite", "Channel width")),
        (SortCol::Standard, "Standard", t!("WLAN-Generation / IEEE-Standard", "Wi-Fi generation / IEEE standard")),
        (SortCol::Security, t!("Sicherheit", "Security"), t!("Verschlüsselung und Authentifizierung", "Encryption and authentication")),
        (SortCol::Rate, "Mbit/s", t!("Theoretische maximale PHY-Rate", "Theoretical maximum PHY rate")),
        (SortCol::Clients, "Clients", t!("Verbundene Clients laut BSS-Load-Element", "Associated clients from the BSS load element")),
        (SortCol::Util, t!("Last", "Load"), t!("Kanalauslastung laut BSS-Load-Element", "Channel utilization from the BSS load element")),
        (SortCol::Seen, t!("Gesehen", "Seen"), t!("Zuletzt im Scan gesehen", "Last seen in a scan")),
    ]
}

impl WifiApp {
    pub(crate) fn page_networks(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        let keys = self.sorted_keys();
        let now = Instant::now();

        // Zeilen: einzeln oder nach SSID gruppiert (stärkster aktueller AP repräsentiert die Gruppe)
        let mut rows: Vec<([u8; 6], RowKind)> = Vec::new();
        let group_count;
        if self.settings.group_ssid {
            let mut order: Vec<String> = Vec::new();
            let mut groups: HashMap<String, Vec<[u8; 6]>> = HashMap::new();
            for k in &keys {
                let g = Self::group_key(&self.aps[k]);
                if !groups.contains_key(&g) {
                    order.push(g.clone());
                }
                groups.entry(g).or_default().push(*k);
            }
            group_count = order.len();
            for g in order {
                let mut members = groups.remove(&g).unwrap_or_default();
                if members.len() == 1 {
                    rows.push((members[0], RowKind::Single));
                    continue;
                }
                members.sort_by_key(|k| {
                    let r = &self.aps[k];
                    (self.is_stale(r, now), -r.info.rssi)
                });
                let mut chans: Vec<u32> = members.iter().map(|k| self.aps[k].info.channel).collect();
                chans.sort();
                chans.dedup();
                let mut bands: Vec<Band> = members.iter().map(|k| self.aps[k].info.band).collect();
                bands.sort();
                bands.dedup();
                let expanded = self.expanded.contains(&g);
                rows.push((
                    members[0],
                    RowKind::Group {
                        key: g,
                        n: members.len(),
                        expanded,
                        chans: chans.iter().map(|c| c.to_string()).collect::<Vec<_>>().join(", "),
                        bands: bands.iter().map(|b| b.label()).collect::<Vec<_>>().join(" / "),
                    },
                ));
                if expanded {
                    rows.extend(members.into_iter().map(|m| (m, RowKind::Child)));
                }
            }
        } else {
            group_count = keys.iter().map(|k| Self::group_key(&self.aps[k])).collect::<std::collections::HashSet<_>>().len();
            rows = keys.iter().map(|k| (*k, RowKind::Single)).collect();
        }

        let mut group = self.settings.group_ssid;
        let mut expand_all = false;
        let mut collapse_all = false;
        page_header(
            ui,
            t!("Netzwerke", "Networks"),
            &tf!("{group_count} SSIDs · {} Access Points", "{group_count} SSIDs · {} access points", keys.len()),
            |ui| {
                if group {
                    if ui.button(t!("Alle zuklappen", "Collapse all")).clicked() {
                        collapse_all = true;
                    }
                    if ui.button(t!("Alle aufklappen", "Expand all")).clicked() {
                        expand_all = true;
                    }
                }
                ui.checkbox(&mut group, t!("Nach SSID gruppieren", "Group by SSID"))
                    .on_hover_text(t!("Eine Zeile je Netz mit dem stärksten AP – Klick klappt alle APs auf", "One row per network showing the strongest AP – click to expand all APs"));
            },
        );
        self.settings.group_ssid = group;
        if expand_all {
            let all: Vec<String> = self.aps.values().map(Self::group_key).collect();
            self.expanded.extend(all);
        }
        if collapse_all {
            self.expanded.clear();
        }

        if rows.is_empty() {
            ui.add_space(60.0);
            ui.vertical_centered(|ui| {
                if self.aps.is_empty() {
                    ui.label(RichText::new("📡").size(40.0).color(p.weak));
                    ui.label(RichText::new(t!("Suche WLAN-Netze …", "Searching for Wi-Fi networks …")).size(17.0).color(p.weak));
                } else {
                    ui.label(RichText::new(t!("Kein Netz passt zum Filter", "No network matches the filter")).size(17.0).color(p.weak));
                    if ui.button(t!("Filter zurücksetzen", "Reset filter")).clicked() {
                        self.f = super::super::Filter::with_hidden(true);
                    }
                }
            });
            return;
        }

        let mut action: Option<RowAction> = None;
        let mut sort_click = None;
        let text_col = p.text;
        let child_bg = p.accent.gamma_multiply(0.06);
        let palette = self.ssid_colors();
        let conn_bssid = self.conn.as_ref().map(|c| c.bssid);

        egui::Frame::none()
            .fill(p.surface)
            .stroke(egui::Stroke::new(1.0f32, p.border))
            .rounding(10.0)
            .inner_margin(egui::Margin::symmetric(6.0, 4.0))
            .show(ui, |ui| {
                egui::ScrollArea::horizontal().show(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    TableBuilder::new(ui)
                        .striped(true)
                        .resizable(true)
                        .sense(Sense::click())
                        .cell_layout(Layout::left_to_right(Align::Center))
                        .column(Column::exact(22.0))
                        .column(Column::initial(170.0).at_least(100.0).clip(true))
                        .column(Column::initial(128.0))
                        .column(Column::initial(104.0).at_least(60.0).clip(true))
                        .column(Column::initial(118.0).at_least(80.0))
                        .column(Column::initial(86.0).at_least(60.0).clip(true))
                        .column(Column::initial(62.0))
                        .column(Column::initial(84.0).clip(true))
                        .column(Column::initial(120.0).at_least(80.0).clip(true))
                        .column(Column::initial(56.0))
                        .column(Column::initial(50.0))
                        .column(Column::initial(56.0))
                        .column(Column::remainder().at_least(60.0))
                        .min_scrolled_height(0.0)
                        .header(28.0, |mut h| {
                            for (col, label, hint) in headers() {
                                h.col(|ui| {
                                    let txt = if self.sort_col == col {
                                        format!("{label} {}", if self.sort_asc { "⏶" } else { "⏷" })
                                    } else {
                                        label.to_string()
                                    };
                                    let r = ui
                                        .add(egui::Label::new(RichText::new(txt).strong().color(p.weak)).sense(Sense::click()))
                                        .on_hover_text(format!("{hint}\n{}", t!("Klicken zum Sortieren", "Click to sort")));
                                    if r.clicked() {
                                        sort_click = Some(col);
                                    }
                                });
                            }
                        })
                        .body(|body| {
                            body.rows(26.0, rows.len(), |mut row| {
                                let (k, kind) = &rows[row.index()];
                                let k = *k;
                                let r = &self.aps[&k];
                                let i = &r.info;
                                let stale = self.is_stale(r, now);
                                let tc = if stale { text_col.gamma_multiply(0.45) } else { text_col };
                                let is_child = matches!(kind, RowKind::Child);
                                row.set_selected(self.selected == Some(k));
                                let shade = |ui: &mut Ui| {
                                    if is_child {
                                        ui.painter().rect_filled(ui.max_rect().expand2(Vec2::new(4.0, 2.0)), 0.0, child_bg);
                                    }
                                };
                                row.col(|ui| {
                                    shade(ui);
                                    let pinned = self.pinned.contains(&k);
                                    let star = RichText::new(if pinned { "★" } else { "☆" })
                                        .color(if pinned { Color32::from_rgb(250, 200, 40) } else { tc.gamma_multiply(0.5) });
                                    if ui.add(egui::Label::new(star).sense(Sense::click())).clicked() {
                                        action = Some(RowAction::Pin(k));
                                    }
                                });
                                row.col(|ui| {
                                    shade(ui);
                                    match kind {
                                        RowKind::Group { expanded, .. } => {
                                            ui.label(RichText::new(if *expanded { "⏷" } else { "▶" }).color(p.accent));
                                        }
                                        RowKind::Child => {
                                            ui.add_space(12.0);
                                            ui.label(RichText::new("›").color(p.weak));
                                        }
                                        RowKind::Single => ui.add_space(14.0),
                                    }
                                    let col = if self.settings.group_ssid {
                                        palette.get(&Self::group_key(r)).copied().unwrap_or(p.accent)
                                    } else {
                                        ap_color(&k)
                                    };
                                    let (rect, _) = ui.allocate_exact_size(Vec2::new(4.0, 16.0), Sense::hover());
                                    ui.painter().rect_filled(rect, 2.0, col);
                                    let name = RichText::new(Self::display_ssid(r)).color(tc);
                                    ui.label(if i.hidden { name.italics() } else if is_child { name } else { name.strong() });
                                    if let RowKind::Group { n, .. } = kind {
                                        crate::widgets::badge(ui, &tf!("{n} APs", "{n} APs"), p.accent);
                                    }
                                    if conn_bssid == Some(k) {
                                        ui.label(RichText::new("🔗").color(p.success)).on_hover_text(t!("Verbunden", "Connected"));
                                    }
                                });
                                row.col(|ui| {
                                    shade(ui);
                                    ui.label(RichText::new(fmt_mac(&k)).monospace().color(tc));
                                });
                                row.col(|ui| {
                                    shade(ui);
                                    let r2 = ui.label(RichText::new(Self::vendor_label(r)).color(tc));
                                    if let Some(n) = &i.ap_name {
                                        r2.on_hover_text(tf!("AP-Name: {n}", "AP name: {n}"));
                                    }
                                });
                                row.col(|ui| {
                                    shade(ui);
                                    signal_bar(ui, i.rssi, stale)
                                });
                                row.col(|ui| {
                                    shade(ui);
                                    match kind {
                                        RowKind::Group { chans, bands, .. } => {
                                            ui.label(RichText::new(chans).color(tc)).on_hover_text(bands.as_str());
                                        }
                                        _ => {
                                            ui.label(RichText::new(i.channel.to_string()).color(tc));
                                            let band = match i.band {
                                                Band::B24 => "2,4G",
                                                Band::B5 => "5G",
                                                Band::B6 => "6G",
                                                Band::Other => "?",
                                            };
                                            ui.label(RichText::new(band).small().color(p.weak));
                                            if i.band == Band::B5 && is_dfs(i.channel) {
                                                ui.label(RichText::new("DFS").small().color(p.warning))
                                                    .on_hover_text(t!("DFS-Kanal: Radar-Erkennung, der AP kann den Kanal wechseln", "DFS channel: radar detection, the AP may change channel"));
                                            }
                                        }
                                    }
                                });
                                row.col(|ui| {
                                    shade(ui);
                                    ui.label(RichText::new(&i.width_label).color(tc));
                                });
                                row.col(|ui| {
                                    shade(ui);
                                    ui.label(RichText::new(i.std_label()).color(tc));
                                });
                                row.col(|ui| {
                                    shade(ui);
                                    let icon = if i.sec_level == SecLevel::Open { "🔓" } else { "🔒" };
                                    let col = if stale { tc } else { sec_color(ui, i.sec_level) };
                                    ui.label(RichText::new(format!("{icon} {}", i.security)).color(col));
                                });
                                row.col(|ui| {
                                    shade(ui);
                                    ui.label(RichText::new(format!("{:.0}", i.max_rate)).color(tc));
                                });
                                row.col(|ui| {
                                    shade(ui);
                                    ui.label(RichText::new(i.stations.map(|v| v.to_string()).unwrap_or("–".into())).color(tc));
                                });
                                row.col(|ui| {
                                    shade(ui);
                                    match i.ch_util {
                                        Some(u) => ui.label(RichText::new(format!("{u:.0} %")).color(load_color(u as f64 * 0.6))),
                                        None => ui.label(RichText::new("–").color(tc)),
                                    };
                                });
                                row.col(|ui| {
                                    shade(ui);
                                    ui.label(RichText::new(fmt_ago(r.age(now))).color(tc));
                                });

                                let resp = row.response();
                                if resp.double_clicked() {
                                    action = Some(RowAction::Finder(k));
                                } else if resp.clicked() && action.is_none() {
                                    action = Some(match kind {
                                        RowKind::Group { key, .. } => RowAction::ToggleGroup(key.clone(), k),
                                        _ => RowAction::Select(k),
                                    });
                                }
                                resp.context_menu(|ui| {
                                    if ui.button(t!("Details anzeigen", "Show details")).clicked() {
                                        action = Some(RowAction::Select(k));
                                        ui.close_menu();
                                    }
                                    let pinned = self.pinned.contains(&k);
                                    if ui.button(if pinned { t!("Markierung entfernen", "Unpin") } else { t!("Markieren ★", "Pin ★") }).clicked() {
                                        action = Some(RowAction::Pin(k));
                                        ui.close_menu();
                                    }
                                    if ui.button(t!("Im Signal-Finder orten", "Locate in signal finder")).clicked() {
                                        action = Some(RowAction::Finder(k));
                                        ui.close_menu();
                                    }
                                    if !i.hidden && ui.button(tf!("Messung für „{}“ starten", "Start survey for \"{}\"", i.ssid)).clicked() {
                                        action = Some(RowAction::Measure(i.ssid.clone()));
                                        ui.close_menu();
                                    }
                                    ui.separator();
                                    if ui.button(t!("BSSID kopieren", "Copy BSSID")).clicked() {
                                        ui.ctx().copy_text(fmt_mac(&k));
                                        ui.close_menu();
                                    }
                                    if !i.hidden && ui.button(t!("SSID kopieren", "Copy SSID")).clicked() {
                                        ui.ctx().copy_text(i.ssid.clone());
                                        ui.close_menu();
                                    }
                                });
                            });
                        });
                });
            });
        ui.add_space(4.0);
        ui.label(
            RichText::new(t!(
                "Tipp: Klick = Details · Doppelklick = orten · Rechtsklick = weitere Aktionen",
                "Tip: click = details · double-click = locate · right-click = more actions"
            ))
            .size(12.0)
            .color(p.weak),
        );

        if let Some(c) = sort_click {
            if self.sort_col == c {
                self.sort_asc = !self.sort_asc;
            } else {
                self.sort_col = c;
                self.sort_asc = matches!(c, SortCol::Ssid | SortCol::Bssid | SortCol::Vendor | SortCol::Channel);
            }
        }
        match action {
            Some(RowAction::Select(k)) => self.selected = if self.selected == Some(k) { None } else { Some(k) },
            Some(RowAction::ToggleGroup(g, k)) => {
                if !self.expanded.remove(&g) {
                    self.expanded.insert(g);
                }
                self.selected = Some(k);
            }
            Some(RowAction::Pin(k)) => self.toggle_pin(k),
            Some(RowAction::Finder(k)) => self.open_finder(k),
            Some(RowAction::Measure(s)) => self.start_measurement(s),
            None => {}
        }
    }
}
