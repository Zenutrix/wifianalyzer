//! Einstellungen und „Über“-Dialog.

use eframe::egui::{self, Align, Align2, Color32, FontId, Layout, Margin, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};

use super::{WifiApp, AUTHOR, REPO, VERSION, WEBSITE};
use crate::i18n::{self, Lang};
use crate::settings::{self, Settings};
use crate::theme::pal;
use crate::widgets::{primary_button, segmented};
use crate::wlan::ScanCmd;
use crate::{t, tf};

/// Bereich im Einstellungsfenster
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SettingsTab {
    Look,
    Scan,
    Survey,
    Data,
    Program,
}

impl SettingsTab {
    const ALL: [SettingsTab; 5] = [SettingsTab::Look, SettingsTab::Scan, SettingsTab::Survey, SettingsTab::Data, SettingsTab::Program];

    fn icon(self) -> &'static str {
        match self {
            SettingsTab::Look => "\u{E6C8}",
            SettingsTab::Scan => "\u{E4EA}",
            SettingsTab::Survey => "\u{E316}",
            SettingsTab::Data => "\u{E256}",
            SettingsTab::Program => "\u{E2CE}",
        }
    }

    fn title(self) -> &'static str {
        match self {
            SettingsTab::Look => t!("Darstellung", "Appearance"),
            SettingsTab::Scan => "Scan",
            SettingsTab::Survey => t!("Messung", "Survey"),
            SettingsTab::Data => t!("Daten", "Data"),
            SettingsTab::Program => t!("Programm", "Program"),
        }
    }

    fn subtitle(self) -> &'static str {
        match self {
            SettingsTab::Look => t!("Sprache, Farbschema und Größe der Oberfläche", "Language, theme and size of the interface"),
            SettingsTab::Scan => t!("Wie oft gesucht wird und wie lange Access Points sichtbar bleiben", "How often to scan and how long access points stay visible"),
            SettingsTab::Survey => t!("Was an jedem Messpunkt zusätzlich gemessen wird und wo Messungen liegen", "What is also measured at each point and where surveys are stored"),
            SettingsTab::Data => t!("Hersteller-Datenbank, Netzwerkliste und Markierungen", "Vendor database, network list and pins"),
            SettingsTab::Program => t!("Version, Updates und Deinstallation", "Version, updates and uninstall"),
        }
    }
}

/// Karte mit mehreren Einstellungen (Zeilen durch feine Linien getrennt)
fn group(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui)) {
    let p = pal(ui);
    if !title.is_empty() {
        ui.label(RichText::new(title).size(12.0).strong().color(p.weak));
        ui.add_space(4.0);
    }
    egui::Frame::none()
        .fill(p.surface2)
        .stroke(Stroke::new(1.0f32, p.border))
        .rounding(10.0)
        .inner_margin(Margin::symmetric(16.0, 6.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            add(ui);
        });
    ui.add_space(16.0);
}

/// Eine Einstellung: Titel + Beschreibung links, Bedienelement rechts; `last` = ohne Trennlinie darunter
fn row(ui: &mut Ui, title: &str, desc: &str, last: bool, control: impl FnOnce(&mut Ui)) {
    let p = pal(ui);
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width((ui.available_width() - 210.0).max(180.0));
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(RichText::new(title).size(14.0).color(p.text));
            if !desc.is_empty() {
                ui.add(egui::Label::new(RichText::new(desc).size(12.0).color(p.weak)).wrap());
            }
        });
        ui.with_layout(Layout::right_to_left(Align::Center), control);
    });
    ui.add_space(10.0);
    if !last {
        let r = ui.available_rect_before_wrap();
        ui.painter().hline(r.x_range(), r.top(), Stroke::new(1.0f32, p.border));
    }
}

