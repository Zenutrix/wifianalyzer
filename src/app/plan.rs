//! Grundriss mit Heatmap und Netzwerktests (Latenz/Durchsatz) pro Messpunkt.
//!
//! - Grundriss laden (PNG/JPEG), Messpunkte per Klick platzieren oder verschieben
//! - Heatmap der Ausleuchtung (IDW-Interpolation, siehe `crate::heatmap`)
//! - Netzwerktest startet beim Öffnen des Messpunkt-Dialogs im Hintergrund und wird dem
//!   Messpunkt zugeordnet, sobald er fertig ist

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, TextureHandle, TextureOptions, Ui, Vec2};

use crate::app::{Page, Sev, WifiApp};
use crate::heatmap;
use crate::measure::{FloorPlan, Quality};
use crate::nettest::{self, NetTest, NetTestPlan};
use crate::theme::pal;
use crate::widgets::primary_button;
use crate::{t, tf};

/// Netzwerktest, dessen Ergebnis noch einem Messpunkt zugeordnet werden muss
pub(crate) struct NetJob {
    pub id: u64,
    pub rx: Receiver<NetTest>,
}

/// Grundriss-Textur (Schlüssel: Name + Größe der Bilddaten)
pub(crate) struct PlanTex {
    key: (String, usize),
    tex: TextureHandle,
    size: [usize; 2],
}

/// Farbe eines Messpunkts nach Bewertung (Heatmap-Farben, unabhängig vom Theme)
pub(crate) fn point_color(rssi: Option<f64>) -> Color32 {
    match rssi {
        Some(v) => {
            let [r, g, b] = heatmap::color(v);
            Color32::from_rgb(r, g, b)
        }
        None => Color32::from_gray(140),
    }
}

/// Bildrechteck mit gleichem Seitenverhältnis wie der Plan, mittig in `area`
fn fit_rect(area: Rect, size: [usize; 2]) -> Rect {
    let aspect = size[0] as f32 / size[1].max(1) as f32;
    let (mut w, mut h) = (area.width(), area.width() / aspect);
    if h > area.height() {
        h = area.height();
        w = h * aspect;
    }
    Rect::from_center_size(area.center(), Vec2::new(w, h))
}

fn to_norm(img: Rect, p: Pos2) -> [f32; 2] {
    [((p.x - img.min.x) / img.width()).clamp(0.0, 1.0), ((p.y - img.min.y) / img.height()).clamp(0.0, 1.0)]
}

fn to_screen(img: Rect, n: [f32; 2]) -> Pos2 {
    Pos2::new(img.min.x + n[0] * img.width(), img.min.y + n[1] * img.height())
}

/// Messpunkt-Markierung: Kreis in Bewertungsfarbe mit Nummer
pub(crate) fn draw_marker(painter: &egui::Painter, c: Pos2, r: f32, no: usize, rssi: Option<f64>, selected: bool) {
    if selected {
        painter.circle_stroke(c, r + 4.0, Stroke::new(2.5f32, Color32::from_rgb(59, 130, 246)));
    }
    painter.circle_filled(c + Vec2::new(0.0, 1.5), r, Color32::from_black_alpha(70));
    painter.circle(c, r, point_color(rssi), Stroke::new(2.0f32, Color32::WHITE));
    painter.text(c, Align2::CENTER_CENTER, no.to_string(), FontId::proportional(r * 1.05), Color32::WHITE);
}

impl WifiApp {
    // ---------------------------------------------------------------- Netzwerktests

