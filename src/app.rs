use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use chrono::{DateTime, Local};
use eframe::egui::{
    self, Align, Align2, Color32, FontId, Layout, RichText, Rounding, Sense, Stroke, TextStyle, Theme, Ui, Vec2, Vec2b,
};
use egui_extras::{Column, TableBuilder};
use egui_plot::{
    uniform_grid_spacer, Bar, BarChart, Corner, Legend, Line, Plot, PlotPoint, PlotPoints, Polygon, Text,
};

use crate::beeper::Beeper;
use crate::ie::{ie_name, SecLevel};
use crate::model::*;
use crate::oui::{self, OuiDb};
use crate::perm::{self, PermStatus};
use crate::measure::{EventKind, Measurement};
use crate::wlan::{self, ConnInfo, IfaceInfo, ScanCmd, ScanMsg};

const ACCENT: Color32 = Color32::from_rgb(30, 136, 229);

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Networks,
    Channels,
    History,
    Rating,
    Analysis,
    Finder,
    Measure,
    Log,
}

#[derive(PartialEq, Clone, Copy)]
enum SortCol {
    Pin,
    Ssid,
    Bssid,
    Vendor,
    Signal,
    Channel,
    Width,
    Standard,
    Security,
    Rate,
    Clients,
    Util,
    Seen,
}

enum RowKind {
    Single,
    Group { key: String, n: usize, expanded: bool, chans: String, bands: String },
    Child,
}

#[derive(PartialEq, Clone, Copy)]
enum Sev {
    Critical,
    Warning,
    Info,
}

struct Filter {
    text: String,
    b24: bool,
    b5: bool,
    b6: bool,
    min_rssi: i32,
    show_hidden: bool,
    only_pinned: bool,
    hide_stale: bool,
    only_open: bool,
}

impl Default for Filter {
    fn default() -> Self {
        Self {
            text: String::new(),
            b24: true,
            b5: true,
            b6: true,
            min_rssi: -100,
            show_hidden: true,
            only_pinned: false,
            hide_stale: false,
            only_open: false,
        }
    }
}

pub struct WifiApp {
    scanner: wlan::ScannerHandle,
    interfaces: Vec<IfaceInfo>,
    iface: Option<u128>,
    aps: HashMap<[u8; 6], ApRecord>,
    conn: Option<ConnInfo>,
    conn_hist: VecDeque<(f64, f64)>,
    error: Option<String>,
    error_code: u32,
    last_scan: Option<Instant>,
    scan_count: u64,
    paused: bool,
    interval: f32,
    tab: Tab,
    chan_band: Band,
    f: Filter,
    /// Suchtext in Kleinbuchstaben (einmal pro Frame berechnet)
    query: String,
    sort_col: SortCol,
    sort_asc: bool,
    selected: Option<[u8; 6]>,
    pinned: HashSet<[u8; 6]>,
    oui: OuiDb,
    oui_rx: Option<Receiver<Result<usize, String>>>,
    oui_msg: Option<String>,
    start: Instant,
    log: VecDeque<(DateTime<Local>, Sev, String)>,
    hist_all: bool,
    hist_window: f64,
    group_ssid: bool,
    expanded: HashSet<String>,
    meas: Option<Measurement>,
    meas_ssid: Option<String>,
    meas_marker: String,
    /// Diagramm folgt den letzten 5 Minuten statt die ganze Messung zu zeigen
    meas_follow: bool,
    splash_done: bool,
    splash_progress: f32,
    splash_done_steps: usize,
    splash_step_t: f32,
    icon_tex: Option<egui::TextureHandle>,
    perm: Option<PermStatus>,
    perm_rx: Option<Receiver<PermStatus>>,
    perm_dismissed: bool,
    perm_last_check: Instant,
    beeper: Beeper,
    beep_on: bool,
    dark: bool,
    stale_after: f32,
    remove_after: f32,
}

fn dark_visuals() -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    v.panel_fill = Color32::from_rgb(22, 25, 31);
    v.window_fill = Color32::from_rgb(28, 32, 39);
    v.extreme_bg_color = Color32::from_rgb(15, 17, 21);
    v.faint_bg_color = Color32::from_rgb(30, 34, 42);
    v.selection.bg_fill = Color32::from_rgb(25, 90, 150);
    v.hyperlink_color = ACCENT;
    v.widgets.inactive.rounding = Rounding::same(5.0);
    v.widgets.hovered.rounding = Rounding::same(5.0);
    v.widgets.active.rounding = Rounding::same(5.0);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0f32, Color32::from_rgb(45, 50, 60));
    v
}

fn light_visuals() -> egui::Visuals {
    let mut v = egui::Visuals::light();
    v.panel_fill = Color32::from_rgb(246, 248, 251);
    v.faint_bg_color = Color32::from_rgb(236, 240, 246);
    v.selection.bg_fill = Color32::from_rgb(170, 205, 245);
    v.hyperlink_color = ACCENT;
    v.widgets.inactive.rounding = Rounding::same(5.0);
    v.widgets.hovered.rounding = Rounding::same(5.0);
    v.widgets.active.rounding = Rounding::same(5.0);
    v
}

fn sec_color(l: SecLevel) -> Color32 {
    match l {
        SecLevel::Open => Color32::from_rgb(231, 76, 60),
        SecLevel::Weak => Color32::from_rgb(230, 126, 34),
        SecLevel::Ok => Color32::from_rgb(120, 190, 90),
        SecLevel::Strong => Color32::from_rgb(46, 204, 113),
    }
}

fn load_color(score: f64) -> Color32 {
    // 0 = frei (grün) ... 60 = stark belegt (rot)
    let q = (score / 60.0).clamp(0.0, 1.0) as f32;
    let hue = (1.0 - q) * 0.33;
    egui::ecolor::Hsva::new(hue, 0.75, 0.85, 1.0).into()
}

/// Kurvenform eines APs (flaches Dach, steile Flanken) für das Kanaldiagramm
fn ap_shape(center: f64, half: f64, rssi: f64) -> Vec<[f64; 2]> {
    let n = 48;
    (0..=n)
        .map(|i| {
            let u = -1.0 + 2.0 * i as f64 / n as f64;
            let s = (1.0 - u.powi(6)).max(0.0);
            [center + u * half, -100.0 + (rssi + 100.0) * s]
        })
        .collect()
}

fn half_span(r: &ApRecord) -> f64 {
    let h = r.info.width as f64 / 10.0;
    if r.info.band == Band::B24 && r.info.width == 20 {
        h + 0.2
    } else {
        h
    }
}