/// Ein-/Aus-Schalter (statt Häkchen)
fn toggle(ui: &mut Ui, on: &mut bool) -> egui::Response {
    let p = pal(ui);
    let size = Vec2::new(40.0, 22.0);
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_responsive(resp.id, *on);
    let bg = if *on { p.accent } else { p.border };
    let bg = if resp.hovered() { bg.gamma_multiply(1.15) } else { bg };
    ui.painter().rect_filled(rect, rect.height() / 2.0, bg);
    let r = rect.height() / 2.0 - 3.0;
    let x = egui::lerp((rect.left() + r + 3.0)..=(rect.right() - r - 3.0), t);
    ui.painter().circle_filled(Pos2::new(x, rect.center().y), r, Color32::WHITE);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn secs(v: f32) -> String {
    if v >= 60.0 {
        format!("{} min", v as u32 / 60)
    } else {
        format!("{} s", v as u32)
    }
}

/// Eintrag der Navigation links
fn nav_entry(ui: &mut Ui, tab: SettingsTab, selected: bool) -> egui::Response {
    let p = pal(ui);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 38.0), Sense::click());
    if selected {
        ui.painter().rect_filled(rect, 8.0, p.accent.gamma_multiply(0.16));
        let bar = Rect::from_min_size(Pos2::new(rect.left(), rect.top() + 9.0), Vec2::new(3.0, rect.height() - 18.0));
        crate::widgets::gradient_rect(ui.painter(), bar, 1.5, crate::theme::GRAD);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, 8.0, p.surface2);
    }
    let col = if selected { p.text } else { p.text.gamma_multiply(0.8) };
    ui.painter().text(Pos2::new(rect.left() + 16.0, rect.center().y), Align2::LEFT_CENTER, tab.icon(), FontId::proportional(17.0), if selected { p.accent } else { p.weak });
    let font = if selected { FontId::new(14.0, crate::theme::semibold()) } else { FontId::proportional(14.0) };
    ui.painter().text(Pos2::new(rect.left() + 44.0, rect.center().y), Align2::LEFT_CENTER, tab.title(), font, col);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

