//! Hauptfenster: Zustand, Datenverarbeitung und Seitenrahmen.

mod dialogs;
mod inspector;
mod pages;
mod plan;
mod report_pdf;
mod settings_ui;
mod shell;
mod survey;
mod update;

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use chrono::{DateTime, Local};
use eframe::egui::{self, Color32};

use crate::beeper::Beeper;
use crate::i18n;
use crate::ie::{BssInfo, SecLevel};
use crate::measure::Measurement;
use crate::model::*;
use crate::oui::{self, OuiDb};
use crate::perm::{self, PermStatus};
use crate::settings::Settings;
use crate::wlan::{self, ConnInfo, IfaceInfo, ScanCmd, ScanMsg};
use crate::{t, tf};

pub const AUTHOR: &str = "Thomas Schöpf";
pub const WEBSITE: &str = "https://wifianalyzer.schoepf-tirol.at";
pub const REPO: &str = "https://github.com/zenutrix/wifianalyzer";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Scan-Intervall (s) während einer laufenden Messung
pub const MEASURE_SCAN_INTERVAL: f32 = 2.0;

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub(crate) enum Page {
    Overview,
    Networks,
    Channels,
    Rating,
    History,
    Findings,
    Measure,
    PlanSurvey,
    Finder,
    Events,
}

#[derive(PartialEq, Clone, Copy)]
pub(crate) enum SortCol {
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
}

#[derive(PartialEq, Eq, Clone, Copy, PartialOrd, Ord, Debug)]
pub(crate) enum Sev {
    Critical,
    Warning,
    Info,
}

impl Sev {
    pub fn label(self) -> &'static str {
        match self {
            Sev::Critical => t!("Kritisch", "Critical"),
            Sev::Warning => t!("Warnung", "Warning"),
            Sev::Info => t!("Hinweis", "Info"),
        }
    }
}

pub(crate) struct Filter {
    pub text: String,
    pub b24: bool,
    pub b5: bool,
    pub b6: bool,
    pub min_rssi: i32,
    pub show_hidden: bool,
    pub only_pinned: bool,
    pub hide_stale: bool,
    pub only_open: bool,
}

impl Filter {
    fn with_hidden(show_hidden: bool) -> Self {
        Self {
            text: String::new(),
            b24: true,
            b5: true,
            b6: true,
            min_rssi: -100,
            show_hidden,
            only_pinned: false,
            hide_stale: false,
            only_open: false,
        }
    }

    pub fn active_count(&self) -> usize {
        [
            !self.text.trim().is_empty(),
            !(self.b24 && self.b5 && self.b6),
            self.min_rssi > -100,
            !self.show_hidden,
            self.only_pinned,
            self.hide_stale,
            self.only_open,
        ]
        .iter()
        .filter(|b| **b)
        .count()
    }
}

pub(crate) struct LogEntry {
    pub time: DateTime<Local>,
    pub sev: Sev,
    pub text: String,
}

pub(crate) struct Toast {
    pub text: String,
    pub sev: Sev,
    pub until: Instant,
}

/// Informationen zum neuesten Release auf GitHub
#[derive(Clone, Default)]
pub(crate) struct ReleaseInfo {
    /// Version ohne "v"
    pub version: String,
    pub notes: String,
    pub url: String,
    pub date: String,
    pub exe_url: Option<String>,
    /// Installer (WiFiAnalyzer-Setup.exe) – wird verwendet, wenn die App installiert ist
    pub setup_url: Option<(String, String)>,
    pub sums_url: Option<String>,
}

#[derive(Default)]
pub(crate) struct UpdateState {
    pub rx: Option<Receiver<Result<Option<ReleaseInfo>, String>>>,
    /// neuestes veröffentlichtes Release, falls bekannt
    pub latest: Option<ReleaseInfo>,
    pub error: Option<String>,
    pub checked: bool,
    pub last_check: Option<Instant>,
    /// Ergebnis soll als Meldung erscheinen (manuelle Prüfung)
    pub manual: bool,
    /// Download der neuen Version läuft (Ergebnis: Pfad der geprüften EXE)
    pub install_rx: Option<Receiver<Result<update::Download, String>>>,
    pub install_error: Option<String>,
}