fn csv_escape(s: &str) -> String {
    if s.contains(';') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

impl WifiApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let ctx = &cc.egui_ctx;
        ctx.set_visuals_of(Theme::Dark, dark_visuals());
        ctx.set_visuals_of(Theme::Light, light_visuals());
        ctx.set_theme(Theme::Dark);
        ctx.all_styles_mut(|s| {
            // Labels sollen Klicks an Tabellenzeilen durchreichen
            s.interaction.selectable_labels = false;
            s.spacing.item_spacing = Vec2::new(8.0, 6.0);
            s.spacing.button_padding = Vec2::new(10.0, 4.0);
            s.text_styles.insert(TextStyle::Body, FontId::proportional(14.0));
            s.text_styles.insert(TextStyle::Button, FontId::proportional(14.0));
            s.text_styles.insert(TextStyle::Small, FontId::proportional(11.5));
            s.text_styles.insert(TextStyle::Heading, FontId::proportional(20.0));
            s.text_styles.insert(TextStyle::Monospace, FontId::monospace(13.0));
        });

        let scanner = wlan::spawn(ctx.clone());
        let interval = 4.0;
        let _ = scanner.tx.send(ScanCmd::SetInterval(interval));
        let mut app = Self {
            scanner,
            interfaces: vec![],
            iface: None,
            aps: HashMap::new(),
            conn: None,
            conn_hist: VecDeque::new(),
            error: None,
            error_code: 0,
            last_scan: None,
            scan_count: 0,
            paused: false,
            interval,
            tab: Tab::Networks,
            chan_band: Band::B24,
            f: Filter::default(),
            query: String::new(),
            sort_col: SortCol::Signal,
            sort_asc: false,
            selected: None,
            pinned: HashSet::new(),
            oui: OuiDb::load(),
            oui_rx: None,
            oui_msg: None,
            start: Instant::now(),
            log: VecDeque::new(),
            hist_all: false,
            hist_window: 300.0,
            group_ssid: true,
            expanded: HashSet::new(),
            meas: None,
            meas_ssid: None,
            meas_marker: String::new(),
            meas_follow: false,
            splash_done: false,
            splash_progress: 0.0,
            splash_done_steps: 0,
            splash_step_t: 0.0,
            icon_tex: eframe::icon_data::from_png_bytes(crate::ICON_PNG).ok().map(|d| {
                ctx.load_texture(
                    "app-icon",
                    egui::ColorImage::from_rgba_unmultiplied([d.width as usize, d.height as usize], &d.rgba),
                    egui::TextureOptions::LINEAR,
                )
            }),
            perm: None,
            perm_rx: None,
            perm_dismissed: false,
            perm_last_check: Instant::now(),
            beeper: Beeper::new(),
            beep_on: false,
            dark: true,
            stale_after: 15.0,
            remove_after: 180.0,
        };
        // Vollständige Hersteller-DB beim ersten Start automatisch laden
        if !app.oui.full_db {
            app.start_oui_download(ctx);
        }
        // Windows-Berechtigungsabfrage (Standort/WLAN) auslösen
        app.start_perm_check(ctx, true);
        app
    }

    fn now_t(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }

    fn add_log(&mut self, sev: Sev, msg: String) {
        self.log.push_front((Local::now(), sev, msg));
        self.log.truncate(2000);
    }

    fn vendor_for(&self, info: &crate::ie::BssInfo) -> (String, bool) {
        if let Some(v) = self.oui.lookup(&info.bssid) {
            return v;
        }
        if let Some(m) = info.wps.as_ref().and_then(|w| w.manufacturer.clone()) {
            return (m, info.bssid[0] & 0x02 != 0);
        }
        if info.bssid[0] & 0x02 != 0 {
            ("(lokal verwaltet)".into(), true)
        } else {
            ("Unbekannt".into(), false)
        }
    }

    /// Hersteller neu bestimmen. Virtuelle (LAA-)BSSIDs ohne OUI-Treffer erben den Hersteller
    /// der physischen Radio-BSSID, die sich nur im ersten bzw. letzten Byte unterscheidet.
    /// `full`: alle Hersteller neu aus der OUI-DB bestimmen (nur nach DB-Neuladen nötig).
    /// Sonst werden nur noch unaufgelöste virtuelle BSSIDs nachgezogen.
    fn refresh_vendors(&mut self, full: bool) {
        let keys: Vec<[u8; 6]> = self.aps.keys().copied().collect();
        if full {
            for k in &keys {
                let v = self.vendor_for(&self.aps[k].info);
                if let Some(r) = self.aps.get_mut(k) {
                    r.vendor = v.0;
                    r.vendor_virtual = v.1;
                }
            }
        }
        for k in &keys {
            if k[0] & 0x02 == 0 || self.aps[k].vendor != "(lokal verwaltet)" {
                continue;
            }
            let parent = keys
                .iter()
                .filter(|p| p[0] & 0x02 == 0)
                .find(|p| p[1..6] == k[1..6])
                .or_else(|| keys.iter().filter(|p| p[0] & 0x02 == 0).find(|p| p[1..5] == k[1..5]));
            if let Some(p) = parent {
                let v = self.aps[p].vendor.clone();
                if let Some(r) = self.aps.get_mut(k) {
                    r.vendor = v;
                    r.vendor_virtual = true;
                }
            }
        }
    }

    fn start_oui_download(&mut self, ctx: &egui::Context) {
        let (tx, rx) = std::sync::mpsc::channel();
        let c = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(oui::download());
            c.request_repaint();
        });
        self.oui_rx = Some(rx);
    }

    fn start_perm_check(&mut self, ctx: &egui::Context, request: bool) {
        if self.perm_rx.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let c = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(perm::check(request));
            c.request_repaint();
        });
        self.perm_rx = Some(rx);
        self.perm_last_check = Instant::now();
    }

    fn poll_perm(&mut self) {
        let Some(rx) = &self.perm_rx else { return };
        let Ok(mut st) = rx.try_recv() else { return };
        self.perm_rx = None;
        // Eine Registry-Prüfung kennt das Ergebnis der Windows-Abfrage nicht -> letztes übernehmen
        if st.access == perm::Access::Unknown {
            if let Some(old) = &self.perm {
                st.access = old.access;
            }
        }
        let was_ok = self.perm.as_ref().map(|p| p.ok());
        if st.ok() && was_ok != Some(true) && self.error_code == wlan::ERROR_ACCESS_DENIED {
            self.add_log(Sev::Info, "Standortzugriff erlaubt – starte Scan".into());
            let _ = self.scanner.tx.send(ScanCmd::ScanNow);
        }
        self.perm = Some(st);
    }

    /// Scan funktioniert tatsächlich (dann ist jede Berechtigungswarnung hinfällig)
    fn scan_works(&self) -> bool {
        self.scan_count > 0 && self.error.is_none()
    }

    fn needs_perm_dialog(&self) -> bool {
        if self.perm_dismissed || self.scan_works() {
            return false;
        }
        self.error_code == wlan::ERROR_ACCESS_DENIED || self.perm.as_ref().is_some_and(|p| !p.ok())
    }

    fn perm_dialog(&mut self, ctx: &egui::Context) {
        if !self.needs_perm_dialog() {
            return;
        }
        // Während der Dialog offen ist, regelmäßig nachsehen, ob der Benutzer die Schalter umgelegt hat
        if self.perm_last_check.elapsed() > Duration::from_secs(3) {
            self.start_perm_check(ctx, false);
            let _ = self.scanner.tx.send(ScanCmd::ScanNow);
        }
        let screen = ctx.screen_rect();
        // Abdunkeln über allem (auch Tabellen-Resize-Griffe), Dialog liegt eine Ebene darüber
        ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("perm_dim")))
            .rect_filled(screen, 0.0, Color32::from_black_alpha(170));

        let checklist = self.perm.as_ref().map(|p| p.checklist()).unwrap_or_default();
        let mut open_settings = false;
        let mut recheck = false;
        let mut later = false;
        egui::Window::new("perm_window")
            .order(egui::Order::Tooltip)
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .fixed_size([560.0, 0.0])
            .frame(egui::Frame::window(&ctx.style()).inner_margin(egui::Margin::same(24.0)).rounding(14.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if let Some(t) = &self.icon_tex {
                        ui.add(egui::Image::new((t.id(), Vec2::splat(64.0))));
                    }
                    ui.vertical(|ui| {
                        ui.add_space(6.0);
                        ui.label(RichText::new("Standortzugriff erforderlich").size(22.0).strong());
                        ui.label(RichText::new("Damit der WiFi Analyzer WLAN-Netze sehen darf").weak());
                    });
                });
                ui.add_space(10.0);
                ui.label(
                    "Windows gibt BSSIDs, Signalstärken und Kanäle nur an Apps heraus, die auf den Standort \
                     zugreifen dürfen (WLAN-Netze verraten den Aufenthaltsort). Der WiFi Analyzer nutzt keine \
                     Standortdaten und sendet nichts ins Internet.",
                );
                ui.add_space(12.0);
                egui::Frame::group(ui.style()).rounding(8.0).inner_margin(egui::Margin::same(12.0)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    for (state, text) in &checklist {
                        ui.horizontal(|ui| {
                            let (icon, col) = match state {
                                Some(true) => ("✔", Color32::from_rgb(46, 204, 113)),
                                Some(false) => ("✖", Color32::from_rgb(231, 76, 60)),
                                None => ("?", Color32::GRAY),
                            };
                            ui.label(RichText::new(icon).strong().color(col));
                            ui.label(*text);
                        });
                    }
                    if checklist.is_empty() {
                        ui.horizontal(|ui| {
                            ui.add(egui::Spinner::new());
                            ui.label("Prüfe Berechtigungen…");
                        });
                    }
                });
                ui.add_space(12.0);
                ui.label(RichText::new("So geht's:").strong());
                ui.label("1.  „Standort-Einstellungen öffnen“ klicken");
                ui.label("2.  „Standortdienste“ und „Desktop-Apps den Zugriff auf Ihren Standort erlauben“ einschalten");
                ui.label("3.  Zurück zum WiFi Analyzer – die Änderung wird automatisch erkannt");
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    let primary = egui::Button::new(RichText::new("⚙  Standort-Einstellungen öffnen").color(Color32::WHITE).strong())
                        .fill(ACCENT)
                        .min_size(Vec2::new(0.0, 34.0));
                    if ui.add(primary).clicked() {
                        open_settings = true;
                    }
                    if ui.add(egui::Button::new("⟳  Erneut anfragen").min_size(Vec2::new(0.0, 34.0))).clicked() {
                        recheck = true;
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add(egui::Button::new("Später").min_size(Vec2::new(0.0, 34.0))).clicked() {
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

    fn display_ssid(r: &ApRecord) -> String {
        if r.info.hidden {
            "‹versteckt›".into()
        } else {
            r.info.ssid.clone()
        }
    }

    // ------------------------------------------------------------------ Daten

    fn poll(&mut self) {
        while let Ok(msg) = self.scanner.rx.try_recv() {
            match msg {
                ScanMsg::Scan(ev) => self.handle_scan(ev),
                ScanMsg::Conn(c) => self.handle_conn(c),
            }
        }
        self.poll_oui();
    }

    fn handle_scan(&mut self, ev: wlan::ScanEvent) {
        {
            self.interfaces = ev.interfaces;
            self.iface = ev.iface;
            self.error_code = ev.error_code;
            if ev.error != self.error {
                if let Some(e) = &ev.error {
                    self.add_log(Sev::Critical, e.clone());
                }
            }
            self.error = ev.error;

            let now = Instant::now();
            let t = self.now_t();
            if let Some(list) = ev.bss {
                if let Some(m) = &mut self.meas {
                    m.on_scan(t, &list);
                }
                self.scan_count += 1;
                self.last_scan = Some(now);
                for b in list {
                    let key = b.bssid;
                    if let Some(r) = self.aps.get_mut(&key) {
                        r.update(b, now, t);
                    } else {
                        let vendor = self.vendor_for(&b);
                        let name = if b.hidden { "‹versteckt›".to_string() } else { b.ssid.clone() };
                        let msg = format!(
                            "Neu: {} ({}) Kanal {} {} dBm – {}",
                            name,
                            fmt_mac(&key),
                            b.channel,
                            b.rssi,
                            vendor.0
                        );
                        if self.scan_count > 1 {
                            self.add_log(Sev::Info, msg);
                        }
                        self.aps.insert(key, ApRecord::new(b, vendor, now, t));
                    }
                }
                self.refresh_vendors(false);
            }
            let remove_after = self.remove_after;
            let gone: Vec<[u8; 6]> = self
                .aps
                .iter()
                .filter(|(_, r)| r.age(now) > remove_after)
                .map(|(k, _)| *k)
                .collect();
            for k in gone {
                if let Some(r) = self.aps.remove(&k) {
                    let msg = format!("Verschwunden: {} ({})", Self::display_ssid(&r), fmt_mac(&k));
                    self.add_log(Sev::Warning, msg);
                }
            }

        }
    }

    /// Schnelle Verbindungsabfrage (1×/s): Kopfzeile, Verlauf, Roaming-Log, laufende Messung
    fn handle_conn(&mut self, conn: Option<ConnInfo>) {
        let t = self.now_t();
        let old_bssid = self.conn.as_ref().map(|c| c.bssid);
        if let Some(c) = &conn {
            if let Some(old) = old_bssid.filter(|o| *o != c.bssid) {
                let msg = format!("Roaming: {} > {} ({})", fmt_mac(&old), fmt_mac(&c.bssid), c.ssid);
                self.add_log(Sev::Warning, msg);
            } else if old_bssid.is_none() {
                let msg = format!("Verbunden mit {} ({})", c.ssid, fmt_mac(&c.bssid));
                self.add_log(Sev::Info, msg);
            }
            if let Some(rssi) = c.rssi {
                self.conn_hist.push_back((t, rssi as f64));
                // 1 Wert/s -> gut eine Stunde Verlauf
                while self.conn_hist.len() > 4000 {
                    self.conn_hist.pop_front();
                }
            }
        } else if old_bssid.is_some() {
            self.add_log(Sev::Warning, "Verbindung getrennt".into());
        }
        if let Some(m) = &mut self.meas {
            m.on_conn(t, conn.as_ref());
        }
        self.conn = conn;
    }

    fn poll_oui(&mut self) {
        if let Some(rx) = &self.oui_rx {
            if let Ok(res) = rx.try_recv() {
                self.oui_rx = None;
                match res {
                    Ok(n) => {
                        self.oui = OuiDb::load();
                        self.refresh_vendors(true);
                        self.oui_msg = Some(format!("Hersteller-Datenbank geladen: {n} Einträge"));
                    }
                    Err(e) => self.oui_msg = Some(format!("Download fehlgeschlagen: {e}")),
                }
            }
        }
    }

    fn visible(&self, r: &ApRecord, now: Instant) -> bool {
        let f = &self.f;
        let band_ok = match r.info.band {
            Band::B24 => f.b24,
            Band::B5 => f.b5,
            Band::B6 => f.b6,
            Band::Other => true,
        };
        if !band_ok || r.info.rssi < f.min_rssi {
            return false;
        }
        if r.info.hidden && !f.show_hidden {
            return false;
        }
        if f.only_pinned && !self.pinned.contains(&r.info.bssid) {
            return false;
        }
        if f.hide_stale && r.age(now) > self.stale_after {
            return false;
        }
        if f.only_open && r.info.sec_level != SecLevel::Open {
            return false;
        }
        if !self.query.is_empty() {
            let q = self.query.as_str();
            let hit = r.info.ssid.to_lowercase().contains(q)
                || fmt_mac(&r.info.bssid).to_lowercase().contains(q)
                || r.vendor.to_lowercase().contains(q)
                || r.info.security.to_lowercase().contains(q)
                || r.info.ap_name.as_deref().is_some_and(|n| n.to_lowercase().contains(q))
                || r.info.channel.to_string() == q;
            if !hit {
                return false;
            }
        }
        true
    }

    fn sorted_keys(&self) -> Vec<[u8; 6]> {
        let now = Instant::now();
        let mut v: Vec<&ApRecord> = self.aps.values().filter(|r| self.visible(r, now)).collect();
        let col = self.sort_col;
        v.sort_by(|a, b| {
            use std::cmp::Ordering;
            let (ia, ib) = (&a.info, &b.info);
            let o = match col {
                SortCol::Pin => self.pinned.contains(&ia.bssid).cmp(&self.pinned.contains(&ib.bssid)),
                SortCol::Ssid => ia.ssid.to_lowercase().cmp(&ib.ssid.to_lowercase()),
                SortCol::Bssid => ia.bssid.cmp(&ib.bssid),
                SortCol::Vendor => a.vendor.to_lowercase().cmp(&b.vendor.to_lowercase()),
                SortCol::Signal => ia.rssi.cmp(&ib.rssi),
                SortCol::Channel => (ia.band, ia.channel).cmp(&(ib.band, ib.channel)),
                SortCol::Width => ia.width.cmp(&ib.width),
                SortCol::Standard => ia.standard.cmp(&ib.standard),
                SortCol::Security => ia.sec_level.cmp(&ib.sec_level),
                SortCol::Rate => ia.max_rate.partial_cmp(&ib.max_rate).unwrap_or(Ordering::Equal),
                SortCol::Clients => ia.stations.cmp(&ib.stations),
                SortCol::Util => ia.ch_util.partial_cmp(&ib.ch_util).unwrap_or(Ordering::Equal),
                SortCol::Seen => b.last_seen.cmp(&a.last_seen),
            };
            let o = if self.sort_asc { o } else { o.reverse() };
            // eindeutiger Abschluss, sonst tauschen gleich starke APs bei jedem Scan die Plätze
            o.then_with(|| ib.rssi.cmp(&ia.rssi)).then_with(|| ia.bssid.cmp(&ib.bssid))
        });
        v.into_iter().map(|r| r.info.bssid).collect()
    }

    fn select_iface(&mut self, g: u128) {
        self.iface = Some(g);
        let _ = self.scanner.tx.send(ScanCmd::SetInterface(g));
    }

    fn export_csv(&mut self) {
        let name = format!("wlan-scan_{}.csv", Local::now().format("%Y-%m-%d_%H-%M-%S"));
        let Some(path) = rfd::FileDialog::new()
            .set_file_name(&name)
            .add_filter("CSV (Excel)", &["csv"])
            .save_file()
        else {
            return;
        };
        let mut s = String::from("\u{feff}SSID;BSSID;Hersteller;RSSI;RSSI min;RSSI max;RSSI avg;Kanal;Band;Frequenz MHz;Breite;Mittenkanal;Standard;Sicherheit;AKM;Cipher;PMF;Max. Rate Mbit/s;Streams;Clients;Auslastung %;Land;Beacon ms;DTIM;802.11k;802.11r;802.11v;WPS;AP-Name;Chipsatz;Uptime;Erstmals;Zuletzt vor s\n");
        let now = Instant::now();
        for k in self.sorted_keys() {
            let r = &self.aps[&k];
            let i = &r.info;
            let wps = i.wps.as_ref().map(|w| {
                format!(
                    "{} {} {}",
                    w.manufacturer.clone().unwrap_or_default(),
                    w.model.clone().unwrap_or_default(),
                    w.model_number.clone().unwrap_or_default()
                )
                .trim()
                .to_string()
            });
            let row = [
                Self::display_ssid(r),
                fmt_mac(&i.bssid),
                r.vendor.clone(),
                i.rssi.to_string(),
                r.rssi_min.to_string(),
                r.rssi_max.to_string(),
                format!("{:.1}", r.rssi_avg()),
                i.channel.to_string(),
                i.band.label().into(),
                i.freq_mhz.to_string(),
                i.width_label.clone(),
                format!("{}", i.center),
                i.std_label(),
                i.security.clone(),
                i.akms.join(", "),
                i.pairwise.join(", "),
                i.pmf.clone(),
                format!("{:.0}", i.max_rate),
                i.streams.to_string(),
                i.stations.map(|v| v.to_string()).unwrap_or_default(),
                i.ch_util.map(|v| format!("{v:.0}")).unwrap_or_default(),
                i.country.clone().unwrap_or_default(),
                format!("{:.1}", i.beacon_ms),
                i.dtim.map(|v| v.to_string()).unwrap_or_default(),
                (if i.k_rrm { "ja" } else { "nein" }).into(),
                (if i.r_ft { "ja" } else { "nein" }).into(),
                (if i.v_btm { "ja" } else { "nein" }).into(),
                wps.unwrap_or_else(|| "nein".into()),
                i.ap_name.clone().unwrap_or_default(),
                i.chipset.clone().unwrap_or_default(),
                fmt_duration(i.uptime_s),
                r.first_seen_wall.format("%Y-%m-%d %H:%M:%S").to_string(),
                format!("{:.0}", r.age(now)),
            ];
            s.push_str(&row.iter().map(|c| csv_escape(c)).collect::<Vec<_>>().join(";"));
            s.push('\n');
        }
        match std::fs::write(&path, s) {
            Ok(_) => self.add_log(Sev::Info, format!("CSV exportiert: {}", path.display())),
            Err(e) => self.add_log(Sev::Critical, format!("CSV-Export fehlgeschlagen: {e}")),
        }
    }

    // ------------------------------------------------------------------ UI-Teile

    fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top")
            .frame(egui::Frame::side_top_panel(&ctx.style()).inner_margin(egui::Margin::symmetric(12.0, 8.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    match &self.icon_tex {
                        Some(t) => {
                            ui.add(egui::Image::new((t.id(), Vec2::splat(30.0))));
                        }
                        None => {
                            ui.label(RichText::new("📶").size(24.0).color(ACCENT));
                        }
                    }
                    ui.label(RichText::new("WiFi Analyzer").size(20.0).strong());
                    ui.add_space(12.0);

                    let cur = self
                        .interfaces
                        .iter()
                        .find(|i| Some(i.guid) == self.iface)
                        .map(|i| i.description.clone())
                        .unwrap_or_else(|| "Kein Adapter".into());
                    let mut pick = None;
                    egui::ComboBox::from_id_salt("iface")
                        .width(260.0)
                        .selected_text(cur)
                        .show_ui(ui, |ui| {
                            for i in &self.interfaces {
                                if ui
                                    .selectable_label(Some(i.guid) == self.iface, format!("{} ({})", i.description, i.state))
                                    .clicked()
                                {
                                    pick = Some(i.guid);
                                }
                            }
                        });
                    if let Some(g) = pick {
                        self.select_iface(g);
                    }

                    let (txt, col) = if self.paused {
                        ("▶ Start", Color32::from_rgb(46, 160, 90))
                    } else {
                        ("⏸ Pause", Color32::from_rgb(200, 120, 40))
                    };
                    if ui.add(egui::Button::new(RichText::new(txt).color(Color32::WHITE)).fill(col)).clicked() {
                        self.paused = !self.paused;
                        let _ = self.scanner.tx.send(ScanCmd::Pause(self.paused));
                    }
                    if ui.button("⟳ Jetzt scannen").clicked() {
                        self.paused = false;
                        let _ = self.scanner.tx.send(ScanCmd::ScanNow);
                    }
                    ui.add_space(8.0);
                    ui.label(RichText::new("Intervall").weak());
                    if ui
                        .add(egui::Slider::new(&mut self.interval, 2.0..=30.0).suffix(" s").step_by(1.0).fixed_decimals(0))
                        .changed()
                    {
                        let _ = self.scanner.tx.send(ScanCmd::SetInterval(self.interval));
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button(if self.dark { "☀" } else { "🌙" }).on_hover_text("Hell/Dunkel").clicked() {
                            self.dark = !self.dark;
                            ctx.set_theme(if self.dark { Theme::Dark } else { Theme::Light });
                        }
                        if ui.button("💾 CSV-Export").on_hover_text("Aktuelle (gefilterte) Liste als CSV speichern").clicked() {
                            self.export_csv();
                        }
                    });
                });
                let status = match self.last_scan {
                    Some(t) => format!(
                        "Scan #{} · {} BSSIDs · {}",
                        self.scan_count,
                        self.aps.len(),
                        fmt_ago(t.elapsed().as_secs_f32())
                    ),
                    None => "Warte auf ersten Scan…".into(),
                };

                // Verbindungsleiste
                ui.add_space(4.0);
                let fill = if self.dark { Color32::from_rgb(30, 36, 46) } else { Color32::from_rgb(228, 236, 247) };
                egui::Frame::none()
                    .fill(fill)
                    .rounding(6.0)
                    .inner_margin(egui::Margin::symmetric(10.0, 6.0))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                        match &self.conn {
                            Some(c) => {
                                ui.label(RichText::new("🔗 Verbunden").strong().color(Color32::from_rgb(46, 204, 113)));
                                ui.label(RichText::new(&c.ssid).strong());
                                ui.label(RichText::new(fmt_mac(&c.bssid)).monospace());
                                if let Some(ch) = c.channel {
                                    ui.label(format!("Kanal {ch}"));
                                }
                                if let Some(rssi) = c.rssi {
                                    ui.label(RichText::new(format!("{rssi} dBm")).strong().color(signal_color(rssi)));
                                }
                                ui.label(format!("Qualität {}%", c.quality));
                                ui.label(format!("Rx {:.0} / Tx {:.0} Mbit/s", c.rx_mbps, c.tx_mbps));
                                ui.label(&c.phy);
                                ui.label(format!("{} · {}", c.auth, c.cipher));
                                if let Some(r) = self.aps.get(&c.bssid) {
                                    ui.label(RichText::new(&r.vendor).weak());
                                }
                                if ui.small_button("auswählen").clicked() {
                                    self.selected = Some(c.bssid);
                                }
                            }
                            None => {
                                ui.label(RichText::new("⛔ Nicht verbunden").weak());
                            }
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            // Statischer Live-Punkt statt Spinner: ein Spinner erzwingt Neuzeichnen mit 60 fps
                            ui.label(RichText::new(&status).weak());
                            let (txt, col) = if self.paused {
                                ("Pausiert", Color32::from_rgb(230, 150, 40))
                            } else if self.error.is_some() {
                                ("Fehler", Color32::from_rgb(231, 76, 60))
                            } else {
                                ("Live", Color32::from_rgb(46, 204, 113))
                            };
                            ui.label(RichText::new(txt).strong().color(col));
                            let (r, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
                            ui.painter().circle_filled(r.center(), 4.5, col);
                        });
                        });
                    });
            });
    }

    fn status_bar(&mut self, ctx: &egui::Context) {
        let show = self.error.is_some() || self.oui_msg.is_some();
        if !show {
            return;
        }
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some(e) = &self.error {
                    ui.label(RichText::new(format!("⚠ {e}")).color(Color32::from_rgb(240, 120, 90)));
                    if self.error_code == wlan::ERROR_ACCESS_DENIED && ui.button("🔑 Berechtigung erteilen…").clicked() {
                        self.perm_dismissed = false;
                    }
                }
                if let Some(m) = self.oui_msg.clone() {
                    ui.label(RichText::new(m).weak());
                    if ui.small_button("✖").clicked() {
                        self.oui_msg = None;
                    }
                }
            });
        });
    }

    fn left_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("filters").resizable(true).default_width(215.0).width_range(190.0..=360.0).show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(6.0);
                let section = |title: &str| RichText::new(title).strong().size(15.0);

                egui::CollapsingHeader::new(section("Filter")).default_open(true).show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.f.text)
                            .hint_text("🔍 SSID, BSSID, Hersteller…")
                            .desired_width(f32::INFINITY),
                    );
                    ui.horizontal(|ui| {
                        ui.toggle_value(&mut self.f.b24, "2,4 GHz");
                        ui.toggle_value(&mut self.f.b5, "5 GHz");
                        ui.toggle_value(&mut self.f.b6, "6 GHz");
                    });
                    ui.label(RichText::new("Mindestsignal").weak());
                    ui.add(egui::Slider::new(&mut self.f.min_rssi, -100..=-30).suffix(" dBm"));
                    ui.checkbox(&mut self.f.show_hidden, "Versteckte SSIDs");
                    ui.checkbox(&mut self.f.only_pinned, "Nur markierte ★");
                    ui.checkbox(&mut self.f.only_open, "Nur offene Netze");
                    ui.checkbox(&mut self.f.hide_stale, "Veraltete ausblenden");
                    let active = !self.f.text.is_empty()
                        || !(self.f.b24 && self.f.b5 && self.f.b6)
                        || self.f.min_rssi > -100
                        || !self.f.show_hidden
                        || self.f.only_pinned
                        || self.f.only_open
                        || self.f.hide_stale;
                    if active && ui.small_button("✖ Filter zurücksetzen").clicked() {
                        self.f = Filter::default();
                    }
                });

                ui.add_space(4.0);
                let now = Instant::now();
                let vis: Vec<&ApRecord> = self.aps.values().filter(|r| self.visible(r, now)).collect();
                let ssids: HashSet<&str> = vis.iter().filter(|r| !r.info.hidden).map(|r| r.info.ssid.as_str()).collect();
                let cnt = |b: Band| vis.iter().filter(|r| r.info.band == b).count();
                // Zählen + stabil sortieren (Anzahl absteigend, dann Name) – sonst springen gleich große Einträge
                fn ranked<K: Ord + Clone>(items: impl Iterator<Item = K>) -> Vec<(K, usize)> {
                    let mut m: std::collections::BTreeMap<K, usize> = std::collections::BTreeMap::new();
                    for k in items {
                        *m.entry(k).or_default() += 1;
                    }
                    let mut v: Vec<_> = m.into_iter().collect();
                    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                    v
                }
                egui::CollapsingHeader::new(section("Übersicht")).default_open(true).show(ui, |ui| {
                egui::Grid::new("summary").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
                    ui.label("BSSIDs");
                    ui.label(RichText::new(vis.len().to_string()).strong());
                    ui.end_row();
                    ui.label("SSIDs");
                    ui.label(RichText::new(ssids.len().to_string()).strong());
                    ui.end_row();
                    ui.label("2,4 GHz");
                    ui.label(cnt(Band::B24).to_string());
                    ui.end_row();
                    ui.label("5 GHz");
                    ui.label(cnt(Band::B5).to_string());
                    ui.end_row();
                    ui.label("6 GHz");
                    ui.label(cnt(Band::B6).to_string());
                    ui.end_row();
                    ui.label("Offen/Unsicher");
                    let weak = vis.iter().filter(|r| r.info.sec_level <= SecLevel::Weak).count();
                    ui.label(RichText::new(weak.to_string()).color(if weak > 0 {
                        Color32::from_rgb(230, 126, 34)
                    } else {
                        ui.visuals().text_color()
                    }));
                    ui.end_row();
                });

                });

                egui::CollapsingHeader::new(section("Standards")).default_open(true).show(ui, |ui| {
                    for (k, n) in ranked(vis.iter().map(|r| r.info.standard.generation(r.info.band))) {
                        bar_row(ui, k, n, vis.len(), ACCENT);
                    }
                });
                egui::CollapsingHeader::new(section("Sicherheit")).default_open(true).show(ui, |ui| {
                    for ((l, k), n) in ranked(vis.iter().map(|r| (r.info.sec_level, r.info.security.as_str()))) {
                        bar_row(ui, k, n, vis.len(), sec_color(l));
                    }
                });
                egui::CollapsingHeader::new(section("Top-Hersteller")).default_open(true).show(ui, |ui| {
                    for (k, n) in ranked(vis.iter().map(|r| r.vendor.as_str())).into_iter().take(10) {
                        bar_row(ui, k, n, vis.len(), Color32::from_rgb(142, 110, 220));
                    }
                });

                egui::CollapsingHeader::new(section("Einstellungen")).default_open(false).show(ui, |ui| {
                    egui::Grid::new("settings").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                        ui.label("Veraltet nach");
                        ui.add(egui::DragValue::new(&mut self.stale_after).range(5.0..=120.0).suffix(" s"));
                        ui.end_row();
                        ui.label("Entfernen nach");
                        ui.add(egui::DragValue::new(&mut self.remove_after).range(20.0..=3600.0).suffix(" s"));
                        ui.end_row();
                    });
                    ui.add_space(4.0);
                    let oui_label = if self.oui_rx.is_some() {
                        "⏳ Lade Hersteller-DB…".to_string()
                    } else if self.oui.full_db {
                        format!("🏭 Hersteller-DB aktualisieren ({})", self.oui.len())
                    } else {
                        "🏭 Hersteller-DB laden".to_string()
                    };
                    if ui
                        .add_enabled(self.oui_rx.is_none(), egui::Button::new(oui_label))
                        .on_hover_text("Lädt die vollständige OUI-Liste (Wireshark/IEEE) für die Hersteller-Erkennung")
                        .clicked()
                    {
                        self.start_oui_download(ctx);
                    }
                    if ui.button("🗑 Liste leeren").clicked() {
                        self.aps.clear();
                        self.selected = None;
                        self.expanded.clear();
                    }
                });
            });
        });
    }

    fn details_panel(&mut self, ctx: &egui::Context) {
        let Some(sel) = self.selected else { return };
        let Some(r) = self.aps.get(&sel) else { return };
        let mut close = false;
        let mut toggle_pin = false;
        let mut finder = false;
        let pinned = self.pinned.contains(&sel);
        let now = Instant::now();
        egui::SidePanel::right("details").resizable(true).default_width(380.0).show(ctx, |ui| {
            let i = &r.info;
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                ui.painter().circle_filled(rect.center(), 6.0, ap_color(&sel));
                ui.label(RichText::new(Self::display_ssid(r)).size(18.0).strong());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("✖").clicked() {
                        close = true;
                    }
                });
            });
            ui.horizontal(|ui| {
                if ui.button(if pinned { "★ Markiert" } else { "☆ Markieren" }).clicked() {
                    toggle_pin = true;
                }
                if ui.button("🎯 Signal-Finder").clicked() {
                    finder = true;
                }
                if ui.button("📋 BSSID").clicked() {
                    ui.ctx().copy_text(fmt_mac(&sel));
                }
            });
            // großes Signal
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{} dBm", i.rssi)).size(30.0).strong().color(signal_color(i.rssi)));
                ui.vertical(|ui| {
                    ui.label(signal_text(i.rssi));
                    ui.label(
                        RichText::new(format!("min {} / Ø {:.1} / max {}", r.rssi_min, r.rssi_avg(), r.rssi_max)).weak(),
                    );
                });
            });
            ui.add(
                egui::ProgressBar::new(signal_frac(i.rssi))
                    .fill(signal_color(i.rssi))
                    .desired_height(8.0),
            );
            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| {
                let kv = |ui: &mut Ui, k: &str, v: String| {
                    ui.label(RichText::new(k).weak());
                    ui.label(v);
                    ui.end_row();
                };
                let yn = |b: bool| if b { "✔ ja".to_string() } else { "✖ nein".to_string() };

                egui::CollapsingHeader::new(RichText::new("Allgemein").strong()).default_open(true).show(ui, |ui| {
                    egui::Grid::new("g_general").num_columns(2).striped(true).show(ui, |ui| {
                        kv(ui, "BSSID", fmt_mac(&i.bssid));
                        kv(ui, "Hersteller", format!("{}{}", r.vendor, if r.vendor_virtual { " (virt. BSSID)" } else { "" }));
                        if let Some(n) = &i.ap_name {
                            kv(ui, "AP-Name", n.clone());
                        }
                        if let Some(c) = &i.chipset {
                            kv(ui, "Chipsatz", c.clone());
                        }
                        kv(ui, "Uptime (TSF)", fmt_duration(i.uptime_s));
                        kv(ui, "Erstmals gesehen", r.first_seen_wall.format("%H:%M:%S").to_string());
                        kv(ui, "Zuletzt gesehen", fmt_ago(r.age(now)));
                        kv(ui, "Messwerte", r.samples.to_string());
                        if let Some(c) = &i.country {
                            kv(ui, "Land", c.clone());
                        }
                        kv(ui, "Im Reg.-Bereich", yn(i.in_reg_domain));
                    });
                });
                egui::CollapsingHeader::new(RichText::new("Funk").strong()).default_open(true).show(ui, |ui| {
                    egui::Grid::new("g_radio").num_columns(2).striped(true).show(ui, |ui| {
                        kv(ui, "Band", i.band.label().into());
                        kv(
                            ui,
                            "Kanal",
                            format!(
                                "{}{}{}",
                                i.channel,
                                if i.band == Band::B5 && is_dfs(i.channel) { " (DFS)" } else { "" },
                                if i.band == Band::B6 && is_psc(i.channel) { " (PSC)" } else { "" }
                            ),
                        );
                        kv(ui, "Frequenz", format!("{} MHz", i.freq_mhz));
                        kv(ui, "Kanalbreite", i.width_label.clone());
                        kv(ui, "Mittenkanal", format!("{}", i.center));
                        kv(ui, "Standard", format!("{} – {}", i.standard.generation(i.band), i.standard.ieee()));
                        kv(ui, "Spatial Streams", format!("{}×{}", i.streams, i.streams));
                        kv(ui, "Max. PHY-Rate", format!("{:.0} Mbit/s", i.max_rate));
                        kv(ui, "Signalqualität", format!("{} %", i.quality));
                        if let Some(c) = i.bss_color {
                            kv(ui, "BSS Color", c.to_string());
                        }
                        if let Some(p) = i.tx_power {
                            kv(ui, "Sendeleistung (TPC)", format!("{p} dBm"));
                        }
                        if let Some(p) = i.power_constraint {
                            kv(ui, "Power Constraint", format!("{p} dB"));
                        }
                        kv(ui, "Beacon-Intervall", format!("{:.1} ms", i.beacon_ms));
                        if let Some(d) = i.dtim {
                            kv(ui, "DTIM-Periode", d.to_string());
                        }
                        let rates = |v: &[f32]| v.iter().map(|r| format!("{r}")).collect::<Vec<_>>().join(", ");
                        kv(ui, "Datenraten", rates(&i.rates));
                        kv(ui, "Basisraten", rates(&i.basic_rates));
                    });
                });
                egui::CollapsingHeader::new(RichText::new("Sicherheit").strong()).default_open(true).show(ui, |ui| {
                    egui::Grid::new("g_sec").num_columns(2).striped(true).show(ui, |ui| {
                        ui.label(RichText::new("Verfahren").weak());
                        ui.label(RichText::new(&i.security).color(sec_color(i.sec_level)).strong());
                        ui.end_row();
                        kv(ui, "AKM", if i.akms.is_empty() { "-".into() } else { i.akms.join(", ") });
                        kv(ui, "Pairwise Cipher", if i.pairwise.is_empty() { "-".into() } else { i.pairwise.join(", ") });
                        kv(ui, "Group Cipher", i.group_cipher.clone());
                        kv(ui, "PMF (802.11w)", i.pmf.clone());
                        if let Some(w) = &i.wps {
                            kv(
                                ui,
                                "WPS",
                                format!(
                                    "aktiv{}{}",
                                    match w.configured {
                                        Some(true) => ", konfiguriert",
                                        Some(false) => ", NICHT konfiguriert",
                                        None => "",
                                    },
                                    if w.locked { ", gesperrt" } else { "" }
                                ),
                            );
                        } else {
                            kv(ui, "WPS", "nein".into());
                        }
                    });
                });
                egui::CollapsingHeader::new(RichText::new("Last & Roaming").strong()).default_open(true).show(ui, |ui| {
                    egui::Grid::new("g_load").num_columns(2).striped(true).show(ui, |ui| {
                        kv(ui, "Clients (BSS Load)", i.stations.map(|v| v.to_string()).unwrap_or("n/a".into()));
                        kv(ui, "Kanalauslastung", i.ch_util.map(|v| format!("{v:.0} %")).unwrap_or("n/a".into()));
                        if let Some(a) = i.admission_cap {
                            kv(ui, "Admission Capacity", format!("{} µs/s", a as u32 * 32));
                        }
                        kv(ui, "802.11k (RRM)", yn(i.k_rrm));
                        kv(ui, "802.11r (Fast Transition)", yn(i.r_ft));
                        kv(ui, "802.11v (BSS Transition)", yn(i.v_btm));
                        kv(ui, "WMM / QoS", yn(i.wmm));
                        kv(ui, "MBO/OCE", yn(i.mbo));
                        kv(ui, "Hotspot 2.0 / Passpoint", yn(i.hs20));
                        kv(ui, "Mesh", yn(i.mesh));
                        kv(ui, "Multi-Link (MLO)", yn(i.mlo));
                    });
                });
                if let Some(w) = &i.wps {
                    egui::CollapsingHeader::new(RichText::new("Geräteinfo (WPS)").strong()).default_open(true).show(ui, |ui| {
                        egui::Grid::new("g_wps").num_columns(2).striped(true).show(ui, |ui| {
                            let o = |v: &Option<String>| v.clone().unwrap_or("-".into());
                            kv(ui, "Hersteller", o(&w.manufacturer));
                            kv(ui, "Modell", o(&w.model));
                            kv(ui, "Modellnummer", o(&w.model_number));
                            kv(ui, "Gerätename", o(&w.device_name));
                            kv(ui, "Seriennummer", o(&w.serial));
                        });
                    });
                }
                egui::CollapsingHeader::new(RichText::new("Vendor-IEs").strong()).default_open(false).show(ui, |ui| {
                    for v in &i.ie_vendors {
                        ui.label(format!("• {v}"));
                    }
                });
                egui::CollapsingHeader::new(RichText::new(format!("Information Elements ({})", i.ies.len())))
                    .default_open(false)
                    .show(ui, |ui| {
                        for ie in &i.ies {
                            let hex: String = ie.data.iter().map(|b| format!("{b:02X} ")).collect();
                            let title = match ie.ext {
                                Some(e) => format!("255/{e} {} ({} B)", ie_name(ie.id, ie.ext), ie.data.len()),
                                None => format!("{} {} ({} B)", ie.id, ie_name(ie.id, None), ie.data.len()),
                            };
                            ui.label(RichText::new(title).strong());
                            ui.label(RichText::new(hex).monospace().small().weak());
                        }
                    });
            });
        });
        if close {
            self.selected = None;
        }
        if toggle_pin && !self.pinned.remove(&sel) {
            self.pinned.insert(sel);
        }
        if finder {
            self.tab = Tab::Finder;
        }
    }

    // ------------------------------------------------------------------ Tabs

    /// Eindeutige Farben je SSID aus einer festen Palette (alphabetisch vergeben, damit stabil)
    fn ssid_colors(&self) -> HashMap<String, Color32> {
        const PALETTE: [Color32; 12] = [
            Color32::from_rgb(66, 165, 245),
            Color32::from_rgb(255, 167, 38),
            Color32::from_rgb(102, 187, 106),
            Color32::from_rgb(239, 83, 80),
            Color32::from_rgb(171, 71, 188),
            Color32::from_rgb(38, 198, 218),
            Color32::from_rgb(255, 238, 88),
            Color32::from_rgb(236, 64, 122),
            Color32::from_rgb(141, 110, 99),
            Color32::from_rgb(156, 204, 101),
            Color32::from_rgb(121, 134, 203),
            Color32::from_rgb(189, 189, 189),
        ];
        let mut names: Vec<String> = self.aps.values().map(Self::group_key).collect();
        names.sort();
        names.dedup();
        names
            .into_iter()
            .enumerate()
            .map(|(i, n)| {
                let c = PALETTE[i % PALETTE.len()];
                (n, if i >= PALETTE.len() { c.gamma_multiply(0.7) } else { c })
            })
            .collect()
    }

    fn group_key(r: &ApRecord) -> String {
        if r.info.hidden {
            "\u{0}hidden".into()
        } else {
            r.info.ssid.clone()
        }
    }

    fn networks_tab(&mut self, ui: &mut Ui) {
        let keys = self.sorted_keys();
        let now = Instant::now();

        // Zeilen aufbauen: einzeln oder nach SSID gruppiert (stärkster AP repräsentiert die Gruppe)
        let mut rows: Vec<([u8; 6], RowKind)> = Vec::new();
        let mut group_count = 0;
        if self.group_ssid {
            let mut order: Vec<String> = Vec::new();
            let mut groups: HashMap<String, Vec<[u8; 6]>> = HashMap::new();
            for k in &keys {
                let g = Self::group_key(&self.aps[k]);
                if !groups.contains_key(&g) {
                    order.push(g.clone());
                }
                groups.entry(g).or_default().push(*k);
            }
            group_count = order.len();
            for g in order {
                let mut members = groups.remove(&g).unwrap_or_default();
                if members.len() == 1 {
                    rows.push((members[0], RowKind::Single));
                    continue;
                }
                // stärkster aktueller AP zuerst, veraltete ans Ende
                members.sort_by_key(|k| {
                    let r = &self.aps[k];
                    (r.age(now) > self.stale_after, -r.info.rssi)
                });
                let rep = members[0];
                let mut chans: Vec<u32> = members.iter().map(|k| self.aps[k].info.channel).collect();
                chans.sort();
                chans.dedup();
                let mut bands: Vec<Band> = members.iter().map(|k| self.aps[k].info.band).collect();
                bands.sort();
                bands.dedup();
                let expanded = self.expanded.contains(&g);
                rows.push((
                    rep,
                    RowKind::Group {
                        key: g,
                        n: members.len(),
                        expanded,
                        chans: chans.iter().map(|c| c.to_string()).collect::<Vec<_>>().join(", "),
                        bands: bands.iter().map(|b| b.label().replace(" GHz", "")).collect::<Vec<_>>().join("/") + " GHz",
                    },
                ));
                if expanded {
                    for m in members {
                        rows.push((m, RowKind::Child));
                    }
                }
            }
        } else {
            rows = keys.iter().map(|k| (*k, RowKind::Single)).collect();
        }

        ui.horizontal(|ui| {
            ui.checkbox(&mut self.group_ssid, "Nach SSID gruppieren");
            if self.group_ssid {
                if ui.small_button("⏷ Alle aufklappen").clicked() {
                    for r in self.aps.values() {
                        self.expanded.insert(Self::group_key(r));
                    }
                }
                if ui.small_button("▶ Alle zuklappen").clicked() {
                    self.expanded.clear();
                }
                ui.label(
                    RichText::new(format!(
                        "{group_count} SSIDs · {} BSSIDs – Klick auf eine Gruppe zeigt alle APs der SSID",
                        keys.len()
                    ))
                    .weak(),
                );
            } else {
                ui.label(RichText::new(format!("{} BSSIDs", keys.len())).weak());
            }
        });

        if rows.is_empty() {
            ui.add_space(60.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("Kein Netz passt zum Filter").size(18.0).weak());
                if ui.button("✖ Filter zurücksetzen").clicked() {
                    self.f = Filter::default();
                }
            });
            return;
        }

        let mut clicked: Option<([u8; 6], Option<String>)> = None;
        let mut pin_toggle = None;
        let mut sort_click = None;
        let headers: [(SortCol, &str); 13] = [
            (SortCol::Pin, "★"),
            (SortCol::Ssid, "SSID"),
            (SortCol::Bssid, "BSSID"),
            (SortCol::Vendor, "Hersteller"),
            (SortCol::Signal, "Signal"),
            (SortCol::Channel, "Kanal"),
            (SortCol::Width, "Breite"),
            (SortCol::Standard, "Standard"),
            (SortCol::Security, "Sicherheit"),
            (SortCol::Rate, "Mbit/s"),
            (SortCol::Clients, "Clients"),
            (SortCol::Util, "Last"),
            (SortCol::Seen, "Gesehen"),
        ];
        let text_col = ui.visuals().text_color();
        let child_bg = ACCENT.gamma_multiply(0.07);
        let palette = self.ssid_colors();
        egui::ScrollArea::horizontal().show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .sense(Sense::click())
                .cell_layout(Layout::left_to_right(Align::Center))
                .column(Column::exact(22.0)) // ★
                .column(Column::initial(170.0).at_least(100.0).clip(true)) // SSID
                .column(Column::initial(128.0)) // BSSID
                .column(Column::initial(104.0).at_least(60.0).clip(true)) // Hersteller
                .column(Column::initial(118.0).at_least(80.0)) // Signal
                .column(Column::initial(86.0).at_least(60.0).clip(true)) // Kanal + Band
                .column(Column::initial(62.0)) // Breite
                .column(Column::initial(84.0).clip(true)) // Standard
                .column(Column::initial(120.0).at_least(80.0).clip(true)) // Sicherheit
                .column(Column::initial(56.0)) // Rate
                .column(Column::initial(50.0)) // Clients
                .column(Column::initial(56.0)) // Auslastung
                .column(Column::remainder().at_least(60.0)) // Gesehen
                .min_scrolled_height(0.0)
                .header(26.0, |mut h| {
                    for (col, label) in headers {
                        h.col(|ui| {
                            let txt = if self.sort_col == col {
                                format!("{label} {}", if self.sort_asc { "⏶" } else { "⏷" })
                            } else {
                                label.to_string()
                            };
                            if ui.add(egui::Label::new(RichText::new(txt).strong()).sense(Sense::click())).clicked() {
                                sort_click = Some(col);
                            }
                        });
                    }
                })
                .body(|body| {
                    body.rows(24.0, rows.len(), |mut row| {
                        let (k, kind) = &rows[row.index()];
                        let k = *k;
                        let r = &self.aps[&k];
                        let i = &r.info;
                        let stale = r.age(now) > self.stale_after;
                        let tc = if stale { text_col.gamma_multiply(0.45) } else { text_col };
                        let is_child = matches!(kind, RowKind::Child);
                        row.set_selected(self.selected == Some(k));
                        let shade = |ui: &mut Ui| {
                            if is_child {
                                ui.painter().rect_filled(ui.max_rect().expand2(Vec2::new(4.0, 2.0)), 0.0, child_bg);
                            }
                        };
                        row.col(|ui| {
                            shade(ui);
                            let p = self.pinned.contains(&k);
                            let star = RichText::new(if p { "★" } else { "☆" })
                                .color(if p { Color32::from_rgb(250, 200, 40) } else { tc.gamma_multiply(0.6) });
                            if ui.add(egui::Label::new(star).sense(Sense::click())).clicked() {
                                pin_toggle = Some(k);
                            }
                        });
                        row.col(|ui| {
                            shade(ui);
                            match kind {
                                RowKind::Group { expanded, .. } => {
                                    ui.label(RichText::new(if *expanded { "⏷" } else { "▶" }).color(ACCENT));
                                }
                                RowKind::Child => {
                                    ui.add_space(12.0);
                                    ui.label(RichText::new("›").weak());
                                }
                                RowKind::Single => {
                                    ui.add_space(14.0);
                                }
                            }
                            let (rect, _) = ui.allocate_exact_size(Vec2::new(4.0, 16.0), Sense::hover());
                            ui.painter().rect_filled(rect, 2.0, if self.group_ssid { palette.get(&Self::group_key(r)).copied().unwrap_or(ACCENT) } else { ap_color(&k) });
                            let t = RichText::new(Self::display_ssid(r)).color(tc);
                            let t = if i.hidden {
                                t.italics()
                            } else if is_child {
                                t
                            } else {
                                t.strong()
                            };
                            ui.label(t);
                            if let RowKind::Group { n, .. } = kind {
                                egui::Frame::none()
                                    .fill(ACCENT.gamma_multiply(0.45))
                                    .rounding(8.0)
                                    .inner_margin(egui::Margin::symmetric(6.0, 0.0))
                                    .show(ui, |ui| {
                                        ui.label(RichText::new(format!("{n} APs")).small().color(Color32::WHITE));
                                    });
                            }
                            if self.conn.as_ref().map(|c| c.bssid) == Some(k) {
                                ui.label(RichText::new("🔗").color(Color32::from_rgb(46, 204, 113)));
                            }
                        });
                        row.col(|ui| {
                            shade(ui);
                            ui.label(RichText::new(fmt_mac(&k)).monospace().color(tc));
                        });
                        row.col(|ui| {
                            shade(ui);
                            ui.label(RichText::new(&r.vendor).color(tc))
                                .on_hover_text(i.ap_name.clone().map(|n| format!("AP-Name: {n}")).unwrap_or_default());
                        });
                        row.col(|ui| {
                            shade(ui);
                            signal_bar(ui, i.rssi, stale)
                        });
                        row.col(|ui| {
                            shade(ui);
                            match kind {
                                RowKind::Group { chans, bands, .. } => {
                                    ui.label(RichText::new(chans).color(tc)).on_hover_text(bands.as_str());
                                }
                                _ => {
                                    ui.label(RichText::new(i.channel.to_string()).color(tc));
                                    let band = match i.band {
                                        Band::B24 => "2,4G",
                                        Band::B5 => "5G",
                                        Band::B6 => "6G",
                                        Band::Other => "?",
                                    };
                                    ui.label(RichText::new(band).small().weak());
                                    if i.band == Band::B5 && is_dfs(i.channel) {
                                        ui.label(RichText::new("DFS").small().color(Color32::from_rgb(220, 140, 60)))
                                            .on_hover_text("DFS-Kanal: Radar-Erkennung, AP kann den Kanal wechseln");
                                    }
                                }
                            }
                        });
                        row.col(|ui| {
                            shade(ui);
                            ui.label(RichText::new(&i.width_label).color(tc));
                        });
                        row.col(|ui| {
                            shade(ui);
                            ui.label(RichText::new(i.std_label()).color(tc));
                        });
                        row.col(|ui| {
                            shade(ui);
                            let icon = if i.sec_level == SecLevel::Open { "🔓" } else { "🔒" };
                            ui.label(
                                RichText::new(format!("{icon} {}", i.security))
                                    .color(if stale { tc } else { sec_color(i.sec_level) }),
                            );
                        });
                        row.col(|ui| {
                            shade(ui);
                            ui.label(RichText::new(format!("{:.0}", i.max_rate)).color(tc)).on_hover_text("Theoretische max. PHY-Rate in Mbit/s");
                        });
                        row.col(|ui| {
                            shade(ui);
                            ui.label(RichText::new(i.stations.map(|v| v.to_string()).unwrap_or("–".into())).color(tc));
                        });
                        row.col(|ui| {
                            shade(ui);
                            match i.ch_util {
                                Some(u) => {
                                    ui.label(RichText::new(format!("{u:.0} %")).color(load_color(u as f64 * 0.6)));
                                }
                                None => {
                                    ui.label(RichText::new("–").color(tc));
                                }
                            }
                        });
                        row.col(|ui| {
                            shade(ui);
                            ui.label(RichText::new(fmt_ago(r.age(now))).color(tc));
                        });
                        if row.response().clicked() {
                            let g = if let RowKind::Group { key, .. } = kind { Some(key.clone()) } else { None };
                            clicked = Some((k, g));
                        }
                    });
                });
        });
        if let Some(c) = sort_click {
            if self.sort_col == c {
                self.sort_asc = !self.sort_asc;
            } else {
                self.sort_col = c;
                self.sort_asc = matches!(c, SortCol::Ssid | SortCol::Bssid | SortCol::Vendor | SortCol::Channel);
            }
        }
        if let Some(k) = pin_toggle {
            if !self.pinned.remove(&k) {
                self.pinned.insert(k);
            }
        } else if let Some((k, group)) = clicked {
            match group {
                Some(g) => {
                    if !self.expanded.remove(&g) {
                        self.expanded.insert(g);
                    }
                    self.selected = Some(k);
                }
                None => self.selected = if self.selected == Some(k) { None } else { Some(k) },
            }
        }
    }

    fn band_selector(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            for b in [Band::B24, Band::B5, Band::B6] {
                ui.selectable_value(&mut self.chan_band, b, RichText::new(b.label()).strong());
            }
        });
    }

    fn channels_tab(&mut self, ui: &mut Ui) {
        self.band_selector(ui);
        let band = self.chan_band;
        let now = Instant::now();
        let mut recs: Vec<&ApRecord> = self
            .aps
            .values()
            .filter(|r| r.info.band == band && self.visible(r, now))
            .collect();
        // schwache zuerst zeichnen, starke oben
        recs.sort_by_key(|r| r.info.rssi);
        let group_ssid = self.group_ssid;
        let palette = self.ssid_colors();
        let color_of = |r: &ApRecord| if group_ssid { palette.get(&Self::group_key(r)).copied().unwrap_or(ACCENT) } else { ap_color(&r.info.bssid) };
        // Beschriftung: je Kanalmitte eine gestapelte Liste der SSIDs (stärkste zuerst), damit sich nichts überlagert
        let mut label_stacks: Vec<(f64, f64, Vec<(String, Color32, [u8; 6])>)> = Vec::new();
        for r in recs.iter().rev() {
            let name = Self::display_ssid(r);
            let idx = label_stacks.iter().position(|(c, _, _)| (c - r.info.center).abs() < 0.01);
            let stack = match idx {
                Some(i) => &mut label_stacks[i],
                None => {
                    label_stacks.push((r.info.center, r.info.rssi as f64, Vec::new()));
                    label_stacks.last_mut().unwrap()
                }
            };
            if !stack.2.iter().any(|(n, _, _)| *n == name) {
                stack.2.push((name, color_of(r), r.info.bssid));
            }
        }

        let (xmin, xmax, steps) = match band {
            Band::B24 => (-1.0, 16.0, [1.0, 1.0, 1.0]),
            Band::B5 => (30.0, 180.0, [4.0, 4.0, 16.0]),
            _ => (-4.0, 238.0, [4.0, 16.0, 32.0]),
        };
        let sel = self.selected;
        let text_col = ui.visuals().text_color();
        let h = ui.available_height() - 60.0;
        let resp = Plot::new(("chan", band as u8))
            .height(h.max(200.0))
            .include_x(xmin)
            .include_x(xmax)
            .include_y(-100.0)
            .include_y(-20.0)
            .allow_zoom(Vec2b::new(band != Band::B24, false))
            .allow_drag(Vec2b::new(band != Band::B24, false))
            .allow_scroll(false)
            .allow_boxed_zoom(false)
            .x_grid_spacer(uniform_grid_spacer(move |_| steps))
            .x_axis_formatter(move |m, _| {
                // nur echte Kanalnummern beschriften (2,4 GHz: 1–14)
                let max = if band == Band::B24 { 14.0 } else { 240.0 };
                if m.value.fract() == 0.0 && m.value >= 1.0 && m.value <= max {
                    format!("{}", m.value)
                } else {
                    String::new()
                }
            })
            .y_axis_formatter(dbm_axis)
            .x_axis_label("Kanal")
            .label_formatter(|_, v| format!("Kanal {:.0}\n{:.0} dBm", v.x, v.y))
            .show(ui, |pui| {
                if band == Band::B5 {
                    // schmales DFS-Band am oberen Rand statt Vollfläche
                    pui.polygon(
                        Polygon::new(PlotPoints::from(vec![[50.0, -22.5], [146.0, -22.5], [146.0, -20.5], [50.0, -20.5]]))
                            .fill_color(Color32::from_rgb(200, 120, 40).gamma_multiply(0.5))
                            .stroke(Stroke::NONE),
                    );
                    pui.text(
                        Text::new(
                            PlotPoint::new(98.0, -23.0),
                            RichText::new("DFS (Kanal 52–144, Radar-Erkennung)").small().color(Color32::from_rgb(220, 140, 60)),
                        )
                        .anchor(Align2::CENTER_TOP),
                    );
                }
                for r in &recs {
                    let c = color_of(r);
                    let is_sel = sel == Some(r.info.bssid);
                    let dim = sel.is_some() && !is_sel;
                    let (fa, sw): (f32, f32) = if is_sel { (0.35, 3.0) } else if dim { (0.05, 1.0) } else { (0.16, 1.6) };
                    let stroke_c = if dim { c.gamma_multiply(0.4) } else { c };
                    pui.polygon(
                        Polygon::new(PlotPoints::from(ap_shape(r.info.center, half_span(r), r.info.rssi as f64)))
                            .fill_color(c.gamma_multiply(fa))
                            .stroke(Stroke::new(sw, stroke_c)),
                    );
                }
                for (center, top, names) in &label_stacks {
                    let shown = names.len().min(5);
                    for (n, (name, c, b)) in names.iter().take(shown).enumerate() {
                        let dim = sel.is_some() && sel != Some(*b);
                        let mut t = RichText::new(name).size(11.5).color(if dim { text_col.gamma_multiply(0.35) } else { *c });
                        if sel == Some(*b) {
                            t = t.strong();
                        }
                        // von oben nach unten: stärkste SSID ganz oben
                        let y = top + 0.8 + (shown - 1 - n) as f64 * 3.0;
                        pui.text(Text::new(PlotPoint::new(*center, y), t).anchor(Align2::CENTER_BOTTOM));
                    }
                    if names.len() > shown {
                        pui.text(
                            Text::new(
                                PlotPoint::new(*center, top + 0.8 + shown as f64 * 3.0),
                                RichText::new(format!("+{} weitere", names.len() - shown)).small().color(text_col.gamma_multiply(0.6)),
                            )
                            .anchor(Align2::CENTER_BOTTOM),
                        );
                    }
                }
                (pui.pointer_coordinate(), pui.response().clicked())
            });
        let (ptr, clicked) = resp.inner;
        // APs unter dem Mauszeiger
        if let Some(p) = ptr {
            let mut hits: Vec<&&ApRecord> = recs
                .iter()
                .filter(|r| (p.x - r.info.center).abs() <= half_span(r) && p.y <= r.info.rssi as f64)
                .collect();
            hits.sort_by_key(|r| -r.info.rssi);
            if clicked {
                self.selected = hits.first().map(|r| r.info.bssid);
            }
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Unter Cursor:").weak());
                for r in hits.iter().take(8) {
                    ui.label(
                        RichText::new(format!("{} ({} dBm, Kanal {})", Self::display_ssid(r), r.info.rssi, r.info.channel))
                            .color(ap_color(&r.info.bssid)),
                    );
                }
            });
        } else {
            ui.label(
                RichText::new(format!(
                    "{} Netze im {}. Klick auf eine Kurve wählt den AP aus{}.",
                    recs.len(),
                    band.label(),
                    if band == Band::B24 { "" } else { " – Mausrad/Ziehen zum Zoomen, Doppelklick zum Zurücksetzen" }
                ))
                .weak(),
            );
        }
    }

    fn history_tab(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.hist_all, false, "Markierte + Ausgewählter");
            ui.selectable_value(&mut self.hist_all, true, "Alle sichtbaren (Top 30)");
            ui.separator();
            ui.label("Zeitraum");
            for (s, l) in [(60.0, "1 min"), (300.0, "5 min"), (900.0, "15 min"), (3600.0, "60 min")] {
                ui.selectable_value(&mut self.hist_window, s, l);
            }
        });
        let now = Instant::now();
        let t_now = self.now_t();
        let win = self.hist_window;
        let mut recs: Vec<&ApRecord> = self
            .aps
            .values()
            .filter(|r| self.visible(r, now))
            .filter(|r| self.hist_all || self.pinned.contains(&r.info.bssid) || self.selected == Some(r.info.bssid))
            .collect();
        recs.sort_by_key(|r| -r.info.rssi);
        recs.truncate(30);
        let show_conn = !self.hist_all && self.conn.is_some();
        if recs.is_empty() && !show_conn {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("Markiere APs mit ☆ in der Netzwerkliste oder wähle einen aus,\num den Signalverlauf zu sehen.").size(16.0).weak());
            });
            return;
        }
        if recs.is_empty() {
            ui.label(RichText::new("Nur die aktuelle Verbindung – weitere APs mit ☆ markieren oder in der Liste auswählen.").weak());
        }
        let sel = self.selected;
        Plot::new("history")
            .legend(Legend::default().position(Corner::LeftTop))
            .include_y(-100.0)
            .include_y(-20.0)
            .include_x(-win)
            .include_x(0.0)
            .allow_zoom(false)
            .allow_drag(false)
            .allow_scroll(false)
            .x_axis_label("Zeit")
            .y_axis_formatter(dbm_axis)
            .x_axis_formatter(|m, _| {
                if m.value > 0.5 {
                    return String::new();
                }
                let s = (-m.value).round().max(0.0) as u64;
                if s == 0 {
                    "jetzt".into()
                } else {
                    format!("-{}:{:02}", s / 60, s % 60)
                }
            })
            .label_formatter(|name, v| {
                let s = (-v.x).max(0.0) as u64;
                format!("{name}\nvor {}:{:02} · {:.0} dBm", s / 60, s % 60, v.y)
            })
            .show(ui, |pui| {
                for r in &recs {
                    let pts: Vec<[f64; 2]> = r
                        .history
                        .iter()
                        .filter(|(t, _)| t_now - t <= win)
                        .map(|(t, v)| [t - t_now, *v])
                        .collect();
                    let w: f32 = if sel == Some(r.info.bssid) { 3.0 } else { 1.8 };
                    pui.line(
                        Line::new(PlotPoints::from(pts))
                            .color(ap_color(&r.info.bssid))
                            .width(w)
                            .name(format!("{} [{}]", Self::display_ssid(r), &fmt_mac(&r.info.bssid)[9..])),
                    );
                }
                if show_conn {
                    let pts: Vec<[f64; 2]> = self
                        .conn_hist
                        .iter()
                        .filter(|(t, _)| t_now - t <= win)
                        .map(|(t, v)| [t - t_now, *v])
                        .collect();
                    pui.line(
                        Line::new(PlotPoints::from(pts))
                            .color(Color32::WHITE)
                            .style(egui_plot::LineStyle::Solid)
                            .width(2.0f32)
                            .name("Aktuelle Verbindung (RSSI)"),
                    );
                }
            });
    }

    /// Störleistung (mW, Anzahl APs) auf einem 20-MHz-Kanal
    fn channel_load(&self, band: Band, ch: u32, now: Instant) -> (f64, usize) {
        let (lo, hi) = (ch as f64 - 2.0, ch as f64 + 2.0);
        let mut mw = 0.0;
        let mut n = 0;
        for r in self.aps.values() {
            if r.info.band != band || r.age(now) > self.stale_after {
                continue;
            }
            let h = half_span(r);
            let ov = hi.min(r.info.center + h) - lo.max(r.info.center - h);
            if ov > 0.0 {
                mw += 10f64.powf(r.info.rssi as f64 / 10.0) * (ov / 4.0).min(1.0);
                n += 1;
            }
        }
        (mw, n)
    }

    fn mw_to_score(mw: f64) -> f64 {
        if mw <= 0.0 {
            0.0
        } else {
            (10.0 * mw.log10() + 100.0).max(0.0)
        }
    }

    fn rating_tab(&mut self, ui: &mut Ui) {
        self.band_selector(ui);
        let band = self.chan_band;
        let now = Instant::now();
        let chans: Vec<u32> = match band {
            Band::B24 => (1..=13).collect(),
            Band::B5 => [36, 40, 44, 48, 52, 56, 60, 64]
                .into_iter()
                .chain((100..=144).step_by(4))
                .chain([149, 153, 157, 161, 165])
                .collect(),
            _ => (1..=233).step_by(4).collect(),
        };
        let loads: Vec<(u32, f64, f64, usize)> = chans
            .iter()
            .map(|&c| {
                let (mw, n) = self.channel_load(band, c, now);
                (c, mw, Self::mw_to_score(mw), n)
            })
            .collect();

        let best_of = |cands: &[u32]| -> Option<(u32, f64, usize)> {
            loads
                .iter()
                .filter(|l| cands.contains(&l.0))
                .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
                .map(|l| (l.0, l.2, l.3))
        };

        // Empfehlungen
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("💡 Empfehlung").strong().size(16.0));
            let rec_line = |ui: &mut Ui, title: &str, r: Option<(u32, f64, usize)>| {
                if let Some((c, s, n)) = r {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(title).weak());
                        ui.label(RichText::new(format!("Kanal {c}")).strong().size(16.0).color(load_color(s)));
                        ui.label(RichText::new(format!("({n} überlappende APs, Störpegel {:.0} dB)", s)).weak());
                    });
                }
            };
            match band {
                Band::B24 => {
                    rec_line(ui, "Nicht überlappend (1/6/11):", best_of(&[1, 6, 11]));
                    rec_line(ui, "Europa (1/5/9/13):", best_of(&[1, 5, 9, 13]));
                }
                Band::B5 => {
                    rec_line(ui, "20 MHz ohne DFS:", best_of(&[36, 40, 44, 48, 149, 153, 157, 161, 165]));
                    rec_line(ui, "20 MHz inkl. DFS:", best_of(&chans));
                    let blocks = [(36u32, 42u32), (52, 58), (100, 106), (116, 122), (132, 138), (149, 155)];
                    let best80 = blocks
                        .iter()
                        .map(|(s, c)| {
                            let mw: f64 = (0..4).map(|k| self.channel_load(band, s + 4 * k, now).0).sum();
                            (*s, *c, mw)
                        })
                        .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap());
                    if let Some((s, c, mw)) = best80 {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("80 MHz Block:").weak());
                            ui.label(
                                RichText::new(format!("Kanal {}–{} (Mitte {c}){}", s, s + 12, if is_dfs(s) { " DFS" } else { "" }))
                                    .strong()
                                    .size(16.0)
                                    .color(load_color(Self::mw_to_score(mw))),
                            );
                        });
                    }
                }
                _ => {
                    let psc: Vec<u32> = chans.iter().copied().filter(|c| is_psc(*c)).collect();
                    rec_line(ui, "PSC-Kanal:", best_of(&psc));
                }
            }
        });

        let h = ui.available_height() - 10.0;
        let bars: Vec<Bar> = loads
            .iter()
            .map(|(c, _, s, n)| {
                Bar::new(*c as f64, s.max(0.5))
                    .width(if band == Band::B24 { 0.7 } else { 2.8 })
                    .fill(load_color(*s))
                    .name(format!("Kanal {c}{} · {n} APs", if band == Band::B5 && is_dfs(*c) { " (DFS)" } else { "" }))
            })
            .collect();
        let steps = if band == Band::B24 { [1.0, 1.0, 5.0] } else { [4.0, 4.0, 16.0] };
        Plot::new(("rating", band as u8))
            .height(h.max(200.0))
            .include_y(0.0)
            .include_y(70.0)
            .allow_zoom(false)
            .allow_drag(false)
            .allow_scroll(false)
            .x_grid_spacer(uniform_grid_spacer(move |_| steps))
            .x_axis_formatter(|m, _| if m.value.fract() == 0.0 && m.value > 0.0 { format!("{}", m.value) } else { String::new() })
            .x_axis_label("Kanal")
            .y_axis_label("Störpegel (dB über -100 dBm)")
            .show(ui, |pui| {
                pui.bar_chart(BarChart::new(bars).element_formatter(Box::new(|b, _| format!("{}\n{:.0} dB", b.name, b.value))));
            });
    }

    fn analysis(&self) -> Vec<(Sev, String, Option<[u8; 6]>)> {
        let now = Instant::now();
        let mut out = Vec::new();
        let vis: Vec<&ApRecord> = self.aps.values().filter(|r| self.visible(r, now) && r.age(now) <= self.stale_after).collect();
        let mut no_pmf: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
        for r in &vis {
            let i = &r.info;
            let n = format!("{} ({}, Kanal {})", Self::display_ssid(r), fmt_mac(&i.bssid), i.channel);
            let b = Some(i.bssid);
            match i.security.as_str() {
                "WEP" => out.push((Sev::Critical, format!("{n}: WEP-Verschlüsselung ist geknackt – sofort umstellen"), b)),
                s if s.starts_with("Offen") => out.push((Sev::Critical, format!("{n}: Offenes Netz ohne Verschlüsselung"), b)),
                s if s.starts_with("WPA-") || s.starts_with("WPA/") => {
                    out.push((Sev::Warning, format!("{n}: Veraltetes WPA(1) aktiv"), b))
                }
                _ => {}
            }
            if i.pairwise.iter().any(|c| c == "TKIP") {
                out.push((Sev::Warning, format!("{n}: TKIP-Cipher aktiviert (veraltet, bremst auf 54 Mbit/s)"), b));
            }
            if let Some(w) = &i.wps {
                if !w.locked {
                    out.push((Sev::Warning, format!("{n}: WPS ist aktiv (PIN-Angriffe möglich)"), b));
                }
            }
            if i.band == Band::B24 && i.width >= 40 {
                out.push((Sev::Warning, format!("{n}: 40 MHz im 2,4-GHz-Band – stört Nachbarkanäle"), b));
            }
            if i.band == Band::B24 && ![1, 5, 6, 9, 11, 13].contains(&i.channel) {
                out.push((Sev::Warning, format!("{n}: Überlappender 2,4-GHz-Kanal {} (besser 1/6/11)", i.channel), b));
            }
            if let Some(u) = i.ch_util {
                if u > 60.0 {
                    out.push((Sev::Warning, format!("{n}: Hohe Kanalauslastung {u:.0} %"), b));
                }
            }
            if let Some(s) = i.stations {
                if s > 40 {
                    out.push((Sev::Info, format!("{n}: Viele Clients ({s})"), b));
                }
            }
            if i.sec_level >= SecLevel::Ok && i.pmf == "Nein" && i.security.starts_with("WPA2") {
                // pro SSID sammeln statt für jede BSSID einen eigenen Hinweis
                *no_pmf.entry(Self::display_ssid(r)).or_default() += 1;
            }
            if i.beacon_ms > 150.0 || i.beacon_ms < 90.0 {
                out.push((Sev::Info, format!("{n}: Ungewöhnliches Beacon-Intervall {:.0} ms", i.beacon_ms), b));
            }
            if i.rssi > -30 {
                out.push((Sev::Info, format!("{n}: Sehr starkes Signal ({} dBm) – AP sehr nah", i.rssi), b));
            }
        }
        for (ssid, n) in no_pmf {
            let what = if n == 1 { "1 BSSID".to_string() } else { format!("{n} BSSIDs") };
            out.push((Sev::Info, format!("SSID \"{ssid}\": PMF (802.11w) nicht aktiv ({what})"), None));
        }
        // Co-Channel
        let mut per_ch: HashMap<(Band, u32), usize> = HashMap::new();
        for r in vis.iter().filter(|r| r.info.rssi >= -75) {
            *per_ch.entry((r.info.band, r.info.channel)).or_default() += 1;
        }
        let mut cc: Vec<_> = per_ch.into_iter().filter(|(_, n)| *n >= 4).collect();
        cc.sort_by(|a, b| b.1.cmp(&a.1));
        for ((band, ch), n) in cc {
            out.push((
                Sev::Warning,
                format!("{} Kanal {ch}: {n} BSSIDs ≥ -75 dBm – Co-Channel-Interferenz", band.label()),
                None,
            ));
        }
        // Gleiche SSID mit unterschiedlicher Sicherheit
        let mut ssid_sec: HashMap<&str, HashSet<&str>> = HashMap::new();
        for r in vis.iter().filter(|r| !r.info.hidden) {
            ssid_sec.entry(r.info.ssid.as_str()).or_default().insert(r.info.security.as_str());
        }
        for (s, secs) in ssid_sec {
            if secs.len() > 1 {
                let mut l: Vec<_> = secs.into_iter().collect();
                l.sort();
                out.push((Sev::Warning, format!("SSID \"{s}\" mit unterschiedlicher Sicherheit: {} (Evil-Twin? Fehlkonfiguration?)", l.join(" / ")), None));
            }
        }
        // Schwere, dann Text: die Listen kommen aus HashMaps und hätten sonst jedes Frame eine andere Reihenfolge
        out.sort_by(|a, b| (a.0 as u8).cmp(&(b.0 as u8)).then_with(|| a.1.cmp(&b.1)));
        out.dedup_by(|a, b| a.1 == b.1);
        out
    }

    fn analysis_tab(&mut self, ui: &mut Ui) {
        let items = self.analysis();
        let (c, w, i) = items.iter().fold((0, 0, 0), |acc, (s, _, _)| match s {
            Sev::Critical => (acc.0 + 1, acc.1, acc.2),
            Sev::Warning => (acc.0, acc.1 + 1, acc.2),
            Sev::Info => (acc.0, acc.1, acc.2 + 1),
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("⛔ {c} kritisch")).strong().color(Color32::from_rgb(231, 76, 60)));
            ui.label(RichText::new(format!("⚠ {w} Warnungen")).strong().color(Color32::from_rgb(230, 160, 40)));
            ui.label(RichText::new(format!("ℹ {i} Hinweise")).strong().color(ACCENT));
        });
        ui.separator();
        let mut pick = None;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            if items.is_empty() {
                ui.label(RichText::new("✔ Keine Auffälligkeiten gefunden").size(16.0).color(Color32::from_rgb(46, 204, 113)));
            }
            for (s, msg, b) in &items {
                let (icon, col) = match s {
                    Sev::Critical => ("⛔", Color32::from_rgb(231, 76, 60)),
                    Sev::Warning => ("⚠", Color32::from_rgb(230, 160, 40)),
                    Sev::Info => ("ℹ", ACCENT),
                };
                ui.horizontal(|ui| {
                    ui.label(RichText::new(icon).color(col));
                    let resp = ui.add(egui::Label::new(msg.as_str()).sense(Sense::click()));
                    if b.is_some() && resp.on_hover_text("Klicken zum Auswählen").clicked() {
                        pick = *b;
                    }
                });
            }
        });
        if pick.is_some() {
            self.selected = pick;
        }
    }

    /// SSIDs zur Auswahl in der Messung: (Name, bester RSSI), stärkste zuerst
    fn ssid_choices(&self) -> Vec<(String, i32)> {
        let now = Instant::now();
        let mut best: HashMap<&str, i32> = HashMap::new();
        for r in self.aps.values().filter(|r| !r.info.hidden && r.age(now) <= self.stale_after) {
            let e = best.entry(r.info.ssid.as_str()).or_insert(-200);
            *e = (*e).max(r.info.rssi);
        }
        let mut v: Vec<(String, i32)> = best.into_iter().map(|(s, r)| (s.to_string(), r)).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        v
    }

    /// Stärkster aktuell sichtbarer AP einer SSID
    fn best_ap_of(&self, ssid: &str) -> Option<&ApRecord> {
        let now = Instant::now();
        self.aps
            .values()
            .filter(|r| !r.info.hidden && r.info.ssid == ssid && r.age(now) <= self.stale_after)
            .max_by_key(|r| r.info.rssi)
    }

    fn export_measurement(&mut self) {
        let Some(m) = &self.meas else { return };
        let safe: String = m.ssid.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
        let name = format!("messung_{}_{}.csv", safe, m.started_wall.format("%Y-%m-%d_%H-%M"));
        let Some(path) = rfd::FileDialog::new().set_file_name(&name).add_filter("CSV (Excel)", &["csv"]).save_file() else {
            return;
        };
        match std::fs::write(&path, m.to_csv()) {
            Ok(_) => self.add_log(Sev::Info, format!("Messung exportiert: {}", path.display())),
            Err(e) => self.add_log(Sev::Critical, format!("Export der Messung fehlgeschlagen: {e}")),
        }
    }

    fn measure_tab(&mut self, ui: &mut Ui) {
        let t_now = self.now_t();
        let running = self.meas.as_ref().is_some_and(|m| m.running);
        let choices = self.ssid_choices();
        if self.meas_ssid.is_none() {
            // Vorauswahl: verbundene SSID, sonst die stärkste
            self.meas_ssid = self.conn.as_ref().map(|c| c.ssid.clone()).or_else(|| choices.first().map(|c| c.0.clone()));
        }
        let ssid = self.meas.as_ref().map(|m| m.ssid.clone()).or_else(|| self.meas_ssid.clone()).unwrap_or_default();
        let connected_ssid = self.conn.as_ref().map(|c| c.ssid.clone());

        // ---------- Steuerleiste ----------
        let mut start = false;
        let mut stop = false;
        let mut discard = false;
        let mut export = false;
        let mut add_marker = false;
        ui.horizontal(|ui| {
            ui.label("SSID");
            ui.add_enabled_ui(!running, |ui| {
                egui::ComboBox::from_id_salt("meas_ssid").width(220.0).selected_text(if ssid.is_empty() { "–" } else { ssid.as_str() }).show_ui(
                    ui,
                    |ui| {
                        for (s, r) in &choices {
                            let tag = if connected_ssid.as_deref() == Some(s.as_str()) { "  🔗" } else { "" };
                            if ui.selectable_label(*s == ssid, format!("{s}  ({r} dBm){tag}")).clicked() {
                                self.meas_ssid = Some(s.clone());
                            }
                        }
                    },
                );
            });
            if running {
                let b = egui::Button::new(RichText::new("⏹ Messung stoppen").color(Color32::WHITE).strong()).fill(Color32::from_rgb(200, 60, 50));
                if ui.add(b).clicked() {
                    stop = true;
                }
            } else {
                let b = egui::Button::new(RichText::new("▶ Messung starten").color(Color32::WHITE).strong()).fill(Color32::from_rgb(40, 150, 80));
                if ui.add_enabled(!ssid.is_empty(), b).clicked() {
                    start = true;
                }
                if self.meas.is_some() && ui.button("🗑 Verwerfen").clicked() {
                    discard = true;
                }
            }
            if self.meas.is_some() && ui.button("💾 CSV").on_hover_text("Messwerte und Ereignisse exportieren").clicked() {
                export = true;
            }
            ui.separator();
            let te = ui.add_enabled(
                running,
                egui::TextEdit::singleline(&mut self.meas_marker).hint_text("Ort / Notiz, z. B. Raum 1.03").desired_width(200.0),
            );
            let enter = te.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui.add_enabled(running, egui::Button::new("📍 Markierung")).on_hover_text("Markiert den aktuellen Zeitpunkt im Verlauf").clicked() || (running && enter) {
                add_marker = true;
            }
            ui.separator();
            ui.checkbox(&mut self.meas_follow, "Nur letzte 5 min");
        });
        if start {
            self.meas = Some(Measurement::new(ssid.clone(), t_now));
            self.add_log(Sev::Info, format!("Messung gestartet: {ssid}"));
        }
        if stop {
            if let Some(m) = &mut self.meas {
                m.stop(t_now);
            }
            self.add_log(Sev::Info, format!("Messung beendet: {ssid}"));
        }
        if discard {
            self.meas = None;
        }
        if export {
            self.export_measurement();
        }
        if add_marker {
            if let Some(m) = &mut self.meas {
                m.add_marker(t_now, &self.meas_marker);
            }
            self.meas_marker.clear();
        }
        ui.add_space(6.0);

        // ---------- Live-Anzeige + Kennzahlen ----------
        let conn_here = self.conn.as_ref().filter(|c| c.ssid == ssid);
        let best = self.best_ap_of(&ssid);
        let live_rssi = conn_here.and_then(|c| c.rssi).or(best.map(|b| b.info.rssi));
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_width(300.0);
                signal_gauge(ui, live_rssi, false, 0.8);
                match conn_here {
                    Some(c) => {
                        let ap = self.aps.get(&c.bssid);
                        ui.label(RichText::new(format!("Verbunden mit {}", fmt_mac(&c.bssid))).strong().color(ap_color(&c.bssid)));
                        ui.label(
                            RichText::new(format!(
                                "{} · Kanal {} · {:.0} Mbit/s",
                                ap.map(|a| a.vendor.as_str()).unwrap_or("?"),
                                c.channel.map(|v| v.to_string()).unwrap_or("?".into()),
                                c.tx_mbps
                            ))
                            .weak(),
                        );
                        if let (Some(b), Some(cr)) = (best, c.rssi) {
                            if b.info.bssid != c.bssid {
                                let d = b.info.rssi - cr;
                                let col = if d >= 10 { Color32::from_rgb(231, 76, 60) } else if d >= 5 { Color32::from_rgb(230, 160, 40) } else { ui.visuals().weak_text_color() };
                                ui.label(RichText::new(format!("Bester AP: {} {} dBm ({:+} dB)", fmt_mac(&b.info.bssid), b.info.rssi, d)).color(col));
                            } else {
                                ui.label(RichText::new("✔ Verbunden mit dem stärksten AP").color(Color32::from_rgb(46, 204, 113)));
                            }
                        }
                    }
                    None => {
                        ui.label(RichText::new("Nicht mit dieser SSID verbunden").strong().color(Color32::from_rgb(230, 160, 40)));
                        ui.label(RichText::new("Anzeige: stärkster AP aus dem Scan. Für Roaming-Messung mit der SSID verbinden.").weak());
                    }
                }
                if self.meas.as_ref().is_some_and(|m| m.running && m.is_sticky()) {
                    ui.label(RichText::new("⚠ Sticky Client: besserer AP verfügbar").strong().color(Color32::from_rgb(231, 76, 60)));
                }
            });
            ui.add_space(12.0);
            match &self.meas {
                Some(m) => {
                    let s = m.stats(t_now);
                    let pct = |v: f64| format!("{:.0} %", v * 100.0);
                    let weak = ui.visuals().text_color();
                    ui.vertical(|ui| {
                        ui.horizontal_wrapped(|ui| {
                            let state = if m.running { "läuft" } else { "beendet" };
                            stat_card(ui, &format!("Dauer ({state})"), &fmt_mmss(s.duration), if m.running { Color32::from_rgb(46, 204, 113) } else { weak });
                            let src = if s.from_scan { " · Scan" } else { "" };
                            stat_card(ui, &format!("Ø Signal{src}"),&s.avg.map(|v| format!("{v:.1} dBm")).unwrap_or("–".into()), s.avg.map(|v| signal_color(v as i32)).unwrap_or(weak));
                            stat_card(
                                ui,
                                "Min / Max",
                                &match (s.min, s.max) {
                                    (Some(a), Some(b)) => format!("{a} / {b}"),
                                    _ => "–".into(),
                                },
                                s.min.map(signal_color).unwrap_or(weak),
                            );
                            stat_card(ui, "≥ -67 dBm (gut)", &pct(s.good), Color32::from_rgb(46, 204, 113));
                            stat_card(ui, "-75…-67 dBm", &pct(s.fair), Color32::from_rgb(241, 196, 15));
                            stat_card(ui, "< -75 dBm (schwach)", &pct(s.bad), Color32::from_rgb(231, 76, 60));
                        });
                        ui.horizontal_wrapped(|ui| {
                            stat_card(ui, "Roams", &s.roams.to_string(), ACCENT);
                            stat_card(ui, "Abbrüche", &s.disconnects.to_string(), if s.disconnects > 0 { Color32::from_rgb(231, 76, 60) } else { weak });
                            stat_card(ui, "Sticky-Phasen", &s.sticky.to_string(), if s.sticky > 0 { Color32::from_rgb(230, 126, 34) } else { weak });
                            stat_card(ui, "APs genutzt", &s.aps_used.to_string(), weak);
                            stat_card(ui, "Messwerte", &s.samples.to_string(), weak);
                            stat_card(ui, "Markierungen", &m.marker_count().to_string(), weak);
                        });
                    });
                }
                None => {
                    ui.vertical(|ui| {
                        ui.add_space(20.0);
                        ui.label(RichText::new("So funktioniert die Messung").strong().size(16.0));
                        ui.label("1.  SSID wählen (am besten die, mit der der Laptop verbunden ist)");
                        ui.label("2.  „Messung starten“ und mit dem Laptop die Fläche ablaufen");
                        ui.label("3.  An wichtigen Stellen eine Markierung setzen (Raum, Stockwerk …)");
                        ui.label("4.  Stoppen – Verlauf, Roams und Statistik bleiben stehen, Export als CSV");
                        ui.add_space(6.0);
                        ui.label(RichText::new("Tipp: Scan-Intervall auf 2–4 s, die Verbindung wird ohnehin jede Sekunde gemessen.").weak());
                    });
                }
            }
        });

        let Some(m) = &self.meas else { return };
        ui.add_space(6.0);

        // ---------- Verlauf ----------
        let end = m.elapsed(t_now).max(10.0);
        let x0 = if self.meas_follow { (end - 300.0).max(0.0) } else { 0.0 };
        let in_view = |t: f64| t >= x0 - 1.0;
        // Verbindungslinie in Abschnitte je AP zerlegen (Farbwechsel = Roam), Lücken > 5 s = Abbruch
        let mut segments: Vec<([u8; 6], u32, Vec<[f64; 2]>)> = Vec::new();
        let mut last_t = f64::NEG_INFINITY;
        for c in m.conn.iter().filter(|c| in_view(c.t)) {
            let new_seg = segments.last().map_or(true, |s| s.0 != c.bssid) || c.t - last_t > 5.0;
            if new_seg {
                // an den vorherigen Abschnitt anschließen, damit die Linie beim Roam nicht abreißt
                let mut pts = Vec::new();
                if let Some(prev) = segments.last() {
                    if c.t - last_t <= 5.0 {
                        if let Some(p) = prev.2.last() {
                            pts.push(*p);
                        }
                    }
                }
                segments.push((c.bssid, c.channel, pts));
            }
            segments.last_mut().unwrap().2.push([c.t, c.rssi as f64]);
            last_t = c.t;
        }
        let best_pts: Vec<[f64; 2]> = m.best.iter().filter(|b| in_view(b.t)).map(|b| [b.t, b.rssi as f64]).collect();
        let no_conn = m.conn.is_empty();
        let events: Vec<(f64, &'static str, String, Color32)> = m
            .events
            .iter()
            .filter(|e| in_view(e.t))
            .filter_map(|e| {
                let (label, col) = match &e.kind {
                    EventKind::Roam { to, .. } => (format!("Roam > {}", &fmt_mac(to)[9..]), Color32::from_rgb(120, 200, 255)),
                    EventKind::Disconnect { .. } => ("Abbruch".to_string(), Color32::from_rgb(231, 76, 60)),
                    EventKind::Connect { .. } => ("Verbunden".to_string(), Color32::from_rgb(46, 204, 113)),
                    EventKind::Sticky { .. } => ("Sticky".to_string(), Color32::from_rgb(230, 126, 34)),
                    EventKind::Marker(t) => (format!("📍 {t}"), Color32::from_rgb(200, 160, 255)),
                    EventKind::Start | EventKind::Stop => return None,
                };
                Some((e.t, e.title(), label, col))
            })
            .collect();
        let vendor_of = |b: &[u8; 6]| self.aps.get(b).map(|a| a.vendor.clone()).unwrap_or_default();
        let plot_h = (ui.available_height() * 0.62).max(180.0);
        Plot::new(("meas_plot", self.meas_follow))
            .height(plot_h)
            .legend(Legend::default().position(Corner::LeftBottom))
            .include_y(-100.0)
            .include_y(-20.0)
            .include_x(x0)
            .include_x(end)
            .allow_zoom(false)
            .allow_drag(false)
            .allow_scroll(false)
            .y_axis_formatter(dbm_axis)
            .x_axis_formatter(|m, _| if m.value >= 0.0 { fmt_mmss(m.value) } else { String::new() })
            .label_formatter(|name, v| format!("{name}\n{} · {:.0} dBm", fmt_mmss(v.x.max(0.0)), v.y))
            .show(ui, |pui| {
                pui.hline(egui_plot::HLine::new(-67.0).color(Color32::from_rgb(46, 204, 113).gamma_multiply(0.5)).style(egui_plot::LineStyle::dashed_dense()));
                pui.hline(egui_plot::HLine::new(-75.0).color(Color32::from_rgb(231, 76, 60).gamma_multiply(0.5)).style(egui_plot::LineStyle::dashed_dense()));
                if !best_pts.is_empty() {
                    // ohne Verbindung ist der Scan-Wert die Hauptlinie, sonst nur Vergleich
                    let line = Line::new(PlotPoints::from(best_pts)).name("Bester AP der SSID (Scan)");
                    pui.line(if no_conn {
                        line.color(ACCENT).width(2.6f32)
                    } else {
                        line.color(Color32::from_gray(150)).width(1.2f32).style(egui_plot::LineStyle::dashed_loose())
                    });
                }
                for (bssid, ch, pts) in segments {
                    let v = vendor_of(&bssid);
                    pui.line(
                        Line::new(PlotPoints::from(pts))
                            .color(ap_color(&bssid))
                            .width(2.6f32)
                            .name(format!("AP {} · Kanal {ch}{}", &fmt_mac(&bssid)[9..], if v.is_empty() { String::new() } else { format!(" · {v}") })),
                    );
                }
                for (t, _, label, col) in &events {
                    pui.vline(egui_plot::VLine::new(*t).color(col.gamma_multiply(0.8)).width(1.5f32));
                    pui.text(
                        Text::new(PlotPoint::new(*t, -21.0), RichText::new(label).small().color(*col)).anchor(Align2::LEFT_TOP),
                    );
                }
            });

        // ---------- Ereignisliste ----------
        ui.add_space(4.0);
        ui.label(RichText::new("Ereignisse").strong());
        let wall = |t: f64| (m.started_wall + chrono::Duration::milliseconds((t * 1000.0) as i64)).format("%H:%M:%S").to_string();
        egui::ScrollArea::vertical().id_salt("meas_events").auto_shrink(false).show(ui, |ui| {
            egui::Grid::new("meas_ev_grid").num_columns(4).striped(true).spacing([14.0, 4.0]).show(ui, |ui| {
                for e in m.events.iter().rev() {
                    let col = match e.kind {
                        EventKind::Roam { .. } => Color32::from_rgb(120, 200, 255),
                        EventKind::Disconnect { .. } => Color32::from_rgb(231, 76, 60),
                        EventKind::Sticky { .. } => Color32::from_rgb(230, 126, 34),
                        EventKind::Marker(_) => Color32::from_rgb(200, 160, 255),
                        EventKind::Connect { .. } => Color32::from_rgb(46, 204, 113),
                        _ => ui.visuals().weak_text_color(),
                    };
                    ui.label(RichText::new(fmt_mmss(e.t)).monospace());
                    ui.label(RichText::new(wall(e.t)).monospace().weak());
                    ui.label(RichText::new(e.title()).strong().color(col));
                    ui.label(e.details());
                    ui.end_row();
                }
            });
        });
    }

    fn finder_tab(&mut self, ui: &mut Ui) {
        let now = Instant::now();
        let mut keys = self.sorted_keys();
        keys.sort_by_key(|k| -self.aps[k].info.rssi);
        ui.horizontal(|ui| {
            ui.label("Ziel-AP:");
            let cur = self
                .selected
                .and_then(|k| self.aps.get(&k))
                .map(|r| format!("{} – {}", Self::display_ssid(r), fmt_mac(&r.info.bssid)))
                .unwrap_or_else(|| "bitte wählen…".into());
            let mut pick = None;
            egui::ComboBox::from_id_salt("finder_pick").width(380.0).selected_text(cur).show_ui(ui, |ui| {
                for k in &keys {
                    let r = &self.aps[k];
                    if ui
                        .selectable_label(
                            self.selected == Some(*k),
                            format!("{} – {} ({} dBm, Kanal {})", Self::display_ssid(r), fmt_mac(k), r.info.rssi, r.info.channel),
                        )
                        .clicked()
                    {
                        pick = Some(*k);
                    }
                }
            });
            if pick.is_some() {
                self.selected = pick;
            }
            ui.checkbox(&mut self.beep_on, "🔊 Piepton (Geigerzähler)");
            ui.label(RichText::new("Tipp: Intervall auf 2 s stellen").weak());
        });

        let Some(r) = self.selected.and_then(|k| self.aps.get(&k)) else {
            self.beeper.set(false, -100);
            ui.add_space(60.0);
            ui.vertical_centered(|ui| ui.label(RichText::new("Wähle einen Access Point, um ihn aufzuspüren.").size(18.0).weak()));
            return;
        };
        let rssi = r.info.rssi;
        let stale = r.age(now) > self.stale_after;
        self.beeper.set(self.beep_on && !stale, rssi);

        ui.add_space(10.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new(Self::display_ssid(r)).size(22.0).strong());
            ui.label(RichText::new(format!("{} · {} · Kanal {} · {}", fmt_mac(&r.info.bssid), r.vendor, r.info.channel, r.info.band.label())).weak());
            signal_gauge(ui, Some(rssi), stale, 1.0);
            ui.label(RichText::new(if stale { "Signal verloren".to_string() } else { signal_text(rssi).to_string() }).size(18.0));
            ui.label(RichText::new(format!("min {}  ·  Ø {:.1}  ·  max {}", r.rssi_min, r.rssi_avg(), r.rssi_max)).weak());
        });

        let t_now = self.now_t();
        let pts: Vec<[f64; 2]> = r.history.iter().filter(|(t, _)| t_now - t <= 180.0).map(|(t, v)| [t - t_now, *v]).collect();
        let col = ap_color(&r.info.bssid);
        Plot::new("finder_plot")
            .height(ui.available_height().max(120.0))
            .include_y(-100.0)
            .include_y(-20.0)
            .include_x(-180.0)
            .include_x(0.0)
            .allow_zoom(false)
            .allow_drag(false)
            .allow_scroll(false)
            .y_axis_formatter(dbm_axis)
            .x_axis_formatter(|m, _| if m.value <= 0.5 { format!("{:.0} s", m.value) } else { String::new() })
            .show(ui, |pui| {
                pui.line(Line::new(PlotPoints::from(pts)).color(col).width(2.5f32).fill(-100.0f32));
            });
    }

    fn log_tab(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{} Ereignisse", self.log.len())).weak());
            if ui.button("Leeren").clicked() {
                self.log.clear();
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            for (t, s, m) in &self.log {
                let col = match s {
                    Sev::Critical => Color32::from_rgb(231, 76, 60),
                    Sev::Warning => Color32::from_rgb(230, 160, 40),
                    Sev::Info => ui.visuals().text_color(),
                };
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t.format("%H:%M:%S").to_string()).monospace().weak());
                    ui.label(RichText::new(m).color(col));
                });
            }
        });
    }
}

