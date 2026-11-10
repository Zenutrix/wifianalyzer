//! Einstellungen und „Über“-Dialog.

use eframe::egui::{self, Align, Align2, Layout, Margin, RichText, Stroke, Ui, Vec2};

use super::{WifiApp, AUTHOR, REPO, VERSION, WEBSITE};
use crate::i18n::{self, Lang};
use crate::settings::{self, Settings};
use crate::theme::pal;
use crate::widgets::{primary_button, segmented};
use crate::wlan::ScanCmd;
use crate::{t, tf};

/// Abschnitt mit Überschrift und Rahmen
fn section(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui)) {
    let p = pal(ui);
    ui.add_space(10.0);
    ui.label(RichText::new(title).size(12.0).strong().color(p.weak));
    ui.add_space(2.0);
    egui::Frame::none()
        .fill(p.surface2)
        .stroke(Stroke::new(1.0f32, p.border))
        .rounding(8.0)
        .inner_margin(Margin::symmetric(14.0, 10.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

/// Eine Einstellung: Titel + Beschreibung links, Bedienelement rechts
fn row(ui: &mut Ui, title: &str, desc: &str, control: impl FnOnce(&mut Ui)) {
    let p = pal(ui);
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width((ui.available_width() - 190.0).max(200.0));
            ui.label(RichText::new(title).color(p.text));
            if !desc.is_empty() {
                ui.add(egui::Label::new(RichText::new(desc).size(12.0).color(p.weak)).wrap());
            }
        });
        ui.with_layout(Layout::right_to_left(Align::Center), control);
    });
    ui.add_space(4.0);
}

fn secs(v: f32) -> String {
    if v >= 60.0 {
        format!("{} min", v as u32 / 60)
    } else {
        format!("{} s", v as u32)
    }
}

