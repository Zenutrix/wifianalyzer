//! Grundriss-Messung als geführter Ablauf: Projekt → Pläne → Maßstab → Messen.
//!
//! Jeder Schritt hat eine eigene, aufgeräumte Ansicht. Die Schrittleiste oben zeigt, wo man steht,
//! und lässt zurückspringen. Gemessen wird erst im letzten Schritt – mit einer Werkzeugleiste im Plan.

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};

use crate::app::plan::{CanvasMode, PlanStep, ScaleDraft};
use crate::app::WifiApp;
use crate::measure::Measurement;
use crate::theme::pal;
use crate::widgets::{card, outline_button, primary_button};
use crate::{t, tf};

const STEPS: [PlanStep; 4] = [PlanStep::Project, PlanStep::Plans, PlanStep::Scale, PlanStep::Measure];

fn step_label(s: PlanStep) -> &'static str {
    match s {
        PlanStep::Project => t!("Projekt", "Project"),
        PlanStep::Plans => t!("Pläne", "Plans"),
        PlanStep::Scale => t!("Maßstab", "Scale"),
        PlanStep::Measure => t!("Messen", "Measure"),
    }
}

/// Höhe der Zeile mit „Zurück“ / „Weiter“
const NAV_H: f32 = 46.0;

impl WifiApp {
    pub(crate) fn page_plan_survey(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        // SSID vorbelegen: verbundene, sonst die stärkste
        if self.meas_ssid.is_none() {
            let choices = self.ssid_choices();
            self.meas_ssid = self.conn.as_ref().map(|c| c.ssid.clone()).or_else(|| choices.first().map(|c| c.0.clone()));
        }
        if self.meas.is_none() {
            self.meas = Some(Measurement::draft(self.meas_ssid.clone().unwrap_or_default(), Vec::new()));
            self.plan_step = PlanStep::Project;
        }
        self.pdf_pick_window(&ctx);
        self.take_dropped_plans(&ctx);

        let has_plans = self.meas.as_ref().is_some_and(|m| !m.plans.is_empty());
        // Maßstab und Messen brauchen mindestens einen Grundriss
        if !has_plans && matches!(self.plan_step, PlanStep::Scale | PlanStep::Measure) {
            self.plan_step = PlanStep::Plans;
        }
        if self.plan_step != PlanStep::Scale {
            self.plan_scale = None;
        }
        if self.plan_fullscreen && self.plan_step != PlanStep::Measure {
            self.set_plan_fullscreen(&ctx, false);
        }
        if !self.plan_fullscreen {
            self.plan_header(ui, has_plans);
        }
        match self.plan_step {
            PlanStep::Project => self.plan_step_project(ui),
            PlanStep::Plans => self.plan_step_plans(ui),
            PlanStep::Scale => self.plan_step_scale(ui),
            PlanStep::Measure => self.plan_step_measure(ui),
        }
        self.plan_drop_overlay(&ctx);
    }

