//! Wiederverwendbare Bausteine der Oberfläche (Karten, Kennzahlen, Signal-Anzeigen …).

use eframe::egui::{self, Align2, Color32, FontId, Margin, Response, RichText, Sense, Stroke, Ui, Vec2};

use crate::ie::SecLevel;
use crate::model::{signal_color, signal_frac};
use crate::theme::pal;

/// Karte mit optionalem Titel (Rahmen, Hintergrund, Innenabstand)
pub fn card<R>(ui: &mut Ui, title: Option<&str>, add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = pal(ui);
    egui::Frame::none()
        .fill(p.surface)
        .stroke(Stroke::new(1.0f32, p.border))
        .rounding(10.0)
        .inner_margin(Margin::same(14.0))
        .show(ui, |ui| {
            // Inhalt immer untereinander, egal in welchem Layout die Karte steht
            ui.vertical(|ui| {
                if let Some(t) = title {
                    ui.label(RichText::new(t).strong().size(14.5).color(p.text));
                    ui.add_space(6.0);
                }
                add(ui)
            })
            .inner
        })
        .inner
}

/// Seitenkopf: Titel, Untertitel und rechtsbündige Aktionen
pub fn page_header(ui: &mut Ui, title: &str, subtitle: &str, actions: impl FnOnce(&mut Ui)) {
    let p = pal(ui);
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(title).size(22.0).strong().color(p.text));
            if !subtitle.is_empty() {
                ui.label(RichText::new(subtitle).color(p.weak));
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), actions);
    });
    ui.add_space(8.0);
}

/// Kennzahl-Kachel (Titel klein, Wert groß, optional Untertitel)
pub fn kpi(ui: &mut Ui, title: &str, value: &str, sub: &str, color: Color32, width: f32) -> Response {
    let p = pal(ui);
    egui::Frame::none()
        .fill(p.surface)
        .stroke(Stroke::new(1.0f32, p.border))
        .rounding(10.0)
        .inner_margin(Margin::symmetric(14.0, 10.0))
        .show(ui, |ui| {
            ui.set_width(width);
            ui.vertical(|ui| {
                ui.label(RichText::new(title).size(12.0).color(p.weak));
                ui.label(RichText::new(value).size(24.0).strong().color(color));
                if !sub.is_empty() {
                    ui.label(RichText::new(sub).size(11.5).color(p.weak));
                }
            });
        })
        .response
}

/// Kleines farbiges Etikett (z. B. Zähler in der Navigation)
pub fn badge(ui: &mut Ui, text: &str, color: Color32) {
    egui::Frame::none()
        .fill(color.gamma_multiply(0.22))
        .stroke(Stroke::new(1.0f32, color.gamma_multiply(0.6)))
        .rounding(9.0)
        .inner_margin(Margin::symmetric(6.0, 0.0))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(11.0).strong().color(color));
        });
}

/// Statuspunkt
pub fn dot(ui: &mut Ui, color: Color32, r: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(r * 2.0 + 2.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), r, color);
}

pub fn sec_color(ui: &Ui, l: SecLevel) -> Color32 {
    let p = pal(ui);
    match l {
        SecLevel::Open => p.danger,
        SecLevel::Weak => p.warning,
        SecLevel::Ok => p.success.gamma_multiply(0.85),
        SecLevel::Strong => p.success,
    }
}

/// Belegungsfarbe 0 = frei (grün) … 60 = stark belegt (rot)
pub fn load_color(score: f64) -> Color32 {
    let q = (score / 60.0).clamp(0.0, 1.0) as f32;
    // kräftig genug für hellen und dunklen Hintergrund
    egui::ecolor::Hsva::new((1.0 - q) * 0.33, 0.85, 0.72, 1.0).into()
}

/// Signalwert als Text + schlanker Balken (Tabellen)
pub fn signal_bar(ui: &mut Ui, rssi: i32, stale: bool) {
    let w = ui.available_width().clamp(70.0, 170.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 18.0), Sense::hover());
    let p = pal(ui);
    let col = if stale { p.weak } else { signal_color(rssi) };
    let painter = ui.painter();
    painter.text(rect.left_center(), Align2::LEFT_CENTER, format!("{rssi} dBm"), FontId::monospace(12.5), col);
    let bar_left = rect.left() + 64.0;
    if rect.right() - bar_left > 12.0 {
        let track = egui::Rect::from_min_max(
            egui::pos2(bar_left, rect.center().y - 3.0),
            egui::pos2(rect.right() - 4.0, rect.center().y + 3.0),
        );
        painter.rect_filled(track, 3.0, p.surface2);
        let mut fr = track;
        fr.set_width((track.width() * signal_frac(rssi)).max(3.0));
        painter.rect_filled(fr, 3.0, col);
    }
}

