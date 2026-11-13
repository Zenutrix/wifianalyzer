//! Grundriss mit Heatmap und Netzwerktests (Latenz/Durchsatz) pro Messpunkt.
//!
//! - Grundrisse je Stockwerk laden (PNG/JPEG oder Seiten aus einem PDF), Messpunkte per Klick platzieren oder verschieben
//! - Heatmap der Ausleuchtung (IDW-Interpolation, siehe `crate::heatmap`)
//! - Netzwerktest startet beim Öffnen des Messpunkt-Dialogs im Hintergrund und wird dem
//!   Messpunkt zugeordnet, sobald er fertig ist

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, TextureHandle, TextureOptions, Ui, Vec2};

use crate::app::{Page, Sev, WifiApp};
use crate::heatmap;
use crate::measure::{FloorPlan, Layer, PdfSource, Quality};
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
    /// ausgewählte Seiten (Index = Seite)
    selected: Vec<bool>,
    /// Seite für die Nummern-Eingabe (Seiten ohne Vorschaubild)
    page: u32,
    /// als neue Stockwerke hinzufügen (sonst ersetzt die erste Seite das aktuelle)
    add: bool,
}

/// Geladener Grundriss: Dateiname, Stockwerk-Bezeichnung, Bilddaten
type PlanLoad = (String, String, Result<(Vec<u8>, u32, u32), String>, Option<PdfSource>);

/// Auftrag für einen scharf gerenderten Ausschnitt: Grundriss, Quelle, Ausschnitt (0..1), Breite in Pixeln
#[derive(Clone, PartialEq)]
pub(crate) struct SharpReq {
    plan: (usize, String, usize),
    src: PdfSource,
    rect: [f32; 4],
    px: u32,
}

/// Scharf gerenderter Ausschnitt eines PDF-Grundrisses
pub(crate) struct SharpTex {
    req: SharpReq,
    tex: TextureHandle,
}

/// Grundriss-Textur (Schlüssel: Name + Größe der Bilddaten)
pub(crate) struct PlanTex {
    key: (usize, String, usize),
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

/// Datei mit den zuletzt verwendeten Grundrissen (bleibt über einen Neustart erhalten)
fn plans_path() -> std::path::PathBuf {
    crate::settings::app_dir().join("grundrisse.json")
}

pub(crate) fn load_saved_plans() -> Vec<FloorPlan> {
    std::fs::read_to_string(plans_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

/// Kurzer Fingerabdruck der Pläne (ohne die Bilddaten zu hashen)
pub(crate) fn plans_sig(plans: &[FloorPlan]) -> u64 {
    let mut h = DefaultHasher::new();
    for p in plans {
        p.name.hash(&mut h);
        p.label.hash(&mut h);
        p.data.len().hash(&mut h);
        p.width_m.map(f32::to_bits).hash(&mut h);
        p.scale_len_m.map(f32::to_bits).hash(&mut h);
    }
    plans.len().hash(&mut h);
    h.finish()
}

/// Schritt der Vorbereitung: Nummer bzw. Häkchen + Text
fn setup_step(ui: &mut Ui, p: &crate::theme::Palette, n: usize, done: bool, text: &str) {
    let (mark, col) = if done { ("\u{E182}".to_string(), p.success) } else { (format!("{n}."), p.accent) };
    ui.label(RichText::new(mark).strong().color(col));
    ui.label(RichText::new(text).strong().color(p.text));
}

impl WifiApp {
    /// Zuletzt verwendete Grundrisse speichern, kurz nachdem sie sich geändert haben
    pub(super) fn save_plans_if_changed(&mut self) {
        let sig = plans_sig(&self.last_plans);
        if sig == self.plans_saved_sig {
            self.plans_pending = None;
            return;
        }
        match self.plans_pending {
            Some((s, t)) if s == sig && t.elapsed() > Duration::from_secs(2) => {
                let res = serde_json::to_string(&self.last_plans).map_err(|e| e.to_string()).and_then(|txt| {
                    std::fs::create_dir_all(crate::settings::app_dir()).and_then(|_| std::fs::write(plans_path(), txt)).map_err(|e| e.to_string())
                });
                if let Err(e) = res {
                    self.add_log(Sev::Warning, tf!("Grundrisse konnten nicht gespeichert werden: {e}", "Could not save the floor plans: {e}"));
                }
                self.plans_saved_sig = sig;
                self.plans_pending = None;
            }
            Some((s, _)) if s == sig => {}
            _ => self.plans_pending = Some((sig, std::time::Instant::now())),
        }
    }

    /// Maßstab-Werkzeug für ein Stockwerk starten
    pub(super) fn plan_start_scale(&mut self, floor: usize) {
        if let Some(m) = self.meas.as_mut() {
            if floor < m.plans.len() {
                m.floor = floor;
            }
        }
        self.plan_scale = Some(ScaleDraft::default());
        self.plan_sel = None;
        self.heat_tex = None;
    }

    /// Vorbereitung der Grundriss-Messung: Pläne laden → Maßstab je Stockwerk → starten.
    /// Liefert true, wenn „ohne Maßstab starten“ geklickt wurde.
    pub(super) fn plan_setup_strip(&mut self, ui: &mut Ui, ssid_ok: bool) -> bool {
        let p = pal(ui);
        let Some(m) = &self.meas else { return false };
        let floors: Vec<(String, Option<f32>)> = m.plans.iter().map(|f| (f.title().to_string(), f.width_m)).collect();
        let n = floors.len();
        let all_scaled = n > 0 && floors.iter().all(|f| f.1.is_some());
        let (mut goto_scale, mut load, mut start_anyway) = (None, false, false);
        crate::widgets::card(ui, None, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                setup_step(ui, &p, 1, n > 0, t!("Grundrisse laden", "Load floor plans"));
                if n == 0 {
                    if ui.button(t!("\u{E31A} Bild oder PDF …", "\u{E31A} Image or PDF …")).clicked() {
                        load = true;
                    }
                } else {
                    ui.label(RichText::new(if n == 1 { t!("1 Stockwerk", "1 floor").to_string() } else { tf!("{n} Stockwerke", "{n} floors") }).color(p.weak));
                }
                ui.separator();
                setup_step(ui, &p, 2, all_scaled, t!("Maßstab je Stockwerk", "Scale per floor"));
                if n == 0 {
                    ui.label(RichText::new("–").color(p.weak));
                }
                for (i, (title, w)) in floors.iter().enumerate() {
                    let (txt, col, tip) = match w {
                        Some(w) => (format!("\u{E182} {title}"), p.success, tf!("Plan ist {w:.1} m breit – klicken, um den Maßstab neu zu setzen", "Plan is {w:.1} m wide – click to set the scale again")),
                        None => (format!("\u{E6B8} {title}"), p.warning, t!("Noch kein Maßstab – klicken und eine bekannte Strecke abmessen, z. B. eine Tür", "No scale yet – click and measure a known distance, e.g. a door").to_string()),
                    };
                    if ui.add(egui::Button::new(RichText::new(txt).color(col)).small()).on_hover_text(tip).clicked() {
                        goto_scale = Some(i);
                    }
                }
                ui.separator();
                setup_step(ui, &p, 3, false, t!("Messung starten", "Start the survey"));
                ui.label(RichText::new(t!("dann an deiner Position in den Plan klicken", "then click your position on the plan")).color(p.weak));
                if n > 0 && !all_scaled && ssid_ok && ui.link(RichText::new(t!("ohne Maßstab starten", "start without scale")).small()).clicked() {
                    start_anyway = true;
                }
                // im Fluss (nicht rechtsbündig), damit beim Umbrechen nichts überlappt
                ui.separator();
                ui.menu_button(t!("\u{E256} Gespeicherte Messungen", "\u{E256} Saved surveys"), |ui| {
                    ui.set_min_width(420.0);
                    self.recent_surveys_ui(ui, true);
                });
            });
        });
        if let Some(i) = goto_scale {
            self.plan_start_scale(i);
        }
        if load {
            self.load_plan(&ui.ctx().clone(), true);
        }
        start_anyway
    }

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

