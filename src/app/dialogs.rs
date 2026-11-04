//! Lade-Popup, Berechtigungsdialog, Einstellungen, „Über“-Dialog und Update-Prüfung.

use std::time::Duration;

use eframe::egui::{self, Align, Align2, Color32, FontId, Layout, Margin, RichText, Sense, Stroke, Ui, Vec2};

use super::{Sev, WifiApp, AUTHOR, REPO, VERSION, WEBSITE};
use crate::i18n::{self, Lang};
use crate::perm;
use crate::settings::{self, Settings};
use crate::theme::{pal, pal_ctx};
use crate::widgets::{primary_button, segmented};
use crate::wlan::{self, ScanCmd};
use crate::{t, tf};

/// Versionsvergleich "1.10.0" > "1.9.2"
pub fn is_newer(latest: &str, current: &str) -> bool {
    let parse = |s: &str| -> Vec<u32> { s.trim_start_matches('v').split('.').map(|p| p.trim().parse().unwrap_or(0)).collect() };
    parse(latest) > parse(current)
}

/// Neueste Version von GitHub-Releases abfragen (blockierend; im Hintergrund aufrufen).
/// Ok(None) = noch kein Release veröffentlicht.
fn fetch_latest_release() -> Result<Option<String>, String> {
    use std::os::windows::process::CommandExt;
    let api = REPO.replace("https://github.com/", "https://api.github.com/repos/") + "/releases/latest";
    let ps = format!(
        "$ProgressPreference='SilentlyContinue'; [Net.ServicePointManager]::SecurityProtocol='Tls12,Tls13'; \
         try {{ (Invoke-RestMethod -UseBasicParsing -TimeoutSec 15 -Headers @{{'User-Agent'='WifiAnalyzer-UpdateCheck'}} -Uri '{api}').tag_name }} \
         catch {{ if ($_.Exception.Response -and [int]$_.Exception.Response.StatusCode -eq 404) {{ 'NONE' }} else {{ throw }} }}"
    );
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &ps])
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(err.lines().next().unwrap_or("error").trim().to_string());
    }
    let tag = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if tag.is_empty() || tag == "NONE" {
        Ok(None)
    } else {
        Ok(Some(tag.trim_start_matches('v').to_string()))
    }
}

impl WifiApp {
    // ------------------------------------------------------------------ Updates