impl WifiApp {
    pub(super) fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.show_settings {
            return;
        }
        let mut open = true;
        let (mut oui, mut clear, mut unpin, mut reset, mut done) = (false, false, false, false, false);
        let (mut uninstall, mut check_update) = (false, false);
        let mut interval_changed = false;
        let screen = ctx.screen_rect();
        let w = (screen.width() - 80.0).clamp(560.0, 860.0);
        let h = (screen.height() - 180.0).clamp(380.0, 600.0);
        egui::Window::new(RichText::new(t!("Einstellungen", "Settings")).strong().size(16.0))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .fixed_size([w, h + 56.0])
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                let p = pal(ui);
                let mut tab = self.settings_tab;
                ui.horizontal_top(|ui| {
                    // Navigation
                    ui.allocate_ui_with_layout(Vec2::new(190.0, h), Layout::top_down(Align::Min), |ui| {
                        ui.set_min_size(Vec2::new(190.0, h));
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.add_space(4.0);
                        for t_ in SettingsTab::ALL {
                            if nav_entry(ui, t_, tab == t_).clicked() {
                                tab = t_;
                            }
                        }
                    });
                    // Trennlinie
                    let (r, _) = ui.allocate_exact_size(Vec2::new(1.0, h), Sense::hover());
                    ui.painter().vline(r.center().x, r.y_range(), Stroke::new(1.0f32, p.border));
                    ui.add_space(14.0);
                    // Inhalt
                    ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), h), Layout::top_down(Align::Min), |ui| {
                        ui.set_min_size(Vec2::new(ui.available_width(), h));
                        ui.label(RichText::new(tab.title()).family(crate::theme::semibold()).size(20.0).color(p.text));
                        ui.label(RichText::new(tab.subtitle()).color(p.weak));
                        ui.add_space(14.0);
                        egui::ScrollArea::vertical().id_salt(("settings_body", tab as u8)).auto_shrink([false, false]).show(ui, |ui| {
                            ui.set_width(ui.available_width() - 10.0);
                            match tab {
                                SettingsTab::Look => self.settings_look(ui),
                                SettingsTab::Scan => interval_changed = self.settings_scan(ui),
                                SettingsTab::Survey => self.settings_survey(ui),
                                SettingsTab::Data => {
                                    let (a, b, c) = self.settings_data(ui);
                                    oui = a;
                                    clear = b;
                                    unpin = c;
                                }
                                SettingsTab::Program => {
                                    let (a, b) = self.settings_program(ui);
                                    check_update = a;
                                    uninstall = b;
                                }
                            }
                        });
                    });
                });
                self.settings_tab = tab;
                // Fußzeile
                ui.add_space(8.0);
                let r = ui.available_rect_before_wrap();
                ui.painter().hline(r.x_range(), r.top(), Stroke::new(1.0f32, p.border));
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button(t!("Auf Standard zurücksetzen", "Reset to defaults")).on_hover_text(t!("Alle Einstellungen außer den Markierungen", "All settings except the pins")).clicked() {
                        reset = true;
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if primary_button(ui, t!("Fertig", "Done"), p.accent).clicked() {
                            done = true;
                        }
                        ui.label(RichText::new(t!("\u{E182} Änderungen werden sofort gespeichert", "\u{E182} Changes are saved immediately")).size(12.0).color(p.weak));
                    });
                });
            });
        if !open || done {
            self.show_settings = false;
            self.uninstall_confirm = false;
        }
        if uninstall {
            self.start_uninstall(ctx);
        }
        if check_update {
            self.start_update_check(ctx, true);
        }
        if interval_changed {
            self.apply_scan_interval();
        }
        if oui {
            self.start_oui_download(ctx);
        }
        if clear {
            self.aps.clear();
            self.selected = None;
            self.expanded.clear();
        }
        if unpin {
            self.pinned.clear();
        }
        if reset {
            let d = Settings::default();
            i18n::set(d.lang);
            crate::theme::apply(ctx, d.dark);
            ctx.set_zoom_factor(1.0);
            self.f.show_hidden = d.show_hidden;
            self.settings = Settings { pinned: self.settings.pinned.clone(), last_version: self.settings.last_version.clone(), ..d };
            self.heat_tex = None;
            self.apply_scan_interval();
            let _ = self.scanner.tx.send(ScanCmd::ScanNow);
        }
    }

    fn settings_look(&mut self, ui: &mut Ui) {
        group(ui, "", |ui| {
            row(ui, t!("Sprache", "Language"), t!("Gilt sofort für die ganze App und die Berichte", "Applies immediately to the whole app and the reports"), false, |ui| {
                let mut lang = i18n::get();
                egui::ComboBox::from_id_salt("lang").width(150.0).selected_text(lang.name()).show_ui(ui, |ui| {
                    for l in [Lang::De, Lang::En] {
                        ui.selectable_value(&mut lang, l, l.name());
                    }
                });
                if lang != i18n::get() {
                    i18n::set(lang);
                }
            });
            row(ui, t!("Farbschema", "Theme"), t!("Hell für draußen und helle Räume, dunkel schont die Augen", "Light for outdoors and bright rooms, dark is easier on the eyes"), false, |ui| {
                let mut dark = self.settings.dark;
                if segmented(ui, &mut dark, &[(true, t!("\u{E330} Dunkel", "\u{E330} Dark")), (false, t!("\u{E472} Hell", "\u{E472} Light"))]) {
                    self.settings.dark = dark;
                    crate::theme::apply(ui.ctx(), dark);
                }
            });
            row(ui, t!("Anzeigegröße", "Display size"), t!("Schrift und Bedienelemente – auch mit Strg + und Strg −", "Text and controls – also with Ctrl + and Ctrl −"), true, |ui| {
                let z = ui.ctx().zoom_factor();
                if ui.add_enabled(z < 1.59, egui::Button::new("\u{E3D4}").min_size(Vec2::splat(28.0))).on_hover_text(t!("Größer", "Larger")).clicked() {
                    ui.ctx().set_zoom_factor((z + 0.1).min(1.6));
                }
                if ui.add(egui::Button::new(format!("{:.0} %", z * 100.0)).min_size(Vec2::new(64.0, 28.0))).on_hover_text(t!("Zurück auf 100 %", "Back to 100 %")).clicked() {
                    ui.ctx().set_zoom_factor(1.0);
                }
                if ui.add_enabled(z > 0.71, egui::Button::new("\u{E32A}").min_size(Vec2::splat(28.0))).on_hover_text(t!("Kleiner", "Smaller")).clicked() {
                    ui.ctx().set_zoom_factor((z - 0.1).max(0.7));
                }
            });
        });
    }

    /// Liefert true, wenn das Scan-Intervall geändert wurde
    fn settings_scan(&mut self, ui: &mut Ui) -> bool {
        let mut changed = false;
        group(ui, t!("SUCHE", "SCANNING"), |ui| {
            row(ui, t!("Scan-Intervall", "Scan interval"), t!("Wie oft nach WLANs gesucht wird – während einer Messung automatisch alle 2 s", "How often to scan for Wi-Fi networks – automatically every 2 s during a survey"), true, |ui| {
                let mut v = self.settings.interval;
                egui::ComboBox::from_id_salt("set_interval").width(150.0).selected_text(secs(v)).show_ui(ui, |ui| {
                    for s in [2.0f32, 3.0, 4.0, 5.0, 10.0, 15.0, 30.0, 60.0] {
                        ui.selectable_value(&mut v, s, secs(s));
                    }
                });
                if v != self.settings.interval {
                    self.settings.interval = v;
                    changed = true;
                }
            });
        });
        group(ui, t!("NETZWERKLISTE", "NETWORK LIST"), |ui| {
            row(ui, t!("Als veraltet anzeigen nach", "Show as stale after"), t!("Nicht mehr empfangene Access Points werden ausgegraut", "Access points no longer received are greyed out"), false, |ui| {
                ui.add(egui::DragValue::new(&mut self.settings.stale_after).range(5.0..=300.0).suffix(" s"));
            });
            row(ui, t!("Aus der Liste entfernen nach", "Remove from list after"), t!("Danach verschwinden sie ganz", "After that they disappear completely"), false, |ui| {
                ui.add(egui::DragValue::new(&mut self.settings.remove_after).range(20.0..=3600.0).suffix(" s"));
            });
            row(ui, t!("Nach SSID gruppieren", "Group by SSID"), t!("Eine Zeile je Netz, Klick klappt alle Access Points auf", "One row per network, click to expand all access points"), true, |ui| {
                toggle(ui, &mut self.settings.group_ssid);
            });
        });
        changed
    }

    fn settings_survey(&mut self, ui: &mut Ui) {
        group(ui, t!("AN JEDEM MESSPUNKT", "AT EVERY POINT"), |ui| {
            row(ui, t!("Latenz messen", "Measure latency"), t!("Ping zum Router und ins Internet (1.1.1.1), ca. 3 s", "Ping to the router and to the internet (1.1.1.1), approx. 3 s"), false, |ui| {
                toggle(ui, &mut self.settings.point_ping);
            });
            row(ui, t!("Speedtest", "Speed test"), t!("Download/Upload über speed.cloudflare.com, ca. 12 s und bis zu 70 MB – nur mit Verbindung zur gemessenen SSID", "Download/upload via speed.cloudflare.com, approx. 12 s and up to 70 MB – only when connected to the surveyed SSID"), true, |ui| {
                toggle(ui, &mut self.settings.point_speed);
            });
        });
        let mut opacity = self.settings.heat_opacity;
        group(ui, t!("GRUNDRISS", "FLOOR PLAN"), |ui| {
            row(ui, t!("Deckkraft der Heatmap", "Heatmap opacity"), t!("Weniger, damit Raumnamen im Plan lesbar bleiben", "Lower it to keep room names on the plan readable"), true, |ui| {
                ui.label(format!("{:.0} %", opacity * 100.0));
                ui.spacing_mut().slider_width = 140.0;
                ui.add(egui::Slider::new(&mut opacity, 0.2..=0.9).show_value(false));
            });
        });
        if opacity != self.settings.heat_opacity {
            self.settings.heat_opacity = opacity;
            self.heat_tex = None;
        }
        group(ui, t!("SPEICHERORT", "LOCATION"), |ui| {
            let dir = super::survey::survey_dir();
            row(ui, t!("Gespeicherte Messungen", "Saved surveys"), &dir.display().to_string(), true, |ui| {
                if ui.button(t!("\u{E256} Öffnen", "\u{E256} Open")).clicked() {
                    let _ = std::fs::create_dir_all(&dir);
                    let _ = std::process::Command::new("explorer").arg(&dir).spawn();
                }
            });
        });
    }

    /// Liefert (Hersteller-Datenbank laden, Liste leeren, Markierungen entfernen)
    fn settings_data(&mut self, ui: &mut Ui) -> (bool, bool, bool) {
        let (mut oui, mut clear, mut unpin) = (false, false, false);
        group(ui, "", |ui| {
            let db = if self.oui.full_db {
                tf!("{} Einträge · Wireshark / IEEE", "{} entries · Wireshark / IEEE", self.oui.len())
            } else {
                tf!("Basisliste mit {} Einträgen – „Aktualisieren“ lädt die vollständige Liste", "Basic list with {} entries – \"Update\" downloads the full list", self.oui.len())
            };
            row(ui, t!("Hersteller-Datenbank", "Vendor database"), &db, false, |ui| {
                let label = if self.oui_rx.is_some() { t!("Lädt …", "Loading …") } else { t!("\u{E036} Aktualisieren", "\u{E036} Update") };
                if ui.add_enabled(self.oui_rx.is_none(), egui::Button::new(label)).clicked() {
                    oui = true;
                }
            });
            row(ui, t!("Netzwerkliste", "Network list"), t!("Alle bisher gefundenen Access Points vergessen", "Forget all access points found so far"), false, |ui| {
                if ui.button(t!("Leeren", "Clear")).clicked() {
                    clear = true;
                }
            });
            row(ui, t!("Markierungen", "Pins"), &tf!("{} markierte Access Points", "{} pinned access points", self.pinned.len()), false, |ui| {
                if ui.add_enabled(!self.pinned.is_empty(), egui::Button::new(t!("Alle entfernen", "Remove all"))).clicked() {
                    unpin = true;
                }
            });
            row(ui, t!("Datenordner", "Data folder"), t!("Einstellungen, Hersteller-Datenbank und Grundrisse (%APPDATA%\\WifiAnalyzer)", "Settings, vendor database and floor plans (%APPDATA%\\WifiAnalyzer)"), true, |ui| {
                if ui.button(t!("\u{E256} Öffnen", "\u{E256} Open")).clicked() {
                    let _ = std::fs::create_dir_all(settings::app_dir());
                    let _ = std::process::Command::new("explorer").arg(settings::app_dir()).spawn();
                }
            });
        });
        (oui, clear, unpin)
    }

    /// Liefert (jetzt nach Updates suchen, deinstallieren)
    fn settings_program(&mut self, ui: &mut Ui) -> (bool, bool) {
        let p = pal(ui);
        let (mut check, mut uninstall) = (false, false);
        // Kopf mit Logo und Version
        egui::Frame::none().fill(p.surface2).stroke(Stroke::new(1.0f32, p.border)).rounding(10.0).inner_margin(Margin::same(14.0)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                if let Some(tex) = &self.icon_tex {
                    ui.add(egui::Image::new((tex.id(), Vec2::splat(48.0))));
                }
                ui.vertical(|ui| {
                    ui.label(RichText::new("WiFi Analyzer").size(17.0).family(crate::theme::semibold()).color(p.text));
                    ui.label(RichText::new(tf!("Version {VERSION} · {AUTHOR}", "Version {VERSION} · {AUTHOR}")).color(p.weak));
                    ui.hyperlink_to(RichText::new("schoepf-tirol.at/Project/WifiAnalyzer").size(12.0), WEBSITE);
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let busy = self.update.rx.is_some();
                    if let Some(v) = self.update.available().map(|r| r.version.clone()) {
                        if primary_button(ui, &tf!("\u{E030} Update auf {v}", "\u{E030} Update to {v}"), p.accent).clicked() {
                            self.show_settings = false;
                            self.show_update = true;
                        }
                    } else if ui.add_enabled(!busy, egui::Button::new(if busy { t!("Sucht …", "Checking …") } else { t!("Nach Updates suchen", "Check for updates") })).clicked() {
                        check = true;
                    }
                });
            });
        });
        ui.add_space(16.0);
        group(ui, "UPDATES", |ui| {
            row(ui, t!("Automatisch nach Updates suchen", "Check for updates automatically"), t!("Beim Start und alle 6 Stunden bei GitHub nachsehen", "At startup and every 6 hours on GitHub"), true, |ui| {
                toggle(ui, &mut self.settings.check_updates);
            });
        });
        group(ui, t!("ENTFERNEN", "REMOVE"), |ui| {
            if crate::app::update::is_installed() {
                let confirm = self.uninstall_confirm;
                row(ui, t!("Deinstallieren", "Uninstall"), t!("WiFi Analyzer von diesem PC entfernen – gespeicherte Messungen in „Dokumente“ bleiben erhalten", "Remove WiFi Analyzer from this PC – saved surveys in \"Documents\" are kept"), false, |ui| {
                    let label = if confirm { t!("Wirklich deinstallieren?", "Really uninstall?") } else { t!("Deinstallieren …", "Uninstall …") };
                    if ui.button(RichText::new(label).color(p.danger)).clicked() {
                        if confirm {
                            uninstall = true;
                        } else {
                            self.uninstall_confirm = true;
                        }
                    }
                });
            } else {
                row(ui, t!("Portable Version", "Portable version"), t!("Zum Entfernen einfach den Ordner mit der EXE löschen", "To remove it, simply delete the folder containing the EXE"), false, |ui| {
                    if ui.button(t!("\u{E256} Ordner öffnen", "\u{E256} Open folder")).clicked() {
                        if let Some(d) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())) {
                            let _ = std::process::Command::new("explorer").arg(d).spawn();
                        }
                    }
                });
            }
            row(ui, t!("Windows-Apps", "Windows apps"), t!("Installierte Apps in den Windows-Einstellungen", "Installed apps in Windows Settings"), true, |ui| {
                if ui.button(t!("Öffnen", "Open")).clicked() {
                    let _ = std::process::Command::new("explorer").arg("ms-settings:appsfeatures").spawn();
                }
            });
        });
        (check, uninstall)
    }

    pub(super) fn about_window(&mut self, ctx: &egui::Context) {
        if !self.show_about {
            return;
        }
        let mut open = true;
        let (mut check, mut whats_new, mut update) = (false, false, false);
        egui::Window::new(RichText::new(t!("Über WiFi Analyzer", "About WiFi Analyzer")).strong().size(16.0))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .min_width(520.0)
            .max_width(520.0)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                let p = pal(ui);
                ui.horizontal(|ui| {
                    if let Some(tex) = &self.icon_tex {
                        ui.add(egui::Image::new((tex.id(), Vec2::splat(76.0))));
                    }
                    ui.vertical(|ui| {
                        ui.label(RichText::new("WiFi Analyzer").size(24.0).strong().color(p.text));
                        ui.label(RichText::new(tf!("Version {VERSION}", "Version {VERSION}")).color(p.weak));
                        ui.label(RichText::new(t!("WLAN-Analyse für Techniker unter Windows", "Wi-Fi analysis for technicians on Windows")).color(p.text));
                    });
                });
                ui.add_space(10.0);
                egui::Grid::new("about").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                    ui.label(RichText::new(t!("Entwickelt von", "Developed by")).color(p.weak));
                    ui.label(RichText::new(AUTHOR).strong());
                    ui.end_row();
                    ui.label(RichText::new("Website").color(p.weak));
                    ui.hyperlink_to("schoepf-tirol.at/Project/WifiAnalyzer", WEBSITE);
                    ui.end_row();
                    ui.label(RichText::new("GitHub").color(p.weak));
                    ui.hyperlink_to("github.com/zenutrix/wifianalyzer", REPO);
                    ui.end_row();
                    ui.label(RichText::new(t!("Fehler & Wünsche", "Bugs & ideas")).color(p.weak));
                    ui.hyperlink_to(t!("Issue auf GitHub anlegen", "Open an issue on GitHub"), format!("{REPO}/issues"));
                    ui.end_row();
                    ui.label(RichText::new(t!("Lizenz", "License")).color(p.weak));
                    ui.hyperlink_to("GNU GPL v3.0", format!("{REPO}/blob/main/LICENSE"));
                    ui.end_row();
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if let Some(v) = self.update.available().map(|r| r.version.clone()) {
                        if primary_button(ui, &tf!("\u{E030} Update auf {v}", "\u{E030} Update to {v}"), p.accent).clicked() {
                            update = true;
                        }
                    } else if ui.add_enabled(self.update.rx.is_none(), egui::Button::new(t!("Nach Updates suchen", "Check for updates"))).clicked() {
                        check = true;
                    }
                    if ui.button(t!("Was ist neu", "What's new")).clicked() {
                        whats_new = true;
                    }
                });
                ui.separator();
                egui::CollapsingHeader::new(t!("Tastenkürzel", "Keyboard shortcuts")).default_open(false).show(ui, |ui| {
                    egui::Grid::new("keys").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
                        for (k, v) in [
                            ("F5", t!("Sofort scannen", "Scan now")),
                            (t!("Strg+F", "Ctrl+F"), t!("Suchen", "Search")),
                            (t!("Strg+E", "Ctrl+E"), t!("Export als CSV", "Export as CSV")),
                            (t!("Strg+1 … 9", "Ctrl+1 … 9"), t!("Seite wechseln", "Switch page")),
                            ("Esc", t!("Details / Dialog schließen", "Close details / dialog")),
                        ] {
                            ui.label(RichText::new(k).monospace().strong());
                            ui.label(v);
                            ui.end_row();
                        }
                    });
                });
                egui::CollapsingHeader::new(t!("Datenschutz & Datenquellen", "Privacy & data sources")).default_open(false).show(ui, |ui| {
                    ui.label(t!(
                        "Alle Messungen bleiben auf diesem PC. Netzwerkzugriffe gibt es nur für die Hersteller-Datenbank (Wireshark/IEEE) und – falls aktiviert – die Update-Prüfung bei GitHub.",
                        "All measurements stay on this PC. The only network access is for the vendor database (Wireshark/IEEE) and – if enabled – the update check on GitHub."
                    ));
                    ui.label(RichText::new(t!("Gebaut mit Rust, egui und der Windows Native WiFi API.", "Built with Rust, egui and the Windows Native WiFi API.")).color(p.weak));
                });
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!(
                        "© {} {AUTHOR}. {}",
                        chrono::Local::now().format("%Y"),
                        t!("Freie Software unter der GPL v3 – ohne jegliche Gewährleistung.", "Free software under the GPL v3 – without any warranty.")
                    ))
                    .size(12.0)
                    .color(p.weak),
                );
            });
        if !open {
            self.show_about = false;
        }
        if check {
            self.start_update_check(ctx, true);
        }
        if whats_new {
            self.show_about = false;
            self.show_whats_new = true;
        }
        if update {
            self.show_about = false;
            self.show_update = true;
        }
    }
}