    /// Netzwerktest starten, falls aktiviert und mit der gemessenen SSID verbunden
    pub(super) fn spawn_net_test(&self) -> Option<Receiver<NetTest>> {
        let m = self.meas.as_ref().filter(|m| m.running)?;
        let plan = NetTestPlan { ping: self.settings.point_ping, speed: self.settings.point_speed };
        if !plan.ping && !plan.speed {
            return None;
        }
        let conn = self.conn.as_ref().filter(|c| c.ssid == m.ssid)?;
        let (tx, rx) = channel();
        let demo = crate::demo::enabled();
        let (rssi, tx_mbps, seed) = (conn.rssi.unwrap_or(-60), conn.tx_mbps, m.points.len() as u64 + 7);
        std::thread::Builder::new()
            .name("nettest".into())
            .spawn(move || {
                let r = if demo {
                    std::thread::sleep(Duration::from_millis(if plan.speed { 4000 } else { 1500 }));
                    nettest::demo(rssi, tx_mbps, plan, seed)
                } else {
                    nettest::run(plan)
                };
                let _ = tx.send(r);
            })
            .ok()?;
        Some(rx)
    }

    /// Fertige Netzwerktests den Messpunkten zuordnen
    pub(super) fn poll_net_jobs(&mut self) {
        if self.net_jobs.is_empty() {
            return;
        }
        let mut done = Vec::new();
        self.net_jobs.retain(|j| match j.rx.try_recv() {
            Ok(r) => {
                done.push((j.id, r));
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => false,
        });
        let mut changed = false;
        for (id, r) in done {
            if let Some(p) = self.meas.as_mut().and_then(|m| m.point_mut(id)) {
                p.net = Some(r);
                changed = true;
            }
        }
        if changed {
            self.survey_changed();
        }
    }

    // ---------------------------------------------------------------- Grundriss laden

    pub(super) fn load_plan(&mut self) {
        if self.meas.is_none() {
            return;
        }
        let Some(path) = rfd::FileDialog::new()
            .add_filter(t!("Bilder (Grundriss)", "Images (floor plan)"), &["png", "jpg", "jpeg", "bmp"])
            .pick_file()
        else {
            return;
        };
        let res = std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| heatmap::prepare_plan(&b));
        match res {
            Ok((data, w, h)) => {
                let name = path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "plan".into());
                let plan = FloorPlan { name: name.clone(), data };
                self.last_plan = Some(plan.clone());
                if let Some(m) = self.meas.as_mut() {
                    m.plan = Some(plan);
                    self.plan_sel = m.points.iter().find(|p| p.pos.is_none()).map(|p| p.id);
                }
                self.survey_changed();
                self.toast(Sev::Info, tf!("Grundriss geladen: {name} ({w} × {h} px)", "Floor plan loaded: {name} ({w} × {h} px)"));
            }
            Err(e) => self.toast(Sev::Critical, tf!("Bild kann nicht geladen werden: {e}", "Cannot load image: {e}")),
        }
    }

    fn remove_plan(&mut self) {
        if let Some(m) = self.meas.as_mut() {
            m.plan = None;
        }
        self.plan_tex = None;
        self.heat_tex = None;
        self.survey_changed();
    }

    /// Textur des Grundrisses (wird bei Bedarf neu erzeugt)
    pub(super) fn plan_texture(&mut self, ctx: &egui::Context) -> Option<(egui::TextureId, [usize; 2])> {
        let plan = self.meas.as_ref()?.plan.as_ref()?;
        let key = (plan.name.clone(), plan.data.len());
        if self.plan_tex.as_ref().map(|t| &t.key) != Some(&key) {
            let img = heatmap::decode_plan(&plan.data).ok()?;
            let size = [img.width() as usize, img.height() as usize];
            let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
            let tex = ctx.load_texture("floor_plan", color, TextureOptions::LINEAR);
            self.plan_tex = Some(PlanTex { key, tex, size });
            self.heat_tex = None;
        }
        self.plan_tex.as_ref().map(|t| (t.tex.id(), t.size))
    }

    /// Heatmap-Textur passend zu den platzierten Messpunkten
    fn heat_texture(&mut self, ctx: &egui::Context, size: [usize; 2]) -> Option<egui::TextureId> {
        let m = self.meas.as_ref()?;
        let pts: Vec<([f32; 2], f64)> = m.points.iter().filter_map(|p| Some((p.pos?, p.rssi?))).collect();
        if pts.is_empty() {
            return None;
        }
        let mut h = DefaultHasher::new();
        for (p, v) in &pts {
            p[0].to_bits().hash(&mut h);
            p[1].to_bits().hash(&mut h);
            v.to_bits().hash(&mut h);
        }
        size.hash(&mut h);
        let key = h.finish();
        if self.heat_tex.as_ref().map(|t| t.0) != Some(key) {
            let aspect = size[0] as f32 / size[1].max(1) as f32;
            let (w, hgt) = if aspect >= 1.0 { (220usize, ((220.0 / aspect) as usize).max(8)) } else { (((220.0 * aspect) as usize).max(8), 220usize) };
            let field = heatmap::field(&pts, w, hgt, aspect);
            let rgba: Vec<u8> = field.iter().flatten().copied().collect();
            let img = egui::ColorImage::from_rgba_unmultiplied([w, hgt], &rgba);
            let tex = ctx.load_texture("heatmap", img, TextureOptions::LINEAR);
            self.heat_tex = Some((key, tex));
        }
        self.heat_tex.as_ref().map(|t| t.1.id())
    }

    // ---------------------------------------------------------------- Grundriss-Ansicht

    pub(super) fn plan_view(&mut self, ui: &mut Ui, height: f32) {
        let p = pal(ui);
        let Some(m) = &self.meas else { return };
        if m.plan.is_none() {
            let rect = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover()).0;
            ui.painter().rect(rect, 12.0, p.surface, Stroke::new(1.0f32, p.border));
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(20.0)).layout(egui::Layout::top_down(egui::Align::Center)));
            child.add_space((height / 2.0 - 90.0).max(10.0));
            child.label(RichText::new("🗺").size(38.0).color(p.weak));
            child.label(RichText::new(t!("Noch kein Grundriss", "No floor plan yet")).size(18.0).strong().color(p.text));
            child.label(
                RichText::new(t!(
                    "Plan als Bild laden (PNG oder JPG, z. B. Foto vom Fluchtplan). Danach die Messpunkte per Klick platzieren – die App färbt die Ausleuchtung ein.",
                    "Load a plan image (PNG or JPG, e.g. a photo of the evacuation plan). Then place the points by clicking – the app colours the coverage."
                ))
                .color(p.weak),
            );
            child.add_space(10.0);
            if primary_button(&mut child, t!("🗺 Grundriss laden", "🗺 Load floor plan"), p.accent).clicked() {
                self.load_plan();
            }
            return;
        }

        // Werkzeugleiste
        let unplaced = m.points.iter().filter(|pt| pt.pos.is_none()).count();
        let sel_no = self.plan_sel.and_then(|id| m.points.iter().position(|pt| pt.id == id)).map(|i| i + 1);
        let (mut change, mut remove) = (false, false);
        let mut heat = self.plan_heat;
        ui.horizontal(|ui| {
            ui.checkbox(&mut heat, t!("Heatmap", "Heatmap"));
            if ui.button(t!("🗺 Anderer Grundriss …", "🗺 Other floor plan …")).clicked() {
                change = true;
            }
            if ui.button(t!("Entfernen", "Remove")).on_hover_text(t!("Grundriss aus dieser Messung entfernen", "Remove the floor plan from this survey")).clicked() {
                remove = true;
            }
            ui.separator();
            let hint = match sel_no {
                Some(n) => tf!("Klick in den Plan setzt Messpunkt {n}", "Click into the plan to place point {n}"),
                None if unplaced > 0 => tf!("{unplaced} Messpunkte ohne Position – in der Liste anklicken und dann im Plan setzen", "{unplaced} points without position – click one in the list, then place it"),
                None => t!("Messpunkte lassen sich mit der Maus verschieben", "Drag points with the mouse to move them").into(),
            };
            ui.label(RichText::new(hint).color(if sel_no.is_some() { p.accent } else { p.weak }));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| legend(ui));
        });
        self.plan_heat = heat;
        if change {
            self.load_plan();
        }
        if remove {
            self.remove_plan();
            return;
        }

        let ctx = ui.ctx().clone();
        let Some((tex, size)) = self.plan_texture(&ctx) else {
            ui.label(RichText::new(t!("Grundriss kann nicht angezeigt werden", "Floor plan cannot be shown")).color(p.danger));
            return;
        };
        let heat_tex = if self.plan_heat { self.heat_texture(&ctx, size) } else { None };
        let h = (height - 30.0).max(160.0);
        let (area, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), h), Sense::click_and_drag());
        let img = fit_rect(area, size);
        let painter = ui.painter_at(area);
        painter.rect_filled(area, 10.0, p.surface);
        painter.image(tex, img, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        if let Some(ht) = heat_tex {
            painter.image(ht, img, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }
        painter.rect_stroke(img, 0.0, Stroke::new(1.0f32, p.border));

        let Some(m) = &self.meas else { return };
        let markers: Vec<(u64, usize, Pos2, Option<f64>, String)> = m
            .points
            .iter()
            .enumerate()
            .filter_map(|(i, pt)| pt.pos.map(|pos| (pt.id, i + 1, to_screen(img, pos), pt.rssi, pt.name.clone())))
            .collect();
        let next_ids: Vec<u64> = m.points.iter().filter(|pt| pt.pos.is_none()).map(|pt| pt.id).collect();
        let running = m.running;
        let mut click_measure: Option<[f32; 2]> = None;
        let r = 11.0;
        for (id, no, c, rssi, _) in &markers {
            draw_marker(&painter, *c, r, *no, *rssi, self.plan_sel == Some(*id) || self.plan_drag == Some(*id));
        }

        // Interaktion: Marker ziehen, klicken = auswählen, Klick ins Leere = ausgewählten Punkt setzen
        let hit = |pos: Pos2| markers.iter().filter(|m| m.2.distance(pos) <= r + 4.0).min_by(|a, b| a.2.distance(pos).total_cmp(&b.2.distance(pos))).map(|m| m.0);
        let mut set_pos: Option<(u64, [f32; 2])> = None;
        if resp.drag_started() {
            self.plan_drag = resp.interact_pointer_pos().and_then(hit);
        }
        if let (Some(id), Some(pos)) = (self.plan_drag, resp.interact_pointer_pos()) {
            if resp.dragged() && img.contains(pos) {
                set_pos = Some((id, to_norm(img, pos)));
            }
        }
        if resp.drag_stopped() && self.plan_drag.is_some() {
            self.plan_drag = None;
            self.survey_changed();
        }
        if resp.clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                match hit(pos) {
                    Some(id) => self.plan_sel = if self.plan_sel == Some(id) { None } else { Some(id) },
                    None => {
                        if let (Some(id), true) = (self.plan_sel, img.contains(pos)) {
                            set_pos = Some((id, to_norm(img, pos)));
                            self.survey_changed();
                            // weiter zum nächsten Punkt ohne Position
                            self.plan_sel = next_ids.iter().copied().find(|x| *x != id);
                        } else if running && img.contains(pos) && self.meas_point.is_none() {
                            // Grundriss-Messung: hier stehe ich -> Messpunkt an dieser Stelle erfassen
                            click_measure = Some(to_norm(img, pos));
                        }
                    }
                }
            }
        }
        if let Some(pos) = resp.hover_pos() {
            if let Some(id) = hit(pos) {
                if let Some((_, no, _, rssi, name)) = markers.iter().find(|mk| mk.0 == id) {
                    let txt = match rssi {
                        Some(v) => format!("{no}. {name}\n{v:.0} dBm · {}", Quality::of(*v).label()),
                        None => format!("{no}. {name}"),
                    };
                    resp.clone().on_hover_text_at_pointer(txt);
                }
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            } else if self.plan_sel.is_some() && img.contains(pos) {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
            }
        }
        if let Some((id, pos)) = set_pos {
            if let Some(pt) = self.meas.as_mut().and_then(|m| m.point_mut(id)) {
                pt.pos = Some(pos);
            }
        }
        if let Some(pos) = click_measure {
            self.open_point_dialog_at(Some(pos));
        }
        // Hinweis unter dem Plan
        if running && self.plan_sel.is_none() {
            ui.label(RichText::new(t!("Klick in den Plan = Messpunkt an dieser Stelle · Punkte lassen sich mit der Maus verschieben", "Click into the plan = point at that spot · drag points to move them")).small().color(p.weak));
        }
    }

    /// Kompakte Live-Zeile der Grundriss-Messung (der Plan braucht den Platz)
    pub(super) fn plan_live_strip(&mut self, ui: &mut Ui, live: Option<i32>, connected: bool) {
        let p = pal(ui);
        let Some(m) = &self.meas else {
            // noch keine Messung: kurze Anleitung
            crate::widgets::card(ui, Some(t!("So funktioniert die Grundriss-Messung", "How the floor plan survey works")), |ui| {
                ui.set_max_width(700.0);
                ui.label(t!("1.  SSID wählen und „Messung starten“", "1.  Choose the SSID and click \"Start survey\""));
                ui.label(t!("2.  Grundriss laden – Bild oder Foto vom Fluchtplan (PNG/JPG)", "2.  Load the floor plan – image or photo of the evacuation plan (PNG/JPG)"));
                ui.label(t!("3.  Durchs Gebäude gehen und an deiner Position in den Plan klicken – Raum eingeben (optional), Enter", "3.  Walk the building and click your position on the plan – enter the room (optional), Enter"));
                ui.label(t!("4.  Die Heatmap zeigt die Ausleuchtung; Export als PDF-Bericht mit Grundriss", "4.  The heatmap shows the coverage; export as PDF report with the floor plan"));
                ui.add_space(6.0);
                ui.label(
                    RichText::new(t!(
                        "Tipp: Der zuletzt verwendete Grundriss wird für die nächste Grundriss-Messung übernommen.",
                        "Tip: the floor plan used last is reused for the next floor plan survey."
                    ))
                    .size(12.0)
                    .color(p.weak),
                );
            });
            return;
        };
        let s = m.stats(self.now_t());
        crate::widgets::card(ui, None, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                match live {
                    Some(v) => {
                        let q = Quality::of(v as f64);
                        let col = point_color(Some(v as f64));
                        ui.label(RichText::new(format!("{v} dBm")).size(26.0).strong().color(col));
                        crate::widgets::badge(ui, q.label(), col);
                    }
                    None => {
                        ui.label(RichText::new("– dBm").size(26.0).color(p.weak));
                    }
                }
                ui.add_space(10.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new(if connected { t!("Live – verbunden mit der SSID", "Live – connected to the SSID") } else { t!("Live – stärkster AP aus dem Scan", "Live – strongest AP from the scan") }).color(p.weak));
                    ui.label(
                        RichText::new(tf!(
                            "{} Messpunkte · {} im Plan · Dauer {}",
                            "{} points · {} on the plan · duration {}",
                            m.points.len(),
                            m.points.iter().filter(|pt| pt.pos.is_some()).count(),
                            crate::widgets::fmt_mmss(s.duration)
                        ))
                        .color(p.text),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let vals: Vec<f64> = m.points.iter().filter_map(|pt| pt.rssi).collect();
                    let weak = m.points.iter().filter(|pt| pt.quality() == Some(Quality::Weak)).count();
                    if !vals.is_empty() {
                        let avg = vals.iter().sum::<f64>() / vals.len() as f64;
                        ui.label(RichText::new(tf!("schwach: {weak}", "weak: {weak}")).color(if weak > 0 { p.danger } else { p.weak }));
                        ui.separator();
                        ui.label(RichText::new(tf!("Ø {avg:.0} dBm an den Punkten", "avg {avg:.0} dBm at the points")).color(point_color(Some(avg))));
                    }
                });
            });
        });
    }

    /// Hinweis, wenn auf der anderen Messungs-Seite eine Messung offen ist
    pub(super) fn other_survey_notice(&mut self, ui: &mut Ui, plan_mode: bool, running: bool, ssid: &str) {
        let p = pal(ui);
        crate::widgets::page_header(ui, if plan_mode { t!("Grundriss-Messung", "Floor plan survey") } else { t!("Messung", "Survey") }, "", |_| {});
        ui.add_space(20.0);
        let (other_page, other_name) = if plan_mode { (Page::Measure, t!("Rundgang-Messung", "walk survey")) } else { (Page::PlanSurvey, t!("Grundriss-Messung", "floor plan survey")) };
        let mut go = false;
        let mut close = false;
        crate::widgets::card(ui, None, |ui| {
            ui.set_max_width(640.0);
            ui.label(
                RichText::new(if running {
                    tf!("Gerade läuft eine {other_name} („{ssid}“).", "A {other_name} is running (\"{ssid}\").")
                } else {
                    tf!("Geöffnet ist eine {other_name} („{ssid}“).", "A {other_name} is open (\"{ssid}\").")
                })
                .size(16.0)
                .strong()
                .color(p.text),
            );
            ui.label(RichText::new(t!("Es ist immer eine Messung gleichzeitig geöffnet. Sie bleibt gespeichert.", "One survey is open at a time. It stays saved.")).color(p.weak));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if primary_button(ui, &tf!("Zur {other_name}", "Go to the {other_name}"), p.accent).clicked() {
                    go = true;
                }
                if !running && ui.button(t!("Schließen und hier neu beginnen", "Close it and start here")).clicked() {
                    close = true;
                }
            });
        });
        if go {
            self.page = other_page;
        }
        if close {
            self.autosave_survey(true);
            self.meas = None;
            self.meas_file = None;
            self.apply_scan_interval();
        }
    }
}

