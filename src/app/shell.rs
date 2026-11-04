//! Rahmen des Hauptfensters: Kopfleiste, Navigation, Statusleiste, Meldungen.

use std::time::Instant;

use eframe::egui::{self, Align, Align2, Color32, FontId, Layout, Margin, Response, RichText, Sense, Stroke, Ui, Vec2};

use super::{Page, Sev, WifiApp, AUTHOR, VERSION, WEBSITE};
use crate::model::{fmt_ago, fmt_mac, signal_color};
use crate::theme::{pal, pal_ctx};
use crate::widgets::dot;
use crate::{t, tf};

/// Reihenfolge der Seiten (auch für Strg+1 … Strg+9)
pub const NAV_ORDER: [Page; 9] = [
    Page::Overview,
    Page::Networks,
    Page::Channels,
    Page::Rating,
    Page::History,
    Page::Findings,
    Page::Measure,
    Page::Finder,
    Page::Events,
];

impl Page {
    pub fn title(self) -> &'static str {
        match self {
            Page::Overview => t!("Übersicht", "Overview"),
            Page::Networks => t!("Netzwerke", "Networks"),
            Page::Channels => t!("Kanäle", "Channels"),
            Page::Rating => t!("Kanalbewertung", "Channel rating"),
            Page::History => t!("Signalverlauf", "Signal history"),
            Page::Findings => t!("Befunde", "Findings"),
            Page::Measure => t!("Messung", "Survey"),
            Page::Finder => t!("Signal-Finder", "Signal finder"),
            Page::Events => t!("Ereignisse", "Events"),
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Page::Overview => "🏠",
            Page::Networks => "📋",
            Page::Channels => "📶",
            Page::Rating => "🏆",
            Page::History => "📈",
            Page::Findings => "⚠",
            Page::Measure => "📏",
            Page::Finder => "🎯",
            Page::Events => "📜",
        }
    }
}

/// Navigationseintrag: Symbol, Text, optionales Zähler-Badge
fn nav_item(ui: &mut Ui, icon: &str, label: &str, selected: bool, badge: Option<(String, Color32)>, hint: &str) -> Response {
    let p = pal(ui);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::click());
    let painter = ui.painter();
    let bg = if selected {
        p.accent_soft
    } else if resp.hovered() {
        p.surface2
    } else {
        Color32::TRANSPARENT
    };
    let r = rect.shrink2(Vec2::new(0.0, 1.0));
    painter.rect_filled(r, 7.0, bg);
    if selected {
        let bar = egui::Rect::from_min_size(r.left_top() + Vec2::new(0.0, 7.0), Vec2::new(3.0, r.height() - 14.0));
        painter.rect_filled(bar, 2.0, p.accent);
    }
    let fg = if selected { p.text } else { p.text.gamma_multiply(0.88) };
    painter.text(r.left_center() + Vec2::new(14.0, 0.0), Align2::LEFT_CENTER, icon, FontId::proportional(15.0), if selected { p.accent } else { p.weak });
    painter.text(r.left_center() + Vec2::new(40.0, 0.0), Align2::LEFT_CENTER, label, FontId::proportional(14.0), fg);
    if let Some((text, col)) = badge {
        let galley = painter.layout_no_wrap(text, FontId::proportional(11.0), col);
        let w = galley.size().x + 12.0;
        let b = egui::Rect::from_center_size(egui::pos2(r.right() - 10.0 - w / 2.0, r.center().y), Vec2::new(w, 18.0));
        painter.rect(b, 9.0, col.gamma_multiply(0.18), Stroke::new(1.0f32, col.gamma_multiply(0.5)));
        painter.galley(b.center() - galley.size() / 2.0, galley, col);
    }
    resp.on_hover_text(hint).on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn section_label(ui: &mut Ui, text: &str) {
    let p = pal(ui);
    ui.add_space(10.0);
    ui.label(RichText::new(text).size(11.0).strong().color(p.weak));
    ui.add_space(2.0);
}