    pub(super) fn start_update_check(&mut self, ctx: &egui::Context, manual: bool) {
        if self.update.rx.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let c = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(fetch_latest_release());
            c.request_repaint();
        });
        self.update.rx = Some(rx);
        self.update.manual = manual;
        self.update.checked = true;
    }

    pub(super) fn maybe_auto_update_check(&mut self, ctx: &egui::Context) {
        if self.settings.check_updates && !self.update.checked && self.start.elapsed() > Duration::from_secs(5) {
            self.start_update_check(ctx, false);
        }
    }

    pub(super) fn poll_update(&mut self) {
        let Some(rx) = &self.update.rx else { return };
        let Ok(res) = rx.try_recv() else { return };
        self.update.rx = None;
        match res {
            Ok(v) => {
                self.update.error = None;
                self.update.latest = v.clone();
                match v {
                    Some(v) if is_newer(&v, VERSION) => self.toast(Sev::Info, tf!("Neue Version {v} verfügbar", "New version {v} available")),
                    _ if self.update.manual => self.toast(Sev::Info, tf!("WiFi Analyzer {VERSION} ist aktuell", "WiFi Analyzer {VERSION} is up to date")),
                    _ => {}
                }
            }
            Err(e) => {
                if self.update.manual {
                    self.toast(Sev::Warning, tf!("Update-Prüfung fehlgeschlagen: {e}", "Update check failed: {e}"));
                }
                self.update.error = Some(e);
            }
        }
    }

    // ------------------------------------------------------------------ Lade-Popup

    /// Kleines Lade-Popup (randloses Fenster). Liefert true, solange es angezeigt wird.
    pub(super) fn splash(&mut self, ctx: &egui::Context) -> bool {
        if self.splash_done {
            return false;
        }
        let t = self.start.elapsed().as_secs_f32();
        let scanned = self.scan_count > 0 || self.error.is_some();
        let oui_ready = self.oui_rx.is_none();
        let perm_ready = self.perm.is_some();
        // Auf die Windows-Abfrage wird gewartet (der Benutzer muss evtl. erst bestätigen), sonst max. 12 s
        let ready = t > 1.6 && scanned && perm_ready && (oui_ready || t > 6.0) || (perm_ready && t > 12.0) || t > 60.0;
        let steps = [
            (t!("Berechtigungen prüfen", "Checking permissions"), perm_ready),
            (t!("WLAN-Adapter initialisieren", "Initializing Wi-Fi adapter"), !self.interfaces.is_empty() || t > 1.0),
            (t!("Hersteller-Datenbank laden", "Loading vendor database"), oui_ready),
            (t!("Erster Netzwerk-Scan", "First network scan"), scanned),
        ];
        let done = steps.iter().filter(|s| s.1).count();

        // Fortschritt: fertige Schritte + langsames "Kriechen" im aktuellen Schritt, nie rückwärts
        if done != self.splash_done_steps {
            self.splash_done_steps = done;
            self.splash_step_t = t;
        }
        let n = steps.len() as f32;
        let target = if ready {
            1.0
        } else {
            let creep = 0.85 * (1.0 - (-(t - self.splash_step_t) / 1.8).exp());
            ((done as f32 + if done < steps.len() { creep } else { 0.0 }) / n).min(0.97)
        };
        let dt = ctx.input(|i| i.stable_dt).clamp(0.0, 0.1);
        let speed = if ready { 9.0 } else { 5.0 };
        let next = self.splash_progress + (target - self.splash_progress) * (1.0 - (-dt * speed).exp());
        self.splash_progress = next.max(self.splash_progress).min(1.0);

        if ready && self.splash_progress > 0.995 {
            self.splash_done = true;
            // Popup -> normales, maximiertes Hauptfenster (Wiederherstellen-Größe mittig)
            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(Vec2::new(1100.0, 680.0)));
            let size = match ctx.input(|i| i.viewport().monitor_size) {
                Some(m) => Vec2::new((m.x * 0.85).min(1600.0), (m.y * 0.85).min(1000.0)),
                None => Vec2::new(1400.0, 860.0),
            };
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
            if let Some(m) = ctx.input(|i| i.viewport().monitor_size) {
                let pos = ((m - size) * 0.5).max(Vec2::ZERO);
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(pos.x, pos.y)));
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            return false;
        }
        let progress = self.splash_progress;
        let accent = crate::theme::DARK.accent;

        egui::CentralPanel::default().frame(egui::Frame::none()).show(ctx, |ui| {
            let rect = ui.max_rect();
            let resp = ui.interact(rect, egui::Id::new("splash_drag"), Sense::drag());
            if resp.drag_started() {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            let p = ui.painter_at(rect);
            let fade = (t / 0.5).clamp(0.0, 1.0);
            let card = rect.shrink(1.0);
            p.rect_filled(card, 18.0, Color32::from_rgb(12, 17, 26));
            let pc = p.with_clip_rect(card.shrink(12.0));
            pc.circle_filled(egui::pos2(card.center().x, card.top() + 60.0), 230.0, Color32::from_rgb(30, 80, 170).gamma_multiply(0.10));
            p.rect_stroke(card, 18.0, Stroke::new(1.5f32, accent.gamma_multiply(0.45)));

            let ic = egui::pos2(card.center().x, card.top() + 100.0);
            for k in 0..3 {
                let ph = (t * 0.6 + k as f32 / 3.0) % 1.0;
                pc.circle_stroke(ic, 50.0 + ph * 190.0, Stroke::new(1.5f32, accent.gamma_multiply((1.0 - ph) * 0.45 * fade)));
            }
            let breathe = 1.0 + (t * 2.0).sin() * 0.03;
            for k in 0..5 {
                p.circle_filled(ic, (52.0 + k as f32 * 7.0) * breathe, Color32::from_rgb(40, 140, 255).gamma_multiply(0.05 * fade));
            }
            if let Some(tex) = &self.icon_tex {
                let r = egui::Rect::from_center_size(ic, Vec2::splat(96.0 * breathe));
                p.image(tex.id(), r, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE.gamma_multiply(fade));
            }
            let weak = Color32::from_rgb(140, 160, 190);
            p.text(egui::pos2(ic.x, card.top() + 186.0), Align2::CENTER_CENTER, "WiFi Analyzer", FontId::proportional(32.0), Color32::WHITE.gamma_multiply(fade));
            p.text(
                egui::pos2(ic.x, card.top() + 216.0),
                Align2::CENTER_CENTER,
                t!("WLAN-Analyse für Techniker", "Wi-Fi analysis for technicians"),
                FontId::proportional(14.5),
                weak.gamma_multiply(fade),
            );

            let bar_w = 360.0;
            let bar = egui::Rect::from_center_size(egui::pos2(ic.x, card.top() + 256.0), Vec2::new(bar_w, 5.0));
            p.rect_filled(bar, 3.0, Color32::from_rgb(32, 44, 66));
            let mut fill = bar;
            fill.set_width((bar_w * progress).max(6.0));
            p.rect_filled(fill, 3.0, accent);
            let gx = fill.left() + ((t * 0.7) % 1.0) * fill.width();
            let glint = egui::Rect::from_center_size(egui::pos2(gx, bar.center().y), Vec2::new(40.0, bar.height()));
            p.with_clip_rect(fill).rect_filled(glint, 3.0, Color32::from_white_alpha(60));
            p.text(egui::pos2(bar.right(), bar.top() - 12.0), Align2::RIGHT_CENTER, format!("{:.0} %", progress * 100.0), FontId::proportional(12.0), weak.gamma_multiply(fade));

            for (i, (label, ok)) in steps.iter().enumerate() {
                let y = bar.bottom() + 24.0 + i as f32 * 23.0;
                let sx = egui::pos2(bar.left() + 40.0, y);
                if *ok {
                    p.text(sx, Align2::CENTER_CENTER, "✔", FontId::proportional(14.0), Color32::from_rgb(63, 185, 80).gamma_multiply(fade));
                } else {
                    let a0 = t * 6.0;
                    let pts: Vec<egui::Pos2> = (0..=16)
                        .map(|s| {
                            let a = a0 + s as f32 / 16.0 * 4.5;
                            egui::pos2(sx.x + 6.0 * a.cos(), sx.y + 6.0 * a.sin())
                        })
                        .collect();
                    p.add(egui::Shape::line(pts, Stroke::new(2.0f32, accent.gamma_multiply(fade))));
                }
                let col = if *ok { Color32::from_rgb(200, 215, 235) } else { Color32::from_rgb(120, 140, 170) };
                p.text(egui::pos2(bar.left() + 60.0, y), Align2::LEFT_CENTER, *label, FontId::proportional(14.0), col.gamma_multiply(fade));
            }

            let footer = egui::pos2(ic.x, card.bottom() - 20.0);
            if !perm_ready && t > 2.0 {
                p.text(
                    footer,
                    Align2::CENTER_CENTER,
                    t!("Bitte die Windows-Abfrage zum Standortzugriff bestätigen", "Please confirm the Windows location access prompt"),
                    FontId::proportional(13.0),
                    Color32::from_rgb(255, 196, 90).gamma_multiply(0.6 + 0.4 * (t * 3.0).sin().abs()),
                );
            } else {
                p.text(footer, Align2::CENTER_CENTER, format!("v{VERSION}  ·  © {AUTHOR}"), FontId::proportional(12.0), Color32::from_rgb(90, 110, 140));
            }
        });
        ctx.request_repaint();
        true
    }

    // ------------------------------------------------------------------ Berechtigung

    fn needs_perm_dialog(&self) -> bool {
        if self.perm_dismissed || self.scan_works() {
            return false;
        }
        self.error_code == wlan::ERROR_ACCESS_DENIED || self.perm.as_ref().is_some_and(|p| !p.ok())
    }

    pub(super) fn perm_dialog(&mut self, ctx: &egui::Context) {
        if !self.needs_perm_dialog() {
            return;
        }
        // Während der Dialog offen ist, regelmäßig nachsehen, ob die Schalter umgelegt wurden
        if self.perm_last_check.elapsed() > Duration::from_secs(3) {
            self.start_perm_check(ctx, false);
            let _ = self.scanner.tx.send(ScanCmd::ScanNow);
        }
        let p = pal_ctx(ctx);
        ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("perm_dim")))
            .rect_filled(ctx.screen_rect(), 0.0, Color32::from_black_alpha(170));

        let checklist = self.perm.as_ref().map(|p| p.checklist()).unwrap_or_default();
        let (mut open_settings, mut recheck, mut later) = (false, false, false);
        egui::Window::new("perm_window")
            .order(egui::Order::Tooltip)
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .fixed_size([560.0, 0.0])
            .frame(egui::Frame::window(&ctx.style()).inner_margin(Margin::same(24.0)).rounding(14.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if let Some(tex) = &self.icon_tex {
                        ui.add(egui::Image::new((tex.id(), Vec2::splat(64.0))));
                    }
                    ui.vertical(|ui| {
                        ui.add_space(6.0);
                        ui.label(RichText::new(t!("Standortzugriff erforderlich", "Location access required")).size(22.0).strong());
                        ui.label(RichText::new(t!("Damit der WiFi Analyzer WLAN-Netze sehen darf", "So that WiFi Analyzer may see Wi-Fi networks")).color(p.weak));
                    });
                });
                ui.add_space(10.0);
                ui.label(t!(
                    "Windows gibt BSSIDs, Signalstärken und Kanäle nur an Apps heraus, die auf den Standort zugreifen dürfen (WLAN-Netze verraten den Aufenthaltsort). Der WiFi Analyzer nutzt keine Standortdaten und sendet nichts ins Internet.",
                    "Windows only hands out BSSIDs, signal strengths and channels to apps that may access the location (Wi-Fi networks reveal where you are). WiFi Analyzer does not use location data and sends nothing to the internet."
                ));
                ui.add_space(12.0);
                egui::Frame::group(ui.style()).rounding(8.0).inner_margin(Margin::same(12.0)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    for (state, text) in &checklist {
                        ui.horizontal(|ui| {
                            let (icon, col) = match state {
                                Some(true) => ("✔", p.success),
                                Some(false) => ("✖", p.danger),
                                None => ("?", p.weak),
                            };
                            ui.label(RichText::new(icon).strong().color(col));
                            ui.label(*text);
                        });
                    }
                    if checklist.is_empty() {
                        ui.horizontal(|ui| {
                            ui.add(egui::Spinner::new());
                            ui.label(t!("Prüfe Berechtigungen …", "Checking permissions …"));
                        });
                    }
                });
                ui.add_space(12.0);
                ui.label(RichText::new(t!("So geht's:", "How to:")).strong());
                ui.label(t!("1.  „Standort-Einstellungen öffnen“ klicken", "1.  Click \"Open location settings\""));
                ui.label(t!(
                    "2.  „Standortdienste“ und „Desktop-Apps den Zugriff auf Ihren Standort erlauben“ einschalten",
                    "2.  Turn on \"Location services\" and \"Let desktop apps access your location\""
                ));
                ui.label(t!("3.  Zurück zum WiFi Analyzer – die Änderung wird automatisch erkannt", "3.  Return to WiFi Analyzer – the change is detected automatically"));
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    if primary_button(ui, t!("⚙  Standort-Einstellungen öffnen", "⚙  Open location settings"), p.accent).clicked() {
                        open_settings = true;
                    }
                    if ui.button(t!("⟳  Erneut anfragen", "⟳  Ask again")).clicked() {
                        recheck = true;
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button(t!("Später", "Later")).clicked() {
                            later = true;
                        }
                        if self.perm_rx.is_some() {
                            ui.add(egui::Spinner::new());
                        }
                    });
                });
            });
        if open_settings {
            perm::open_location_settings();
        }
        if recheck {
            self.start_perm_check(ctx, true);
            let _ = self.scanner.tx.send(ScanCmd::ScanNow);
        }
        if later {
            self.perm_dismissed = true;
        }
        ctx.request_repaint_after(Duration::from_millis(500));
    }

    // ------------------------------------------------------------------ Einstellungen

    pub(super) fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.show_settings {
            return;
        }
        let mut open = true;
        let (mut oui, mut clear, mut unpin, mut reset, mut check) = (false, false, false, false, false);
        egui::Window::new(RichText::new(t!("⚙ Einstellungen", "⚙ Settings")).strong())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(480.0)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                let p = pal(ui);
                let group = |ui: &mut Ui, title: &str, add: &mut dyn FnMut(&mut Ui)| {
                    ui.add_space(6.0);
                    ui.label(RichText::new(title).strong().color(p.text));
                    egui::Frame::group(ui.style()).rounding(8.0).inner_margin(Margin::same(10.0)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        add(ui);
                    });
                };

                group(ui, t!("Darstellung", "Appearance"), &mut |ui| {
                    egui::Grid::new("set_look").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                        ui.label(t!("Sprache", "Language"));
                        let mut lang = i18n::get();
                        egui::ComboBox::from_id_salt("lang").selected_text(lang.name()).show_ui(ui, |ui| {
                            for l in [Lang::De, Lang::En] {
                                ui.selectable_value(&mut lang, l, l.name());
                            }
                        });
                        if lang != i18n::get() {
                            i18n::set(lang);
                        }
                        ui.end_row();
                        ui.label(t!("Farbschema", "Theme"));
                        let mut dark = self.settings.dark;
                        if segmented(ui, &mut dark, &[(true, t!("Dunkel", "Dark")), (false, t!("Hell", "Light"))]) {
                            self.settings.dark = dark;
                            crate::theme::apply(ui.ctx(), dark);
                        }
                        ui.end_row();
                    });
                });

                group(ui, "Scan", &mut |ui| {
                    egui::Grid::new("set_scan").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                        ui.label(t!("Scan-Intervall", "Scan interval"));
                        if ui.add(egui::Slider::new(&mut self.settings.interval, 2.0..=60.0).suffix(" s").step_by(1.0).fixed_decimals(0)).changed() {
                            self.apply_scan_interval();
                        }
                        ui.end_row();
                        ui.label(t!("Als veraltet markieren nach", "Mark as stale after"));
                        ui.add(egui::DragValue::new(&mut self.settings.stale_after).range(5.0..=300.0).suffix(" s"));
                        ui.end_row();
                        ui.label(t!("Aus der Liste entfernen nach", "Remove from list after"));
                        ui.add(egui::DragValue::new(&mut self.settings.remove_after).range(20.0..=3600.0).suffix(" s"));
                        ui.end_row();
                        ui.label(t!("Netzwerkliste", "Network list"));
                        ui.checkbox(&mut self.settings.group_ssid, t!("Nach SSID gruppieren", "Group by SSID"));
                        ui.end_row();
                    });
                });

                group(ui, t!("Hersteller-Datenbank", "Vendor database"), &mut |ui| {
                    ui.horizontal(|ui| {
                        let state = if self.oui.full_db {
                            tf!("{} Einträge (vollständig)", "{} entries (complete)", self.oui.len())
                        } else {
                            tf!("{} Einträge (Basisliste)", "{} entries (basic list)", self.oui.len())
                        };
                        ui.label(RichText::new(state).color(p.weak));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let label = if self.oui_rx.is_some() { t!("Lädt …", "Loading …") } else { t!("Aktualisieren", "Update") };
                            if ui.add_enabled(self.oui_rx.is_none(), egui::Button::new(label)).clicked() {
                                oui = true;
                            }
                        });
                    });
                    ui.label(RichText::new(t!("Quelle: Wireshark manuf bzw. IEEE OUI-Register", "Source: Wireshark manuf or IEEE OUI registry")).size(12.0).color(p.weak));
                });

                group(ui, t!("Updates", "Updates"), &mut |ui| {
                    ui.checkbox(&mut self.settings.check_updates, t!("Beim Start nach neuen Versionen suchen (GitHub)", "Check for new versions at startup (GitHub)"));
                    ui.horizontal(|ui| {
                        let status = match (&self.update.latest, &self.update.error) {
                            (Some(v), _) if is_newer(v, VERSION) => tf!("Version {v} verfügbar", "Version {v} available"),
                            (Some(_), _) => tf!("v{VERSION} ist aktuell", "v{VERSION} is up to date"),
                            (None, Some(_)) => t!("Prüfung fehlgeschlagen", "Check failed").into(),
                            (None, None) if self.update.checked && self.update.rx.is_none() => t!("Noch keine Version veröffentlicht", "No release published yet").into(),
                            _ => tf!("Installiert: v{VERSION}", "Installed: v{VERSION}"),
                        };
                        ui.label(RichText::new(status).color(p.weak));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.add_enabled(self.update.rx.is_none(), egui::Button::new(t!("Jetzt prüfen", "Check now"))).clicked() {
                                check = true;
                            }
                        });
                    });
                });

                group(ui, t!("Daten", "Data"), &mut |ui| {
                    ui.horizontal_wrapped(|ui| {
                        if ui.button(t!("Netzwerkliste leeren", "Clear network list")).clicked() {
                            clear = true;
                        }
                        if ui.button(t!("Alle Markierungen entfernen", "Remove all pins")).clicked() {
                            unpin = true;
                        }
                        if ui.button(t!("📂 Datenordner öffnen", "📂 Open data folder")).clicked() {
                            let _ = std::fs::create_dir_all(settings::app_dir());
                            let _ = std::process::Command::new("explorer").arg(settings::app_dir()).spawn();
                        }
                    });
                });

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button(t!("Auf Standard zurücksetzen", "Reset to defaults")).clicked() {
                        reset = true;
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(t!("Änderungen werden automatisch gespeichert", "Changes are saved automatically")).size(12.0).color(p.weak));
                    });
                });
            });
        if !open {
            self.show_settings = false;
        }
        if oui {
            self.start_oui_download(ctx);
        }
        if check {
            self.start_update_check(ctx, true);
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
            let _ = self.scanner.tx.send(ScanCmd::SetInterval(d.interval));
            self.f.show_hidden = d.show_hidden;
            self.settings = Settings { pinned: self.settings.pinned.clone(), ..d };
        }
    }

    // ------------------------------------------------------------------ Über

    pub(super) fn about_window(&mut self, ctx: &egui::Context) {
        if !self.show_about {
            return;
        }
        let mut open = true;
        let mut check = false;
        egui::Window::new(RichText::new(t!("Über WiFi Analyzer", "About WiFi Analyzer")).strong())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(500.0)
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
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let status = match &self.update.latest {
                        Some(v) if is_newer(v, VERSION) => RichText::new(tf!("⬆ Version {v} ist verfügbar", "⬆ Version {v} is available")).color(p.accent),
                        Some(_) => RichText::new(t!("✔ Du verwendest die neueste Version", "✔ You are using the latest version")).color(p.success),
                        None => RichText::new(t!("Update-Status unbekannt", "Update status unknown")).color(p.weak),
                    };
                    ui.label(status);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.update.latest.as_deref().is_some_and(|v| is_newer(v, VERSION)) {
                            ui.hyperlink_to(t!("Herunterladen", "Download"), format!("{REPO}/releases/latest"));
                        }
                        if ui.add_enabled(self.update.rx.is_none(), egui::Button::new(t!("Nach Updates suchen", "Check for updates"))).clicked() {
                            check = true;
                        }
                    });
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
                        t!(
                            "Freie Software unter der GPL v3 – ohne jegliche Gewährleistung.",
                            "Free software under the GPL v3 – without any warranty."
                        )
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
    }
}
