//! Rahmen des Hauptfensters: Kopfleiste, Navigation, Statusleiste, Meldungen.

use std::time::Instant;

use eframe::egui::{self, Align, Align2, Color32, FontId, Layout, Margin, Response, RichText, Sense, Stroke, Ui, Vec2};

use super::{Page, Sev, WifiApp, AUTHOR, VERSION, WEBSITE};
use crate::model::{fmt_mac, signal_color};
use crate::theme::{pal, pal_ctx};
use crate::widgets::dot;
use crate::{t, tf};

/// Reihenfolge der Seiten (auch für Strg+1 … Strg+9)
pub const NAV_ORDER: [Page; 10] = [
    Page::Overview,
    Page::Networks,
    Page::Channels,
    Page::Rating,
    Page::History,
    Page::Findings,
    Page::Measure,
    Page::PlanSurvey,
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
            Page::PlanSurvey => t!("Grundriss-Messung", "Floor plan survey"),
            Page::Finder => t!("AP-Finder", "AP finder"),
            Page::Events => t!("Ereignisse", "Events"),
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Page::Overview => "\u{E2C2}",
            Page::Networks => "\u{E2F2}",
            Page::Channels => "\u{E150}",
            Page::Rating => "\u{E67E}",
            Page::History => "\u{E154}",
            Page::Findings => "\u{E4E0}",
            Page::Measure => "\u{E6B8}",
            Page::PlanSurvey => "\u{E31A}",
            Page::Finder => "\u{E1D6}",
            Page::Events => "\u{E1A0}",
        }
    }
}

/// Navigationseintrag: Symbol, Text, optionales Zähler-Badge
fn nav_item(ui: &mut Ui, icon: &str, label: &str, selected: bool, badge: Option<(String, Color32)>, hint: &str, collapsed: bool) -> Response {
    let p = pal(ui);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 32.0), Sense::click());
    if collapsed {
        // nur Symbol, Zähler als Punkt; der Name steht im Tooltip
        let r = rect.shrink2(Vec2::new(0.0, 1.0));
        let bg = if selected { p.accent_soft } else if resp.hovered() { p.surface2 } else { Color32::TRANSPARENT };
        ui.painter().rect_filled(r, 7.0, bg);
        ui.painter().text(r.center(), Align2::CENTER_CENTER, icon, FontId::proportional(17.0), if selected { p.accent } else { p.weak });
        if let Some((_, col)) = badge {
            ui.painter().circle_filled(r.right_top() + Vec2::new(-7.0, 8.0), 3.5, col);
        }
        return resp.on_hover_text(format!("{label}\n{hint}")).on_hover_cursor(egui::CursorIcon::PointingHand);
    }
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
        // Markierung im Verlauf des Logos (Cyan oben, Violett unten)
        let bar = egui::Rect::from_min_size(r.left_top() + Vec2::new(0.0, 7.0), Vec2::new(3.0, r.height() - 14.0));
        let g = crate::theme::GRAD_LINE;
        let mut mesh = egui::Mesh::default();
        mesh.colored_vertex(bar.left_top(), g[0]);
        mesh.colored_vertex(bar.right_top(), g[0]);
        mesh.colored_vertex(bar.right_bottom(), g[1]);
        mesh.colored_vertex(bar.left_bottom(), g[1]);
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        painter.add(egui::Shape::mesh(mesh));
    }
    let fg = if selected { p.text } else { p.text.gamma_multiply(0.88) };
    painter.text(r.left_center() + Vec2::new(14.0, 0.0), Align2::LEFT_CENTER, icon, FontId::proportional(15.0), if selected { p.accent } else { p.weak });
    let font = if selected { FontId::new(14.0, crate::theme::semibold()) } else { FontId::proportional(14.0) };
    painter.text(r.left_center() + Vec2::new(40.0, 0.0), Align2::LEFT_CENTER, label, font, fg);
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
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        ui.label(RichText::new(text).family(crate::theme::semibold()).size(10.5).color(p.weak.gamma_multiply(0.85)));
    });
    ui.add_space(2.0);
}

/// Kleines, flaches Abzeichen (fest 18 px hoch – unabhängig vom Layout)
fn pill(ui: &mut Ui, text: &str, col: Color32) {
    let galley = ui.painter().layout_no_wrap(text.to_string(), FontId::new(10.5, crate::theme::semibold()), col);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(galley.size().x + 14.0, 18.0), Sense::hover());
    ui.painter().rect(rect, 9.0, col.gamma_multiply(0.16), Stroke::new(1.0f32, col.gamma_multiply(0.55)));
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, col);
}