impl WifiApp {
    pub(super) fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.show_settings {
            return;
        }
        let mut open = true;
        let (mut oui, mut clear, mut unpin, mut reset, mut done) = (false, false, false, false, false);
        let mut uninstall = false;
        let mut interval_changed = false;
        egui::Window::new(RichText::new(t!("Einstellungen", "Settings")).strong().size(16.0))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .min_width(560.0)
            .max_width(560.0)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                let p = pal(ui);
                let max_h = (ui.ctx().screen_rect().height() - 200.0).max(300.0);
                egui::ScrollArea::vertical().max_height(max_h).min_scrolled_height(max_h.min(640.0)).show(ui, |ui| {
                    section(ui, t!("ALLGEMEIN", "GENERAL"), |ui| {
                        row(ui, t!("Sprache", "Language"), "", |ui| {
                            let mut lang = i18n::get();
                            egui::ComboBox::from_id_salt("lang").width(140.0).selected_text(lang.name()).show_ui(ui, |ui| {
                                for l in [Lang::De, Lang::En] {
                                    ui.selectable_value(&mut lang, l, l.name());
                                }
                            });
                            if lang != i18n::get() {
                                i18n::set(lang);
                            }
                        });
                        row(ui, t!("Farbschema", "Theme"), "", |ui| {
                            let mut dark = self.settings.dark;
                            if segmented(ui, &mut dark, &[(false, t!("Hell", "Light")), (true, t!("Dunkel", "Dark"))]) {
                                self.settings.dark = dark;
                                crate::theme::apply(ui.ctx(), dark);
                            }
                        });
                        row(
                            ui,
                            t!("Automatisch nach Updates suchen", "Check for updates automatically"),
                            t!("Beim Start und alle 6 Stunden bei GitHub nachsehen", "At startup and every 6 hours on GitHub"),
                            |ui| {
                                ui.checkbox(&mut self.settings.check_updates, "");
                            },
                        );
                    });

                    section(ui, "SCAN", |ui| {
                        row(
                            ui,
                            t!("Scan-Intervall", "Scan interval"),
                            t!("Wie oft nach WLANs gesucht wird – während einer Messung automatisch alle 2 s", "How often to scan for Wi-Fi networks – automatically every 2 s during a survey"),
                            |ui| {
                                let mut v = self.settings.interval;
                                egui::ComboBox::from_id_salt("set_interval").width(140.0).selected_text(secs(v)).show_ui(ui, |ui| {
                                    for s in [2.0f32, 3.0, 4.0, 5.0, 10.0, 15.0, 30.0, 60.0] {
                                        ui.selectable_value(&mut v, s, secs(s));
                                    }
                                });
                                if v != self.settings.interval {
                                    self.settings.interval = v;
                                    interval_changed = true;
                                }
                            },
                        );
                        row(
                            ui,
                            t!("Als veraltet anzeigen nach", "Show as stale after"),
                            t!("Nicht mehr empfangene APs werden ausgegraut", "APs no longer received are greyed out"),
                            |ui| {
                                ui.add(egui::DragValue::new(&mut self.settings.stale_after).range(5.0..=300.0).suffix(" s"));
                            },
                        );
                        row(
                            ui,
                            t!("Aus der Liste entfernen nach", "Remove from list after"),
                            "",
                            |ui| {
                                ui.add(egui::DragValue::new(&mut self.settings.remove_after).range(20.0..=3600.0).suffix(" s"));
                            },
                        );
                        row(
                            ui,
                            t!("Netzwerkliste nach SSID gruppieren", "Group network list by SSID"),
                            t!("Eine Zeile je Netz, Klick klappt alle Access Points auf", "One row per network, click to expand all access points"),
                            |ui| {
                                ui.checkbox(&mut self.settings.group_ssid, "");
                            },
                        );
                    });

                    section(ui, t!("DATEN", "DATA"), |ui| {
                        let db = if self.oui.full_db {
                            tf!("{} Einträge · Wireshark / IEEE", "{} entries · Wireshark / IEEE", self.oui.len())
                        } else {
                            tf!("Basisliste mit {} Einträgen", "Basic list with {} entries", self.oui.len())
                        };
                        row(ui, t!("Hersteller-Datenbank", "Vendor database"), &db, |ui| {
                            let label = if self.oui_rx.is_some() { t!("Lädt …", "Loading …") } else { t!("Aktualisieren", "Update") };
                            if ui.add_enabled(self.oui_rx.is_none(), egui::Button::new(label)).clicked() {
                                oui = true;
                            }
                        });
                        row(ui, t!("Netzwerkliste", "Network list"), t!("Alle bisher gefundenen APs vergessen", "Forget all access points found so far"), |ui| {
                            if ui.button(t!("Leeren", "Clear")).clicked() {
                                clear = true;
                            }
                        });
                        row(ui, t!("Markierungen", "Pins"), &tf!("{} markierte Access Points", "{} pinned access points", self.pinned.len()), |ui| {
                            if ui.add_enabled(!self.pinned.is_empty(), egui::Button::new(t!("Alle entfernen", "Remove all"))).clicked() {
                                unpin = true;
                            }
                        });
                        row(ui, t!("Datenordner", "Data folder"), "%APPDATA%\\WifiAnalyzer", |ui| {
                            if ui.button(t!("Öffnen", "Open")).clicked() {
                                let _ = std::fs::create_dir_all(settings::app_dir());
                                let _ = std::process::Command::new("explorer").arg(settings::app_dir()).spawn();
                            }
                        });
                    });

                    section(ui, t!("PROGRAMM", "PROGRAM"), |ui| {
                        if crate::app::update::is_installed() {
                            let confirm = self.uninstall_confirm;
                            row(
                                ui,
                                t!("Deinstallieren", "Uninstall"),
                                t!("WiFi Analyzer von diesem PC entfernen – gespeicherte Messungen in „Dokumente“ bleiben erhalten", "Remove WiFi Analyzer from this PC – saved surveys in \"Documents\" are kept"),
                                |ui| {
                                    let label = if confirm { t!("Wirklich deinstallieren?", "Really uninstall?") } else { t!("Deinstallieren …", "Uninstall …") };
                                    if ui.button(RichText::new(label).color(p.danger)).clicked() {
                                        if confirm {
                                            uninstall = true;
                                        } else {
                                            self.uninstall_confirm = true;
                                        }
                                    }
                                },
                            );
                        } else {
                            row(
                                ui,
                                t!("Entfernen", "Remove"),
                                t!("Portable Version: zum Entfernen einfach den Ordner mit der EXE löschen", "Portable version: to remove it, simply delete the folder containing the EXE"),
                                |ui| {
                                    if ui.button(t!("Ordner öffnen", "Open folder")).clicked() {
                                        if let Some(d) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())) {
                                            let _ = std::process::Command::new("explorer").arg(d).spawn();
                                        }
                                    }
                                },
                            );
                        }
                        row(ui, t!("Windows-Apps", "Windows apps"), t!("Installierte Apps in den Windows-Einstellungen", "Installed apps in Windows Settings"), |ui| {
                            if ui.button(t!("Öffnen", "Open")).clicked() {
                                let _ = std::process::Command::new("explorer").arg("ms-settings:appsfeatures").spawn();
                            }
                        });
                    });
                });

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button(t!("Auf Standard zurücksetzen", "Reset to defaults")).clicked() {
                        reset = true;
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if primary_button(ui, t!("Fertig", "Done"), p.accent).clicked() {
                            done = true;
                        }
                        ui.label(RichText::new(t!("Wird automatisch gespeichert", "Saved automatically")).size(12.0).color(p.weak));
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
            self.f.show_hidden = d.show_hidden;
            self.settings = Settings { pinned: self.settings.pinned.clone(), last_version: self.settings.last_version.clone(), ..d };
            self.apply_scan_interval();
            let _ = self.scanner.tx.send(ScanCmd::ScanNow);
        }
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
                    ui.hyperlink_to("wifianalyzer.schoepf-tirol.at", WEBSITE);
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
