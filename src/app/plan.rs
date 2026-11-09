//! Grundriss mit Heatmap und Netzwerktests (Latenz/Durchsatz) pro Messpunkt.
//!
//! - Grundriss laden (PNG/JPEG oder eine Seite aus einem PDF), Messpunkte per Klick platzieren oder verschieben
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
use crate::measure::{FloorPlan, Layer, Quality};
use crate::nettest::{self, NetTest, NetTestPlan};
use crate::theme::pal;
use crate::widgets::primary_button;
use crate::{t, tf};

/// Netzwerktest, dessen Ergebnis noch einem Messpunkt zugeordnet werden muss
pub(crate) struct NetJob {
    pub id: u64,
    pub rx: Receiver<NetTest>,
}

/// Maßstab setzen: zwei Punkte einer bekannten Strecke + deren Länge
#[derive(Default)]
pub(crate) struct ScaleDraft {
    a: Option<[f32; 2]>,
    b: Option<[f32; 2]>,
    meters: String,
}

/// Seitenauswahl für einen mehrseitigen PDF-Grundriss
pub(crate) struct PdfPick {
    path: std::path::PathBuf,
    name: String,
    pages: u32,
    thumbs: Vec<(TextureHandle, [usize; 2])>,
    page: u32,
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

    pub(super) fn load_plan(&mut self, ctx: &egui::Context) {
        if self.meas.is_none() {
            return;
        }
        let Some(path) = rfd::FileDialog::new()
            .add_filter(t!("Grundriss (Bild oder PDF)", "Floor plan (image or PDF)"), &["png", "jpg", "jpeg", "bmp", "pdf"])
            .add_filter("PDF", &["pdf"])
            .add_filter(t!("Bilder", "Images"), &["png", "jpg", "jpeg", "bmp"])
            .pick_file()
        else {
            return;
        };
        let name = path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "plan".into());
        if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
            self.open_pdf_plan(ctx, path, name);
            return;
        }
        let res = std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| heatmap::prepare_plan(&b));
        self.apply_plan(name, res);
    }

    /// PDF: einseitig direkt übernehmen, sonst Seitenauswahl mit Vorschaubildern
    fn open_pdf_plan(&mut self, ctx: &egui::Context, path: std::path::PathBuf, name: String) {
        match crate::pdfimport::thumbnails(path.clone(), 360) {
            Ok((0, _)) => self.toast(Sev::Critical, t!("Das PDF enthält keine Seiten", "The PDF has no pages").into()),
            Ok((1, _)) => self.load_pdf_page(path, name, 0, 1),
            Ok((pages, imgs)) => {
                let thumbs = imgs
                    .iter()
                    .enumerate()
                    .map(|(i, img)| {
                        let size = [img.width() as usize, img.height() as usize];
                        let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
                        (ctx.load_texture(format!("pdf_thumb_{i}"), color, TextureOptions::LINEAR), size)
                    })
                    .collect();
                self.pdf_pick = Some(PdfPick { path, name, pages, thumbs, page: 0 });
            }
            Err(e) => self.toast(Sev::Critical, tf!("PDF kann nicht geladen werden: {e}", "Cannot load PDF: {e}")),
        }
    }

    /// Eine PDF-Seite in voller Auflösung rendern und als Grundriss übernehmen
    fn load_pdf_page(&mut self, path: std::path::PathBuf, name: String, page: u32, pages: u32) {
        let res = crate::pdfimport::render_page(path, page, 2400).and_then(|png| heatmap::prepare_plan(&png));
        let name = if pages > 1 { tf!("{name} – Seite {}", "{name} – page {}", page + 1) } else { name };
        self.apply_plan(name, res);
    }

    /// Fertig vorbereitete Bilddaten als Grundriss der laufenden Messung setzen
    fn apply_plan(&mut self, name: String, res: Result<(Vec<u8>, u32, u32), String>) {
        match res {
            Ok((data, w, h)) => {
                let plan = FloorPlan::new(name.clone(), data);
                self.last_plan = Some(plan.clone());
                if let Some(m) = self.meas.as_mut() {
                    m.plan = Some(plan);
                    self.plan_sel = m.points.iter().find(|p| p.pos.is_none()).map(|p| p.id);
                }
                self.survey_changed();
                self.toast(Sev::Info, tf!("Grundriss geladen: {name} ({w} × {h} px)", "Floor plan loaded: {name} ({w} × {h} px)"));
            }
            Err(e) => self.toast(Sev::Critical, tf!("Grundriss kann nicht geladen werden: {e}", "Cannot load floor plan: {e}")),
        }
    }

    /// Fenster: Seite eines mehrseitigen PDFs wählen
    fn pdf_pick_window(&mut self, ctx: &egui::Context) {
        let Some(pick) = self.pdf_pick.as_mut() else { return };
        let mut open = true;
        let mut chosen: Option<u32> = None;
        let screen = ctx.screen_rect();
        egui::Window::new(RichText::new(t!("\u{E230} Seite für den Grundriss wählen", "\u{E230} Choose the floor plan page")).strong())
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([screen.width().min(1300.0) * 0.8, screen.height() * 0.7])
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                let p = pal(ui);
                ui.label(RichText::new(tf!("{} · {} Seiten – Seite anklicken (z. B. das richtige Stockwerk), Doppelklick übernimmt", "{} · {} pages – click a page (e.g. the right floor), double-click to use it", pick.name, pick.pages)).color(p.weak));
                ui.add_space(6.0);
                let max_h = (screen.height() * 0.6).max(240.0);
                egui::ScrollArea::vertical().max_height(max_h).auto_shrink([false, true]).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(14.0, 14.0);
                        for (i, (tex, size)) in pick.thumbs.iter().enumerate() {
                            let box_px = 200.0;
                            let scale = box_px / size[0].max(size[1]) as f32;
                            let img_size = Vec2::new(size[0] as f32 * scale, size[1] as f32 * scale);
                            let (rect, resp) = ui.allocate_exact_size(Vec2::new(box_px + 16.0, box_px + 40.0), Sense::click());
                            let sel = pick.page == i as u32;
                            let bg = if sel { p.accent.gamma_multiply(0.18) } else if resp.hovered() { p.surface } else { Color32::TRANSPARENT };
                            ui.painter().rect(rect, 10.0, bg, Stroke::new(if sel { 2.0f32 } else { 1.0 }, if sel { p.accent } else { p.border }));
                            let img_rect = Rect::from_center_size(Pos2::new(rect.center().x, rect.top() + 8.0 + box_px / 2.0), img_size);
                            ui.painter().rect_filled(img_rect, 2.0, Color32::WHITE);
                            ui.painter().image(tex.id(), img_rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                            ui.painter().text(
                                Pos2::new(rect.center().x, rect.bottom() - 16.0),
                                Align2::CENTER_CENTER,
                                tf!("Seite {}", "Page {}", i + 1),
                                FontId::proportional(13.0),
                                if sel { p.accent } else { p.text },
                            );
                            if resp.clicked() {
                                pick.page = i as u32;
                            }
                            if resp.double_clicked() {
                                chosen = Some(i as u32);
                            }
                            resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                        }
                    });
                });
                ui.add_space(8.0);
                ui.separator();
                ui.horizontal(|ui| {
                    if pick.pages as usize > pick.thumbs.len() {
                        ui.label(RichText::new(t!("Seite:", "Page:")).color(p.weak));
                        let mut n = pick.page + 1;
                        if ui.add(egui::DragValue::new(&mut n).range(1..=pick.pages)).changed() {
                            pick.page = n - 1;
                        }
                        ui.label(RichText::new(tf!("von {} (Vorschau nur für die ersten {})", "of {} (preview for the first {} only)", pick.pages, pick.thumbs.len())).color(p.weak));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if primary_button(ui, &tf!("Seite {} übernehmen", "Use page {}", pick.page + 1), p.accent).clicked() {
                            chosen = Some(pick.page);
                        }
                    });
                });
            });
        if let Some(page) = chosen {
            if let Some(pick) = self.pdf_pick.take() {
                self.load_pdf_page(pick.path, pick.name, page, pick.pages);
            }
        } else if !open {
            self.pdf_pick = None;
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
        let pts = m.heat_points(self.plan_layer);
        if pts.is_empty() {
            return None;
        }
        let width_m = m.plan.as_ref().and_then(|p| p.width_m);
        let mut h = DefaultHasher::new();
        width_m.map(f32::to_bits).hash(&mut h);
        heatmap::scale_to_string(&heatmap::scale()).hash(&mut h);
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
            let field = heatmap::field(&pts, w, hgt, aspect, width_m);
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
        self.pdf_pick_window(&ui.ctx().clone());
        let Some(m) = &self.meas else { return };
        if m.plan.is_none() {
            let rect = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover()).0;
            ui.painter().rect(rect, 12.0, p.surface, Stroke::new(1.0f32, p.border));
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(20.0)).layout(egui::Layout::top_down(egui::Align::Center)));
            child.add_space((height / 2.0 - 90.0).max(10.0));
            child.label(RichText::new("\u{E31A}").size(38.0).color(p.weak));
            child.label(RichText::new(t!("Noch kein Grundriss", "No floor plan yet")).size(18.0).strong().color(p.text));
            child.label(
                RichText::new(t!(
                    "Plan als Bild oder PDF laden (PNG, JPG, Foto vom Fluchtplan oder PDF – bei mehreren Seiten wählst du die richtige aus). Danach die Messpunkte per Klick platzieren – die App färbt die Ausleuchtung ein.",
                    "Load a plan as image or PDF (PNG, JPG, a photo of the evacuation plan or PDF – with several pages you pick the right one). Then place the points by clicking – the app colours the coverage."
                ))
                .color(p.weak),
            );
            child.add_space(10.0);
            if primary_button(&mut child, t!("\u{E31A} Grundriss laden", "\u{E31A} Load floor plan"), p.accent).clicked() {
                self.load_plan(&ui.ctx().clone());
            }
            return;
        }

        // Werkzeugleiste
        let unplaced = m.points.iter().filter(|pt| pt.pos.is_none()).count();
        let sel_no = self.plan_sel.and_then(|id| m.points.iter().position(|pt| pt.id == id)).map(|i| i + 1);
        let with_data = m.layers_with_data();
        let (width_m, plan_px) = (m.plan.as_ref().and_then(|p| p.width_m), self.plan_tex.as_ref().map(|t| t.size));
        let (mut change, mut remove, mut start_scale) = (false, false, false);
        let mut heat = self.plan_heat;
        let mut layer = self.plan_layer;
        ui.horizontal(|ui| {
            ui.checkbox(&mut heat, t!("Heatmap", "Heatmap"));
            let opts: Vec<(Layer, String)> = Layer::ALL
                .iter()
                .map(|l| (*l, if *l == Layer::Measured || with_data.contains(l) { l.label().to_string() } else { format!("{} –", l.label()) }))
                .collect();
            let opts_ref: Vec<(Layer, &str)> = opts.iter().map(|(l, s)| (*l, s.as_str())).collect();
            crate::widgets::segmented(ui, &mut layer, &opts_ref);
            ui.separator();
            let scale_btn = ui.button(t!("\u{E6B8} Maßstab", "\u{E6B8} Scale")).on_hover_text(t!(
                "Zwei Punkte einer bekannten Strecke anklicken (z. B. eine Wand) und die Länge in Metern eingeben",
                "Click two points of a known distance (e.g. a wall) and enter its length in metres"
            ));
            if scale_btn.clicked() {
                start_scale = true;
            }
            if ui.button(t!("\u{E6C8} Farbskala", "\u{E6C8} Colour scale")).on_hover_text(t!("dBm-Schwellen und Farben der Heatmap einstellen", "Set the dBm thresholds and colours of the heatmap")).clicked() {
                self.show_scale_editor = !self.show_scale_editor;
            }
            if ui.button(t!("\u{E31A} Anderer Grundriss …", "\u{E31A} Other floor plan …")).clicked() {
                change = true;
            }
            if ui.button(t!("Entfernen", "Remove")).on_hover_text(t!("Grundriss aus dieser Messung entfernen", "Remove the floor plan from this survey")).clicked() {
                remove = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| legend(ui));
        });
        if layer != self.plan_layer {
            self.plan_layer = layer;
            self.heat_tex = None;
        }
        if start_scale {
            self.plan_scale = Some(ScaleDraft::default());
            self.plan_sel = None;
        }
        // Statuszeile: Maßstab bzw. Hinweis
        let mut apply_scale: Option<f32> = None;
        let mut cancel_scale = false;
        ui.horizontal(|ui| {
            if let Some(sd) = self.plan_scale.as_mut() {
                ui.label(RichText::new("\u{E6B8}").color(p.accent));
                match (sd.a, sd.b) {
                    (None, _) => {
                        ui.label(RichText::new(t!("Maßstab: auf den Anfang einer bekannten Strecke klicken (z. B. Wandlänge, Türbreite)", "Scale: click the start of a known distance (e.g. wall length, door width)")).color(p.accent));
                    }
                    (Some(_), None) => {
                        ui.label(RichText::new(t!("Jetzt auf das Ende der Strecke klicken", "Now click the end of the distance")).color(p.accent));
                    }
                    (Some(_), Some(_)) => {
                        ui.label(RichText::new(t!("Länge der Strecke:", "Length of the distance:")).color(p.text));
                        let r = ui.add(egui::TextEdit::singleline(&mut sd.meters).desired_width(70.0).hint_text("z. B. 8,5"));
                        if sd.meters.is_empty() && !r.has_focus() {
                            r.request_focus();
                        }
                        ui.label("m");
                        let parsed = sd.meters.trim().replace(',', ".").parse::<f32>().ok().filter(|v| *v > 0.05 && *v < 10_000.0);
                        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if ui.add_enabled(parsed.is_some(), egui::Button::new(t!("Übernehmen", "Apply"))).clicked() || (enter && parsed.is_some()) {
                            apply_scale = parsed;
                        }
                    }
                }
                if ui.button(t!("Abbrechen", "Cancel")).clicked() {
                    cancel_scale = true;
                }
            } else {
                match width_m {
                    Some(wm) => {
                        let hm = plan_px.map(|s| wm * s[1] as f32 / s[0].max(1) as f32).unwrap_or(0.0);
                        ui.label(RichText::new(tf!("\u{E6B8} Maßstab: Plan {wm:.1} × {hm:.1} m", "\u{E6B8} Scale: plan {wm:.1} × {hm:.1} m")).color(p.weak));
                    }
                    None => {
                        ui.label(RichText::new(t!("\u{E6B8} Kein Maßstab gesetzt – angenommen 40 m Breite. Für eine genaue Heatmap „Maßstab“ setzen.", "\u{E6B8} No scale set – assuming 40 m width. Set the \"Scale\" for an accurate heatmap.")).color(p.warning));
                    }
                }
                ui.separator();
                let hint = match sel_no {
                    Some(n) => tf!("Klick in den Plan setzt Messpunkt {n}", "Click into the plan to place point {n}"),
                    None if unplaced > 0 => tf!("{unplaced} Messpunkte ohne Position – in der Liste anklicken und dann im Plan setzen", "{unplaced} points without position – click one in the list, then place it"),
                    None => t!("Klick in den Plan = Messpunkt · Punkte lassen sich verschieben", "Click into the plan = point · points can be dragged").into(),
                };
                ui.label(RichText::new(hint).color(if sel_no.is_some() { p.accent } else { p.weak }));
            }
        });
        if cancel_scale {
            self.plan_scale = None;
        }
        if let (Some(meters), Some(size)) = (apply_scale, plan_px) {
            if let Some(sd) = self.plan_scale.take() {
                if let (Some(a), Some(b)) = (sd.a, sd.b) {
                    let len_px = (((b[0] - a[0]) * size[0] as f32).powi(2) + ((b[1] - a[1]) * size[1] as f32).powi(2)).sqrt();
                    if len_px > 2.0 {
                        let wm = meters * size[0] as f32 / len_px;
                        if let Some(pl) = self.meas.as_mut().and_then(|m| m.plan.as_mut()) {
                            pl.width_m = Some(wm);
                            pl.scale_line = Some([a, b]);
                            pl.scale_len_m = Some(meters);
                            self.last_plan = Some(pl.clone());
                        }
                        self.heat_tex = None;
                        self.survey_changed();
                        self.toast(Sev::Info, tf!("Maßstab gesetzt: Plan ist {wm:.1} m breit", "Scale set: plan is {wm:.1} m wide"));
                    }
                }
            }
        }
        self.plan_heat = heat;
        self.scale_editor_window(ui.ctx());
        if change {
            self.load_plan(&ui.ctx().clone());
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
        let h = (height - 56.0).max(160.0);
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
            .filter_map(|(i, pt)| pt.pos.map(|pos| (pt.id, i + 1, to_screen(img, pos), pt.value_for(self.plan_layer), pt.name.clone())))
            .collect();
        let heat_pts = m.heat_points(self.plan_layer);
        let plan_m = m.plan.as_ref().map(|pl| (pl.width_m, pl.scale_line)).unwrap_or((None, None));
        let scaling = self.plan_scale.is_some();
        let next_ids: Vec<u64> = m.points.iter().filter(|pt| pt.pos.is_none()).map(|pt| pt.id).collect();
        let running = m.running;
        let mut click_measure: Option<[f32; 2]> = None;
        let r = 11.0;
        for (id, no, c, rssi, _) in &markers {
            draw_marker(&painter, *c, r, *no, *rssi, self.plan_sel == Some(*id) || self.plan_drag == Some(*id));
        }
        // Maßstabsleiste unten links
        if let Some(wm) = plan_m.0 {
            let len = heatmap::nice_length(wm * 0.18);
            let px = len / wm * img.width();
            let base = Pos2::new(img.min.x + 14.0, img.max.y - 14.0);
            let bg = Rect::from_min_max(Pos2::new(base.x - 8.0, base.y - 22.0), Pos2::new(base.x + px + 8.0, base.y + 6.0));
            painter.rect_filled(bg, 4.0, Color32::from_white_alpha(225));
            let ink = Color32::from_rgb(40, 44, 52);
            painter.line_segment([base, base + Vec2::new(px, 0.0)], Stroke::new(2.0f32, ink));
            for x in [0.0, px] {
                painter.line_segment([base + Vec2::new(x, -5.0), base + Vec2::new(x, 3.0)], Stroke::new(2.0f32, ink));
            }
            let label = if len < 1.0 { format!("{:.0} cm", len * 100.0) } else { format!("{len:.0} m") };
            painter.text(base + Vec2::new(px / 2.0, -12.0), Align2::CENTER_CENTER, label, FontId::proportional(11.5), ink);
        }
        // Maßstab setzen: Linie zeichnen
        if let Some(sd) = &self.plan_scale {
            let accent = Color32::from_rgb(59, 130, 246);
            if let Some([a, b]) = plan_m.1 {
                painter.line_segment([to_screen(img, a), to_screen(img, b)], Stroke::new(1.5f32, accent.gamma_multiply(0.35)));
            }
            let end = sd.b.map(|b| to_screen(img, b)).or_else(|| resp.hover_pos().filter(|h| img.contains(*h)));
            if let Some(a) = sd.a.map(|a| to_screen(img, a)) {
                if let Some(e) = end {
                    painter.line_segment([a, e], Stroke::new(3.0f32, accent));
                    painter.circle(e, 5.0, Color32::WHITE, Stroke::new(2.0f32, accent));
                }
                painter.circle(a, 5.0, Color32::WHITE, Stroke::new(2.0f32, accent));
            }
        }

        // Interaktion: Marker ziehen, klicken = auswählen, Klick ins Leere = ausgewählten Punkt setzen
        let hit = |pos: Pos2| markers.iter().filter(|m| m.2.distance(pos) <= r + 4.0).min_by(|a, b| a.2.distance(pos).total_cmp(&b.2.distance(pos))).map(|m| m.0);
        let mut set_pos: Option<(u64, [f32; 2])> = None;
        if resp.drag_started() && !scaling {
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
        let mut scale_click: Option<[f32; 2]> = None;
        if resp.clicked() && scaling {
            scale_click = resp.interact_pointer_pos().filter(|p| img.contains(*p)).map(|p| to_norm(img, p));
        } else if resp.clicked() {
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
        if let (Some(c), Some(sd)) = (scale_click, self.plan_scale.as_mut()) {
            if sd.a.is_none() || sd.b.is_some() {
                sd.a = Some(c);
                sd.b = None;
            } else {
                sd.b = Some(c);
            }
        }
        if let Some(pos) = resp.hover_pos() {
            if scaling {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
            } else if let Some(id) = hit(pos) {
                if let Some((_, no, _, rssi, name)) = markers.iter().find(|mk| mk.0 == id) {
                    let txt = match rssi {
                        Some(v) => format!("{no}. {name}\n{v:.0} dBm · {}", Quality::of(*v).label()),
                        None => format!("{no}. {name}"),
                    };
                    resp.clone().on_hover_text_at_pointer(txt);
                }
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            } else if img.contains(pos) {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
                // geschätzter Wert unter dem Mauszeiger
                if self.plan_heat && !heat_pts.is_empty() {
                    let n = to_norm(img, pos);
                    let wm = plan_m.0.unwrap_or(heatmap::DEFAULT_WIDTH_M);
                    let hm = wm * img.height() / img.width();
                    if let Some((v, d)) = heatmap::estimate(&heat_pts, n[0], n[1], wm, hm) {
                        let txt = if v < heatmap::scale_min() {
                            tf!("{} · kaum Empfang (< {:.0} dBm)", "{} · hardly any signal (< {:.0} dBm)", self.plan_layer.label(), heatmap::scale_min())
                        } else {
                            tf!("{} · ≈ {v:.0} dBm (geschätzt, {d:.1} m vom nächsten Messpunkt)", "{} · ≈ {v:.0} dBm (estimated, {d:.1} m from the nearest point)", self.plan_layer.label())
                        };
                        resp.clone().on_hover_text_at_pointer(txt);
                    }
                }
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
    }

    /// Fenster: Farbskala der Heatmap (dBm-Schwellen + Farben)
    fn scale_editor_window(&mut self, ctx: &egui::Context) {
        if !self.show_scale_editor {
            return;
        }
        let mut open = true;
        let mut stops = heatmap::scale();
        let mut changed = false;
        let mut reset = false;
        egui::Window::new(RichText::new(t!("\u{E6C8} Farbskala der Heatmap", "\u{E6C8} Heatmap colour scale")).strong())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(340.0)
            .anchor(Align2::RIGHT_TOP, [-24.0, 150.0])
            .show(ctx, |ui| {
                let p = pal(ui);
                ui.label(RichText::new(t!("Ab welchem Signal welche Farbe gilt. Schwächer als die unterste Schwelle bleibt frei.", "Which colour applies from which signal. Weaker than the lowest threshold stays empty.")).size(12.0).color(p.weak));
                ui.add_space(6.0);
                let mut remove = None;
                egui::Grid::new("scale_grid").num_columns(4).spacing([10.0, 6.0]).show(ui, |ui| {
                    let n = stops.len();
                    for (i, (d, c)) in stops.iter_mut().enumerate() {
                        ui.label(RichText::new(if i == 0 { t!("ab", "from") } else { t!("ab", "from") }).color(p.weak));
                        if ui.add(egui::DragValue::new(d).range(-100.0..=-20.0).speed(0.5).suffix(" dBm").fixed_decimals(0)).changed() {
                            changed = true;
                        }
                        if ui.color_edit_button_srgb(c).changed() {
                            changed = true;
                        }
                        if ui.add_enabled(n > 2, egui::Button::new("\u{E4A6}").small()).clicked() {
                            remove = Some(i);
                        }
                        ui.end_row();
                    }
                });
                if let Some(i) = remove {
                    stops.remove(i);
                    changed = true;
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button(t!("+ Schwelle", "+ Threshold")).clicked() {
                        let last = stops.last().map(|s| s.0).unwrap_or(-85.0);
                        stops.push(((last - 5.0).max(-100.0), [140, 0, 0]));
                        changed = true;
                    }
                    if ui.button(t!("Standard", "Default")).clicked() {
                        reset = true;
                    }
                });
                ui.add_space(4.0);
                // Vorschau
                let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 14.0), Sense::hover());
                let (lo, hi) = (stops.iter().map(|s| s.0).fold(f64::MAX, f64::min), stops.iter().map(|s| s.0).fold(f64::MIN, f64::max) + 5.0);
                let steps = 60;
                for k in 0..steps {
                    let v = lo + (hi - lo) * k as f64 / (steps - 1) as f64;
                    let [r, g, b] = heatmap::color(v);
                    let x0 = rect.min.x + rect.width() * k as f32 / steps as f32;
                    ui.painter().rect_filled(Rect::from_min_max(Pos2::new(x0, rect.min.y), Pos2::new(x0 + rect.width() / steps as f32 + 0.5, rect.max.y)), 0.0, Color32::from_rgb(r, g, b));
                }
            });
        if reset {
            heatmap::set_scale(Vec::new());
            self.settings.heat_scale.clear();
            self.heat_tex = None;
        } else if changed {
            heatmap::set_scale(stops);
            self.settings.heat_scale = heatmap::scale_to_string(&heatmap::scale());
            self.heat_tex = None;
        }
        if !open {
            self.show_scale_editor = false;
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
    let (lo, hi) = (heatmap::scale_min(), heatmap::scale_max());
    ui.label(RichText::new(format!("{hi:.0} dBm")).small().color(p.weak));
    let (rect, _) = ui.allocate_exact_size(Vec2::new(140.0, 10.0), Sense::hover());
    let steps = 28;
    for i in 0..steps {
        let v = lo + (hi - lo) * i as f64 / (steps - 1) as f64;
        let x0 = rect.min.x + rect.width() * i as f32 / steps as f32;
        let r = Rect::from_min_max(Pos2::new(x0, rect.min.y), Pos2::new(x0 + rect.width() / steps as f32 + 0.5, rect.max.y));
        let [rr, g, b] = heatmap::color(v);
        ui.painter().rect_filled(r, 0.0, Color32::from_rgb(rr, g, b));
    }
    ui.label(RichText::new(format!("{lo:.0}")).small().color(p.weak));
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