fn bar_row(ui: &mut Ui, label: &str, n: usize, total: usize, col: Color32) {
    let frac = if total == 0 { 0.0 } else { n as f32 / total as f32 };
    ui.horizontal(|ui| {
        let w = ui.available_width();
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 18.0), Sense::hover());
        let p = ui.painter();
        p.rect_filled(rect, 3.0, ui.visuals().faint_bg_color);
        let mut fr = rect;
        fr.set_width(rect.width() * frac);
        p.rect_filled(fr, 3.0, col.gamma_multiply(0.55));
        p.text(
            rect.left_center() + Vec2::new(6.0, 0.0),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(12.5),
            ui.visuals().strong_text_color(),
        );
        p.text(
            rect.right_center() - Vec2::new(6.0, 0.0),
            Align2::RIGHT_CENTER,
            n.to_string(),
            FontId::proportional(12.5),
            ui.visuals().strong_text_color(),
        );
        resp.on_hover_text(format!("{label}: {n} ({:.0} %)", frac * 100.0));
    });
}

/// Sekunden als m:ss bzw. h:mm:ss
fn fmt_mmss(t: f64) -> String {
    let s = t.max(0.0).round() as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// Halbkreis-Anzeige für einen Signalpegel (Signal-Finder, Messung). `scale` 1.0 = 360×200 px.
fn signal_gauge(ui: &mut Ui, rssi: Option<i32>, stale: bool, scale: f32) {
    let size = Vec2::new(360.0, 200.0) * scale;
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    let c = egui::pos2(rect.center().x, rect.bottom() - 20.0 * scale);
    let rad = 160.0 * scale;
    let thick = 26.0 * scale;
    let seg = 60;
    let frac_now = rssi.map(signal_frac).unwrap_or(0.0);
    for k in 0..seg {
        let a0 = std::f32::consts::PI * (1.0 - k as f32 / seg as f32);
        let a1 = std::f32::consts::PI * (1.0 - (k + 1) as f32 / seg as f32);
        let frac = k as f32 / seg as f32;
        let rs = (-100.0 + frac * 70.0) as i32;
        let on = rssi.is_some() && frac <= frac_now;
        let col = if !on {
            ui.visuals().faint_bg_color
        } else if stale {
            Color32::GRAY
        } else {
            signal_color(rs)
        };
        let pt = |a: f32, r: f32| egui::pos2(c.x + r * a.cos(), c.y - r * a.sin());
        p.add(egui::Shape::convex_polygon(
            vec![pt(a0, rad), pt(a1, rad), pt(a1, rad - thick), pt(a0, rad - thick)],
            col,
            Stroke::NONE,
        ));
    }
    let (txt, col) = match rssi {
        Some(r) if !stale => (r.to_string(), signal_color(r)),
        Some(r) => (r.to_string(), Color32::GRAY),
        None => ("–".to_string(), Color32::GRAY),
    };
    p.text(c - Vec2::new(0.0, 55.0 * scale), Align2::CENTER_CENTER, txt, FontId::proportional(64.0 * scale), col);
    p.text(
        c - Vec2::new(0.0, 12.0 * scale),
        Align2::CENTER_CENTER,
        "dBm",
        FontId::proportional(18.0 * scale),
        ui.visuals().weak_text_color(),
    );
}

/// Kleine Kennzahl-Kachel für die Messung
fn stat_card(ui: &mut Ui, title: &str, value: &str, color: Color32) {
    egui::Frame::group(ui.style()).rounding(8.0).inner_margin(egui::Margin::symmetric(12.0, 8.0)).show(ui, |ui| {
        ui.set_min_width(118.0);
        ui.vertical(|ui| {
            ui.label(RichText::new(title).small().weak());
            ui.label(RichText::new(value).size(20.0).strong().color(color));
        });
    });
}

/// y-Achse für Signalpegel: nur volle 10er beschriften, mit Einheit
fn dbm_axis(m: egui_plot::GridMark, _range: &std::ops::RangeInclusive<f64>) -> String {
    if (m.value % 10.0).abs() < 1e-6 && m.value <= -10.0 && m.value >= -100.0 {
        format!("{} dBm", m.value)
    } else {
        String::new()
    }
}

fn signal_bar(ui: &mut Ui, rssi: i32, stale: bool) {
    // Wert links (gut lesbar in beiden Themes), schlanker Balken rechts daneben
    let w = ui.available_width().clamp(70.0, 170.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 18.0), Sense::hover());
    let p = ui.painter();
    let col = if stale { Color32::GRAY } else { signal_color(rssi) };
    p.text(
        rect.left_center(),
        Align2::LEFT_CENTER,
        format!("{rssi} dBm"),
        FontId::monospace(12.5),
        col,
    );
    let bar_left = rect.left() + 64.0;
    if rect.right() - bar_left > 12.0 {
        let track = egui::Rect::from_min_max(egui::pos2(bar_left, rect.center().y - 3.0), egui::pos2(rect.right() - 4.0, rect.center().y + 3.0));
        p.rect_filled(track, 3.0, ui.visuals().faint_bg_color.gamma_multiply(1.6));
        let mut fr = track;
        fr.set_width((track.width() * signal_frac(rssi)).max(3.0));
        p.rect_filled(fr, 3.0, col);
    }
}

