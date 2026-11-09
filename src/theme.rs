//! Farben und Stil: ruhiges, neutrales Business-Design (an GitHub Primer angelehnt).

use eframe::egui::{self, Color32, FontId, Rounding, Stroke, TextStyle, Theme, Vec2};

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color32,
    pub surface: Color32,
    pub surface2: Color32,
    pub border: Color32,
    pub text: Color32,
    pub weak: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub danger: Color32,
    pub purple: Color32,
}

pub const DARK: Palette = Palette {
    bg: Color32::from_rgb(13, 17, 23),
    surface: Color32::from_rgb(22, 27, 34),
    surface2: Color32::from_rgb(30, 36, 45),
    border: Color32::from_rgb(48, 54, 61),
    text: Color32::from_rgb(230, 237, 243),
    weak: Color32::from_rgb(139, 148, 158),
    accent: Color32::from_rgb(47, 129, 247),
    accent_soft: Color32::from_rgb(28, 50, 86),
    success: Color32::from_rgb(63, 185, 80),
    warning: Color32::from_rgb(210, 153, 34),
    danger: Color32::from_rgb(248, 81, 73),
    purple: Color32::from_rgb(163, 113, 247),
};

pub const LIGHT: Palette = Palette {
    bg: Color32::from_rgb(246, 248, 250),
    surface: Color32::from_rgb(255, 255, 255),
    surface2: Color32::from_rgb(240, 243, 246),
    border: Color32::from_rgb(208, 215, 222),
    text: Color32::from_rgb(31, 35, 40),
    weak: Color32::from_rgb(101, 109, 118),
    accent: Color32::from_rgb(9, 105, 218),
    accent_soft: Color32::from_rgb(221, 236, 255),
    success: Color32::from_rgb(26, 127, 55),
    warning: Color32::from_rgb(154, 103, 0),
    danger: Color32::from_rgb(207, 34, 46),
    purple: Color32::from_rgb(130, 80, 223),
};

/// Palette passend zum aktuellen Theme
pub fn pal(ui: &egui::Ui) -> Palette {
    if ui.visuals().dark_mode {
        DARK
    } else {
        LIGHT
    }
}

pub fn pal_ctx(ctx: &egui::Context) -> Palette {
    if ctx.style().visuals.dark_mode {
        DARK
    } else {
        LIGHT
    }
}

fn visuals(p: &Palette, dark: bool) -> egui::Visuals {
    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    // Eingabefelder und Diagramme: knapp dunkler als Karten statt Schwarz
    v.extreme_bg_color = if dark { Color32::from_rgb(15, 19, 26) } else { Color32::WHITE };
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Rect { aspect_ratio: 0.55 };
    v.faint_bg_color = p.surface2;
    v.code_bg_color = p.surface2;
    v.hyperlink_color = p.accent;
    v.selection.bg_fill = p.accent_soft;
    v.selection.stroke = Stroke::new(1.0f32, p.accent);
    v.window_stroke = Stroke::new(1.0f32, p.border);
    v.window_rounding = Rounding::same(10.0);
    v.menu_rounding = Rounding::same(8.0);
    v.window_shadow.color = Color32::from_black_alpha(if dark { 120 } else { 40 });
    v.popup_shadow.color = Color32::from_black_alpha(if dark { 100 } else { 30 });

    let r = Rounding::same(7.0);
    v.widgets.noninteractive.bg_fill = p.surface;
    v.widgets.noninteractive.weak_bg_fill = p.surface;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0f32, p.border);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0f32, p.text);
    v.widgets.noninteractive.rounding = r;

    v.widgets.inactive.bg_fill = p.surface2;
    v.widgets.inactive.weak_bg_fill = p.surface2;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0f32, p.border);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0f32, p.text);
    v.widgets.inactive.rounding = r;

    v.widgets.hovered.bg_fill = if dark { Color32::from_rgb(38, 45, 56) } else { Color32::from_rgb(232, 236, 240) };
    v.widgets.hovered.weak_bg_fill = v.widgets.hovered.bg_fill;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0f32, p.weak);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0f32, p.text);
    v.widgets.hovered.rounding = r;

    v.widgets.active.bg_fill = p.accent_soft;
    v.widgets.active.weak_bg_fill = p.accent_soft;
    v.widgets.active.bg_stroke = Stroke::new(1.0f32, p.accent);
    v.widgets.active.fg_stroke = Stroke::new(1.0f32, p.text);
    v.widgets.active.rounding = r;

    v.widgets.open = v.widgets.active;
    v.striped = true;
    v
}