    /// Titel mit Projektname links, Schrittleiste rechts
    fn plan_header(&mut self, ui: &mut Ui, has_plans: bool) {
        let p = pal(ui);
        let Some(m) = &self.meas else { return };
        let all_scaled = has_plans && m.plans.iter().all(|f| f.width_m.is_some());
        let sub = match (m.project.trim(), m.ssid.as_str()) {
            ("", "") => t!("Neues Projekt", "New project").to_string(),
            ("", s) => tf!("WLAN „{s}“", "Wi-Fi \"{s}\""),
            (pr, "") => pr.to_string(),
            (pr, s) => tf!("{pr} · WLAN „{s}“", "{pr} · Wi-Fi \"{s}\""),
        };
        let cur = self.plan_step;
        let ssid_ok = !m.ssid.is_empty();
        let mut goto = None;
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(t!("Grundriss-Messung", "Floor plan survey")).family(crate::theme::semibold()).size(23.0).color(p.text));
                ui.label(RichText::new(sub).color(p.weak));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                // rechts nach links: Messen, Maßstab, Pläne, Projekt
                for (i, s) in STEPS.iter().enumerate().rev() {
                    let done = match s {
                        PlanStep::Project => ssid_ok && cur != PlanStep::Project,
                        PlanStep::Plans => has_plans && cur != PlanStep::Plans,
                        PlanStep::Scale => all_scaled && cur != PlanStep::Scale,
                        PlanStep::Measure => false,
                    };
                    let enabled = match s {
                        PlanStep::Project | PlanStep::Plans => true,
                        PlanStep::Scale | PlanStep::Measure => has_plans && ssid_ok,
                    };
                    if step_chip(ui, i + 1, step_label(*s), *s == cur, done, enabled).clicked() && enabled {
                        goto = Some(*s);
                    }
                    if i > 0 {
                        let (r, _) = ui.allocate_exact_size(Vec2::new(26.0, 30.0), Sense::hover());
                        ui.painter().line_segment([Pos2::new(r.min.x + 4.0, r.center().y), Pos2::new(r.max.x - 4.0, r.center().y)], Stroke::new(2.0f32, if done || STEPS[i - 1] == cur { p.border } else { p.border.gamma_multiply(0.6) }));
                    }
                }
            });
        });
        ui.add_space(10.0);
        if let Some(s) = goto {
            self.goto_plan_step(s);
        }
    }

    pub(super) fn goto_plan_step(&mut self, s: PlanStep) {
        self.plan_step = s;
        self.plan_new_confirm = false;
        if s == PlanStep::Scale {
            // mit dem ersten Stockwerk ohne Maßstab beginnen
            let first = self.meas.as_ref().and_then(|m| m.plans.iter().position(|f| f.width_m.is_none()));
            if let Some(i) = first {
                self.show_floor(i);
            }
            self.plan_scale = Some(ScaleDraft::default());
        }
        // vor dem Start mit dem ersten Stockwerk beginnen
        if s == PlanStep::Measure && self.meas.as_ref().is_some_and(|m| m.draft) {
            self.show_floor(0);
        }
    }

    /// Zeile mit „Zurück“ links und Hauptaktion rechts
    fn plan_nav(&mut self, ui: &mut Ui, back: Option<PlanStep>, next: impl FnOnce(&mut Self, &mut Ui)) {
        let mut go_back = false;
        ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), NAV_H), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            if back.is_some() && ui.add(egui::Button::new(t!("\u{E058}  Zurück", "\u{E058}  Back")).min_size(Vec2::new(0.0, 30.0))).clicked() {
                go_back = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| next(self, ui));
        });
        if let (true, Some(b)) = (go_back, back) {
            self.goto_plan_step(b);
        }
    }

    // ---------------------------------------------------------------- 1. Projekt

    fn plan_step_project(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        let choices = self.ssid_choices();
        let connected_ssid = self.conn.as_ref().map(|c| c.ssid.clone());
        let Some(m) = self.meas.as_ref() else { return };
        let draft = m.draft;
        let running = m.running;
        let (mut project, mut ssid) = (m.project.clone(), m.ssid.clone());
        let started = m.started_wall.format("%d.%m.%Y %H:%M").to_string();
        let n_points = m.points.len();
        let (mut ping_on, mut speed_on) = (self.settings.point_ping, self.settings.point_speed);
        let (mut next, mut new_project) = (false, false);
        let confirm = self.plan_new_confirm;
        let wide = ui.available_width() >= 1000.0;
        let form = |ui: &mut Ui, project: &mut String, ssid: &mut String, ping_on: &mut bool, speed_on: &mut bool, next: &mut bool, new_project: &mut bool| {
            card(ui, Some(if draft { t!("Neues Projekt", "New project") } else { t!("Projekt", "Project") }), |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(t!("Projektname", "Project name")).color(p.weak));
                ui.add(egui::TextEdit::singleline(project).desired_width(f32::INFINITY).hint_text(t!("z. B. Hotel Post – Haus B", "e.g. Hotel Post – building B")));
                ui.label(RichText::new(t!("Steht im Dateinamen und im Bericht.", "Used in the file name and the report.")).small().color(p.weak));
                ui.add_space(10.0);
                ui.label(RichText::new(t!("Welches WLAN wird gemessen?", "Which Wi-Fi is surveyed?")).color(p.weak));
                ui.add_enabled_ui(draft, |ui| {
                    egui::ComboBox::from_id_salt("plan_ssid")
                        .width(ui.available_width().min(420.0))
                        .selected_text(if ssid.is_empty() { t!("– bitte wählen –", "– please choose –") } else { ssid.as_str() })
                        .show_ui(ui, |ui| {
                            for (s, r) in &choices {
                                let tag = if connected_ssid.as_deref() == Some(s.as_str()) { t!("  \u{E2E2} verbunden", "  \u{E2E2} connected") } else { "" };
                                if ui.selectable_label(s == ssid, format!("{s}  ({r} dBm){tag}")).clicked() {
                                    *ssid = s.clone();
                                }
                            }
                        });
                });
                ui.label(
                    RichText::new(if draft {
                        t!("Am besten das WLAN, mit dem der Laptop verbunden ist – dann werden auch Roaming und Datenrate gemessen.", "Ideally the Wi-Fi the laptop is connected to – then roaming and data rate are measured too.")
                    } else {
                        t!("Das WLAN einer begonnenen Messung lässt sich nicht mehr ändern.", "The Wi-Fi of a started survey can't be changed.")
                    })
                    .small()
                    .color(p.weak),
                );
                ui.add_space(10.0);
                ui.label(RichText::new(t!("An jedem Messpunkt zusätzlich messen", "Also measure at every point")).color(p.weak));
                ui.checkbox(ping_on, t!("Latenz – Ping zum Router und ins Internet (ca. 3 s)", "Latency – ping to the router and the internet (approx. 3 s)"));
                ui.checkbox(speed_on, t!("Speedtest – Download/Upload (ca. 12 s, bis zu 70 MB)", "Speed test – download/upload (approx. 12 s, up to 70 MB)"));
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let label = if draft { t!("Weiter: Pläne laden  \u{E06C}", "Next: load plans  \u{E06C}") } else { t!("Weiter: Pläne  \u{E06C}", "Next: plans  \u{E06C}") };
                    if ui.add_enabled_ui(!ssid.is_empty(), |ui| primary_button(ui, label, p.accent)).inner.clicked() {
                        *next = true;
                    }
                    if !draft {
                        ui.add_space(8.0);
                        let label = if confirm { t!("Wirklich neues Projekt?", "Really start a new project?") } else { t!("Neues Projekt beginnen", "Start a new project") };
                        let tip = if running { t!("Erst die laufende Messung stoppen", "Stop the running survey first") } else { t!("Die aktuelle Messung bleibt gespeichert", "The current survey stays saved") };
                        if ui.add_enabled(!running, egui::Button::new(label)).on_hover_text(tip).on_disabled_hover_text(tip).clicked() {
                            *new_project = true;
                        }
                    }
                });
                if !draft {
                    ui.add_space(6.0);
                    ui.label(RichText::new(tf!("Begonnen am {started} · {n_points} Messpunkte", "Started on {started} · {n_points} points")).small().color(p.weak));
                }
            });
        };
        if wide {
            let w = ui.available_width();
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(Vec2::new((w * 0.48).min(620.0), 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    form(ui, &mut project, &mut ssid, &mut ping_on, &mut speed_on, &mut next, &mut new_project);
                });
                ui.add_space(12.0);
                ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    card(ui, None, |ui| {
                        ui.set_width(ui.available_width());
                        self.recent_surveys_ui(ui, true);
                    });
                });
            });
        } else {
            ui.set_max_width(640.0);
            form(ui, &mut project, &mut ssid, &mut ping_on, &mut speed_on, &mut next, &mut new_project);
            ui.add_space(12.0);
            card(ui, None, |ui| {
                ui.set_width(ui.available_width());
                self.recent_surveys_ui(ui, true);
            });
        }
        self.settings.point_ping = ping_on;
        self.settings.point_speed = speed_on;
        let mut changed = false;
        if let Some(m) = self.meas.as_mut() {
            if m.project != project {
                m.project = project;
                changed = !m.draft;
            }
            if m.draft && m.ssid != ssid {
                m.ssid = ssid.clone();
                self.meas_ssid = Some(ssid);
            }
        }
        if changed {
            self.survey_changed();
        }
        if new_project {
            if confirm || self.meas.as_ref().is_none_or(|m| m.points.is_empty()) {
                self.close_plan_project();
            } else {
                self.plan_new_confirm = true;
            }
        }
        if next {
            self.goto_plan_step(PlanStep::Plans);
        }
    }

    /// Offene Grundriss-Messung schließen (bleibt gespeichert) und mit einem neuen Projekt beginnen
    pub(super) fn close_plan_project(&mut self) {
        self.autosave_survey(true);
        self.meas = None;
        self.meas_file = None;
        self.meas_discard_confirm = false;
        self.plan_new_confirm = false;
        self.plan_sel = None;
        self.plan_step = PlanStep::Project;
        self.apply_scan_interval();
    }

    // ---------------------------------------------------------------- 2. Pläne

    fn plan_step_plans(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        let ctx = ui.ctx().clone();
        let n = self.meas.as_ref().map_or(0, |m| m.plans.len());
        let body_h = ui.available_height() - NAV_H - 4.0;
        egui::ScrollArea::vertical().id_salt("plan_step_plans").max_height(body_h).auto_shrink([false, false]).show(ui, |ui| {
            // Ablagefläche
            let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
            let h = if n == 0 { 210.0 } else { 120.0 };
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), h), Sense::click());
            let hot = hovering || resp.hovered();
            ui.painter().rect_filled(rect, 12.0, if hot { p.accent.gamma_multiply(0.12) } else { p.surface });
            dashed_rect(ui.painter(), rect.shrink(1.0), if hot { p.accent } else { p.border });
            let c = rect.center();
            let (dy, big) = if n == 0 { (-34.0, 40.0) } else { (-22.0, 28.0) };
            ui.painter().text(c + Vec2::new(0.0, dy), Align2::CENTER_CENTER, "\u{E61E}", FontId::proportional(big), if hot { p.accent } else { p.weak });
            let title = if n == 0 { t!("Grundriss hierher ziehen", "Drag a floor plan here") } else { t!("Weiteres Stockwerk hierher ziehen", "Drag another floor here") };
            ui.painter().text(c + Vec2::new(0.0, dy + big * 0.5 + 16.0), Align2::CENTER_CENTER, title, FontId::new(16.0, crate::theme::semibold()), p.text);
            ui.painter().text(
                c + Vec2::new(0.0, dy + big * 0.5 + 38.0),
                Align2::CENTER_CENTER,
                t!("oder klicken, um eine Datei zu wählen · PNG, JPG oder PDF · bei mehrseitigen PDFs die Seiten auswählen", "or click to choose a file · PNG, JPG or PDF · pick the pages of multi-page PDFs"),
                FontId::proportional(12.5),
                p.weak,
            );
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                self.load_plan(&ctx, true);
            }
            ui.add_space(12.0);

            // Pläne vom letzten Projekt übernehmen
            if n == 0 && !self.last_plans.is_empty() {
                let names: Vec<String> = self.last_plans.iter().map(|f| f.title().to_string()).collect();
                let mut reuse = false;
                card(ui, None, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("\u{E102}").size(20.0).color(p.accent));
                        ui.vertical(|ui| {
                            ui.label(RichText::new(t!("Gleiches Gebäude wie zuletzt?", "Same building as last time?")).strong().color(p.text));
                            ui.label(RichText::new(names.join(" · ")).color(p.weak));
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(tf!("Pläne übernehmen ({})", "Use these plans ({})", names.len())).clicked() {
                                reuse = true;
                            }
                        });
                    });
                });
                if reuse {
                    let plans = self.last_plans.clone();
                    if let Some(m) = self.meas.as_mut() {
                        m.plans = plans;
                        m.floor = 0;
                    }
                    self.plan_tex = None;
                    self.heat_tex = None;
                    self.survey_changed();
                }
                ui.add_space(12.0);
            }

            if n > 0 {
                ui.label(RichText::new(t!("Stockwerke – Namen anklicken zum Umbenennen, Pfeile zum Sortieren", "Floors – click a name to rename, arrows to reorder")).color(p.weak));
                ui.add_space(6.0);
                self.floor_cards(ui);
            }
        });
        let all_scaled = self.meas.as_ref().is_some_and(|m| !m.plans.is_empty() && m.plans.iter().all(|f| f.width_m.is_some()));
        self.plan_nav(ui, Some(PlanStep::Project), |me, ui| {
            let p = pal(ui);
            let (label, target) = if all_scaled { (t!("Weiter: Messen  \u{E06C}", "Next: measure  \u{E06C}"), PlanStep::Measure) } else { (t!("Weiter: Maßstab setzen  \u{E06C}", "Next: set the scale  \u{E06C}"), PlanStep::Scale) };
            if ui.add_enabled_ui(n > 0, |ui| primary_button(ui, label, p.accent)).inner.clicked() {
                me.goto_plan_step(target);
            }
            if all_scaled && ui.button(t!("Maßstab prüfen", "Check the scale")).clicked() {
                me.goto_plan_step(PlanStep::Scale);
            }
            if n == 0 {
                ui.label(RichText::new(t!("Mindestens einen Grundriss laden", "Load at least one floor plan")).color(p.weak));
            }
        });
    }

    /// Kacheln der Stockwerke: Vorschau, Name, Maßstab, Sortieren, Ersetzen, Entfernen
    fn floor_cards(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        let ctx = ui.ctx().clone();
        let n = self.meas.as_ref().map_or(0, |m| m.plans.len());
        let counts: Vec<usize> = self.meas.as_ref().map(|m| (0..n).map(|f| m.points.iter().filter(|pt| pt.floor == f && pt.pos.is_some()).count()).collect()).unwrap_or_default();
        let thumbs: Vec<Option<(egui::TextureId, [usize; 2])>> = (0..n).map(|i| self.plan_thumb(&ctx, i)).collect();
        let confirm = self.floor_remove_confirm;
        let (mut mv, mut remove, mut ask, mut replace, mut renamed) = (None, None, None, None, false);
        let Some(m) = self.meas.as_mut() else { return };
        let plans_after = ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(12.0, 12.0);
            for i in 0..n {
                let fp = &mut m.plans[i];
                egui::Frame::none().fill(p.surface).stroke(Stroke::new(1.0f32, p.border)).rounding(10.0).inner_margin(10.0).show(ui, |ui| {
                    ui.vertical(|ui| {
                    ui.set_width(232.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(tf!("Stockwerk {}", "Floor {}", i + 1)).small().color(p.weak));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let tip = if counts[i] > 0 { tf!("Entfernen – {} Messpunkte verlieren ihre Position", "Remove – {} points lose their position", counts[i]) } else { t!("Stockwerk entfernen", "Remove floor").to_string() };
                            if confirm == Some(i) {
                                if ui.small_button(RichText::new(t!("Wirklich?", "Sure?")).color(p.danger)).on_hover_text(tip).clicked() {
                                    remove = Some(i);
                                }
                            } else if ui.small_button("\u{E4A6}").on_hover_text(tip).clicked() {
                                if counts[i] > 0 {
                                    ask = Some(i);
                                } else {
                                    remove = Some(i);
                                }
                            }
                            if ui.add_enabled(i + 1 < n, egui::Button::new("\u{E06C}").small()).on_hover_text(t!("Nach hinten", "Move right")).clicked() {
                                mv = Some((i, i + 1));
                            }
                            if ui.add_enabled(i > 0, egui::Button::new("\u{E058}").small()).on_hover_text(t!("Nach vorne", "Move left")).clicked() {
                                mv = Some((i, i - 1));
                            }
                        });
                    });
                    // Vorschau
                    let (r, _) = ui.allocate_exact_size(Vec2::new(232.0, 150.0), Sense::hover());
                    ui.painter().rect_filled(r, 6.0, Color32::WHITE);
                    if let Some((tex, size)) = thumbs[i] {
                        let aspect = size[0] as f32 / size[1].max(1) as f32;
                        let inner = r.shrink(4.0);
                        let (mut w, mut h) = (inner.width(), inner.width() / aspect);
                        if h > inner.height() {
                            h = inner.height();
                            w = h * aspect;
                        }
                        ui.painter().image(tex, Rect::from_center_size(inner.center(), Vec2::new(w, h)), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                    }
                    ui.add_space(4.0);
                    let out = egui::TextEdit::singleline(&mut fp.label).desired_width(232.0).hint_text(fp.name.as_str()).font(egui::TextStyle::Body).show(ui);
                    // beim Anklicken alles markieren – Tippen ersetzt dann den Dateinamen
                    if out.response.gained_focus() {
                        let mut st = out.state.clone();
                        let n = fp.label.chars().count();
                        st.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(n))));
                        st.store(ui.ctx(), out.response.id);
                    }
                    if out.response.changed() {
                        renamed = true;
                    }
                    ui.horizontal(|ui| {
                        match fp.width_m {
                            Some(w) => ui.label(RichText::new(tf!("\u{E182} {w:.1} m breit", "\u{E182} {w:.1} m wide")).color(p.success)),
                            None => ui.label(RichText::new(t!("\u{E6B8} Maßstab fehlt", "\u{E6B8} No scale yet")).color(p.warning)),
                        };
                        if counts[i] > 0 {
                            ui.label(RichText::new(tf!("· {} Punkte", "· {} points", counts[i])).color(p.weak));
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button(t!("Ersetzen …", "Replace …")).on_hover_text(t!("Anderen Plan für dieses Stockwerk laden – Messpunkte bleiben", "Load another plan for this floor – points stay")).clicked() {
                                replace = Some(i);
                            }
                        });
                    });
                    });
                });
            }
            if let Some((a, b)) = mv {
                m.move_floor(a, b);
            }
            m.plans.clone()
        })
        .inner;
        if ask.is_some() || remove.is_some() || mv.is_some() {
            self.floor_remove_confirm = ask;
        }
        if renamed || mv.is_some() {
            self.last_plans = plans_after;
            self.plan_tex = None;
            self.heat_tex = None;
            self.survey_changed();
        }
        if let Some(i) = remove {
            if let Some(m) = self.meas.as_mut() {
                m.floor = i;
            }
            self.remove_plan();
        }
        if let Some(i) = replace {
            if let Some(m) = self.meas.as_mut() {
                m.floor = i;
            }
            self.load_plan(&ctx, false);
        }
    }

    // ---------------------------------------------------------------- 3. Maßstab

    fn plan_step_scale(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        if self.plan_scale.is_none() {
            self.plan_scale = Some(ScaleDraft::default());
        }
        let Some(m) = self.meas.as_ref() else { return };
        let floors: Vec<(String, Option<f32>)> = m.plans.iter().map(|f| (f.title().to_string(), f.width_m)).collect();
        let cur = m.floor;
        let cur_scale = m.plan().and_then(|f| f.width_m.map(|w| (w, f.scale_len_m)));
        let plan_px = self.plan_tex.as_ref().map(|t| t.size);
        let missing = floors.iter().filter(|f| f.1.is_none()).count();
        let body_h = ui.available_height() - NAV_H - 4.0;
        let side_w = 300.0;
        let (mut pick, mut apply, mut redo) = (None, None, false);
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(Vec2::new(side_w, body_h), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_min_height(body_h);
                card(ui, Some(t!("Stockwerke", "Floors")), |ui| {
                    ui.set_width(ui.available_width());
                    for (i, (title, w)) in floors.iter().enumerate() {
                        let sel = i == cur;
                        let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::click());
                        if sel {
                            ui.painter().rect_filled(r, 6.0, p.accent.gamma_multiply(0.18));
                        } else if resp.hovered() {
                            ui.painter().rect_filled(r, 6.0, p.surface2);
                        }
                        let (icon, col) = if w.is_some() { ("\u{E182}", p.success) } else { ("\u{E6B8}", p.warning) };
                        ui.painter().text(Pos2::new(r.min.x + 12.0, r.center().y), Align2::LEFT_CENTER, icon, FontId::proportional(15.0), col);
                        let tp = ui.painter().with_clip_rect(Rect::from_min_max(Pos2::new(r.min.x + 32.0, r.min.y), Pos2::new(r.max.x - 70.0, r.max.y)));
                        tp.text(Pos2::new(r.min.x + 32.0, r.center().y), Align2::LEFT_CENTER, title, FontId::proportional(14.0), p.text);
                        let right = w.map(|w| format!("{w:.1} m")).unwrap_or_else(|| t!("fehlt", "missing").to_string());
                        ui.painter().text(Pos2::new(r.max.x - 10.0, r.center().y), Align2::RIGHT_CENTER, right, FontId::proportional(12.5), if w.is_some() { p.weak } else { p.warning });
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            pick = Some(i);
                        }
                    }
                });
                ui.add_space(10.0);
                card(ui, Some(t!("So setzt du den Maßstab", "How to set the scale")), |ui| {
                    ui.set_width(ui.available_width());
                    let Some(sd) = self.plan_scale.as_mut() else { return };
                    let stage = match (sd.a, sd.b) {
                        (None, _) => 0,
                        (Some(_), None) => 1,
                        _ => 2,
                    };
                    for (k, txt) in [
                        t!("Anfang einer bekannten Strecke anklicken", "Click the start of a known distance"),
                        t!("Ende der Strecke anklicken", "Click the end of the distance"),
                        t!("Länge in Metern eingeben", "Enter the length in metres"),
                    ]
                    .iter()
                    .enumerate()
                    {
                        let (mark, col) = if k < stage { ("\u{E182}".to_string(), p.success) } else { (format!("{}.", k + 1), if k == stage { p.accent } else { p.weak }) };
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(mark).strong().color(col));
                            ui.label(RichText::new(*txt).color(if k == stage { p.text } else { p.weak }));
                        });
                    }
                    ui.add_space(6.0);
                    if stage == 2 {
                        ui.horizontal(|ui| {
                            let r = ui.add(egui::TextEdit::singleline(&mut sd.meters).desired_width(80.0).hint_text(t!("z. B. 0,9", "e.g. 0.9")));
                            if sd.meters.is_empty() && !r.has_focus() {
                                r.request_focus();
                            }
                            ui.label("m");
                            let parsed = sd.meters.trim().replace(',', ".").parse::<f32>().ok().filter(|v| *v > 0.05 && *v < 10_000.0);
                            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                            if ui.add_enabled_ui(parsed.is_some(), |ui| primary_button(ui, t!("Übernehmen", "Apply"), p.accent)).inner.clicked() || (enter && parsed.is_some()) {
                                apply = parsed;
                            }
                        });
                        if ui.link(RichText::new(t!("Strecke neu ansetzen", "Draw the distance again")).small()).clicked() {
                            redo = true;
                        }
                    } else {
                        ui.label(
                            RichText::new(t!(
                                "Gut geeignet: eine lange Wand mit Bemaßung im Plan. Eine Tür ist meist 0,80–1,00 m breit. Mausrad = zoomen für genaues Setzen.",
                                "Good choice: a long wall with a dimension on the plan. A door is usually 0.80–1.00 m wide. Mouse wheel = zoom for precise placement."
                            ))
                            .small()
                            .color(p.weak),
                        );
                    }
                    if let (Some((w, len)), Some(px)) = (cur_scale, plan_px) {
                        ui.add_space(8.0);
                        let hm = w * px[1] as f32 / px[0].max(1) as f32;
                        let from = len.map(|l| tf!(" (Strecke {l} m)", " (distance {l} m)")).unwrap_or_default();
                        ui.label(RichText::new(tf!("\u{E182} Plan ist {w:.1} × {hm:.1} m{from}", "\u{E182} Plan is {w:.1} × {hm:.1} m{from}")).color(p.success));
                        ui.label(RichText::new(t!("Neue Strecke anklicken, um ihn zu ändern.", "Click a new distance to change it.")).small().color(p.weak));
                    }
                });
            });
            ui.add_space(12.0);
            let avail = Vec2::new(ui.available_width(), body_h);
            ui.allocate_ui_with_layout(avail, egui::Layout::top_down(egui::Align::Min), |ui| {
                self.plan_canvas(ui, avail, CanvasMode::Scale);
            });
        });
        if redo {
            self.plan_scale = Some(ScaleDraft::default());
        }
        if let Some(meters) = apply {
            self.apply_scale(meters);
        }
        if let Some(i) = pick {
            self.show_floor(i);
            self.plan_scale = Some(ScaleDraft::default());
        }
        self.plan_nav(ui, Some(PlanStep::Plans), |me, ui| {
            let p = pal(ui);
            if missing == 0 {
                if primary_button(ui, t!("Weiter: Messen  \u{E06C}", "Next: measure  \u{E06C}"), p.accent).clicked() {
                    me.goto_plan_step(PlanStep::Measure);
                }
            } else {
                if ui
                    .button(t!("Ohne Maßstab weiter", "Continue without scale"))
                    .on_hover_text(t!("Die Heatmap nimmt dann 40 m Planbreite an – die Reichweite stimmt nicht genau.", "The heatmap then assumes a 40 m wide plan – the coverage won't be accurate."))
                    .clicked()
                {
                    me.goto_plan_step(PlanStep::Measure);
                }
                let txt = if missing == 1 { t!("1 Stockwerk ohne Maßstab", "1 floor without scale").to_string() } else { tf!("{missing} Stockwerke ohne Maßstab", "{missing} floors without scale") };
                ui.label(RichText::new(txt).color(p.warning));
            }
        });
    }

    // ---------------------------------------------------------------- 4. Messen

    fn plan_step_measure(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        let Some(m) = self.meas.as_ref() else { return };
        let (draft, running) = (m.draft, m.running);
        let ssid = m.ssid.clone();
        let floors: Vec<(String, usize)> = m.plans.iter().enumerate().map(|(i, f)| (f.title().to_string(), m.points.iter().filter(|pt| pt.floor == i && pt.pos.is_some()).count())).collect();
        let mut floor = m.floor;
        let has_points = !m.points.is_empty();
        let live_now: Option<i32> = self.conn.as_ref().filter(|c| c.ssid == ssid).and_then(|c| c.rssi).or(self.best_ap_of(&ssid).map(|b| b.info.rssi));
        let connected_now = self.conn.as_ref().is_some_and(|c| c.ssid == ssid);
        let confirm = self.meas_discard_confirm;

        // Enter = Messpunkt ohne Position erfassen (solange nicht gerade getippt wird)
        if running && self.meas_point.is_none() && self.plan_sel.is_none() && !ui.ctx().wants_keyboard_input() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            self.open_point_dialog();
        }

        let (mut start, mut stop, mut close) = (false, false, false);
        let (mut export_all, mut export_points, mut export_report, mut export_pdf, mut save_as) = (false, false, false, false, false);
        let (mut ping_on, mut speed_on, mut full_table) = (self.settings.point_ping, self.settings.point_speed, self.plan_full_table);
        // Aktionsleiste: Start/Stopp und Live-Wert links, Stockwerke in der Mitte, Export und Menü rechts
        egui::Frame::none().fill(p.surface).stroke(Stroke::new(1.0f32, p.border)).rounding(10.0).inner_margin(egui::Margin::symmetric(10.0, 7.0)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                if running {
                    if outline_button(ui, t!("\u{E46C}  Messung stoppen", "\u{E46C}  Stop survey"), p.danger).clicked() {
                        stop = true;
                    }
                } else if draft {
                    if primary_button(ui, t!("\u{E3D0}  Messung starten", "\u{E3D0}  Start survey"), p.accent).clicked() {
                        start = true;
                    }
                } else if primary_button(ui, t!("\u{E3D0}  Erneut messen", "\u{E3D0}  Measure again"), p.accent)
                    .on_hover_text(t!("Neue Messung mit denselben Plänen und demselben Projekt (eigene Datei)", "New survey with the same plans and project (separate file)"))
                    .clicked()
                {
                    start = true;
                }
                ui.add_space(6.0);
                if draft {
                    ui.label(RichText::new(tf!("WLAN „{ssid}“", "Wi-Fi \"{ssid}\"")).color(p.weak));
                } else {
                    self.plan_live_inline(ui, live_now, connected_now);
                }
                if floors.len() > 1 {
                    ui.separator();
                    ui.label(RichText::new("\u{E466}").color(p.weak));
                    let labels: Vec<String> = floors.iter().map(|(t, c)| if *c > 0 { format!("{t}  {c}") } else { t.clone() }).collect();
                    let opts: Vec<(usize, &str)> = labels.iter().enumerate().map(|(i, s)| (i, s.as_str())).collect();
                    crate::widgets::segmented(ui, &mut floor, &opts);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.menu_button("\u{E1FE}", |ui| {
                        ui.set_min_width(280.0);
                        if !draft && ui.button(t!("\u{E248} Speichern unter …", "\u{E248} Save as …")).on_hover_text(t!("Wird ohnehin automatisch gespeichert", "Saved automatically anyway")).clicked() {
                            save_as = true;
                            ui.close_menu();
                        }
                        if !draft {
                            ui.checkbox(&mut full_table, t!("Messpunkte als Tabelle unter dem Plan", "Points as a table below the plan"));
                        }
                        ui.separator();
                        ui.label(RichText::new(t!("An jedem Messpunkt zusätzlich", "Also at every point")).small().color(p.weak));
                        ui.checkbox(&mut ping_on, t!("Latenz (ca. 3 s)", "Latency (approx. 3 s)"));
                        ui.checkbox(&mut speed_on, t!("Speedtest (ca. 12 s, bis 70 MB)", "Speed test (approx. 12 s, up to 70 MB)"));
                        ui.separator();
                        let label = if confirm { t!("\u{E4F6} Wirklich schließen?", "\u{E4F6} Really close?") } else { t!("\u{E4F6} Projekt schließen", "\u{E4F6} Close project") };
                        if ui.add_enabled(!running, egui::Button::new(label)).on_hover_text(t!("Die Messung bleibt gespeichert", "The survey stays saved")).clicked() {
                            close = true;
                            if confirm || !has_points {
                                ui.close_menu();
                            }
                        }
                    })
                    .response
                    .on_hover_text(t!("Weitere Aktionen", "More actions"));
                    if !draft {
                        ui.menu_button(t!("\u{E20C} Export", "\u{E20C} Export"), |ui| {
                            if ui.button(t!("\u{E702} Bericht für Kunden als PDF", "\u{E702} Customer report as PDF")).clicked() {
                                export_pdf = true;
                                ui.close_menu();
                            }
                            if ui.button(t!("\u{E288} Bericht als Webseite (HTML)", "\u{E288} Report as web page (HTML)")).clicked() {
                                export_report = true;
                                ui.close_menu();
                            }
                            if ui.button(t!("\u{E198} Messpunkte als Liste (CSV / Excel)", "\u{E198} Points as list (CSV / Excel)")).clicked() {
                                export_points = true;
                                ui.close_menu();
                            }
                            if ui.button(t!("\u{E154} Alle Messwerte (CSV / Excel)", "\u{E154} All samples (CSV / Excel)")).clicked() {
                                export_all = true;
                                ui.close_menu();
                            }
                        });
                    }
                    if let Some(f) = &self.meas_file {
                        let name = f.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                        let (txt, col) = match (&self.meas_save_error, self.meas_dirty_since.is_some()) {
                            (Some(_), _) => (t!("\u{E4E0} nicht gespeichert", "\u{E4E0} not saved").to_string(), p.danger),
                            (None, true) => (t!("\u{E248} speichert …", "\u{E248} saving …").to_string(), p.weak),
                            (None, false) => (t!("\u{E182} gespeichert", "\u{E182} saved").to_string(), p.weak),
                        };
                        if ui.link(RichText::new(txt).small().color(col)).on_hover_text(tf!("{name}\nKlick: Ordner öffnen", "{name}\nClick: open folder")).clicked() {
                            if let Some(dir) = f.parent() {
                                crate::app::survey::open_path(dir);
                            }
                        }
                    }
                });
            });
        });
        ui.add_space(8.0);

        self.settings.point_ping = ping_on;
        self.settings.point_speed = speed_on;
        self.plan_full_table = full_table;
        if !close {
            self.meas_discard_confirm = false;
        }
        if floor != self.meas.as_ref().map_or(0, |m| m.floor) {
            self.show_floor(floor);
        }

        // Plan mit Werkzeugleiste, daneben (breit) oder darunter die Messpunkte
        let avail_w = ui.available_width();
        let avail_h = ui.available_height() - 4.0;
        if draft {
            if self.plan_canvas(ui, Vec2::new(avail_w, avail_h), CanvasMode::Measure) {
                start = true;
            }
        } else if !self.plan_full_table && avail_w >= 900.0 {
            let list_w = 290.0;
            ui.horizontal_top(|ui| {
                let size = Vec2::new(avail_w - list_w - 12.0, avail_h);
                ui.allocate_ui_with_layout(size, egui::Layout::top_down(egui::Align::Min), |ui| {
                    self.plan_canvas(ui, size, CanvasMode::Measure);
                });
                ui.allocate_ui_with_layout(Vec2::new(list_w, avail_h), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_min_size(Vec2::new(list_w, avail_h));
                    self.plan_points_list(ui);
                });
            });
        } else {
            let h = (avail_h - 200.0).clamp(260.0, (avail_h * 0.70).max(260.0));
            self.plan_canvas(ui, Vec2::new(avail_w, h), CanvasMode::Measure);
            ui.add_space(8.0);
            self.points_table(ui);
        }

        if start {
            self.start_survey(ssid, true);
        }
        if stop {
            self.stop_measurement();
        }
        if close {
            if confirm || !has_points {
                self.close_plan_project();
            } else {
                self.meas_discard_confirm = true;
            }
        }
        if export_all {
            self.export_measurement();
        }
        if export_points {
            self.export_points_csv();
        }
        if export_pdf {
            self.export_pdf();
        }
        if export_report {
            self.export_report();
        }
        if save_as {
            self.save_survey_as();
        }
    }

    // ---------------------------------------------------------------- Drag & Drop

    /// Ins Fenster gezogene Grundrisse übernehmen (aus jedem Schritt; danach Schritt „Pläne“)
    fn take_dropped_plans(&mut self, ctx: &egui::Context) {
        let files: Vec<std::path::PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        if files.is_empty() || self.pdf_pick.is_some() {
            return;
        }
        if self.plan_step != PlanStep::Measure || self.meas.as_ref().is_some_and(|m| !m.running) {
            self.plan_step = PlanStep::Plans;
        }
        for f in files {
            self.load_plan_path(ctx, f, true);
            // ein mehrseitiges PDF öffnet die Seitenauswahl – weitere Dateien danach einzeln
            if self.pdf_pick.is_some() {
                break;
            }
        }
    }

    /// Hinweis über der ganzen Seite, solange eine Datei über das Fenster gezogen wird
    fn plan_drop_overlay(&self, ctx: &egui::Context) {
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
        if !hovering || self.plan_step == PlanStep::Plans {
            return;
        }
        let screen = ctx.screen_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("plan_drop")));
        let p = crate::theme::pal_ctx(ctx);
        painter.rect_filled(screen, 0.0, Color32::from_black_alpha(140));
        let r = Rect::from_center_size(screen.center(), Vec2::new(460.0, 150.0));
        painter.rect(r, 14.0, p.surface, Stroke::new(2.0f32, p.accent));
        painter.text(r.center() + Vec2::new(0.0, -26.0), Align2::CENTER_CENTER, "\u{E61E}", FontId::proportional(34.0), p.accent);
        painter.text(r.center() + Vec2::new(0.0, 18.0), Align2::CENTER_CENTER, t!("Loslassen, um den Grundriss hinzuzufügen", "Drop to add the floor plan"), FontId::new(16.0, crate::theme::semibold()), p.text);
        painter.text(r.center() + Vec2::new(0.0, 42.0), Align2::CENTER_CENTER, t!("PNG, JPG oder PDF – wird ein neues Stockwerk", "PNG, JPG or PDF – becomes a new floor"), FontId::proportional(12.5), p.weak);
    }
}