/// Horizontaler Mengenbalken mit Beschriftung (Verteilungen)
pub fn bar_row(ui: &mut Ui, label: &str, n: usize, total: usize, col: Color32) {
    let p = pal(ui);
    let frac = if total == 0 { 0.0 } else { n as f32 / total as f32 };
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 28.0), Sense::hover());
    let painter = ui.painter();
    painter.text(rect.left_top() + Vec2::new(0.0, 2.0), Align2::LEFT_TOP, label, FontId::proportional(13.0), p.text);
    painter.text(rect.right_top() + Vec2::new(0.0, 2.0), Align2::RIGHT_TOP, n.to_string(), FontId::proportional(13.0), p.weak);
    let track = egui::Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - 6.0), egui::pos2(rect.right(), rect.bottom() - 2.0));
    painter.rect_filled(track, 2.0, p.surface2);
    let mut fr = track;
    fr.set_width(track.width() * frac);
    painter.rect_filled(fr, 2.0, col);
    resp.on_hover_text(format!("{label}: {n} ({:.0} %)", frac * 100.0));
}

/// Halbkreis-Anzeige für einen Signalpegel. `scale` 1.0 = 360×200 px.
pub fn signal_gauge(ui: &mut Ui, rssi: Option<i32>, stale: bool, scale: f32) {
    let p = pal(ui);
    let size = Vec2::new(360.0, 200.0) * scale;
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter();
    let c = egui::pos2(rect.center().x, rect.bottom() - 20.0 * scale);
    let rad = 160.0 * scale;
    let thick = 24.0 * scale;
    let seg = 60;
    let frac_now = rssi.map(signal_frac).unwrap_or(0.0);
    for k in 0..seg {
        let a0 = std::f32::consts::PI * (1.0 - k as f32 / seg as f32);
        let a1 = std::f32::consts::PI * (1.0 - (k + 1) as f32 / seg as f32);
        let frac = k as f32 / seg as f32;
        let on = rssi.is_some() && frac <= frac_now;
        let col = if !on {
            p.surface2
        } else if stale {
            p.weak
        } else {
            signal_color((-100.0 + frac * 70.0) as i32)
        };
        let pt = |a: f32, r: f32| egui::pos2(c.x + r * a.cos(), c.y - r * a.sin());
        painter.add(egui::Shape::convex_polygon(
            vec![pt(a0, rad), pt(a1, rad), pt(a1, rad - thick), pt(a0, rad - thick)],
            col,
            Stroke::NONE,
        ));
    }
    let (txt, col) = match rssi {
        Some(r) if !stale => (r.to_string(), signal_color(r)),
        Some(r) => (r.to_string(), p.weak),
        None => ("–".to_string(), p.weak),
    };
    painter.text(c - Vec2::new(0.0, 55.0 * scale), Align2::CENTER_CENTER, txt, FontId::proportional(60.0 * scale), col);
    painter.text(c - Vec2::new(0.0, 12.0 * scale), Align2::CENTER_CENTER, "dBm", FontId::proportional(16.0 * scale), p.weak);
}

/// Schlüssel/Wert-Zeile in einem Grid
pub fn kv(ui: &mut Ui, k: &str, v: impl Into<egui::WidgetText>) {
    let p = pal(ui);
    ui.label(RichText::new(k).color(p.weak));
    ui.label(v);
    ui.end_row();
}

pub fn yes_no(b: bool) -> String {
    if b {
        format!("✔ {}", crate::t!("ja", "yes"))
    } else {
        format!("– {}", crate::t!("nein", "no"))
    }
}

/// y-Achse für Signalpegel: nur volle 10er beschriften, mit Einheit
pub fn dbm_axis(m: egui_plot::GridMark, _range: &std::ops::RangeInclusive<f64>) -> String {
    if (m.value % 10.0).abs() < 1e-6 && m.value <= -10.0 && m.value >= -100.0 {
        format!("{} dBm", m.value)
    } else {
        String::new()
    }
}

/// Sekunden als m:ss bzw. h:mm:ss
pub fn fmt_mmss(t: f64) -> String {
    let s = t.max(0.0).round() as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// Primärer (gefüllter) Button
pub fn primary_button(ui: &mut Ui, text: &str, color: Color32) -> Response {
    ui.add(egui::Button::new(RichText::new(text).color(Color32::WHITE).strong()).fill(color).min_size(Vec2::new(0.0, 28.0)))
}

/// Segment-Umschalter (wie Tabs), liefert true bei Änderung
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) -> bool {
    let p = pal(ui);
    let mut changed = false;
    egui::Frame::none()
        .fill(p.surface2)
        .stroke(Stroke::new(1.0f32, p.border))
        .rounding(7.0)
        .inner_margin(Margin::same(2.0))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.horizontal(|ui| {
                for (v, label) in options {
                    let sel = *value == *v;
                    let txt = RichText::new(*label).color(if sel { p.text } else { p.weak });
                    let b = egui::Button::new(txt).fill(if sel { p.surface } else { Color32::TRANSPARENT }).stroke(if sel {
                        Stroke::new(1.0f32, p.border)
                    } else {
                        Stroke::NONE
                    });
                    if ui.add(b).clicked() && !sel {
                        *value = *v;
                        changed = true;
                    }
                }
            });
        });
    changed
}