/// Schriftfamilie für Überschriften und Kennzahlen (Segoe UI Semibold)
pub fn semibold() -> egui::FontFamily {
    egui::FontFamily::Name("semibold".into())
}

/// Windows-Systemschriften (Segoe UI, Consolas) + Phosphor-Icons laden.
/// Die Schriften werden zur Laufzeit aus dem Windows-Ordner gelesen (nicht mitgeliefert);
/// fehlen sie, bleiben die eingebauten egui-Schriften.
pub fn install_fonts(ctx: &egui::Context) {
    use egui::{FontData, FontDefinitions, FontFamily};
    let mut fonts = FontDefinitions::default();
    let dir = std::path::PathBuf::from(std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into())).join("Fonts");
    let mut load = |key: &str, file: &str| -> bool {
        match std::fs::read(dir.join(file)) {
            Ok(b) => {
                fonts.font_data.insert(key.to_string(), FontData::from_owned(b));
                true
            }
            Err(_) => false,
        }
    };
    let ui_font = load("segoe", "segoeui.ttf");
    let semi = load("segoe-sb", "seguisb.ttf");
    let sym = load("segoe-sym", "seguisym.ttf");
    let defaults = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    {
        let prop = fonts.families.entry(FontFamily::Proportional).or_default();
        if ui_font {
            prop.insert(0, "segoe".into());
        }
        if sym {
            prop.push("segoe-sym".into());
        }
    }
    // Semibold: eigene Familie, Rest wie Proportional (Fallbacks)
    let mut sb: Vec<String> = Vec::new();
    if semi {
        sb.push("segoe-sb".into());
    } else if ui_font {
        sb.push("segoe".into());
    }
    sb.extend(defaults);
    if sym {
        sb.push("segoe-sym".into());
    }
    fonts.families.insert(semibold(), sb);
    // Icons: Phosphor direkt hinter der Hauptschrift (Proportional) bzw. in Semibold
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    if let Some(f) = fonts.families.get_mut(&semibold()) {
        f.insert(1.min(f.len()), "phosphor".into());
    }
    ctx.set_fonts(fonts);
}

pub fn apply(ctx: &egui::Context, dark: bool) {
    ctx.set_visuals_of(Theme::Dark, visuals(&DARK, true));
    ctx.set_visuals_of(Theme::Light, visuals(&LIGHT, false));
    ctx.set_theme(if dark { Theme::Dark } else { Theme::Light });
    crate::model::LIGHT_THEME.store(!dark, std::sync::atomic::Ordering::Relaxed);
    ctx.all_styles_mut(|s| {
        s.interaction.selectable_labels = false; // Labels sollen Klicks an Tabellenzeilen durchreichen
        s.spacing.item_spacing = Vec2::new(8.0, 6.0);
        s.spacing.button_padding = Vec2::new(10.0, 5.0);
        s.spacing.interact_size.y = 26.0;
        s.spacing.slider_rail_height = 5.0;
        s.spacing.scroll = egui::style::ScrollStyle::thin();
        s.text_styles.insert(TextStyle::Body, FontId::proportional(14.0));
        s.text_styles.insert(TextStyle::Button, FontId::proportional(14.0));
        s.text_styles.insert(TextStyle::Small, FontId::proportional(11.5));
        s.text_styles.insert(TextStyle::Heading, FontId::proportional(20.0));
        s.text_styles.insert(TextStyle::Monospace, FontId::monospace(13.0));
    });
}