impl WifiApp {
    /// Fenster mit allen Tastenkürzeln (F1)
    pub(super) fn shortcuts_window(&mut self, ctx: &egui::Context) {
        if !self.show_shortcuts {
            return;
        }
        let mut open = true;
        egui::Window::new(RichText::new(t!("\u{E2D8} Tastenkürzel", "\u{E2D8} Keyboard shortcuts")).strong())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                let p = pal(ui);
                let groups: [(&str, Vec<(&str, &str)>); 3] = [
                    (
                        t!("Allgemein", "General"),
                        vec![
                            ("Strg+1 … 9", t!("Seite wechseln (Reihenfolge der Seitenleiste)", "Switch page (sidebar order)")),
                            ("F5", t!("Sofort scannen", "Scan now")),
                            (t!("Leertaste", "Space"), t!("Scan anhalten / fortsetzen", "Pause / resume scanning")),
                            ("Strg+F", t!("Suchen (Netzwerkliste)", "Search (network list)")),
                            ("Strg+E", t!("Exportieren (CSV)", "Export (CSV)")),
                            ("Esc", t!("Fenster / Auswahl schließen", "Close window / selection")),
                            ("Strg+B", t!("Seitenleiste ein-/ausklappen", "Collapse / expand the sidebar")),
                            ("Strg + / −", t!("Anzeige größer / kleiner", "Larger / smaller display")),
                            ("F1", t!("Diese Übersicht", "This overview")),
                        ],
                    ),
                    (
                        t!("Messung", "Survey"),
                        vec![
                            (t!("Enter oder M", "Enter or M"), t!("Messpunkt erfassen (laufende Messung)", "Add a point (running survey)")),
                            ("Strg+Z", t!("Letzte Änderung an den Messpunkten zurücknehmen", "Undo the last change to the points")),
                        ],
                    ),
                    (
                        t!("Grundriss", "Floor plan"),
                        vec![
                            (t!("Mausrad", "Mouse wheel"), t!("Zoomen", "Zoom")),
                            (t!("Ziehen", "Drag"), t!("Verschieben (freie Fläche, rechte oder mittlere Maustaste)", "Pan (empty space, right or middle button)")),
                            ("0", t!("Plan einpassen", "Fit plan")),
                            ("F11", t!("Vollbild für die Begehung", "Full screen for the walk-through")),
                            (t!("Rechtsklick Stockwerk", "Right-click floor"), t!("Umbenennen / entfernen", "Rename / remove")),
                        ],
                    ),
                ];
                for (title, rows) in groups {
                    ui.label(RichText::new(title).strong().color(p.text));
                    egui::Grid::new(("keys", title)).num_columns(2).min_col_width(170.0).spacing([18.0, 4.0]).show(ui, |ui| {
                        for (k, what) in rows {
                            ui.label(RichText::new(k).monospace().color(p.accent));
                            ui.label(RichText::new(what).color(p.text));
                            ui.end_row();
                        }
                    });
                    ui.add_space(8.0);
                }
            });
        if !open {
            self.show_shortcuts = false;
        }
    }

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
                    ui.label(RichText::new("WiFi Analyzer").family(crate::theme::semibold()).size(17.0).color(p.text));
                    ui.label(RichText::new(format!("v{VERSION}")).size(11.5).color(p.weak));
                    if crate::demo::enabled() {
                        pill(ui, "DEMO", p.purple);
                    }
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
                        .selected_text(RichText::new(format!("\u{E4EA}  {cur}")).color(p.text))
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
                        (t!("\u{E3D0} Fortsetzen", "\u{E3D0} Resume"), t!("Scannen fortsetzen", "Resume scanning"))
                    } else {
                        (t!("\u{E39E} Pause", "\u{E39E} Pause"), t!("Scannen anhalten (Verbindung wird weiter gemessen)", "Pause scanning (connection is still measured)"))
                    };
                    if ui.button(txt).on_hover_text(hint).clicked() {
                        self.set_paused(!self.paused);
                    }
                    if ui.button(t!("\u{E094} Scannen", "\u{E094} Scan")).on_hover_text(t!("Sofort scannen (F5)", "Scan now (F5)")).clicked() {
                        self.scan_now();
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        // Seltenes im ⋯-Menü: Design, Updates, Tastenkürzel, Über
                        let mut check_updates = false;
                        ui.menu_button("\u{E1FE}", |ui| {
                            ui.set_min_width(240.0);
                            let (theme_icon, theme_txt) = if self.settings.dark { ("\u{E472}", t!("Helles Design", "Light theme")) } else { ("\u{E330}", t!("Dunkles Design", "Dark theme")) };
                            if ui.button(format!("{theme_icon}  {theme_txt}")).clicked() {
                                self.settings.dark = !self.settings.dark;
                                crate::theme::apply(ctx, self.settings.dark);
                                ui.close_menu();
                            }
                            if self.update.rx.is_some() {
                                ui.add_enabled(false, egui::Button::new(t!("\u{E094}  Prüfe auf Updates …", "\u{E094}  Checking for updates …")));
                            } else {
                                let label = match (&self.update.latest, &self.update.error) {
                                    (Some(_), _) => tf!("\u{E182}  v{VERSION} ist aktuell – erneut prüfen", "\u{E182}  v{VERSION} is up to date – check again"),
                                    (None, Some(_)) => t!("\u{E094}  Updates prüfen (letzte Prüfung fehlgeschlagen)", "\u{E094}  Check for updates (last check failed)").into(),
                                    (None, None) => t!("\u{E094}  Nach Updates suchen", "\u{E094}  Check for updates").into(),
                                };
                                if ui.button(label).clicked() {
                                    check_updates = true;
                                    ui.close_menu();
                                }
                            }
                            if ui.button(t!("\u{E2D8}  Tastenkürzel  (F1)", "\u{E2D8}  Keyboard shortcuts  (F1)")).clicked() {
                                self.show_shortcuts = true;
                                ui.close_menu();
                            }
                            ui.separator();
                            if ui.button(t!("\u{E2CE}  Über WiFi Analyzer", "\u{E2CE}  About WiFi Analyzer")).clicked() {
                                self.show_about = true;
                                ui.close_menu();
                            }
                        })
                        .response
                        .on_hover_text(t!("Design, Updates, Tastenkürzel, Über", "Theme, updates, keyboard shortcuts, about"));
                        if check_updates {
                            self.start_update_check(ctx, true);
                        }
                        if ui.button(t!("\u{E270} Einstellungen", "\u{E270} Settings")).clicked() {
                            self.show_settings = true;
                        }
                        // nur bei verfügbarem Update sichtbar
                        if let Some(v) = self.update.available().map(|r| r.version.clone()) {
                            let b = egui::Button::new(RichText::new(tf!("\u{E030} Update auf {v}", "\u{E030} Update to {v}")).color(Color32::WHITE).strong()).fill(p.accent);
                            if ui.add(b).on_hover_text(t!("Neue Version verfügbar – Details und Installation", "New version available – details and installation")).clicked() {
                                self.show_update = true;
                            }
                        }
                        let has_meas = self.meas.is_some();
                        ui.menu_button(t!("\u{E20C} Export", "\u{E20C} Export"), |ui| {
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

        let collapsed = self.settings.nav_collapsed;
        // weich zwischen schmal (nur Symbole) und breit wechseln
        let w = 60.0 + 168.0 * ctx.animate_bool_with_time(egui::Id::new("nav_open"), !collapsed, 0.16);
        egui::SidePanel::left("nav")
            .exact_width(w)
            .resizable(false)
            .frame(egui::Frame::none().fill(p.surface).inner_margin(Margin::symmetric(10.0, 6.0)).stroke(Stroke::new(1.0f32, p.border)))
            .show(ctx, |ui| {
                // Verbindung unten fixieren, Rest scrollbar darüber
                egui::TopBottomPanel::bottom("conn_card")
                    .frame(egui::Frame::none().inner_margin(Margin::symmetric(0.0, 8.0)))
                    .show_inside(ui, |ui| {
                        if collapsed {
                            self.connection_mini(ui);
                        } else {
                            self.connection_card(ui);
                        }
                        ui.add_space(4.0);
                        let (icon, tip) = if collapsed {
                            ("\u{E13A}", t!("Seitenleiste ausklappen (Strg+B)", "Expand sidebar (Ctrl+B)"))
                        } else {
                            ("\u{E138}", t!("Seitenleiste einklappen – mehr Platz für Plan und Tabellen (Strg+B)", "Collapse sidebar – more room for plan and tables (Ctrl+B)"))
                        };
                        let txt = if collapsed { icon.to_string() } else { format!("{icon}  {}", t!("Einklappen", "Collapse")) };
                        let b = egui::Button::new(RichText::new(txt).size(12.5).color(p.weak)).frame(false).min_size(Vec2::new(ui.available_width(), 24.0));
                        if ui.add(b).on_hover_text(tip).clicked() {
                            self.settings.nav_collapsed = !collapsed;
                        }
                    });

                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    if collapsed {
                        ui.add_space(8.0);
                    } else {
                        section_label(ui, t!("ANALYSE", "ANALYSIS"));
                    }
                    for page in [Page::Overview, Page::Networks, Page::Channels, Page::Rating, Page::History, Page::Findings] {
                        let badge = match page {
                            Page::Networks if ssids > 0 => Some((ssids.to_string(), p.weak)),
                            Page::Findings if crit > 0 => Some((crit.to_string(), p.danger)),
                            Page::Findings if warn > 0 => Some((warn.to_string(), p.warning)),
                            _ => None,
                        };
                        let hint = self.nav_hint(page);
                        if nav_item(ui, page.icon(), page.title(), self.page == page, badge, &hint, collapsed).clicked() {
                            self.page = page;
                        }
                    }
                    if collapsed {
                        ui.add_space(4.0);
                        ui.separator();
                        ui.add_space(4.0);
                    } else {
                        section_label(ui, t!("WERKZEUGE", "TOOLS"));
                    }
                    for page in [Page::Measure, Page::PlanSurvey, Page::Finder, Page::Events] {
                        let plan_running = self.meas.as_ref().is_some_and(|m| m.plan_mode);
                        let badge = match page {
                            Page::Measure if running && !plan_running => Some(("REC".to_string(), p.danger)),
                            Page::PlanSurvey if running && plan_running => Some(("REC".to_string(), p.danger)),
                            _ => None,
                        };
                        let hint = self.nav_hint(page);
                        if nav_item(ui, page.icon(), page.title(), self.page == page, badge, &hint, collapsed).clicked() {
                            self.page = page;
                        }
                    }

                    // Filter nur dort, wo sie wirken (Analyse-Seiten)
                    if !collapsed && matches!(self.page, Page::Overview | Page::Networks | Page::Channels | Page::Rating | Page::History | Page::Findings) {
                        section_label(ui, t!("FILTER", "FILTER"));
                        self.filter_panel(ui);
                    }
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
            Page::Measure => t!("Rundgang: Verlauf, Roaming und Messpunkte pro Raum", "Walk survey: timeline, roaming and points per room"),
            Page::PlanSurvey => t!("Grundriss laden, in den Plan klicken – Heatmap der Ausleuchtung", "Load a floor plan, click into it – coverage heatmap"),
            Page::Finder => t!("Access Point über die Signalstärke orten", "Locate an access point by signal strength"),
            Page::Events => t!("Protokoll: neue Netze, Roaming, Fehler", "Log: new networks, roaming, errors"),
        };
        if n <= 9 {
            tf!("{what}  (Strg+{n})", "{what}  (Ctrl+{n})")
        } else {
            what.to_string()
        }
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
                    ui.label(RichText::new("\u{E4E0}").size(20.0).color(p.danger));
                    ui.vertical(|ui| {
                        ui.label(RichText::new(t!("Scan nicht möglich", "Scanning not possible")).strong().color(p.text));
                        ui.label(RichText::new(&err).color(p.text));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button(t!("\u{E094} Erneut versuchen", "\u{E094} Retry")).clicked() {
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
                .hint_text(t!("\u{E30C} Suchen (Strg+F)", "\u{E30C} Search (Ctrl+F)"))
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
        // seltener gebrauchte Filter eingeklappt, damit die Navigation Platz hat
        egui::CollapsingHeader::new(RichText::new(t!("Weitere Filter", "More filters")).color(p.weak))
            .id_salt("more_filters")
            .default_open(false)
            .show(ui, |ui| {
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
            });
        let n = self.f.active_count();
        if n > 0 && ui.button(tf!("\u{E4F6} {n} Filter zurücksetzen", "\u{E4F6} Reset {n} filters")).clicked() {
            self.f = super::Filter::with_hidden(true);
        }
    }

    /// Verbindung in der eingeklappten Seitenleiste: nur Signalwert mit Farbe
    fn connection_mini(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 40.0), Sense::hover());
        ui.painter().rect(rect, 8.0, p.surface2, Stroke::new(1.0f32, p.border));
        match &self.conn {
            Some(c) => {
                let col = c.rssi.map(signal_color).unwrap_or(p.weak);
                let txt = c.rssi.map(|r| r.to_string()).unwrap_or("–".into());
                ui.painter().text(rect.center() - Vec2::new(0.0, 5.0), Align2::CENTER_CENTER, txt, FontId::new(15.0, crate::theme::semibold()), col);
                ui.painter().text(rect.center() + Vec2::new(0.0, 10.0), Align2::CENTER_CENTER, "dBm", FontId::proportional(9.5), p.weak);
                resp.on_hover_text(tf!("Verbunden mit {}\n{}", "Connected to {}\n{}", c.ssid, fmt_mac(&c.bssid)));
            }
            None => {
                ui.painter().text(rect.center(), Align2::CENTER_CENTER, "\u{E4EA}", FontId::proportional(16.0), p.weak);
                resp.on_hover_text(t!("Nicht verbunden", "Not connected"));
            }
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
                        // kompakt: SSID, dann Signal · Kanal · Rate, dann AP
                        ui.horizontal(|ui| {
                            dot(ui, p.success, 4.0);
                            ui.add(egui::Label::new(RichText::new(&c.ssid).family(crate::theme::semibold()).color(p.text)).truncate())
                                .on_hover_text(t!("Verbunden", "Connected"));
                        });
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            if let Some(r) = c.rssi {
                                ui.label(RichText::new(format!("{r} dBm")).family(crate::theme::semibold()).color(signal_color(r)));
                            }
                            let mut rest = Vec::new();
                            if let Some(ch) = c.channel {
                                rest.push(tf!("Kanal {ch}", "Ch {ch}"));
                            }
                            rest.push(format!("{:.0} Mbit/s", c.tx_mbps));
                            ui.add(egui::Label::new(RichText::new(rest.join(" · ")).size(12.0).color(p.weak)).truncate());
                        });
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
                    let status = if self.last_scan.is_some() {
                        tf!("{} Access Points in Reichweite", "{} access points in range", self.aps.len())
                    } else {
                        t!("Suche WLAN-Netze …", "Searching for Wi-Fi networks …").into()
                    };
                    ui.label(RichText::new(status).size(12.5).color(p.weak));
                    if self.meas.as_ref().is_some_and(|m| m.running) {
                        ui.separator();
                        dot(ui, p.danger, 4.0);
                        ui.label(RichText::new(t!("Messung läuft", "Survey running")).size(12.5).color(p.danger));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.hyperlink_to(RichText::new("schoepf-tirol.at/Project/WifiAnalyzer").size(12.0).color(p.weak), WEBSITE);
                        ui.label(RichText::new(format!("© {} {AUTHOR} · GPL-3.0 ·", chrono::Local::now().format("%Y"))).size(12.0).color(p.weak));
                    });
                });
            });
    }

    /// Hintergrund abdunkeln, solange ein Dialog offen ist (liegt zwischen Seite und Fenstern)
    pub(super) fn dim_behind_dialogs(&self, ctx: &egui::Context) {
        let open = self.show_settings || self.show_about || self.show_update || self.show_whats_new || self.show_shortcuts || self.meas_point.is_some() || self.pdf_pick.is_some();
        let a = ctx.animate_bool_with_time(egui::Id::new("dialog_dim"), open, 0.15);
        if a > 0.0 {
            let dark = ctx.style().visuals.dark_mode;
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::PanelResizeLine, egui::Id::new("dialog_dim_layer")));
            painter.rect_filled(ctx.screen_rect(), 0.0, Color32::from_black_alpha((a * if dark { 120.0 } else { 70.0 }) as u8));
        }
    }

    pub(super) fn toasts(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        self.toasts.retain(|t| t.until > now);
        if self.toasts.is_empty() {
            return;
        }
        let p = pal_ctx(ctx);
        egui::Area::new(egui::Id::new("toasts"))
            // oben mittig: verdeckt weder Plan-Legende noch Listen
            .anchor(Align2::CENTER_TOP, [0.0, 64.0])
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
                        .rounding(10.0)
                        .shadow(egui::Shadow { offset: Vec2::new(0.0, 6.0), blur: 20.0, spread: 0.0, color: Color32::from_black_alpha(80) })
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
