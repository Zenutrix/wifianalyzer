//! Detailansicht (rechts) für den ausgewählten Access Point.

use std::time::Instant;

use eframe::egui::{self, Align, Layout, Margin, RichText, Stroke, Ui};

use super::WifiApp;
use crate::ie::ie_name;
use crate::model::*;
use crate::theme::{pal, pal_ctx};
use crate::widgets::{kv, sec_color, yes_no};
use crate::{t, tf};

fn section(ui: &mut Ui, id: &str, title: &str, open: bool, add: impl FnOnce(&mut Ui)) {
    let p = pal(ui);
    egui::CollapsingHeader::new(RichText::new(title).strong().color(p.text)).id_salt(id).default_open(open).show(ui, |ui| {
        egui::Grid::new(format!("{id}_grid")).num_columns(2).spacing([14.0, 5.0]).striped(true).show(ui, add);
    });
}

impl WifiApp {
    pub(super) fn inspector(&mut self, ctx: &egui::Context) {
        let Some(sel) = self.selected else { return };
        let Some(r) = self.aps.get(&sel) else { return };
        let p = pal_ctx(ctx);
        let now = Instant::now();
        let stale = self.is_stale(r, now);
        let pinned = self.pinned.contains(&sel);
        let (mut close, mut pin, mut finder, mut measure) = (false, false, false, false);
        let ssid = r.info.ssid.clone();

        egui::SidePanel::right("inspector")
            .resizable(true)
            .default_width(380.0)
            .width_range(320.0..=600.0)
            .frame(egui::Frame::none().fill(p.surface).inner_margin(Margin::same(16.0)).stroke(Stroke::new(1.0f32, p.border)))
            .show(ctx, |ui| {
                let i = &r.info;
                ui.horizontal(|ui| {
                    crate::widgets::dot(ui, ap_color(&sel), 5.0);
                    ui.label(RichText::new(Self::display_ssid(r)).size(19.0).strong().color(p.text));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("\u{E4F6}").on_hover_text(t!("Schließen (Esc)", "Close (Esc)")).clicked() {
                            close = true;
                        }
                    });
                });
                ui.label(RichText::new(format!("{}  ·  {}", fmt_mac(&sel), Self::vendor_label(r))).monospace().color(p.weak));
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.button(if pinned { t!("★ Markiert", "★ Pinned") } else { t!("☆ Markieren", "☆ Pin") }).clicked() {
                        pin = true;
                    }
                    if ui.button(t!("\u{E1D6} Orten", "\u{E1D6} Locate")).on_hover_text(t!("Im AP-Finder öffnen", "Open in AP finder")).clicked() {
                        finder = true;
                    }
                    if !i.hidden && ui.button(t!("\u{E6B8} Messen", "\u{E6B8} Survey")).on_hover_text(t!("Messung für diese SSID starten", "Start a survey for this SSID")).clicked() {
                        measure = true;
                    }
                    if ui.button("\u{E198} BSSID").on_hover_text(t!("BSSID kopieren", "Copy BSSID")).clicked() {
                        ui.ctx().copy_text(fmt_mac(&sel));
                    }
                });
                ui.add_space(8.0);

                // Signal
                egui::Frame::none().fill(p.surface2).rounding(8.0).inner_margin(Margin::same(12.0)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let col = if stale { p.weak } else { signal_color(i.rssi) };
                        ui.label(RichText::new(format!("{} dBm", i.rssi)).size(30.0).strong().color(col));
                        ui.vertical(|ui| {
                            ui.label(RichText::new(if stale { t!("veraltet", "stale") } else { signal_text(i.rssi) }).strong().color(col));
                            ui.label(RichText::new(tf!("min {} · Ø {:.1} · max {}", "min {} · avg {:.1} · max {}", r.rssi_min, r.rssi_avg(), r.rssi_max)).color(p.weak));
                        });
                    });
                    ui.add(egui::ProgressBar::new(signal_frac(i.rssi)).fill(signal_color(i.rssi)).desired_height(6.0));
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{} · {} · {}", i.band.label(), tf!("Kanal {}", "Channel {}", i.channel), i.width_label)).color(p.weak));
                    });
                });
                ui.add_space(6.0);

                egui::ScrollArea::vertical().show(ui, |ui| {
                    section(ui, "i_general", t!("Allgemein", "General"), true, |ui| {
                        kv(ui, "BSSID", fmt_mac(&i.bssid));
                        let virt = if r.vendor_virtual { t!(" (virtuelle BSSID)", " (virtual BSSID)") } else { "" };
                        kv(ui, t!("Hersteller", "Vendor"), format!("{}{virt}", Self::vendor_label(r)));
                        if let Some(n) = &i.ap_name {
                            kv(ui, t!("AP-Name", "AP name"), n.clone());
                        }
                        if let Some(c) = &i.chipset {
                            kv(ui, t!("Chipsatz", "Chipset"), c.clone());
                        }
                        kv(ui, t!("Laufzeit (TSF)", "Uptime (TSF)"), fmt_duration(i.uptime_s));
                        kv(ui, t!("Erstmals gesehen", "First seen"), r.first_seen_wall.format("%H:%M:%S").to_string());
                        if stale {
                            kv(ui, t!("Zuletzt gesehen", "Last seen"), fmt_ago(r.age(now)));
                        }
                        kv(ui, t!("Messwerte", "Samples"), r.samples.to_string());
                        if let Some(c) = &i.country {
                            kv(ui, t!("Land", "Country"), c.clone());
                        }
                    });
                    section(ui, "i_radio", t!("Funk", "Radio"), true, |ui| {
                        let mut ch = i.channel.to_string();
                        if i.band == Band::B5 && is_dfs(i.channel) {
                            ch.push_str(" (DFS)");
                        }
                        if i.band == Band::B6 && is_psc(i.channel) {
                            ch.push_str(" (PSC)");
                        }
                        kv(ui, t!("Kanal", "Channel"), ch);
                        kv(ui, t!("Frequenz", "Frequency"), format!("{} MHz", i.freq_mhz));
                        kv(ui, t!("Kanalbreite", "Channel width"), i.width_label.clone());
                        kv(ui, t!("Mittenkanal", "Center channel"), format!("{}", i.center));
                        kv(ui, "Standard", format!("{} – {}", i.standard.generation(i.band), i.standard.ieee()));
                        kv(ui, "Spatial Streams", format!("{}×{}", i.streams, i.streams));
                        kv(ui, t!("Max. PHY-Rate", "Max PHY rate"), format!("{:.0} Mbit/s", i.max_rate));
                        kv(ui, t!("Signalqualität", "Signal quality"), format!("{} %", i.quality));
                        if let Some(c) = i.bss_color {
                            kv(ui, "BSS Color", c.to_string());
                        }
                        if let Some(pw) = i.tx_power {
                            kv(ui, t!("Sendeleistung (TPC)", "Tx power (TPC)"), format!("{pw} dBm"));
                        }
                        if let Some(pc) = i.power_constraint {
                            kv(ui, "Power Constraint", format!("{pc} dB"));
                        }
                        kv(ui, t!("Beacon-Intervall", "Beacon interval"), format!("{:.1} ms", i.beacon_ms));
                        if let Some(d) = i.dtim {
                            kv(ui, t!("DTIM-Periode", "DTIM period"), d.to_string());
                        }
                        let rates = |v: &[f32]| v.iter().map(|r| format!("{r}")).collect::<Vec<_>>().join(", ");
                        kv(ui, t!("Datenraten", "Data rates"), rates(&i.rates));
                        kv(ui, t!("Basisraten", "Basic rates"), rates(&i.basic_rates));
                    });
                    section(ui, "i_sec", t!("Sicherheit", "Security"), true, |ui| {
                        kv(ui, t!("Verfahren", "Method"), RichText::new(&i.security).strong().color(sec_color(ui, i.sec_level)));
                        kv(ui, "AKM", if i.akms.is_empty() { "–".into() } else { i.akms.join(", ") });
                        kv(ui, "Pairwise Cipher", if i.pairwise.is_empty() { "–".into() } else { i.pairwise.join(", ") });
                        kv(ui, "Group Cipher", i.group_cipher.clone());
                        kv(ui, "PMF (802.11w)", i.pmf.label());
                        let wps = match &i.wps {
                            Some(w) => {
                                let mut s = t!("aktiv", "active").to_string();
                                match w.configured {
                                    Some(true) => s.push_str(t!(", konfiguriert", ", configured")),
                                    Some(false) => s.push_str(t!(", NICHT konfiguriert", ", NOT configured")),
                                    None => {}
                                }
                                if w.locked {
                                    s.push_str(t!(", gesperrt", ", locked"));
                                }
                                s
                            }
                            None => t!("aus", "off").into(),
                        };
                        kv(ui, "WPS", wps);
                    });
                    section(ui, "i_load", t!("Last & Roaming", "Load & roaming"), true, |ui| {
                        kv(ui, t!("Clients (BSS Load)", "Clients (BSS load)"), i.stations.map(|v| v.to_string()).unwrap_or("–".into()));
                        kv(ui, t!("Kanalauslastung", "Channel utilization"), i.ch_util.map(|v| format!("{v:.0} %")).unwrap_or("–".into()));
                        if let Some(a) = i.admission_cap {
                            kv(ui, "Admission Capacity", format!("{} µs/s", a as u32 * 32));
                        }
                        kv(ui, "802.11k (RRM)", yes_no(i.k_rrm));
                        kv(ui, "802.11r (Fast Transition)", yes_no(i.r_ft));
                        kv(ui, "802.11v (BSS Transition)", yes_no(i.v_btm));
                        kv(ui, "WMM / QoS", yes_no(i.wmm));
                        kv(ui, "MBO / OCE", yes_no(i.mbo));
                        kv(ui, "Hotspot 2.0", yes_no(i.hs20));
                        kv(ui, "Mesh", yes_no(i.mesh));
                        kv(ui, "Multi-Link (MLO)", yes_no(i.mlo));
                    });
                    if let Some(w) = &i.wps {
                        section(ui, "i_wps", t!("Geräteinfo (WPS)", "Device info (WPS)"), false, |ui| {
                            let o = |v: &Option<String>| v.clone().unwrap_or("–".into());
                            kv(ui, t!("Hersteller", "Manufacturer"), o(&w.manufacturer));
                            kv(ui, t!("Modell", "Model"), o(&w.model));
                            kv(ui, t!("Modellnummer", "Model number"), o(&w.model_number));
                            kv(ui, t!("Gerätename", "Device name"), o(&w.device_name));
                            kv(ui, t!("Seriennummer", "Serial number"), o(&w.serial));
                        });
                    }
                    egui::CollapsingHeader::new(RichText::new(t!("Hersteller-IEs", "Vendor IEs")).strong().color(p.text)).default_open(false).show(ui, |ui| {
                        for v in &i.ie_vendors {
                            ui.label(format!("• {v}"));
                        }
                    });
                    egui::CollapsingHeader::new(RichText::new(tf!("Information Elements ({})", "Information elements ({})", i.ies.len())).strong().color(p.text))
                        .default_open(false)
                        .show(ui, |ui| {
                            for ie in &i.ies {
                                let hex: String = ie.data.iter().map(|b| format!("{b:02X} ")).collect();
                                let title = match ie.ext {
                                    Some(e) => format!("255/{e} {} ({} B)", ie_name(ie.id, ie.ext), ie.data.len()),
                                    None => format!("{} {} ({} B)", ie.id, ie_name(ie.id, None), ie.data.len()),
                                };
                                ui.label(RichText::new(title).strong());
                                ui.label(RichText::new(hex).monospace().small().color(p.weak));
                            }
                        });
                });
            });
        if close {
            self.selected = None;
        }
        if pin {
            self.toggle_pin(sel);
        }
        if finder {
            self.open_finder(sel);
        }
        if measure {
            self.start_measurement(ssid);
        }
    }
}