impl WifiApp {
    /// Kleines Lade-Popup (randloses Fenster). Liefert true, solange es angezeigt wird.
    fn splash(&mut self, ctx: &egui::Context) -> bool {
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
            ("Berechtigungen prüfen", perm_ready),
            ("WLAN-Adapter initialisieren", !self.interfaces.is_empty() || t > 1.0),
            ("Hersteller-Datenbank laden", oui_ready),
            ("Erster Netzwerk-Scan", scanned),
        ];
        let done = steps.iter().filter(|s| s.1).count();

        // Fortschritt: fertige Schritte + langsames "Kriechen" im aktuellen Schritt (erreicht nie den nächsten),
        // weich angenähert und nie rückwärts. Bei "fertig" auf 100 % laufen lassen und erst dann schließen.
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
            // Popup -> normales, maximiertes Hauptfenster
            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(Vec2::new(1000.0, 620.0)));
            // Wiederherstellen-Größe mittig setzen, sonst landet das Fenster beim Verkleinern
            // an der alten Popup-Position (teilweise außerhalb des Bildschirms)
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

        egui::CentralPanel::default().frame(egui::Frame::none()).show(ctx, |ui| {
            let rect = ui.max_rect();
            // Popup mit der Maus verschiebbar
            let resp = ui.interact(rect, egui::Id::new("splash_drag"), Sense::drag());
            if resp.drag_started() {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            let p = ui.painter_at(rect);
            let fade = (t / 0.5).clamp(0.0, 1.0);
            let card = rect.shrink(1.0);

            // Karte mit abgerundeten Ecken, dunkler Hintergrund + Akzent-Rand
            p.rect_filled(card, 18.0, Color32::from_rgb(12, 19, 34));
            // Effekte nur innerhalb der Karte (nicht in die runden Ecken)
            let pc = p.with_clip_rect(card.shrink(12.0));
            pc.circle_filled(egui::pos2(card.center().x, card.top() + 60.0), 230.0, Color32::from_rgb(30, 80, 170).gamma_multiply(0.10));
            p.rect_stroke(card, 18.0, Stroke::new(1.5f32, ACCENT.gamma_multiply(0.45)));

            let ic = egui::pos2(card.center().x, card.top() + 105.0);
            // Radar-Wellen um das Icon
            for k in 0..3 {
                let ph = (t * 0.6 + k as f32 / 3.0) % 1.0;
                pc.circle_stroke(ic, 50.0 + ph * 190.0, Stroke::new(1.5f32, ACCENT.gamma_multiply((1.0 - ph) * 0.45 * fade)));
            }
            // App-Icon, leicht "atmend", mit Leuchten
            let breathe = 1.0 + (t * 2.0).sin() * 0.03;
            for k in 0..5 {
                p.circle_filled(ic, (52.0 + k as f32 * 7.0) * breathe, Color32::from_rgb(40, 140, 255).gamma_multiply(0.05 * fade));
            }
            if let Some(tex) = &self.icon_tex {
                let r = egui::Rect::from_center_size(ic, Vec2::splat(96.0 * breathe));
                p.image(tex.id(), r, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE.gamma_multiply(fade));
            }

            p.text(egui::pos2(ic.x, card.top() + 192.0), Align2::CENTER_CENTER, "WiFi Analyzer", FontId::proportional(32.0), Color32::WHITE.gamma_multiply(fade));
            p.text(
                egui::pos2(ic.x, card.top() + 222.0),
                Align2::CENTER_CENTER,
                "WLAN-Analyse für Techniker",
                FontId::proportional(14.5),
                Color32::from_rgb(140, 170, 210).gamma_multiply(fade),
            );

            // Fortschrittsbalken mit wanderndem Lichtreflex
            let bar_w = 360.0;
            let bar = egui::Rect::from_center_size(egui::pos2(ic.x, card.top() + 258.0), Vec2::new(bar_w, 5.0));
            p.rect_filled(bar, 3.0, Color32::from_rgb(32, 44, 66));
            let mut fill = bar;
            fill.set_width((bar_w * progress).max(6.0));
            p.rect_filled(fill, 3.0, ACCENT);
            // Lichtreflex, der nur über den gefüllten Teil wandert
            let gx = fill.left() + ((t * 0.7) % 1.0) * fill.width();
            let glint = egui::Rect::from_center_size(egui::pos2(gx, bar.center().y), Vec2::new(40.0, bar.height()));
            p.with_clip_rect(fill).rect_filled(glint, 3.0, Color32::from_white_alpha(60));
            p.text(
                egui::pos2(bar.right(), bar.top() - 12.0),
                Align2::RIGHT_CENTER,
                format!("{:.0} %", progress * 100.0),
                FontId::proportional(12.0),
                Color32::from_rgb(120, 150, 190).gamma_multiply(fade),
            );

            // Schritte
            for (i, (label, ok)) in steps.iter().enumerate() {
                let y = bar.bottom() + 24.0 + i as f32 * 23.0;
                let sx = egui::pos2(bar.left() + 40.0, y);
                if *ok {
                    p.text(sx, Align2::CENTER_CENTER, "✔", FontId::proportional(14.0), Color32::from_rgb(46, 204, 113).gamma_multiply(fade));
                } else {
                    let a0 = t * 6.0;
                    let pts: Vec<egui::Pos2> = (0..=16)
                        .map(|s| {
                            let a = a0 + s as f32 / 16.0 * 4.5;
                            egui::pos2(sx.x + 6.0 * a.cos(), sx.y + 6.0 * a.sin())
                        })
                        .collect();
                    p.add(egui::Shape::line(pts, Stroke::new(2.0f32, ACCENT.gamma_multiply(fade))));
                }
                p.text(
                    egui::pos2(bar.left() + 60.0, y),
                    Align2::LEFT_CENTER,
                    *label,
                    FontId::proportional(14.0),
                    (if *ok { Color32::from_rgb(200, 215, 235) } else { Color32::from_rgb(120, 140, 170) }).gamma_multiply(fade),
                );
            }

            let footer_y = card.bottom() - 20.0;
            if !perm_ready && t > 2.0 {
                p.text(
                    egui::pos2(ic.x, footer_y),
                    Align2::CENTER_CENTER,
                    "Bitte die Windows-Abfrage zum Standortzugriff bestätigen",
                    FontId::proportional(13.0),
                    Color32::from_rgb(255, 196, 90).gamma_multiply(0.6 + 0.4 * (t * 3.0).sin().abs()),
                );
            } else {
                p.text(
                    egui::pos2(ic.x, footer_y),
                    Align2::CENTER_CENTER,
                    format!("Version {}", env!("CARGO_PKG_VERSION")),
                    FontId::proportional(12.0),
                    Color32::from_rgb(90, 110, 140),
                );
            }
        });
        ctx.request_repaint();
        true
    }
}

