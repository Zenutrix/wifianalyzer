//! Lade-Popup und Berechtigungsdialog.

use std::time::Duration;

use eframe::egui::{self, Align, Align2, Color32, FontId, Layout, Margin, RichText, Sense, Stroke, Vec2};

use super::{WifiApp, AUTHOR, VERSION};
use crate::perm;
use crate::theme::pal_ctx;
use crate::widgets::primary_button;
use crate::wlan::{self, ScanCmd};
use crate::t;

impl WifiApp {
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
                    p.text(sx, Align2::CENTER_CENTER, "\u{E182}", FontId::proportional(14.0), Color32::from_rgb(63, 185, 80).gamma_multiply(fade));
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
                    "Windows gibt BSSIDs, Signalstärken und Kanäle nur an Apps heraus, die auf den Standort zugreifen dürfen (WLAN-Netze verraten den Aufenthaltsort). Der WiFi Analyzer ermittelt selbst keine Position und überträgt keine Messdaten.",
                    "Windows only hands out BSSIDs, signal strengths and channels to apps that may access the location (Wi-Fi networks reveal where you are). WiFi Analyzer does not determine your position and transmits no measurement data."
                ));
                ui.add_space(12.0);
                egui::Frame::group(ui.style()).rounding(8.0).inner_margin(Margin::same(12.0)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    for (state, text) in &checklist {
                        ui.horizontal(|ui| {
                            let (icon, col) = match state {
                                Some(true) => ("\u{E182}", p.success),
                                Some(false) => ("\u{E4F6}", p.danger),
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
                    if primary_button(ui, t!("\u{E270}  Standort-Einstellungen öffnen", "\u{E270}  Open location settings"), p.accent).clicked() {
                        open_settings = true;
                    }
                    if ui.button(t!("\u{E094}  Erneut anfragen", "\u{E094}  Ask again")).clicked() {
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

}