impl UpdateState {
    /// Version eines verfügbaren Updates (neuer als die laufende)
    pub fn available(&self) -> Option<&ReleaseInfo> {
        self.latest.as_ref().filter(|r| update::is_newer(&r.version, VERSION))
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

    settings: Settings,
    settings_saved: Settings,
    settings_dirty_since: Option<Instant>,

    page: Page,
    chan_band: Band,
    f: Filter,
    /// Suchtext in Kleinbuchstaben (einmal pro Frame berechnet)
    query: String,
    focus_search: bool,
    sort_col: SortCol,
    sort_asc: bool,
    selected: Option<[u8; 6]>,
    pinned: HashSet<[u8; 6]>,
    expanded: HashSet<String>,

    oui: OuiDb,
    oui_rx: Option<Receiver<Result<usize, String>>>,
    start: Instant,
    log: VecDeque<LogEntry>,
    toasts: Vec<Toast>,

    hist_all: bool,
    hist_window: f64,
    findings_filter: [bool; 3],

    meas: Option<Measurement>,
    meas_ssid: Option<String>,
    /// offener Dialog „Messpunkt erfassen“
    meas_point: Option<survey::PointDraft>,
    /// Datei der aktuellen Messung (automatisch gespeichert)
    meas_file: Option<std::path::PathBuf>,
    meas_dirty_since: Option<Instant>,
    meas_last_save: Option<Instant>,
    meas_save_error: Option<String>,
    meas_discard_confirm: bool,
    survey_recent: Option<(Instant, Vec<survey::RecentSurvey>)>,
    /// laufende Netzwerktests, deren Ergebnis noch einem Messpunkt zugeordnet wird
    net_jobs: Vec<plan::NetJob>,
    plan_tex: Option<plan::PlanTex>,
    heat_tex: Option<(u64, egui::TextureHandle)>,
    /// Messpunkt, der mit dem nächsten Klick in den Grundriss gesetzt wird
    plan_sel: Option<u64>,
    plan_drag: Option<u64>,
    plan_heat: bool,
    /// Heatmap-Ebene: Messwert oder 2,4/5/6 GHz
    plan_layer: crate::measure::Layer,
    /// Maßstab wird gerade gesetzt
    plan_scale: Option<plan::ScaleDraft>,
    show_scale_editor: bool,
    /// Seitenauswahl beim Laden eines mehrseitigen PDF-Grundrisses
    pdf_pick: Option<plan::PdfPick>,
    /// zuletzt verwendeter Grundriss (für die nächste Grundriss-Messung)
    last_plan: Option<crate::measure::FloorPlan>,
    meas_follow: bool,

    splash_done: bool,
    splash_progress: f32,
    splash_done_steps: usize,
    splash_step_t: f32,
    icon_tex: Option<egui::TextureHandle>,

    perm: Option<PermStatus>,
    perm_rx: Option<Receiver<PermStatus>>,
    location_held: bool,
    perm_dismissed: bool,
    perm_last_check: Instant,

    show_settings: bool,
    show_about: bool,
    show_update: bool,
    show_whats_new: bool,
    /// Dialog wurde automatisch nach einem Versionswechsel geöffnet
    whats_new_upgraded: bool,
    update: UpdateState,

    beeper: Beeper,
    beep_on: bool,
}

impl WifiApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let ctx = &cc.egui_ctx;
        let settings = Settings::load();
        i18n::set(settings.lang);
        crate::theme::install_fonts(ctx);
        crate::theme::apply(ctx, settings.dark);
        crate::heatmap::set_scale(crate::heatmap::scale_from_string(&settings.heat_scale));