impl eframe::App for WifiApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // transparent, damit das Lade-Popup runde Ecken hat (die Panels füllen danach alles)
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        self.poll_perm();
        self.query = self.f.text.trim().to_lowercase();
        if self.splash(ctx) {
            return;
        }
        self.top_bar(ctx);
        self.status_bar(ctx);
        self.left_panel(ctx);
        self.details_panel(ctx);
        if self.tab != Tab::Finder {
            self.beeper.set(false, -100);
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                let tabs = [
                    (Tab::Networks, "📋 Netzwerke"),
                    (Tab::Channels, "📊 Kanäle"),
                    (Tab::History, "📈 Signalverlauf"),
                    (Tab::Rating, "🏆 Kanalbewertung"),
                    (Tab::Analysis, "⚠ Analyse"),
                    (Tab::Finder, "🎯 Signal-Finder"),
                    (Tab::Measure, "📏 Messung"),
                    (Tab::Log, "📜 Ereignisse"),
                ];
                for (t, l) in tabs {
                    ui.selectable_value(&mut self.tab, t, RichText::new(l).size(15.0));
                }
            });
            ui.separator();
            if self.aps.is_empty() && self.tab == Tab::Networks {
                ui.add_space(80.0);
                ui.vertical_centered(|ui| {
                    if self.error.is_some() {
                        ui.label(RichText::new("⚠ Scan nicht möglich – siehe Statuszeile unten").size(18.0));
                    } else {
                        ui.label(RichText::new("📡").size(40.0).color(ACCENT));
                        ui.label(RichText::new("Suche WLAN-Netze…").size(18.0).weak());
                    }
                });
                return;
            }
            match self.tab {
                Tab::Networks => self.networks_tab(ui),
                Tab::Channels => self.channels_tab(ui),
                Tab::History => self.history_tab(ui),
                Tab::Rating => self.rating_tab(ui),
                Tab::Analysis => self.analysis_tab(ui),
                Tab::Finder => self.finder_tab(ui),
                Tab::Measure => self.measure_tab(ui),
                Tab::Log => self.log_tab(ui),
            }
        });

        self.perm_dialog(ctx);
        ctx.request_repaint_after(Duration::from_millis(1000));
    }
}
