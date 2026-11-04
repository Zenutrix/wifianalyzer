//! Ereignisprotokoll (neue / verschwundene Netze, Roaming, Fehler).

use eframe::egui::{self, RichText, Ui};

use crate::app::{Sev, WifiApp};
use crate::theme::pal;
use crate::widgets::{badge, page_header};
use crate::{t, tf};

impl WifiApp {
    pub(crate) fn page_events(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        let mut clear = false;
        let mut copy = false;
        page_header(ui, t!("Ereignisse", "Events"), &tf!("{} Einträge seit Programmstart", "{} entries since start", self.log.len()), |ui| {
            if ui.button(t!("Leeren", "Clear")).clicked() {
                clear = true;
            }
            if ui.button(t!("📋 Kopieren", "📋 Copy")).on_hover_text(t!("Protokoll in die Zwischenablage", "Copy log to clipboard")).clicked() {
                copy = true;
            }
        });
        if clear {
            self.log.clear();
        }
        if copy {
            let text: String = self.log.iter().rev().map(|e| format!("{}\t{}\t{}\n", e.time.format("%Y-%m-%d %H:%M:%S"), e.sev.label(), e.text)).collect();
            ui.ctx().copy_text(text);
            self.toast(Sev::Info, t!("Protokoll kopiert", "Log copied").into());
        }
        if self.log.is_empty() {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| ui.label(RichText::new(t!("Noch keine Ereignisse", "No events yet")).size(17.0).color(p.weak)));
            return;
        }
        crate::widgets::card(ui, None, |ui| {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                egui::Grid::new("events").num_columns(3).striped(true).spacing([14.0, 6.0]).show(ui, |ui| {
                    for e in &self.log {
                        let col = match e.sev {
                            Sev::Critical => p.danger,
                            Sev::Warning => p.warning,
                            Sev::Info => p.accent,
                        };
                        ui.label(RichText::new(e.time.format("%H:%M:%S").to_string()).monospace().color(p.weak));
                        badge(ui, e.sev.label(), col);
                        ui.label(RichText::new(&e.text).color(p.text));
                        ui.end_row();
                    }
                });
            });
        });
    }
}