        let scanner = if crate::demo::enabled() { crate::demo::spawn(ctx.clone()) } else { wlan::spawn(ctx.clone()) };
        let _ = scanner.tx.send(ScanCmd::SetInterval(settings.interval));
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
            f: Filter::with_hidden(settings.show_hidden),
            pinned: settings.pinned.iter().copied().collect(),
            settings_saved: settings.clone(),
            settings,
            settings_dirty_since: None,
            page: Page::Overview,
            chan_band: Band::B24,
            query: String::new(),
            focus_search: false,
            sort_col: SortCol::Signal,
            sort_asc: false,
            selected: None,
            expanded: HashSet::new(),
            oui: OuiDb::load(),
            oui_rx: None,
            start: Instant::now(),
            log: VecDeque::new(),
            toasts: Vec::new(),
            hist_all: false,
            hist_window: 300.0,
            findings_filter: [true, true, true],
            meas: None,
            meas_ssid: None,
            meas_point: None,
            meas_file: None,
            meas_dirty_since: None,
            meas_last_save: None,
            meas_save_error: None,
            meas_discard_confirm: false,
            survey_recent: None,
            net_jobs: Vec::new(),
            plan_tex: None,
            heat_tex: None,
            plan_sel: None,
            plan_drag: None,
            plan_heat: true,
            plan_layer: crate::measure::Layer::Measured,
            plan_scale: None,
            show_scale_editor: false,
            pdf_pick: None,
            last_plan: None,
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
            location_held: false,
            perm_dismissed: false,
            perm_last_check: Instant::now(),
            show_settings: false,
            show_about: false,
            show_update: false,
            show_whats_new: false,
            whats_new_upgraded: false,
            update: UpdateState::default(),
            beeper: Beeper::new(),
            beep_on: false,
        };
        // Vollständige Hersteller-DB beim ersten Start automatisch laden
        if !app.oui.full_db {
            app.start_oui_download(ctx);
        }
        // Windows-Berechtigungsabfrage (Standort/WLAN) auslösen
        app.start_perm_check(ctx, true);
        update::cleanup_old_exe();
        // Nach einem Update einmalig zeigen, was neu ist (nicht beim allerersten Start)
        if !app.settings.last_version.is_empty() && app.settings.last_version != VERSION {
            app.show_whats_new = true;
            app.whats_new_upgraded = true;
        }
        app.settings.last_version = VERSION.to_string();
        app
    }

    fn now_t(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }

    fn add_log(&mut self, sev: Sev, text: String) {
        self.log.push_front(LogEntry { time: Local::now(), sev, text });
        self.log.truncate(2000);
    }

    /// Kurze Meldung unten rechts (verschwindet nach einigen Sekunden)
    fn toast(&mut self, sev: Sev, text: String) {
        self.toasts.push(Toast { text, sev, until: Instant::now() + Duration::from_secs(6) });
    }

    // ------------------------------------------------------------------ Hersteller

    /// (Hersteller, virtuelle BSSID). Leerer Name = unbekannt.
    fn vendor_for(&self, info: &BssInfo) -> (String, bool) {
        if let Some(v) = self.oui.lookup(&info.bssid) {
            return v;
        }
        let laa = info.bssid[0] & 0x02 != 0;
        match info.wps.as_ref().and_then(|w| w.manufacturer.clone()) {
            Some(m) => (m, laa),
            None => (String::new(), laa),
        }
    }

    /// Anzeigename des Herstellers (inkl. "unbekannt")
    fn vendor_label(r: &ApRecord) -> String {
        if !r.vendor.is_empty() {
            r.vendor.clone()
        } else if r.vendor_virtual {
            t!("(lokal verwaltet)", "(locally administered)").into()
        } else {
            t!("Unbekannt", "Unknown").into()
        }
    }

    /// Hersteller nachziehen. Virtuelle (LAA-)BSSIDs ohne OUI-Treffer erben den Hersteller der
    /// physischen Radio-BSSID, die sich nur im ersten bzw. letzten Byte unterscheidet.
    /// `full`: alle neu aus der OUI-DB bestimmen (nur nach dem Neuladen der DB nötig).
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
            if k[0] & 0x02 == 0 || !self.aps[k].vendor.is_empty() {
                continue;
            }
            let physical = |p: &&[u8; 6]| p[0] & 0x02 == 0 && !self.aps[*p].vendor.is_empty();
            let parent = keys
                .iter()
                .filter(physical)
                .find(|p| p[1..6] == k[1..6])
                .or_else(|| keys.iter().filter(physical).find(|p| p[1..5] == k[1..5]));
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

    fn poll_oui(&mut self) {
        let Some(rx) = &self.oui_rx else { return };
        let Ok(res) = rx.try_recv() else { return };
        self.oui_rx = None;
        match res {
            Ok(n) => {
                self.oui = OuiDb::load();
                self.refresh_vendors(true);
                self.toast(Sev::Info, tf!("Hersteller-Datenbank geladen: {n} Einträge", "Vendor database loaded: {n} entries"));
            }
            Err(e) => self.toast(Sev::Warning, tf!("Hersteller-Datenbank: {e}", "Vendor database: {e}")),
        }
    }

    // ------------------------------------------------------------------ Berechtigungen

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
        // Eine reine Registry-Prüfung kennt das Ergebnis der Windows-Abfrage nicht -> letztes übernehmen
        if st.access == perm::Access::Unknown {
            if let Some(old) = &self.perm {
                st.access = old.access;
            }
        }
        let was_ok = self.perm.as_ref().map(|p| p.ok());
        if st.ok() && was_ok != Some(true) && self.error_code == wlan::ERROR_ACCESS_DENIED {
            self.add_log(Sev::Info, t!("Standortzugriff erlaubt – starte Scan", "Location access granted – scanning").into());
            let _ = self.scanner.tx.send(ScanCmd::ScanNow);
        }
        if st.ok() && !self.location_held {
            // Standort-Symbol in der Taskleiste ruhig halten statt bei jedem Scan blinken zu lassen
            if !crate::demo::enabled() {
                perm::hold_location_session();
            }
            self.location_held = true;
        }
        self.perm = Some(st);
    }

    /// Scan funktioniert tatsächlich (dann ist jede Berechtigungswarnung hinfällig)
    fn scan_works(&self) -> bool {
        self.scan_count > 0 && self.error.is_none()
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
        self.poll_perm();
        self.poll_update();
    }

    fn handle_scan(&mut self, ev: wlan::ScanEvent) {
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
                    if self.scan_count > 1 {
                        let name = if b.hidden { t!("‹versteckt›", "‹hidden›").to_string() } else { b.ssid.clone() };
                        let msg = tf!(
                            "Neues Netz: {} ({}) · Kanal {} · {} dBm",
                            "New network: {} ({}) · channel {} · {} dBm",
                            name,
                            fmt_mac(&key),
                            b.channel,
                            b.rssi
                        );
                        self.add_log(Sev::Info, msg);
                    }
                    self.aps.insert(key, ApRecord::new(b, vendor, now, t));
                }
            }
            self.refresh_vendors(false);
        }
        let remove_after = self.settings.remove_after;
        let gone: Vec<[u8; 6]> = self.aps.iter().filter(|(_, r)| r.age(now) > remove_after).map(|(k, _)| *k).collect();
        for k in gone {
            if let Some(r) = self.aps.remove(&k) {
                let msg = tf!("Nicht mehr sichtbar: {} ({})", "No longer visible: {} ({})", Self::display_ssid(&r), fmt_mac(&k));
                self.add_log(Sev::Warning, msg);
            }
        }
    }

    /// Schnelle Verbindungsabfrage (1×/s): Kopfzeile, Verlauf, Roaming-Log, laufende Messung
    fn handle_conn(&mut self, conn: Option<ConnInfo>) {
        let t = self.now_t();
        let old_bssid = self.conn.as_ref().map(|c| c.bssid);
        if let Some(c) = &conn {
            if let Some(old) = old_bssid.filter(|o| *o != c.bssid) {
                let msg = tf!("Roaming: {} > {} ({})", "Roaming: {} > {} ({})", fmt_mac(&old), fmt_mac(&c.bssid), c.ssid);
                self.add_log(Sev::Warning, msg);
            } else if old_bssid.is_none() {
                let msg = tf!("Verbunden mit {} ({})", "Connected to {} ({})", c.ssid, fmt_mac(&c.bssid));
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
            self.add_log(Sev::Warning, t!("Verbindung getrennt", "Disconnected").into());
        }
        if let Some(m) = &mut self.meas {
            m.on_conn(t, conn.as_ref());
        }
        self.conn = conn;
    }

    // ------------------------------------------------------------------ Abfragen

    fn display_ssid(r: &ApRecord) -> String {
        if r.info.hidden {
            t!("‹versteckt›", "‹hidden›").into()
        } else {
            r.info.ssid.clone()
        }
    }

    fn group_key(r: &ApRecord) -> String {
        if r.info.hidden {
            "\u{0}hidden".into()
        } else {
            r.info.ssid.clone()
        }
    }

    fn is_stale(&self, r: &ApRecord, now: Instant) -> bool {
        r.age(now) > self.settings.stale_after
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
        if (r.info.hidden && !f.show_hidden)
            || (f.only_pinned && !self.pinned.contains(&r.info.bssid))
            || (f.hide_stale && self.is_stale(r, now))
            || (f.only_open && r.info.sec_level != SecLevel::Open)
        {
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

    fn visible_records(&self) -> Vec<&ApRecord> {
        let now = Instant::now();
        self.aps.values().filter(|r| self.visible(r, now)).collect()
    }

    fn sorted_keys(&self) -> Vec<[u8; 6]> {
        let mut v = self.visible_records();
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
            };
            let o = if self.sort_asc { o } else { o.reverse() };
            // eindeutiger Abschluss, sonst tauschen gleich starke APs bei jedem Scan die Plätze
            o.then_with(|| ib.rssi.cmp(&ia.rssi)).then_with(|| ia.bssid.cmp(&ib.bssid))
        });
        v.into_iter().map(|r| r.info.bssid).collect()
    }

    /// Eindeutige Farben je SSID aus einer festen Palette (alphabetisch vergeben, damit stabil)
    fn ssid_colors(&self) -> HashMap<String, Color32> {
        const PALETTE: [Color32; 12] = [
            Color32::from_rgb(66, 165, 245),
            Color32::from_rgb(255, 167, 38),
            Color32::from_rgb(102, 187, 106),
            Color32::from_rgb(239, 83, 80),
            Color32::from_rgb(171, 71, 188),
            Color32::from_rgb(38, 198, 218),
            Color32::from_rgb(255, 213, 79),
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

    /// SSIDs zur Auswahl: (Name, bester RSSI), stärkste zuerst
    fn ssid_choices(&self) -> Vec<(String, i32)> {
        let now = Instant::now();
        let mut best: HashMap<&str, i32> = HashMap::new();
        for r in self.aps.values().filter(|r| !r.info.hidden && !self.is_stale(r, now)) {
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
            .filter(|r| !r.info.hidden && r.info.ssid == ssid && !self.is_stale(r, now))
            .max_by_key(|r| r.info.rssi)
    }

    fn select_iface(&mut self, g: u128) {
        self.iface = Some(g);
        let _ = self.scanner.tx.send(ScanCmd::SetInterface(g));
    }

    fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        let _ = self.scanner.tx.send(ScanCmd::Pause(paused));
    }

    fn scan_now(&mut self) {
        self.paused = false;
        let _ = self.scanner.tx.send(ScanCmd::ScanNow);
    }

    fn toggle_pin(&mut self, k: [u8; 6]) {
        if !self.pinned.remove(&k) {
            self.pinned.insert(k);
        }
    }

    fn open_finder(&mut self, k: [u8; 6]) {
        self.selected = Some(k);
        self.page = Page::Finder;
    }

    /// Rundgang-Messung starten (Verlauf, Messpunkte per Enter)
    fn start_measurement(&mut self, ssid: String) {
        self.start_survey(ssid, false);
    }

    /// Messung starten; `plan_mode` = Grundriss-Messung
    fn start_survey(&mut self, ssid: String, plan_mode: bool) {
        // vorherige Messung sichern, dann neu beginnen (eigene Datei)
        self.autosave_survey(true);
        self.meas_file = None;
        self.meas_dirty_since = None;
        self.meas_last_save = None;
        self.meas_point = None;
        self.meas_ssid = Some(ssid.clone());
        // Grundriss-Messung: Plan der vorherigen Grundriss-Messung übernehmen (gleiches Gebäude)
        let plan = if plan_mode { self.meas.as_ref().filter(|m| m.plan_mode).and_then(|m| m.plan.clone()).or_else(|| self.last_plan.clone()) } else { None };
        let mut m = Measurement::new(ssid.clone(), self.now_t());
        m.plan = plan;
        m.plan_mode = plan_mode;
        self.meas = Some(m);
        self.net_jobs.clear();
        self.plan_sel = None;
        self.add_log(Sev::Info, tf!("Messung gestartet: {ssid}", "Measurement started: {ssid}"));
        self.page = if plan_mode { Page::PlanSurvey } else { Page::Measure };
        self.apply_scan_interval();
        self.scan_now(); // ersten Wert nicht erst nach dem nächsten Intervall holen
    }

    fn stop_measurement(&mut self) {
        let t = self.now_t();
        if let Some(m) = &mut self.meas {
            m.stop(t);
            let ssid = m.ssid.clone();
            self.add_log(Sev::Info, tf!("Messung beendet: {ssid}", "Survey stopped: {ssid}"));
        }
        self.meas_point = None;
        self.autosave_survey(true);
        self.apply_scan_interval();
    }

    /// Scan-Intervall an den Scanner geben – während einer Messung höchstens 2 s,
    /// damit auch ohne Verbindung (nur Scan-Werte) genug Messpunkte entstehen
    fn apply_scan_interval(&self) {
        let running = self.meas.as_ref().is_some_and(|m| m.running);
        let s = if running { self.settings.interval.min(MEASURE_SCAN_INTERVAL) } else { self.settings.interval };
        let _ = self.scanner.tx.send(ScanCmd::SetInterval(s));
    }

    // ------------------------------------------------------------------ Export

    fn export_networks(&mut self) {
        let name = format!("wifi-scan_{}.csv", Local::now().format("%Y-%m-%d_%H-%M-%S"));
        let Some(path) = rfd::FileDialog::new().set_file_name(&name).add_filter("CSV (Excel)", &["csv"]).save_file() else {
            return;
        };
        match std::fs::write(&path, self.networks_csv()) {
            Ok(_) => self.toast(Sev::Info, tf!("Exportiert: {}", "Exported: {}", path.display())),
            Err(e) => self.toast(Sev::Critical, tf!("Export fehlgeschlagen: {e}", "Export failed: {e}")),
        }
    }

    fn networks_csv(&self) -> String {
        let esc = survey::esc_csv;
        let header = t!(
            "SSID;BSSID;Hersteller;RSSI;RSSI min;RSSI max;RSSI Ø;Kanal;Band;Frequenz MHz;Breite;Mittenkanal;Standard;Sicherheit;AKM;Cipher;PMF;Max. Rate Mbit/s;Streams;Clients;Auslastung %;Land;Beacon ms;DTIM;802.11k;802.11r;802.11v;WPS;AP-Name;Chipsatz;Uptime;Erstmals gesehen;Zuletzt vor s",
            "SSID;BSSID;Vendor;RSSI;RSSI min;RSSI max;RSSI avg;Channel;Band;Frequency MHz;Width;Center channel;Standard;Security;AKM;Cipher;PMF;Max rate Mbit/s;Streams;Clients;Utilization %;Country;Beacon ms;DTIM;802.11k;802.11r;802.11v;WPS;AP name;Chipset;Uptime;First seen;Last seen s ago"
        );
        let mut s = format!("\u{feff}{header}\n");
        let now = Instant::now();
        let yn = |b: bool| if b { t!("ja", "yes") } else { t!("nein", "no") }.to_string();
        for k in self.sorted_keys() {
            let r = &self.aps[&k];
            let i = &r.info;
            let wps = i.wps.as_ref().map(|w| {
                [w.manufacturer.clone(), w.model.clone(), w.model_number.clone()]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" ")
            });
            let row = [
                Self::display_ssid(r),
                fmt_mac(&i.bssid),
                Self::vendor_label(r),
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
                i.pmf.label().into(),
                format!("{:.0}", i.max_rate),
                i.streams.to_string(),
                i.stations.map(|v| v.to_string()).unwrap_or_default(),
                i.ch_util.map(|v| format!("{v:.0}")).unwrap_or_default(),
                i.country.clone().unwrap_or_default(),
                format!("{:.1}", i.beacon_ms),
                i.dtim.map(|v| v.to_string()).unwrap_or_default(),
                yn(i.k_rrm),
                yn(i.r_ft),
                yn(i.v_btm),
                wps.unwrap_or_else(|| yn(false)),
                i.ap_name.clone().unwrap_or_default(),
                i.chipset.clone().unwrap_or_default(),
                fmt_duration(i.uptime_s),
                r.first_seen_wall.format("%Y-%m-%d %H:%M:%S").to_string(),
                format!("{:.0}", r.age(now)),
            ];
            s.push_str(&row.iter().map(|c| esc(c)).collect::<Vec<_>>().join(";"));
            s.push('\n');
        }
        s
    }

    fn export_measurement(&mut self) {
        let Some(m) = &self.meas else { return };
        let safe: String = m.ssid.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
        let name = format!("{}_{}_{}.csv", t!("messung", "measurement"), safe, m.started_wall.format("%Y-%m-%d_%H-%M"));
        let csv = m.to_csv();
        let Some(path) = rfd::FileDialog::new().set_file_name(&name).add_filter("CSV (Excel)", &["csv"]).save_file() else {
            return;
        };
        match std::fs::write(&path, csv) {
            Ok(_) => self.toast(Sev::Info, tf!("Messung exportiert: {}", "Measurement exported: {}", path.display())),
            Err(e) => self.toast(Sev::Critical, tf!("Export fehlgeschlagen: {e}", "Export failed: {e}")),
        }
    }

    // ------------------------------------------------------------------ Einstellungen

    /// Laufenden Zustand in die Einstellungen übernehmen und (gedrosselt) speichern
    fn sync_settings(&mut self) {
        let mut pinned: Vec<[u8; 6]> = self.pinned.iter().copied().collect();
        pinned.sort();
        self.settings.pinned = pinned;
        self.settings.show_hidden = self.f.show_hidden;
        self.settings.lang = i18n::get();
        if self.settings != self.settings_saved {
            let since = *self.settings_dirty_since.get_or_insert_with(Instant::now);
            if since.elapsed() > Duration::from_millis(800) {
                self.save_settings();
            }
        }
    }

    fn save_settings(&mut self) {
        // Demo-Modus: echte Einstellungen nicht verändern (z. B. Markierungen erfundener APs)
        if !crate::demo::enabled() {
            self.settings.save();
        }
        self.settings_saved = self.settings.clone();
        self.settings_dirty_since = None;
    }

    // ------------------------------------------------------------------ Tastenkürzel

    fn shortcuts(&mut self, ctx: &egui::Context) {
        use egui::{Key, Modifiers};
        let typing = ctx.wants_keyboard_input();
        let (f5, ctrl_f, ctrl_e, esc, num) = ctx.input_mut(|i| {
            let num = (1..=9).find(|n| {
                let key = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9][*n - 1];
                i.consume_key(Modifiers::CTRL, key)
            });
            (
                i.consume_key(Modifiers::NONE, Key::F5),
                i.consume_key(Modifiers::CTRL, Key::F),
                i.consume_key(Modifiers::CTRL, Key::E),
                !typing && i.key_pressed(Key::Escape),
                num,
            )
        });
        if f5 {
            self.scan_now();
        }
        if ctrl_f {
            if !matches!(self.page, Page::Networks | Page::Channels | Page::History) {
                self.page = Page::Networks;
            }
            self.focus_search = true;
        }
        if ctrl_e {
            if matches!(self.page, Page::Measure | Page::PlanSurvey) && self.meas.is_some() {
                self.export_measurement();
            } else {
                self.export_networks();
            }
        }
        if esc {
            if self.show_settings || self.show_about || self.show_update || self.show_whats_new || self.meas_point.is_some() || self.plan_scale.is_some() || self.show_scale_editor || self.pdf_pick.is_some() {
                self.meas_point = None;
                self.plan_scale = None;
                self.show_scale_editor = false;
                self.pdf_pick = None;
                self.show_settings = false;
                self.show_about = false;
                self.show_update = false;
                self.show_whats_new = false;
            } else {
                self.selected = None;
            }
        }
        if let Some(n) = num {
            self.page = shell::NAV_ORDER[n - 1];
        }
    }
}

