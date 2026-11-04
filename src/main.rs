#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! WiFi Analyzer – WLAN-Analyse für Windows.
//! © Thomas Schöpf · https://wifianalyzer.schoepf-tirol.at · GPL-3.0-or-later

mod app;
mod beeper;
mod i18n;
mod ie;
mod measure;
mod model;
mod oui;
mod perm;
mod settings;
mod theme;
mod widgets;
mod wlan;

use eframe::egui;

/// App-Icon (erzeugt mit tools/make_icon.py)
pub const ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");

/// Größe des Lade-Popups. Nach dem Laden wird das Fenster zum maximierten Hauptfenster.
pub const SPLASH_SIZE: [f32; 2] = [560.0, 440.0];

fn main() -> eframe::Result<()> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("WiFi Analyzer")
        .with_inner_size(SPLASH_SIZE)
        .with_decorations(false)
        .with_resizable(false)
        .with_transparent(true);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(ICON_PNG) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions { viewport, centered: true, ..Default::default() };
    eframe::run_native(
        "WiFi Analyzer",
        options,
        Box::new(|cc| Ok(Box::new(app::WifiApp::new(cc)))),
    )
}