/// Farblegende der Heatmap
fn legend(ui: &mut Ui) {
    let p = pal(ui);
    // rechts-nach-links-Layout: zuerst das rechte Ende
    ui.label(RichText::new("-50 dBm").small().color(p.weak));
    let (rect, _) = ui.allocate_exact_size(Vec2::new(140.0, 10.0), Sense::hover());
    let steps = 28;
    for i in 0..steps {
        let v = -85.0 + 35.0 * i as f64 / (steps - 1) as f64;
        let x0 = rect.min.x + rect.width() * i as f32 / steps as f32;
        let r = Rect::from_min_max(Pos2::new(x0, rect.min.y), Pos2::new(x0 + rect.width() / steps as f32 + 0.5, rect.max.y));
        let [rr, g, b] = heatmap::color(v);
        ui.painter().rect_filled(r, 0.0, Color32::from_rgb(rr, g, b));
    }
    ui.label(RichText::new("-85").small().color(p.weak));
}

/// Kleiner, anklickbarer Grundriss im Messpunkt-Dialog
pub(crate) fn plan_picker(ui: &mut Ui, tex: egui::TextureId, size: [usize; 2], pos: &mut Option<[f32; 2]>, others: &[([f32; 2], Option<f64>, usize)], next_no: usize) {
    let p = pal(ui);
    let w = ui.available_width();
    let h = (w * size[1] as f32 / size[0].max(1) as f32).clamp(80.0, 210.0);
    let (area, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
    let img = fit_rect(area, size);
    let painter = ui.painter_at(area);
    painter.rect_filled(area, 8.0, p.surface2);
    painter.image(tex, img, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
    for (o, rssi, no) in others {
        draw_marker(&painter, to_screen(img, *o), 7.0, *no, *rssi, false);
    }
    if let Some(pp) = pos {
        draw_marker(&painter, to_screen(img, *pp), 9.0, next_no, None, true);
    }
    if resp.clicked() {
        if let Some(c) = resp.interact_pointer_pos().filter(|c| img.contains(*c)) {
            *pos = Some(to_norm(img, c));
        }
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    }
}