impl eframe::App for WifiApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // transparent, damit das Lade-Popup runde Ecken hat (die Panels füllen danach alles)
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        self.query = self.f.text.trim().to_lowercase();
        if self.splash(ctx) {
            return;
        }
        self.maybe_auto_update_check(ctx);
        self.shortcuts(ctx);

        self.top_bar(ctx);
        self.status_bar(ctx);
        self.sidebar(ctx);
        if matches!(self.page, Page::Networks | Page::Channels | Page::History | Page::Findings | Page::Overview) {
            self.inspector(ctx);
        }
        if self.page != Page::Finder {
            self.beeper.set(false, -100);
        }

        let p = crate::theme::pal_ctx(ctx);
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(p.bg).inner_margin(egui::Margin::symmetric(20.0, 14.0)))
            .show(ctx, |ui| {
                self.problem_banner(ui);
                self.page_content(ui)
            });

        self.perm_dialog(ctx);
        self.settings_window(ctx);
        self.about_window(ctx);
        self.update_window(ctx);
        self.whats_new_window(ctx);
        self.point_dialog(ctx);
        self.poll_net_jobs();
        if !self.net_jobs.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        self.autosave_survey(false);
        self.poll_install(ctx);
        self.toasts(ctx);
        self.sync_settings();
        ctx.request_repaint_after(Duration::from_millis(1000));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.autosave_survey(true);
        self.sync_settings();
        self.save_settings();
    }
}

impl WifiApp {
    fn page_content(&mut self, ui: &mut egui::Ui) {
        match self.page {
            Page::Overview => self.page_overview(ui),
            Page::Networks => self.page_networks(ui),
            Page::Channels => self.page_channels(ui),
            Page::Rating => self.page_rating(ui),
            Page::History => self.page_history(ui),
            Page::Findings => self.page_findings(ui),
            Page::Measure => self.page_measure(ui, false),
            Page::PlanSurvey => self.page_measure(ui, true),
            Page::Finder => self.page_finder(ui),
            Page::Events => self.page_events(ui),
        }
    }
}