impl WifiApp {
    pub(super) fn top_bar(&mut self, ctx: &egui::Context) {
        let p = pal_ctx(ctx);
        egui::TopBottomPanel::top("top")
            .exact_height(54.0)
            .frame(egui::Frame::none().fill(p.surface).inner_margin(Margin::symmetric(16.0, 8.0)).stroke(Stroke::new(1.0f32, p.border)))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    if let Some(tex) = &self.icon_tex {
                        ui.add(egui::Image::new((tex.id(), Vec2::splat(28.0))));
                    }
                    ui.label(RichText::new("WiFi Analyzer").size(17.0).strong().color(p.text));
                    ui.label(RichText::new(format!("v{VERSION}")).size(11.5).color(p.weak));
                    ui.add_space(18.0);

                    // Adapter
                    let cur = self
                        .interfaces
                        .iter()
                        .find(|i| Some(i.guid) == self.iface)
                        .map(|i| i.description.clone())
                        .unwrap_or_else(|| t!("Kein Adapter", "No adapter").into());
                    let mut pick = None;
                    egui::ComboBox::from_id_salt("iface")
                        .width(250.0)
                        .selected_text(RichText::new(format!("📡  {cur}")).color(p.text))
                        .show_ui(ui, |ui| {
                            for i in &self.interfaces {
                                if ui.selectable_label(Some(i.guid) == self.iface, format!("{} ({})", i.description, i.state)).clicked() {
                                    pick = Some(i.guid);
                                }
                            }
                        })
                        .response
                        .on_hover_text(t!("WLAN-Adapter für den Scan", "Wi-Fi adapter used for scanning"));
                    if let Some(g) = pick {
                        self.select_iface(g);
                    }

                    ui.add_space(6.0);
                    let (txt, hint) = if self.paused {
                        (t!("▶ Fortsetzen", "▶ Resume"), t!("Scannen fortsetzen", "Resume scanning"))
                    } else {
                        (t!("⏸ Pause", "⏸ Pause"), t!("Scannen anhalten (Verbindung wird weiter gemessen)", "Pause scanning (connection is still measured)"))
                    };
                    if ui.button(txt).on_hover_text(hint).clicked() {
                        self.set_paused(!self.paused);
                    }
                    if ui.button(t!("⟳ Scannen", "⟳ Scan")).on_hover_text(t!("Sofort scannen (F5)", "Scan now (F5)")).clicked() {
                        self.scan_now();
                    }
                    let mut interval = self.settings.interval;
                    egui::ComboBox::from_id_salt("interval")
                        .width(110.0)
                        .selected_text(tf!("Alle {} s", "Every {} s", interval as u32))
                        .show_ui(ui, |ui| {
                            for s in [2.0f32, 3.0, 4.0, 5.0, 10.0, 15.0, 30.0, 60.0] {
                                ui.selectable_value(&mut interval, s, tf!("Alle {} s", "Every {} s", s as u32));
                            }
                        })
                        .response
                        .on_hover_text(t!("Scan-Intervall", "Scan interval"));
                    if interval != self.settings.interval {
                        self.settings.interval = interval;
                        let _ = self.scanner.tx.send(crate::wlan::ScanCmd::SetInterval(interval));
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("ℹ").on_hover_text(t!("Über WiFi Analyzer", "About WiFi Analyzer")).clicked() {
                            self.show_about = true;
                        }
                        if ui.button("⚙").on_hover_text(t!("Einstellungen", "Settings")).clicked() {
                            self.show_settings = true;
                        }
                        let theme_icon = if self.settings.dark { "☀" } else { "🌙" };
                        if ui.button(theme_icon).on_hover_text(t!("Hell / Dunkel", "Light / dark")).clicked() {
                            self.settings.dark = !self.settings.dark;
                            crate::theme::apply(ctx, self.settings.dark);
                        }
                        let has_meas = self.meas.is_some();
                        ui.menu_button(t!("⬇ Export", "⬇ Export"), |ui| {
                            if ui.button(t!("Netzwerkliste als CSV …  (Strg+E)", "Network list as CSV …  (Ctrl+E)")).clicked() {
                                ui.close_menu();
                                self.export_networks();
                            }
                            if ui.add_enabled(has_meas, egui::Button::new(t!("Messung als CSV …", "Survey as CSV …"))).clicked() {
                                ui.close_menu();
                                self.export_measurement();
                            }
                        });
                    });
                });
            });
    }

    pub(super) fn sidebar(&mut self, ctx: &egui::Context) {
        let p = pal_ctx(ctx);
        let findings = self.findings();
        let crit = findings.iter().filter(|f| f.sev == Sev::Critical).count();
        let warn = findings.iter().filter(|f| f.sev == Sev::Warning).count();
        let ssids = self.ssid_choices().len();
        let running = self.meas.as_ref().is_some_and(|m| m.running);

        egui::SidePanel::left("nav")
            .exact_width(228.0)
            .resizable(false)
            .frame(egui::Frame::none().fill(p.surface).inner_margin(Margin::symmetric(10.0, 6.0)).stroke(Stroke::new(1.0f32, p.border)))
            .show(ctx, |ui| {
                // Verbindung unten fixieren, Rest scrollbar darüber
                egui::TopBottomPanel::bottom("conn_card")
                    .frame(egui::Frame::none().inner_margin(Margin::symmetric(0.0, 8.0)))
                    .show_inside(ui, |ui| self.connection_card(ui));

                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    section_label(ui, t!("ANALYSE", "ANALYSIS"));
                    for page in [Page::Overview, Page::Networks, Page::Channels, Page::Rating, Page::History, Page::Findings] {
                        let badge = match page {
                            Page::Networks if ssids > 0 => Some((ssids.to_string(), p.weak)),
                            Page::Findings if crit > 0 => Some((crit.to_string(), p.danger)),
                            Page::Findings if warn > 0 => Some((warn.to_string(), p.warning)),
                            _ => None,
                        };
                        let hint = self.nav_hint(page);
                        if nav_item(ui, page.icon(), page.title(), self.page == page, badge, &hint).clicked() {
                            self.page = page;
                        }
                    }
                    section_label(ui, t!("WERKZEUGE", "TOOLS"));
                    for page in [Page::Measure, Page::Finder, Page::Events] {
                        let badge = match page {
                            Page::Measure if running => Some(("REC".to_string(), p.danger)),
                            _ => None,
                        };
                        let hint = self.nav_hint(page);
                        if nav_item(ui, page.icon(), page.title(), self.page == page, badge, &hint).clicked() {
                            self.page = page;
                        }
                    }

                    section_label(ui, t!("FILTER", "FILTER"));
                    self.filter_panel(ui);
                });
            });
    }

    fn nav_hint(&self, page: Page) -> String {
        let n = super::shell::NAV_ORDER.iter().position(|p| *p == page).map(|i| i + 1).unwrap_or(0);
        let what = match page {
            Page::Overview => t!("Kennzahlen, Verbindung und Empfehlungen auf einen Blick", "Key figures, connection and recommendations at a glance"),
            Page::Networks => t!("Alle Netze und Access Points mit Details", "All networks and access points with details"),
            Page::Channels => t!("Spektrum: welche Netze welche Kanäle belegen", "Spectrum: which networks occupy which channels"),
            Page::Rating => t!("Störpegel je Kanal und Kanalempfehlung", "Interference per channel and channel recommendation"),
            Page::History => t!("Signalstärke über die Zeit", "Signal strength over time"),
            Page::Findings => t!("Sicherheits- und Konfigurationsprobleme mit Empfehlungen", "Security and configuration issues with recommendations"),
            Page::Measure => t!("Walk-Survey mit Roaming-Erkennung", "Walk survey with roaming detection"),
            Page::Finder => t!("Access Point über die Signalstärke orten", "Locate an access point by signal strength"),
            Page::Events => t!("Protokoll: neue Netze, Roaming, Fehler", "Log: new networks, roaming, errors"),
        };
        tf!("{what}  (Strg+{n})", "{what}  (Ctrl+{n})")
    }

    /// Hinweis-Banner oben auf jeder Seite, wenn der Scan nicht funktioniert – mit passender Aktion
    pub(super) fn problem_banner(&mut self, ui: &mut Ui) {
        let Some(err) = self.error.clone() else { return };
        let p = pal(ui);
        let code = self.error_code;
        let mut grant = false;
        egui::Frame::none()
            .fill(p.danger.gamma_multiply(0.12))
            .stroke(Stroke::new(1.0f32, p.danger.gamma_multiply(0.6)))
            .rounding(8.0)
            .inner_margin(Margin::symmetric(14.0, 10.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new("⚠").size(20.0).color(p.danger));
                    ui.vertical(|ui| {
                        ui.label(RichText::new(t!("Scan nicht möglich", "Scanning not possible")).strong().color(p.text));
                        ui.label(RichText::new(&err).color(p.text));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button(t!("⟳ Erneut versuchen", "⟳ Retry")).clicked() {
                            self.scan_now();
                        }
                        match code {
                            crate::wlan::ERROR_RADIO_OFF => {
                                if ui.button(t!("WLAN-Einstellungen öffnen", "Open Wi-Fi settings")).clicked() {
                                    let _ = std::process::Command::new("explorer").arg("ms-settings:network-wifi").spawn();
                                }
                            }
                            crate::wlan::ERROR_ACCESS_DENIED => {
                                if ui.button(t!("Berechtigung erteilen …", "Grant permission …")).clicked() {
                                    grant = true;
                                }
                            }
                            _ => {}
                        }
                    });
                });
            });
        ui.add_space(10.0);
        if grant {
            self.perm_dismissed = false;
        }
    }

    fn filter_panel(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        let search = ui.add(
            egui::TextEdit::singleline(&mut self.f.text)
                .hint_text(t!("🔍 Suchen (Strg+F)", "🔍 Search (Ctrl+F)"))
                .desired_width(f32::INFINITY),
        );
        if self.focus_search {
            search.request_focus();
            self.focus_search = false;
        }
        search.on_hover_text(t!("SSID, BSSID, Hersteller, Sicherheit, AP-Name oder Kanalnummer", "SSID, BSSID, vendor, security, AP name or channel number"));
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.toggle_value(&mut self.f.b24, t!("2,4 GHz", "2.4 GHz"));
            ui.toggle_value(&mut self.f.b5, "5 GHz");
            ui.toggle_value(&mut self.f.b6, "6 GHz");
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new(t!("Mindestsignal", "Min. signal")).color(p.weak));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(if self.f.min_rssi <= -100 { t!("alle", "all").to_string() } else { format!("{} dBm", self.f.min_rssi) }).color(p.text));
            });
        });
        ui.spacing_mut().slider_width = ui.available_width() - 8.0;
        ui.add(egui::Slider::new(&mut self.f.min_rssi, -100..=-30).show_value(false))
            .on_hover_text(t!("Schwächere Netze ausblenden", "Hide weaker networks"));
        ui.checkbox(&mut self.f.show_hidden, t!("Versteckte SSIDs", "Hidden SSIDs"));
        ui.checkbox(&mut self.f.only_pinned, t!("Nur markierte ★", "Pinned only ★"));
        ui.checkbox(&mut self.f.only_open, t!("Nur offene Netze", "Open networks only"));
        ui.checkbox(&mut self.f.hide_stale, t!("Veraltete ausblenden", "Hide stale"));
        let n = self.f.active_count();
        if n > 0 && ui.button(tf!("✖ {n} Filter zurücksetzen", "✖ Reset {n} filters")).clicked() {
            self.f = super::Filter::with_hidden(true);
        }
    }

    fn connection_card(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        let mut select = None;
        egui::Frame::none()
            .fill(p.surface2)
            .stroke(Stroke::new(1.0f32, p.border))
            .rounding(8.0)
            .inner_margin(Margin::same(10.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                match &self.conn {
                    Some(c) => {
                        ui.horizontal(|ui| {
                            dot(ui, p.success, 4.0);
                            ui.label(RichText::new(t!("Verbunden", "Connected")).size(12.0).color(p.success));
                        });
                        ui.label(RichText::new(&c.ssid).strong().color(p.text));
                        ui.horizontal(|ui| {
                            if let Some(r) = c.rssi {
                                ui.label(RichText::new(format!("{r} dBm")).strong().color(signal_color(r)));
                            }
                            if let Some(ch) = c.channel {
                                ui.label(RichText::new(tf!("Kanal {ch}", "Ch {ch}")).color(p.weak));
                            }
                        });
                        ui.label(RichText::new(format!("{:.0} Mbit/s · {}", c.tx_mbps, c.phy)).size(12.0).color(p.weak));
                        let vendor = self.aps.get(&c.bssid).map(Self::vendor_label).unwrap_or_default();
                        let r = ui
                            .add(egui::Label::new(RichText::new(format!("{} {vendor}", fmt_mac(&c.bssid))).size(11.5).monospace().color(p.weak)).sense(Sense::click()))
                            .on_hover_text(t!("Access Point anzeigen", "Show access point"));
                        if r.clicked() {
                            select = Some(c.bssid);
                        }
                    }
                    None => {
                        ui.horizontal(|ui| {
                            dot(ui, p.weak, 4.0);
                            ui.label(RichText::new(t!("Nicht verbunden", "Not connected")).color(p.weak));
                        });
                    }
                }
            });
        if let Some(b) = select {
            self.selected = Some(b);
            if !matches!(self.page, Page::Networks | Page::Channels | Page::History | Page::Overview) {
                self.page = Page::Networks;
            }
        }
    }

    pub(super) fn status_bar(&mut self, ctx: &egui::Context) {
        let p = pal_ctx(ctx);
        egui::TopBottomPanel::bottom("status")
            .exact_height(28.0)
            .frame(egui::Frame::none().fill(p.surface).inner_margin(Margin::symmetric(14.0, 4.0)).stroke(Stroke::new(1.0f32, p.border)))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    let (txt, col) = if self.error.is_some() {
                        (t!("Fehler", "Error"), p.danger)
                    } else if self.paused {
                        (t!("Pausiert", "Paused"), p.warning)
                    } else {
                        ("Live", p.success)
                    };
                    dot(ui, col, 4.0);
                    ui.label(RichText::new(txt).size(12.5).strong().color(col));
                    let scan = match self.last_scan {
                        Some(t) => tf!(
                            "Scan #{} · {} · {} Access Points",
                            "Scan #{} · {} · {} access points",
                            self.scan_count,
                            fmt_ago(t.elapsed().as_secs_f32()),
                            self.aps.len()
                        ),
                        None => t!("Warte auf ersten Scan …", "Waiting for first scan …").into(),
                    };
                    ui.label(RichText::new(scan).size(12.5).color(p.weak));
                    if let Some(v) = self.update.latest.clone().filter(|v| super::dialogs::is_newer(v, VERSION)) {
                        ui.separator();
                        ui.hyperlink_to(
                            RichText::new(tf!("⬆ Version {v} verfügbar", "⬆ Version {v} available")).size(12.5).color(p.accent),
                            format!("{}/releases/latest", super::REPO),
                        );
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.hyperlink_to(RichText::new("wifianalyzer.schoepf-tirol.at").size(12.0).color(p.weak), WEBSITE);
                        ui.label(RichText::new(format!("© {} {AUTHOR} · GPL-3.0 ·", chrono::Local::now().format("%Y"))).size(12.0).color(p.weak));
                    });
                });
            });
    }

    pub(super) fn toasts(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        self.toasts.retain(|t| t.until > now);
        if self.toasts.is_empty() {
            return;
        }
        let p = pal_ctx(ctx);
        egui::Area::new(egui::Id::new("toasts"))
            .anchor(Align2::RIGHT_BOTTOM, [-16.0, -40.0])
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                for t in &self.toasts {
                    let col = match t.sev {
                        Sev::Critical => p.danger,
                        Sev::Warning => p.warning,
                        Sev::Info => p.accent,
                    };
                    egui::Frame::none()
                        .fill(p.surface)
                        .stroke(Stroke::new(1.0f32, col.gamma_multiply(0.7)))
                        .rounding(8.0)
                        .shadow(egui::Shadow { offset: Vec2::new(0.0, 4.0), blur: 14.0, spread: 0.0, color: Color32::from_black_alpha(60) })
                        .inner_margin(Margin::symmetric(14.0, 10.0))
                        .show(ui, |ui| {
                            ui.set_max_width(420.0);
                            ui.horizontal(|ui| {
                                dot(ui, col, 4.0);
                                ui.label(RichText::new(&t.text).color(p.text));
                            });
                        });
                    ui.add_space(6.0);
                }
            });
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
    }
}