    /// Grundriss laden. `add` = als neues Stockwerk, sonst ersetzt er den des aktuellen Stockwerks.
    pub(super) fn load_plan(&mut self, ctx: &egui::Context, add: bool) {
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
            self.open_pdf_plan(ctx, path, name, add);
            return;
        }
        let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let res = std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| heatmap::prepare_plan(&b));
        self.apply_plans(vec![(name, stem, res, None)], add);
    }

    /// PDF: einseitig direkt übernehmen, sonst Seitenauswahl mit Vorschaubildern
    fn open_pdf_plan(&mut self, ctx: &egui::Context, path: std::path::PathBuf, name: String, add: bool) {
        match crate::pdfimport::thumbnails(path.clone(), 360) {
            Ok((0, _)) => self.toast(Sev::Critical, t!("Das PDF enthält keine Seiten", "The PDF has no pages").into()),
            Ok((1, _)) => self.load_pdf_pages(path, name, vec![0], 1, add),
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
                let mut selected = vec![false; pages as usize];
                selected[0] = true;
                self.pdf_pick = Some(PdfPick { path, name, pages, thumbs, selected, page: 0, add });
            }
            Err(e) => self.toast(Sev::Critical, tf!("PDF kann nicht geladen werden: {e}", "Cannot load PDF: {e}")),
        }
    }

    /// PDF-Seiten in voller Auflösung rendern und als Grundrisse übernehmen (je Seite ein Stockwerk)
    fn load_pdf_pages(&mut self, path: std::path::PathBuf, name: String, pages: Vec<u32>, total: u32, add: bool) {
        let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let items = pages
            .into_iter()
            .map(|page| {
                let res = crate::pdfimport::render_page(path.clone(), page, 2400).and_then(|png| heatmap::prepare_plan(&png));
                let src = Some(PdfSource { path: path.to_string_lossy().to_string(), page, size: file_size });
                if total > 1 {
                    (tf!("{name} – Seite {}", "{name} – page {}", page + 1), tf!("Seite {}", "Page {}", page + 1), res, src)
                } else {
                    (name.clone(), stem.clone(), res, src)
                }
            })
            .collect();
        self.apply_plans(items, add);
    }

    /// Geladene Grundrisse übernehmen: der erste ersetzt (wenn nicht `add`) den des aktuellen
    /// Stockwerks, alle weiteren werden als neue Stockwerke angehängt
    fn apply_plans(&mut self, items: Vec<PlanLoad>, add: bool) {
        let mut errors = Vec::new();
        let mut loaded: Vec<(FloorPlan, u32, u32)> = Vec::new();
        for (name, label, res, src) in items {
            match res {
                Ok((data, w, h)) => {
                    let mut plan = FloorPlan::new(name, data);
                    plan.label = label;
                    plan.pdf = src;
                    loaded.push((plan, w, h));
                }
                Err(e) => errors.push(e),
            }
        }
        if let Some(e) = errors.first() {
            self.toast(Sev::Critical, tf!("Grundriss kann nicht geladen werden: {e}", "Cannot load floor plan: {e}"));
        }
        if loaded.is_empty() {
            return;
        }
        let n = loaded.len();
        let first = (loaded[0].0.title().to_string(), loaded[0].1, loaded[0].2);
        let Some(m) = self.meas.as_mut() else { return };
        for (i, (plan, _, _)) in loaded.into_iter().enumerate() {
            if i == 0 && !add && !m.plans.is_empty() {
                let f = m.floor;
                m.plans[f] = plan;
            } else {
                m.plans.push(plan);
                if i == 0 {
                    m.floor = m.plans.len() - 1;
                }
            }
        }
        self.plan_sel = m.points.iter().find(|p| p.pos.is_none()).map(|p| p.id);
        self.last_plans = m.plans.clone();
        // ohne Maßstab: gleich das Maßstab-Werkzeug öffnen
        self.plan_scale = m.plan().filter(|p| p.width_m.is_none()).map(|_| ScaleDraft::default());
        self.heat_tex = None;
        self.survey_changed();
        let (title, w, h) = first;
        if n == 1 {
            self.toast(Sev::Info, tf!("Grundriss geladen: {title} ({w} × {h} px)", "Floor plan loaded: {title} ({w} × {h} px)"));
        } else {
            self.toast(Sev::Info, tf!("{n} Stockwerke geladen", "{n} floors loaded"));
        }
    }

    /// Fenster: Seiten eines mehrseitigen PDFs wählen (jede gewählte Seite = ein Stockwerk)
    fn pdf_pick_window(&mut self, ctx: &egui::Context) {
        let Some(pick) = self.pdf_pick.as_mut() else { return };
        let mut open = true;
        let mut chosen: Option<Vec<u32>> = None;
        let screen = ctx.screen_rect();
        egui::Window::new(RichText::new(t!("\u{E702} Seiten für den Grundriss wählen", "\u{E702} Choose the floor plan pages")).strong())
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([screen.width().min(1300.0) * 0.8, screen.height() * 0.7])
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                let p = pal(ui);
                ui.label(
                    RichText::new(tf!(
                        "{} · {} Seiten – Seiten anklicken, jede gewählte Seite wird ein Stockwerk. Doppelklick übernimmt nur diese Seite.",
                        "{} · {} pages – click pages, each selected page becomes a floor. Double-click uses just that page.",
                        pick.name,
                        pick.pages
                    ))
                    .color(p.weak),
                );
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
                            let sel = pick.selected[i];
                            let bg = if sel { p.accent.gamma_multiply(0.18) } else if resp.hovered() { p.surface } else { Color32::TRANSPARENT };
                            ui.painter().rect(rect, 10.0, bg, Stroke::new(if sel { 2.0f32 } else { 1.0 }, if sel { p.accent } else { p.border }));
                            let img_rect = Rect::from_center_size(Pos2::new(rect.center().x, rect.top() + 8.0 + box_px / 2.0), img_size);
                            ui.painter().rect_filled(img_rect, 2.0, Color32::WHITE);
                            ui.painter().image(tex.id(), img_rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                            // Auswahl-Häkchen oben rechts
                            let cb = Rect::from_center_size(Pos2::new(rect.right() - 18.0, rect.top() + 18.0), Vec2::splat(20.0));
                            ui.painter().rect(cb, 5.0, if sel { p.accent } else { p.surface2 }, Stroke::new(1.0f32, if sel { p.accent } else { p.border }));
                            if sel {
                                ui.painter().text(cb.center(), Align2::CENTER_CENTER, "\u{E182}", FontId::proportional(14.0), Color32::WHITE);
                            }
                            ui.painter().text(
                                Pos2::new(rect.center().x, rect.bottom() - 16.0),
                                Align2::CENTER_CENTER,
                                tf!("Seite {}", "Page {}", i + 1),
                                FontId::proportional(13.0),
                                if sel { p.accent } else { p.text },
                            );
                            if resp.double_clicked() {
                                chosen = Some(vec![i as u32]);
                            } else if resp.clicked() {
                                pick.selected[i] = !sel;
                            }
                            resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                        }
                    });
                });
                ui.add_space(8.0);
                ui.separator();
                let sel: Vec<u32> = pick.selected.iter().enumerate().filter(|(_, s)| **s).map(|(i, _)| i as u32).collect();
                ui.horizontal(|ui| {
                    let all = sel.len() == pick.selected.len();
                    if ui.button(if all { t!("Keine", "None") } else { t!("Alle Seiten", "All pages") }).clicked() {
                        pick.selected.iter_mut().for_each(|s| *s = !all);
                    }
                    if pick.pages as usize > pick.thumbs.len() {
                        ui.separator();
                        ui.label(RichText::new(t!("Seite", "Page")).color(p.weak));
                        let mut n = pick.page + 1;
                        if ui.add(egui::DragValue::new(&mut n).range(1..=pick.pages)).changed() {
                            pick.page = n - 1;
                        }
                        let on = pick.selected[pick.page as usize];
                        if ui.button(if on { t!("abwählen", "deselect") } else { t!("auswählen", "select") }).clicked() {
                            pick.selected[pick.page as usize] = !on;
                        }
                        ui.label(RichText::new(tf!("(Vorschau nur für die ersten {})", "(preview for the first {} only)", pick.thumbs.len())).color(p.weak));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let label = match sel.as_slice() {
                            [] => t!("Keine Seite gewählt", "No page selected").to_string(),
                            [one] => tf!("Seite {} übernehmen", "Use page {}", one + 1),
                            many => tf!("{} Seiten als Stockwerke übernehmen", "Use {} pages as floors", many.len()),
                        };
                        let resp = ui.add_enabled_ui(!sel.is_empty(), |ui| primary_button(ui, &label, p.accent)).inner;
                        if resp.clicked() && !sel.is_empty() {
                            chosen = Some(sel.clone());
                        }
                        if sel.len() > 1 {
                            ui.label(RichText::new(sel.iter().map(|i| (i + 1).to_string()).collect::<Vec<_>>().join(", ")).color(p.weak));
                        }
                    });
                });
            });
        if let Some(pages) = chosen {
            if let Some(pick) = self.pdf_pick.take() {
                self.load_pdf_pages(pick.path, pick.name, pages, pick.pages, pick.add);
            }
        } else if !open {
            self.pdf_pick = None;
        }
    }

    /// Stand der Messpunkte vor einer Änderung merken (Strg+Z)
    pub(super) fn push_undo(&mut self, what: &str) {
        if let Some(m) = &self.meas {
            self.plan_undo.push((m.started_wall, what.to_string(), m.points.clone()));
            if self.plan_undo.len() > 40 {
                self.plan_undo.remove(0);
            }
        }
    }

    /// Beschreibung der Änderung, die „Rückgängig“ zurücknehmen würde
    pub(super) fn undo_label(&self) -> Option<String> {
        let w = self.meas.as_ref()?.started_wall;
        self.plan_undo.iter().rev().find(|u| u.0 == w).map(|u| u.1.clone())
    }

    /// Letzte Änderung an den Messpunkten zurücknehmen
    pub(super) fn undo_points(&mut self) {
        let Some(w) = self.meas.as_ref().map(|m| m.started_wall) else { return };
        // Einträge anderer Messungen verwerfen
        while let Some(u) = self.plan_undo.pop() {
            if u.0 != w {
                continue;
            }
            if let Some(m) = self.meas.as_mut() {
                m.restore_points(u.2);
            }
            self.plan_drag = None;
            self.plan_sel = None;
            self.heat_tex = None;
            self.survey_changed();
            self.toast(Sev::Info, tf!("Rückgängig: {}", "Undone: {}", u.1));
            return;
        }
    }

    /// Aktuelles Stockwerk samt Grundriss entfernen
    fn remove_plan(&mut self) {
        if let Some(m) = self.meas.as_mut() {
            let f = m.floor;
            m.remove_floor(f);
            self.last_plans = m.plans.clone();
        }
        self.plan_tex = None;
        self.heat_tex = None;
        self.plan_scale = None;
        self.survey_changed();
    }

    /// Reiter für die Stockwerke (Wechseln, Umbenennen, Hinzufügen, Entfernen)
    fn floor_tabs(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        let Some(m) = self.meas.as_mut() else { return };
        let mut switch: Option<usize> = None;
        let mut remove: Option<usize> = None;
        let mut add = false;
        let mut changed = false;
        let cur = m.floor;
        let n = m.plans.len();
        let counts: Vec<usize> = (0..n).map(|f| m.points.iter().filter(|pt| pt.floor == f && pt.pos.is_some()).count()).collect();
        let confirm = self.floor_remove_confirm;
        let mut ask_confirm: Option<usize> = None;
        ui.horizontal(|ui| {
            ui.label(RichText::new(t!("\u{E466} Stockwerk", "\u{E466} Floor")).color(p.weak));
            for (i, fp) in m.plans.iter_mut().enumerate() {
                let sel = i == cur;
                let label = if counts[i] > 0 { format!("{}  {}", fp.title(), counts[i]) } else { fp.title().to_string() };
                let text = RichText::new(label).color(if sel { Color32::WHITE } else { p.text });
                let b = egui::Button::new(text).fill(if sel { p.accent } else { p.surface2 }).stroke(Stroke::new(1.0f32, if sel { p.accent } else { p.border }));
                let resp = ui.add(b).on_hover_text(tf!(
                    "{}\n{} Messpunkte platziert · Rechtsklick: umbenennen / entfernen",
                    "{}\n{} points placed · right-click: rename / remove",
                    fp.name,
                    counts[i]
                ));
                if resp.clicked() {
                    switch = Some(i);
                }
                resp.context_menu(|ui| {
                    ui.label(RichText::new(t!("Bezeichnung", "Name")).small().color(p.weak));
                    let r = ui.add(egui::TextEdit::singleline(&mut fp.label).desired_width(180.0).hint_text(t!("z. B. EG, 1. OG", "e.g. Ground floor")));
                    if r.changed() {
                        changed = true;
                    }
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        ui.close_menu();
                    }
                    ui.separator();
                    if confirm == Some(i) {
                        ui.label(RichText::new(tf!("{} Messpunkte verlieren ihre Position.", "{} points will lose their position.", counts[i])).small().color(p.warning));
                        if ui.button(RichText::new(t!("\u{E4A6} Wirklich entfernen", "\u{E4A6} Really remove")).color(p.danger)).clicked() {
                            remove = Some(i);
                            ui.close_menu();
                        }
                    } else if ui.button(t!("\u{E4A6} Stockwerk entfernen …", "\u{E4A6} Remove floor …")).clicked() {
                        ask_confirm = Some(i);
                    }
                });
            }
            if ui.button(t!("+ Stockwerk", "+ Floor")).on_hover_text(t!("Weiteren Grundriss (Bild oder PDF-Seiten) als Stockwerk hinzufügen", "Add another floor plan (image or PDF pages) as a floor")).clicked() {
                add = true;
            }
        });
        if ask_confirm.is_some() || remove.is_some() || switch.is_some() {
            self.floor_remove_confirm = ask_confirm;
        }
        if let Some(i) = switch {
            if i != m.floor {
                m.floor = i;
                self.plan_scale = None;
                self.heat_tex = None;
            }
        }
        if changed {
            self.last_plans = m.plans.clone();
            self.survey_changed();
        }
        if let Some(i) = remove {
            if let Some(m) = self.meas.as_mut() {
                m.floor = i;
            }
            self.remove_plan();
        }
        if add {
            self.load_plan(&ui.ctx().clone(), true);
        }
    }

    /// Textur des Grundrisses (wird bei Bedarf neu erzeugt)
    pub(super) fn plan_texture(&mut self, ctx: &egui::Context) -> Option<(egui::TextureId, [usize; 2])> {
        let m = self.meas.as_ref()?;
        let plan = m.plan()?;
        let key = (m.floor, plan.name.clone(), plan.data.len());
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

    /// PDF-Grundriss beim Zoomen scharf: rendert den sichtbaren Ausschnitt im Hintergrund direkt
    /// aus dem PDF nach, sobald die Ansicht ruht. Liefert Textur und Lage auf dem Bildschirm.
    fn sharp_plan(&mut self, ctx: &egui::Context, img: Rect, visible: Rect, base_px: usize) -> Option<(egui::TextureId, Rect)> {
        let ident = self.meas.as_ref().and_then(|m| m.plan().map(|p| ((m.floor, p.name.clone(), p.data.len()), p.pdf.clone())));
        let Some((plan_key, Some(src))) = ident else {
            self.plan_sharp = None;
            self.plan_sharp_want = None;
            return None;
        };
        // fertigen Auftrag übernehmen
        if let Some((req, rx)) = &self.plan_sharp_job {
            if let Ok(res) = rx.try_recv() {
                match res {
                    Some(im) => {
                        let color = egui::ColorImage::from_rgba_unmultiplied([im.width() as usize, im.height() as usize], im.as_raw());
                        self.plan_sharp = Some(SharpTex { req: req.clone(), tex: ctx.load_texture("floor_plan_sharp", color, TextureOptions::LINEAR) });
                    }
                    None => self.plan_sharp_off = Some(req.plan.clone()),
                }
                self.plan_sharp_job = None;
            }
        }
        if self.plan_sharp.as_ref().is_some_and(|s| s.req.plan != plan_key || s.req.src != src) {
            self.plan_sharp = None;
        }
        let ppp = ctx.pixels_per_point();
        // nur nötig, wenn das gespeicherte Bild vergrößert gezeigt wird
        let upscaled = img.width() * ppp > base_px as f32 * 1.15;
        if !upscaled || visible.width() < 8.0 || visible.height() < 8.0 || self.plan_sharp_off.as_ref() == Some(&plan_key) {
            self.plan_sharp = None;
            self.plan_sharp_want = None;
            return None;
        }
        // sichtbarer Ausschnitt in Plan-Anteilen
        let vis = [
            ((visible.min.x - img.min.x) / img.width()).clamp(0.0, 1.0),
            ((visible.min.y - img.min.y) / img.height()).clamp(0.0, 1.0),
            ((visible.max.x - img.min.x) / img.width()).clamp(0.0, 1.0),
            ((visible.max.y - img.min.y) / img.height()).clamp(0.0, 1.0),
        ];
        // passt der vorhandene Ausschnitt noch? (deckt die Ansicht ab, Auflösung stimmt ungefähr)
        let fits = self.plan_sharp.as_ref().is_some_and(|s| {
            let r = s.req.rect;
            let density = s.req.px as f32 / ((r[2] - r[0]) * img.width() * ppp);
            r[0] <= vis[0] + 1e-4 && r[1] <= vis[1] + 1e-4 && r[2] >= vis[2] - 1e-4 && r[3] >= vis[3] - 1e-4 && (0.85..=2.5).contains(&density)
        });
        if fits {
            self.plan_sharp_want = None;
        } else {
            // Ausschnitt mit Rand, damit kleines Verschieben nicht sofort neu rendert
            let (mx, my) = ((vis[2] - vis[0]) * 0.2, (vis[3] - vis[1]) * 0.2);
            let q = 400.0;
            let rect = [
                ((vis[0] - mx).max(0.0) * q).floor() / q,
                ((vis[1] - my).max(0.0) * q).floor() / q,
                ((vis[2] + mx).min(1.0) * q).ceil() / q,
                ((vis[3] + my).min(1.0) * q).ceil() / q,
            ];
            let (w, h) = ((rect[2] - rect[0]) * img.width() * ppp, (rect[3] - rect[1]) * img.height() * ppp);
            let limit = 4096.0 / w.max(h).max(1.0);
            let px = ((w * limit.min(1.0) / 32.0).ceil() * 32.0).max(64.0) as u32;
            let req = SharpReq { plan: plan_key, src, rect, px };
            if self.plan_sharp_want.as_ref().map(|w| &w.0) != Some(&req) {
                self.plan_sharp_want = Some((req.clone(), Instant::now()));
            }
            let rested = self.plan_sharp_want.as_ref().is_some_and(|w| w.1.elapsed() >= Duration::from_millis(160));
            // genau dieser Ausschnitt liegt schon vor (z. B. an der Auflösungsgrenze): nichts zu tun
            let have = self.plan_sharp.as_ref().is_some_and(|s| s.req == req);
            if have {
                self.plan_sharp_want = None;
            } else if self.plan_sharp_job.is_none() && rested {
                let (tx, rx) = channel();
                let (job, c) = (req.clone(), ctx.clone());
                std::thread::spawn(move || {
                    let path = std::path::PathBuf::from(&job.src.path);
                    let same = std::fs::metadata(&path).map(|m| m.len() == job.src.size).unwrap_or(false);
                    let res = if same { crate::pdfimport::render_region(path, job.src.page, job.rect, job.px).ok() } else { None };
                    let _ = tx.send(res);
                    c.request_repaint();
                });
                self.plan_sharp_job = Some((req, rx));
            } else {
                ctx.request_repaint_after(Duration::from_millis(80));
            }
        }
        self.plan_sharp.as_ref().map(|s| {
            let r = s.req.rect;
            let at = |x: f32, y: f32| Pos2::new(img.min.x + x * img.width(), img.min.y + y * img.height());
            (s.tex.id(), Rect::from_min_max(at(r[0], r[1]), at(r[2], r[3])))
        })
    }

    /// Heatmap-Textur passend zu den platzierten Messpunkten
    fn heat_texture(&mut self, ctx: &egui::Context, size: [usize; 2]) -> Option<egui::TextureId> {
        let m = self.meas.as_ref()?;
        let pts = m.heat_points(m.floor, self.plan_layer);
        if pts.is_empty() {
            return None;
        }
        let width_m = m.plan().and_then(|p| p.width_m);
        let mut h = DefaultHasher::new();
        m.floor.hash(&mut h);
        width_m.map(f32::to_bits).hash(&mut h);
        heatmap::scale_to_string(&heatmap::scale()).hash(&mut h);
        heatmap::no_signal_setting().hash(&mut h);
        self.settings.heat_opacity.to_bits().hash(&mut h);
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
            // Deckkraft nach Einstellung (die Berichte verwenden die feste Standard-Deckkraft)
            let k = self.settings.heat_opacity / heatmap::OPACITY;
            let rgba: Vec<u8> = field.iter().flat_map(|c| [c[0], c[1], c[2], (c[3] as f32 * k).min(255.0) as u8]).collect();
            let img = egui::ColorImage::from_rgba_unmultiplied([w, hgt], &rgba);
            let tex = ctx.load_texture("heatmap", img, TextureOptions::LINEAR);
            self.heat_tex = Some((key, tex));
        }
        self.heat_tex.as_ref().map(|t| t.1.id())
    }

    // ---------------------------------------------------------------- Grundriss-Ansicht

    pub(super) fn plan_view(&mut self, ui: &mut Ui, height: f32) {
        let p = pal(ui);
        let top = ui.cursor().top();
        self.pdf_pick_window(&ui.ctx().clone());
        let Some(m) = &self.meas else { return };
        if m.plans.is_empty() {
            let rect = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover()).0;
            ui.painter().rect(rect, 12.0, p.surface, Stroke::new(1.0f32, p.border));
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(20.0)).layout(egui::Layout::top_down(egui::Align::Center)));
            child.add_space((height / 2.0 - 90.0).max(10.0));
            child.label(RichText::new("\u{E31A}").size(38.0).color(p.weak));
            child.label(RichText::new(t!("Noch kein Grundriss", "No floor plan yet")).size(18.0).strong().color(p.text));
            child.label(
                RichText::new(t!(
                    "Plan als Bild oder PDF laden (PNG, JPG, Foto vom Fluchtplan oder PDF). Bei mehreren Seiten wird jede gewählte Seite ein Stockwerk; weitere Stockwerke lassen sich jederzeit hinzufügen. Danach die Messpunkte per Klick platzieren – die App färbt die Ausleuchtung ein.",
                    "Load a plan as image or PDF (PNG, JPG, a photo of the evacuation plan or PDF). With several pages each selected page becomes a floor; more floors can be added at any time. Then place the points by clicking – the app colours the coverage."
                ))
                .color(p.weak),
            );
            child.add_space(10.0);
            if primary_button(&mut child, t!("\u{E31A} Grundriss laden", "\u{E31A} Load floor plan"), p.accent).clicked() {
                self.load_plan(&ui.ctx().clone(), true);
            }
            return;
        }

        // Werkzeugleiste
        let draft = m.draft;
        let unplaced = m.points.iter().filter(|pt| pt.pos.is_none()).count();
        let sel_no = self.plan_sel.and_then(|id| m.points.iter().position(|pt| pt.id == id)).map(|i| i + 1);
        let with_data = m.layers_with_data(m.floor);
        let (width_m, plan_px) = (m.plan().and_then(|p| p.width_m), self.plan_tex.as_ref().map(|t| t.size));
        // Statuszeile: Maßstab bzw. Hinweis
        let mut apply_scale: Option<f32> = None;
        let mut cancel_scale = false;
        let mut zoom_to: Option<f32> = None;
        ui.horizontal(|ui| {
            // Stockwerke und Hinweis teilen sich eine Zeile – der Plan bekommt den Platz
            self.floor_tabs(ui);
            ui.separator();
            if let Some(sd) = self.plan_scale.as_mut() {
                ui.label(RichText::new("\u{E6B8}").color(p.accent));
                match (sd.a, sd.b) {
                    (None, _) => {
                        ui.add(egui::Label::new(RichText::new(t!("Maßstab: Anfang einer bekannten Strecke anklicken – z. B. eine Tür (meist 0,80–1,00 m)", "Scale: click the start of a known distance – e.g. a door (usually 0.80–1.00 m)")).color(p.accent)).truncate());
                    }
                    (Some(_), None) => {
                        ui.label(RichText::new(t!("Jetzt auf das Ende der Strecke klicken", "Now click the end of the distance")).color(p.accent));
                    }
                    (Some(_), Some(_)) => {
                        ui.label(RichText::new(t!("Länge der Strecke:", "Length of the distance:")).color(p.text));
                        let r = ui.add(egui::TextEdit::singleline(&mut sd.meters).desired_width(70.0).hint_text(t!("z. B. 0,9", "e.g. 0.9")));
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
                        ui.label(RichText::new(tf!("\u{E6B8} {wm:.1} × {hm:.1} m", "\u{E6B8} {wm:.1} × {hm:.1} m")).color(p.weak)).on_hover_text(t!("Größe des Plans laut Maßstab", "Plan size according to the scale"));
                    }
                    None => {
                        ui.label(RichText::new(t!("\u{E6B8} Kein Maßstab", "\u{E6B8} No scale")).color(p.warning))
                            .on_hover_text(t!("Angenommen werden 40 m Breite. Für eine genaue Heatmap „Maßstab“ setzen.", "Assuming 40 m width. Set the \"Scale\" for an accurate heatmap."));
                    }
                }
                ui.separator();
                let hint = if draft {
                    t!("Maßstab je Stockwerk setzen, dann „Messung starten“", "Set the scale per floor, then \"Start survey\"").into()
                } else {
                    match sel_no {
                    Some(n) => tf!("Klick in den Plan setzt Messpunkt {n}", "Click into the plan to place point {n}"),
                    None if unplaced > 0 => tf!("{unplaced} Messpunkte ohne Position – in der Liste anklicken und dann im Plan setzen", "{unplaced} points without position – click one in the list, then place it"),
                    None => t!("Klick in den Plan = Messpunkt · Punkte lassen sich verschieben", "Click into the plan = point · points can be dragged").into(),
                    }
                };
                ui.add(egui::Label::new(RichText::new(hint).color(if sel_no.is_some() { p.accent } else { p.weak })).truncate());
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
                        if let Some(m) = self.meas.as_mut() {
                            if let Some(pl) = m.plan_mut() {
                                pl.width_m = Some(wm);
                                pl.scale_line = Some([a, b]);
                                pl.scale_len_m = Some(meters);
                            }
                            self.last_plans = m.plans.clone();
                        }
                        self.heat_tex = None;
                        self.survey_changed();
                        // Vorbereitung: weiter mit dem nächsten Stockwerk ohne Maßstab
                        let next = self.meas.as_ref().filter(|m| m.draft).and_then(|m| m.plans.iter().position(|f| f.width_m.is_none()).map(|i| (i, m.plans[i].title().to_string())));
                        match next {
                            Some((i, name)) => {
                                self.plan_start_scale(i);
                                self.toast(Sev::Info, tf!("Maßstab gesetzt ({wm:.1} m breit) – jetzt „{name}“", "Scale set ({wm:.1} m wide) – now \"{name}\""));
                            }
                            None => self.toast(Sev::Info, tf!("Maßstab gesetzt: Plan ist {wm:.1} m breit", "Scale set: plan is {wm:.1} m wide")),
                        }
                    }
                }
            }
        }
        self.scale_editor_window(ui.ctx());

        let ctx = ui.ctx().clone();
        let Some((tex, size)) = self.plan_texture(&ctx) else {
            ui.label(RichText::new(t!("Grundriss kann nicht angezeigt werden", "Floor plan cannot be shown")).color(p.danger));
            return;
        };
        let heat_tex = if self.plan_heat { self.heat_texture(&ctx, size) } else { None };
        // Höhe abzüglich der Zeile mit Reitern und Hinweis
        let h = (height - (ui.cursor().top() - top) - 4.0).max(160.0);
        let (area, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), h), Sense::click_and_drag());
        // oben Platz für die schwebende Werkzeugleiste lassen, damit sie den Plan nicht verdeckt
        let base = fit_rect(Rect::from_min_max(Pos2::new(area.min.x, area.min.y + 50.0), area.max), size);
        // anderes Stockwerk: wieder eingepasst anzeigen
        let view_key = self.meas.as_ref().map(|m| (m.floor, m.plans.len())).unwrap_or_default();
        if self.plan_view_key != view_key {
            self.plan_view_key = view_key;
            self.plan_zoom = 1.0;
            self.plan_pan = Vec2::ZERO;
        }
        if !ui.ctx().wants_keyboard_input() && ui.input(|i| i.key_pressed(egui::Key::Num0) && i.modifiers.is_none()) {
            zoom_to = Some(1.0);
        }
        // Zoom per Mausrad: der Punkt unter dem Mauszeiger bleibt stehen
        let mut zoom_at: Option<(f32, Pos2)> = zoom_to.map(|z| (z, area.center()));
        if let Some(hp) = resp.hover_pos() {
            let (scroll, zd) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            let f = zd * (scroll * 0.0015).exp();
            if (f - 1.0).abs() > 1e-4 {
                zoom_at = Some((self.plan_zoom * f, hp));
                ui.ctx().input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
            }
        }
        if let Some((z, at)) = zoom_at {
            let (old, new) = (self.plan_zoom, z.clamp(1.0, 8.0));
            let rel = at - (base.center() + self.plan_pan);
            self.plan_pan += rel - rel * (new / old);
            self.plan_zoom = new;
            if new <= 1.0 {
                self.plan_pan = Vec2::ZERO;
            }
        }
        let max_pan = base.size() * (self.plan_zoom - 1.0) / 2.0;
        self.plan_pan = Vec2::new(self.plan_pan.x.clamp(-max_pan.x, max_pan.x), self.plan_pan.y.clamp(-max_pan.y, max_pan.y));
        let img = Rect::from_center_size(base.center() + self.plan_pan, base.size() * self.plan_zoom);
        let visible = img.intersect(area);
        let painter = ui.painter_at(area);
        painter.rect_filled(area, 10.0, p.surface);
        painter.image(tex, img, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        // PDF-Grundriss: gezoomten Ausschnitt scharf darüberlegen
        if let Some((st, at)) = self.sharp_plan(&ctx, img, visible, size[0]) {
            painter.image(st, at, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }
        if let Some(ht) = heat_tex {
            painter.image(ht, img, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }
        painter.rect_stroke(img, 0.0, Stroke::new(1.0f32, p.border));
        if heat_tex.is_some() {
            legend_overlay(&painter, visible);
        }

        let Some(m) = &self.meas else { return };
        let markers: Vec<(u64, usize, Pos2, Option<f64>, String)> = m
            .points
            .iter()
            .enumerate()
            .filter(|(_, pt)| pt.floor == m.floor)
            .filter_map(|(i, pt)| pt.pos.map(|pos| (pt.id, i + 1, to_screen(img, pos), pt.value_for(self.plan_layer), pt.name.clone())))
            .collect();
        let floor = m.floor;
        let heat_pts = m.heat_points(floor, self.plan_layer);
        let plan_m = m.plan().map(|pl| (pl.width_m, pl.scale_line)).unwrap_or((None, None));
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
            let base = Pos2::new(visible.min.x + 14.0, visible.max.y - 14.0);
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
        if resp.drag_started_by(egui::PointerButton::Primary) && !scaling {
            self.plan_drag = resp.interact_pointer_pos().and_then(hit);
            if self.plan_drag.is_some() {
                self.push_undo(t!("Punkt verschoben", "Point moved"));
            }
        }
        if let (Some(id), Some(pos)) = (self.plan_drag, resp.interact_pointer_pos()) {
            if resp.dragged_by(egui::PointerButton::Primary) && img.contains(pos) {
                set_pos = Some((id, to_norm(img, pos)));
            }
        }
        // Verschieben: auf freier Fläche ziehen oder mit rechter/mittlerer Maustaste
        let panning = resp.dragged() && self.plan_drag.is_none() && self.plan_zoom > 1.0;
        if panning {
            self.plan_pan += resp.drag_delta();
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
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
                            self.push_undo(t!("Punkt gesetzt", "Point placed"));
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
        if let Some(pos) = resp.hover_pos().filter(|_| !panning) {
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
                            tf!("{} · kein brauchbares Signal (< {:.0} dBm)", "{} · no usable signal (< {:.0} dBm)", self.plan_layer.label(), heatmap::scale_min())
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
                pt.floor = floor;
            }
        }
        let (mut change, mut start_scale, mut toggle_table) = (false, false, false);
        let full_table = self.plan_full_table;
        let mut heat = self.plan_heat;
        let mut layer = self.plan_layer;
        let mut opacity = self.settings.heat_opacity;
        // schwebende Werkzeugleiste oben links im Plan
        let mut tb = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(area.min + Vec2::new(8.0, 8.0), Pos2::new(area.max.x - 210.0, area.min.y + 46.0))).layout(egui::Layout::left_to_right(egui::Align::Min)));
        overlay_frame(&p).show(&mut tb, |ui| {
            ui.horizontal(|ui| {
            ui.checkbox(&mut heat, t!("Heatmap", "Heatmap"));
            ui.add_enabled_ui(heat, |ui| {
                ui.spacing_mut().slider_width = 70.0;
                ui.add(egui::Slider::new(&mut opacity, 0.2..=0.9).show_value(false)).on_hover_text(tf!(
                    "Deckkraft der Heatmap: {:.0} % – weniger, damit der Plan (Raumnamen) lesbar bleibt",
                    "Heatmap opacity: {:.0} % – lower it to keep the plan (room names) readable",
                    opacity * 100.0
                ));
            });
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
            // bei voller Tabelle zurück zur Liste (sonst steht der Umschalter im Kopf der Liste)
            if !draft && full_table && ui.button(t!("\u{EC24} Liste daneben", "\u{EC24} List beside")).on_hover_text(t!("Kompakte Messpunkt-Liste rechts neben dem Plan", "Compact point list to the right of the plan")).clicked() {
                toggle_table = true;
            }
            });
        });
        // Zoom und Vollbild oben rechts
        let zoom = self.plan_zoom;
        let mut zoom_btn: Option<f32> = None;
        let mut toggle_fs = false;
        let mut zb = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(Pos2::new(area.max.x - 260.0, area.min.y + 8.0), Pos2::new(area.max.x - 8.0, area.min.y + 46.0))).layout(egui::Layout::right_to_left(egui::Align::Min)));
        overlay_frame(&p).show(&mut zb, |ui| {
            ui.horizontal(|ui| {
                let tip = t!(
                    "Mausrad = zoomen · ziehen = verschieben (auf freier Fläche oder mit rechter/mittlerer Maustaste) · Taste 0 = einpassen",
                    "Mouse wheel = zoom · drag = pan (on empty space or with the right/middle button) · key 0 = fit"
                );
                let (icon, tip_fs) = if self.plan_fullscreen { ("\u{E1CE}", t!("Vollbild verlassen (F11 oder Esc)", "Leave full screen (F11 or Esc)")) } else { ("\u{E1D0}", t!("Vollbild für die Begehung (F11)", "Full screen for the walk-through (F11)")) };
                if ui.button(icon).on_hover_text(tip_fs).clicked() {
                    toggle_fs = true;
                }
                if ui.button("\u{E310}").on_hover_text(t!("Vergrößern", "Zoom in")).clicked() {
                    zoom_btn = Some(zoom * 1.25);
                }
                if ui.add(egui::Button::new(RichText::new(format!("{:.0} %", zoom * 100.0)).color(p.weak)).frame(false)).on_hover_text(tip).clicked() {
                    zoom_btn = Some(1.0);
                }
                if ui.add_enabled(zoom > 1.0, egui::Button::new("\u{E30E}")).on_hover_text(t!("Verkleinern", "Zoom out")).clicked() {
                    zoom_btn = Some(zoom / 1.25);
                }
            });
        });
        if let Some(z) = zoom_btn {
            let new = z.clamp(1.0, 8.0);
            self.plan_pan = if new <= 1.0 { Vec2::ZERO } else { self.plan_pan * (new / self.plan_zoom) };
            self.plan_zoom = new;
        }
        if toggle_fs {
            self.set_plan_fullscreen(ui.ctx(), !self.plan_fullscreen);
        }
        self.plan_heat = heat;
        if change {
            self.load_plan(&ui.ctx().clone(), false);
        }
        if toggle_table {
            self.plan_full_table = !self.plan_full_table;
        }
        if opacity != self.settings.heat_opacity {
            self.settings.heat_opacity = opacity;
            self.heat_tex = None;
        }
        if layer != self.plan_layer {
            self.plan_layer = layer;
            self.heat_tex = None;
        }
        if start_scale {
            self.plan_scale = Some(ScaleDraft::default());
            self.plan_sel = None;
        }
        if let Some(pos) = click_measure {
            self.open_point_dialog_at(Some(pos));
        }
    }

    /// Vollbild der Grundriss-Seite ein-/ausschalten (ohne Kopfleiste, Seitenleiste und Statusleiste)
    pub(crate) fn set_plan_fullscreen(&mut self, ctx: &egui::Context, on: bool) {
        self.plan_fullscreen = on;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(on));
    }

    /// Stockwerk anzeigen (Plan, Heatmap und Maßstab-Werkzeug zurücksetzen)
    pub(super) fn show_floor(&mut self, f: usize) {
        if let Some(m) = self.meas.as_mut() {
            if m.floor != f && f < m.plans.len() {
                m.floor = f;
                self.heat_tex = None;
                self.plan_scale = None;
            }
        }
    }

    /// Kompakte Messpunkt-Liste neben dem Plan (breite Bildschirme)
    pub(super) fn plan_points_list(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        let Some(m) = &self.meas else { return };
        let multi = m.plans.len() > 1;
        let floor_names: Vec<String> = m.plans.iter().map(|f| f.title().to_string()).collect();
        let rows: Vec<(u64, usize, String, Option<f64>, Option<usize>, String)> =
            m.points.iter().enumerate().map(|(i, pt)| (pt.id, i + 1, pt.name.clone(), pt.value_for(self.plan_layer), pt.pos.map(|_| pt.floor), pt.note.clone())).collect();
        let running = m.running;
        let sel = self.plan_sel;
        let (mut select, mut delete, mut full) = (None, None, false);
        crate::widgets::card(ui, None, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(ui.available_height() - 2.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(tf!("Messpunkte ({})", "Points ({})", rows.len())).strong().color(p.text));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button(t!("\u{E476} Tabelle", "\u{E476} Table")).on_hover_text(t!("Alle Spalten (AP, Rate, Ping, Notiz …) in einer Tabelle unter dem Plan", "All columns (AP, rate, ping, note …) in a table below the plan")).clicked() {
                        full = true;
                    }
                });
            });
            ui.add_space(4.0);
            if rows.is_empty() {
                ui.label(
                    RichText::new(if running {
                        t!("Noch keine Messpunkte. An deiner Position in den Plan klicken.", "No points yet. Click your position on the plan.")
                    } else {
                        t!("Keine Messpunkte.", "No points.")
                    })
                    .color(p.weak),
                );
                return;
            }
            egui::ScrollArea::vertical().id_salt("plan_points_list").auto_shrink(false).show(ui, |ui| {
                for (id, no, name, v, floor, note) in &rows {
                    let selected = sel == Some(*id);
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 38.0), Sense::click());
                    let painter = ui.painter_at(rect);
                    if selected {
                        painter.rect_filled(rect, 6.0, p.accent.gamma_multiply(0.18));
                    } else if resp.hovered() {
                        painter.rect_filled(rect, 6.0, p.surface2);
                    }
                    draw_marker(&painter, Pos2::new(rect.min.x + 15.0, rect.center().y), 10.0, *no, *v, selected);
                    let right = match v {
                        Some(x) => (format!("{x:.0} dBm"), point_color(Some(*x))),
                        None => ("–".to_string(), p.weak),
                    };
                    let rw = painter.layout_no_wrap(right.0.clone(), FontId::proportional(13.0), right.1).size().x;
                    painter.text(Pos2::new(rect.max.x - 6.0, rect.center().y), Align2::RIGHT_CENTER, &right.0, FontId::proportional(13.0), right.1);
                    let text_rect = Rect::from_min_max(Pos2::new(rect.min.x + 32.0, rect.min.y), Pos2::new(rect.max.x - rw - 14.0, rect.max.y));
                    let tp = painter.with_clip_rect(text_rect);
                    tp.text(Pos2::new(text_rect.min.x, rect.min.y + 11.0), Align2::LEFT_CENTER, name, FontId::proportional(13.5), p.text);
                    let (sub, sub_col) = match floor {
                        None => (t!("ohne Position – anklicken, dann im Plan setzen", "no position – click, then place on the plan").to_string(), p.warning),
                        Some(f) if multi => (floor_names.get(*f).cloned().unwrap_or_default() + if note.is_empty() { "" } else { " · " } + note, p.weak),
                        Some(_) => (note.clone(), p.weak),
                    };
                    tp.text(Pos2::new(text_rect.min.x, rect.min.y + 27.0), Align2::LEFT_CENTER, sub, FontId::proportional(11.0), sub_col);
                    let tip = match v {
                        Some(x) => tf!("{no}. {name}\n{x:.0} dBm · {}\nKlick: auswählen und im Plan neu setzen · Rechtsklick: löschen", "{no}. {name}\n{x:.0} dBm · {}\nClick: select and place again · right-click: delete", Quality::of(*x).label()),
                        None => tf!("{no}. {name}\nKlick: auswählen und im Plan setzen · Rechtsklick: löschen", "{no}. {name}\nClick: select and place · right-click: delete"),
                    };
                    let resp = resp.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand);
                    if resp.clicked() {
                        select = Some((*id, *floor));
                    }
                    resp.context_menu(|ui| {
                        if ui.button(t!("\u{E4A6} Messpunkt löschen", "\u{E4A6} Delete point")).clicked() {
                            delete = Some(no - 1);
                            ui.close_menu();
                        }
                    });
                }
            });
        });
        if full {
            self.plan_full_table = true;
        }
        if let Some((id, floor)) = select {
            self.plan_sel = if self.plan_sel == Some(id) { None } else { Some(id) };
            if let Some(f) = floor {
                self.show_floor(f);
            }
        }
        if let Some(i) = delete {
            self.push_undo(t!("Messpunkt gelöscht", "Point deleted"));
            if let Some(m) = self.meas.as_mut() {
                m.remove_point(i);
            }
            self.plan_sel = None;
            self.heat_tex = None;
            self.survey_changed();
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
        let mut nosig: Option<String> = None;
        egui::Window::new(RichText::new(t!("\u{E6C8} Farbskala der Heatmap", "\u{E6C8} Heatmap colour scale")).strong())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(340.0)
            .anchor(Align2::RIGHT_TOP, [-24.0, 150.0])
            .show(ctx, |ui| {
                let p = pal(ui);
                ui.label(RichText::new(t!("Ab welchem Signal welche Farbe gilt. Schwächer als die unterste Schwelle gilt als „kein Signal“.", "Which colour applies from which signal. Weaker than the lowest threshold counts as \"no signal\".")).size(12.0).color(p.weak));
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
                ui.add_space(8.0);
                ui.label(RichText::new(tf!("Kein Signal (schwächer als {:.0} dBm oder kein Empfang):", "No signal (weaker than {:.0} dBm or no reception):", stops.iter().map(|s| s.0).fold(f64::MAX, f64::min))).color(p.text));
                ui.horizontal(|ui| {
                    let cur = heatmap::no_signal();
                    if ui.radio(cur.is_some(), t!("Farbe", "Colour")).clicked() && cur.is_none() {
                        nosig = Some(String::new());
                    }
                    let mut c = cur.unwrap_or(heatmap::DEFAULT_NO_SIGNAL);
                    if ui.add_enabled_ui(cur.is_some(), |ui| ui.color_edit_button_srgb(&mut c)).inner.changed() {
                        nosig = Some(format!("{:02x}{:02x}{:02x}", c[0], c[1], c[2]));
                    }
                    if ui.radio(cur.is_none(), t!("Durchsichtig (nicht einfärben)", "Transparent (no colour)")).clicked() {
                        nosig = Some("none".into());
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
        if let Some(v) = nosig {
            heatmap::set_no_signal(&v);
            self.settings.heat_nosignal = v;
            self.heat_tex = None;
        }
        if reset {
            heatmap::set_scale(Vec::new());
            heatmap::set_no_signal("");
            self.settings.heat_scale.clear();
            self.settings.heat_nosignal.clear();
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

    /// Live-Wert der Grundriss-Messung, eingebettet in die Messpunkt-Zeile (der Plan braucht den Platz)
    pub(super) fn plan_live_inline(&self, ui: &mut Ui, live: Option<i32>, connected: bool) {
        let p = pal(ui);
        let Some(m) = &self.meas else { return };
        let s = m.stats(self.now_t());
        match live {
            Some(v) => {
                let col = point_color(Some(v as f64));
                ui.label(RichText::new(format!("{v} dBm")).size(19.0).family(crate::theme::semibold()).color(col))
                    .on_hover_text(if connected { t!("Live – verbunden mit der SSID", "Live – connected to the SSID") } else { t!("Live – stärkster AP aus dem Scan", "Live – strongest AP from the scan") });
                crate::widgets::badge(ui, Quality::of(v as f64).label(), col);
            }
            None => {
                ui.label(RichText::new("– dBm").size(19.0).color(p.weak));
            }
        }
        let placed = m.points.iter().filter(|pt| pt.pos.is_some()).count();
        ui.label(RichText::new(tf!("{} Punkte · {} im Plan · {}", "{} points · {} on plan · {}", m.points.len(), placed, crate::widgets::fmt_mmss(s.duration))).color(p.weak));
        let vals: Vec<f64> = m.points.iter().filter_map(|pt| pt.rssi).collect();
        if !vals.is_empty() {
            let avg = vals.iter().sum::<f64>() / vals.len() as f64;
            let weak = m.points.iter().filter(|pt| pt.quality() == Some(Quality::Weak)).count();
            ui.label(RichText::new(format!("Ø {avg:.0} dBm")).color(point_color(Some(avg)))).on_hover_text(t!("Durchschnitt an den Messpunkten", "Average at the points"));
            if weak > 0 {
                ui.label(RichText::new(tf!("{weak} schwach", "{weak} weak")).color(p.danger));
            }
        }
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

/// Halbdurchsichtiger Rahmen für Leisten, die über dem Plan schweben
fn overlay_frame(p: &crate::theme::Palette) -> egui::Frame {
    egui::Frame::none()
        .fill(p.surface.gamma_multiply(0.94))
        .stroke(Stroke::new(1.0f32, p.border))
        .rounding(9.0)
        .inner_margin(egui::Margin::symmetric(8.0, 5.0))
        .shadow(egui::Shadow { offset: Vec2::new(0.0, 3.0), blur: 10.0, spread: 0.0, color: Color32::from_black_alpha(60) })
}

/// Farblegende der Heatmap unten rechts im Plan
fn legend_overlay(painter: &egui::Painter, area: Rect) {
    let (lo, hi) = (heatmap::scale_min(), heatmap::scale_max());
    let ink = Color32::from_rgb(40, 44, 52);
    let font = FontId::proportional(11.0);
    let nosig = heatmap::no_signal();
    let ns_label = t!("kein Signal", "no signal");
    let bar_w = 120.0;
    let ns_w = if nosig.is_some() { 16.0 + painter.layout_no_wrap(ns_label.to_string(), font.clone(), ink).size().x + 10.0 } else { 0.0 };
    let lo_txt = format!("{lo:.0}");
    let hi_txt = format!("{hi:.0} dBm");
    let lo_w = painter.layout_no_wrap(lo_txt.clone(), font.clone(), ink).size().x;
    let hi_w = painter.layout_no_wrap(hi_txt.clone(), font.clone(), ink).size().x;
    let w = 10.0 + ns_w + lo_w + 6.0 + bar_w + 6.0 + hi_w + 10.0;
    let bg = Rect::from_min_size(Pos2::new(area.max.x - w - 10.0, area.max.y - 34.0), Vec2::new(w, 24.0));
    painter.rect_filled(bg, 4.0, Color32::from_white_alpha(225));
    let cy = bg.center().y;
    let mut x = bg.min.x + 10.0;
    if let Some([r, g, b]) = nosig {
        painter.rect_filled(Rect::from_center_size(Pos2::new(x + 5.0, cy), Vec2::splat(10.0)), 2.0, Color32::from_rgb(r, g, b));
        painter.text(Pos2::new(x + 16.0, cy), Align2::LEFT_CENTER, ns_label, font.clone(), ink);
        x += ns_w;
    }
    painter.text(Pos2::new(x, cy), Align2::LEFT_CENTER, lo_txt, font.clone(), ink);
    x += lo_w + 6.0;
    let steps = 30;
    for i in 0..steps {
        let v = lo + (hi - lo) * i as f64 / (steps - 1) as f64;
        let x0 = x + bar_w * i as f32 / steps as f32;
        let [rr, g, b] = heatmap::color(v);
        painter.rect_filled(Rect::from_min_max(Pos2::new(x0, cy - 5.0), Pos2::new(x0 + bar_w / steps as f32 + 0.5, cy + 5.0)), 0.0, Color32::from_rgb(rr, g, b));
    }
    x += bar_w + 6.0;
    painter.text(Pos2::new(x, cy), Align2::LEFT_CENTER, hi_txt, font, ink);
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