/// Schritt in der Schrittleiste: Kreis mit Nummer bzw. Häkchen und Name
fn step_chip(ui: &mut Ui, n: usize, label: &str, active: bool, done: bool, enabled: bool) -> egui::Response {
    let p = pal(ui);
    let font = FontId::new(14.0, if active { crate::theme::semibold() } else { egui::FontFamily::Proportional });
    let text_w = ui.fonts(|f| f.layout_no_wrap(label.to_string(), font.clone(), p.text).size().x);
    let (r, resp) = ui.allocate_exact_size(Vec2::new(text_w + 46.0, 32.0), if enabled { Sense::click() } else { Sense::hover() });
    if active {
        ui.painter().rect_filled(r, 16.0, p.accent.gamma_multiply(0.16));
    } else if resp.hovered() && enabled {
        ui.painter().rect_filled(r, 16.0, p.surface2);
    }
    let c = Pos2::new(r.min.x + 16.0, r.center().y);
    let (fill, fg) = if active {
        (p.accent, Color32::WHITE)
    } else if done {
        (p.success, Color32::WHITE)
    } else {
        (p.surface2, if enabled { p.text } else { p.weak })
    };
    ui.painter().circle(c, 11.0, fill, Stroke::new(1.0f32, if active || done { fill } else { p.border }));
    let mark = if done && !active { "\u{E182}".to_string() } else { n.to_string() };
    ui.painter().text(c, Align2::CENTER_CENTER, mark, FontId::proportional(12.0), fg);
    let col = if active { p.text } else if enabled { p.text.gamma_multiply(0.85) } else { p.weak };
    ui.painter().text(Pos2::new(r.min.x + 34.0, r.center().y), Align2::LEFT_CENTER, label, font, col);
    if enabled && !active {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else if !enabled {
        resp.on_hover_text(t!("Erst einen Grundriss laden und das WLAN wählen", "Load a floor plan and choose the Wi-Fi first"))
    } else {
        resp
    }
}

/// Gestrichelter Rahmen (Ablagefläche)
fn dashed_rect(painter: &egui::Painter, r: Rect, col: Color32) {
    let stroke = Stroke::new(1.5f32, col);
    let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
    for w in pts.windows(2) {
        painter.extend(egui::Shape::dashed_line(&[w[0], w[1]], stroke, 8.0, 6.0));
    }
}
