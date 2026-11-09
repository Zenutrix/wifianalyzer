//! WLAN-Messung ("Walk-Survey"): zeichnet für eine SSID das Signal der Verbindung (1×/s),
//! den jeweils stärksten AP der SSID (pro Scan) sowie Roams, Abbrüche, Sticky-Client-Phasen
//! und frei gesetzte Markierungen auf. Dazu kommen benannte Messpunkte (Raum/Ort) mit gemitteltem
//! Wert pro Stelle – für Bericht und Kunde. Messungen lassen sich als Datei speichern und wieder öffnen.

use std::collections::HashSet;

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use crate::nettest::NetTest;

use crate::ie::BssInfo;
use crate::model::fmt_mac;
use crate::wlan::ConnInfo;

/// Ab diesem Abstand (dB) zwischen bestem und verbundenem AP gilt der Client als "klebend"
const STICKY_DB: i32 = 10;
/// ... wenn der Zustand mindestens so lange (s) anhält
const STICKY_SECS: f64 = 15.0;

#[derive(Clone, Serialize, Deserialize)]
pub struct ConnSample {
    pub t: f64,
    pub rssi: i32,
    pub bssid: [u8; 6],
    pub channel: u32,
    pub tx_mbps: f64,
    pub rx_mbps: f64,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct BestSample {
    pub t: f64,
    pub rssi: i32,
    pub bssid: [u8; 6],
    pub channel: u32,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum EventKind {
    Start,
    Stop,
    Roam { from: [u8; 6], to: [u8; 6], rssi_from: i32, rssi_to: i32, ch_from: u32, ch_to: u32 },
    Disconnect { bssid: [u8; 6] },
    Connect { bssid: [u8; 6], rssi: i32 },
    Sticky { conn: [u8; 6], conn_rssi: i32, best: [u8; 6], best_rssi: i32 },
    Marker(String),
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Event {
    /// Sekunden seit Messbeginn
    pub t: f64,
    pub kind: EventKind,
}

impl Event {
    pub fn title(&self) -> &'static str {
        match self.kind {
            EventKind::Start => "Start",
            EventKind::Stop => crate::t!("Stopp", "Stop"),
            EventKind::Roam { .. } => "Roam",
            EventKind::Disconnect { .. } => crate::t!("Abbruch", "Disconnect"),
            EventKind::Connect { .. } => crate::t!("Verbunden", "Connected"),
            EventKind::Sticky { .. } => "Sticky Client",
            EventKind::Marker(_) => crate::t!("Messpunkt", "Point"),
        }
    }

    pub fn details(&self) -> String {
        match &self.kind {
            EventKind::Start => crate::t!("Messung gestartet", "Measurement started").into(),
            EventKind::Stop => crate::t!("Messung beendet", "Measurement stopped").into(),
            EventKind::Roam { from, to, rssi_from, rssi_to, ch_from, ch_to } => crate::tf!(
                "{} (Kanal {ch_from}, {rssi_from} dBm) > {} (Kanal {ch_to}, {rssi_to} dBm)",
                "{} (channel {ch_from}, {rssi_from} dBm) > {} (channel {ch_to}, {rssi_to} dBm)",
                fmt_mac(from),
                fmt_mac(to)
            ),
            EventKind::Disconnect { bssid } => crate::tf!("Verbindung zu {} verloren", "Lost connection to {}", fmt_mac(bssid)),
            EventKind::Connect { bssid, rssi } => crate::tf!("Verbunden mit {} ({rssi} dBm)", "Connected to {} ({rssi} dBm)", fmt_mac(bssid)),
            EventKind::Sticky { conn, conn_rssi, best, best_rssi } => crate::tf!(
                "bleibt an {} ({conn_rssi} dBm), obwohl {} mit {best_rssi} dBm verfügbar ist",
                "stays on {} ({conn_rssi} dBm) although {} is available at {best_rssi} dBm",
                fmt_mac(conn),
                fmt_mac(best)
            ),
            EventKind::Marker(text) => text.clone(),
        }
    }
}

/// Bewertung eines Signalwerts – verständlich für Kunden
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Quality {
    Excellent,
    Good,
    Fair,
    Weak,
}

impl Quality {
    pub fn of(rssi: f64) -> Self {
        if rssi >= -60.0 {
            Quality::Excellent
        } else if rssi >= -67.0 {
            Quality::Good
        } else if rssi >= -75.0 {
            Quality::Fair
        } else {
            Quality::Weak
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Quality::Excellent => crate::t!("Sehr gut", "Excellent"),
            Quality::Good => crate::t!("Gut", "Good"),
            Quality::Fair => crate::t!("Ausreichend", "Fair"),
            Quality::Weak => crate::t!("Schwach", "Weak"),
        }
    }

    /// Was der Wert praktisch bedeutet
    pub fn hint(self) -> &'static str {
        match self {
            Quality::Excellent => crate::t!("Ideal, auch für Videotelefonie und große Datenmengen", "Ideal, also for video calls and large transfers"),
            Quality::Good => crate::t!("Stabil für Sprache, Video und normale Nutzung", "Stable for voice, video and normal use"),
            Quality::Fair => crate::t!("Surfen und E-Mail ok, Telefonie/Video evtl. instabil", "Browsing and e-mail ok, voice/video may be unstable"),
            Quality::Weak => crate::t!("Abbrüche und langsame Verbindung zu erwarten", "Drop-outs and slow connection to be expected"),
        }
    }

    /// RGB für Bericht und Oberfläche
    pub fn rgb(self) -> (u8, u8, u8) {
        match self {
            Quality::Excellent => (26, 127, 55),
            Quality::Good => (45, 164, 78),
            Quality::Fair => (191, 135, 0),
            Quality::Weak => (207, 34, 46),
        }
    }
}

/// Benannter Messpunkt (Raum, Ort) mit dem an dieser Stelle gemittelten Wert
#[derive(Clone, Serialize, Deserialize)]
pub struct MeasPoint {
    pub name: String,
    #[serde(default)]
    pub note: String,
    /// Sekunden seit Messbeginn (Zeitpunkt des Speicherns)
    pub t: f64,
    /// Ø Signal in dBm über die Zeit am Punkt
    pub rssi: Option<f64>,
    pub min: Option<i32>,
    pub max: Option<i32>,
    pub samples: usize,
    /// Wert stammt aus dem Scan (stärkster AP der SSID), weil nicht verbunden
    pub from_scan: bool,
    /// verbundener (bzw. stärkster) AP
    pub bssid: Option<[u8; 6]>,
    pub channel: u32,
    pub tx_mbps: f64,
    /// stärkster AP der SSID laut letztem Scan
    pub best_bssid: Option<[u8; 6]>,
    pub best_rssi: Option<i32>,
    /// Anzahl APs der SSID, die an dieser Stelle zu sehen sind
    pub aps_visible: usize,
    /// eindeutige Nummer innerhalb der Messung (für nachträglich eintreffende Netzwerkwerte)
    #[serde(default)]
    pub id: u64,
    /// Position im Grundriss (0..1 relativ zu Breite/Höhe des Plans)
    #[serde(default)]
    pub pos: Option<[f32; 2]>,
    /// stärkster AP der SSID je Frequenzband an dieser Stelle (dBm)
    #[serde(default)]
    pub bands: BandValues,
    /// Latenz/Durchsatz an dieser Stelle
    #[serde(default)]
    pub net: Option<NetTest>,
}

/// Grundriss (Bilddatei) einer Messung – wird in der Messungsdatei mitgespeichert
#[derive(Clone, Serialize, Deserialize)]
pub struct FloorPlan {
    pub name: String,
    #[serde(with = "b64")]
    pub data: Vec<u8>,
    /// reale Breite des Plans in Metern (aus dem Maßstab)
    #[serde(default)]
    pub width_m: Option<f32>,
    /// Linie, mit der der Maßstab gesetzt wurde (Plan-Koordinaten 0..1)
    #[serde(default)]
    pub scale_line: Option<[[f32; 2]; 2]>,
    /// eingegebene Länge dieser Linie in Metern
    #[serde(default)]
    pub scale_len_m: Option<f32>,
}

impl FloorPlan {
    pub fn new(name: String, data: Vec<u8>) -> Self {
        Self { name, data, width_m: None, scale_line: None, scale_len_m: None }
    }
}

/// Signal je Frequenzband (stärkster AP der SSID), dBm
#[derive(Clone, Copy, Default, Debug, PartialEq, Serialize, Deserialize)]
pub struct BandValues {
    pub b24: Option<i32>,
    pub b5: Option<i32>,
    pub b6: Option<i32>,
}

impl BandValues {
    pub fn any(&self) -> bool {
        self.b24.is_some() || self.b5.is_some() || self.b6.is_some()
    }

    /// Kurztext „2,4 -48 · 5 -55 · 6 -62“
    pub fn summary(&self) -> String {
        let mut v = Vec::new();
        for (l, x) in [("2,4", self.b24), ("5", self.b5), ("6", self.b6)] {
            if let Some(x) = x {
                v.push(format!("{l}: {x}"));
            }
        }
        v.join(" · ")
    }
}

/// Ebene der Heatmap: gemessener Wert oder ein Frequenzband
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Layer {
    Measured,
    B24,
    B5,
    B6,
}

impl Layer {
    pub const ALL: [Layer; 4] = [Layer::Measured, Layer::B24, Layer::B5, Layer::B6];

    pub fn label(self) -> &'static str {
        match self {
            Layer::Measured => crate::t!("Messwert", "Measured"),
            Layer::B24 => "2,4 GHz",
            Layer::B5 => "5 GHz",
            Layer::B6 => "6 GHz",
        }
    }
}

/// Bilddaten als Base64 in der JSON-Datei
mod b64 {
    use base64::Engine;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&base64::engine::general_purpose::STANDARD.encode(v))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        base64::engine::general_purpose::STANDARD.decode(s.as_bytes()).map_err(serde::de::Error::custom)
    }
}

impl MeasPoint {
    pub fn quality(&self) -> Option<Quality> {
        self.rssi.map(Quality::of)
    }

    /// Wert für eine Heatmap-Ebene
    pub fn value_for(&self, layer: Layer) -> Option<f64> {
        match layer {
            Layer::Measured => self.rssi,
            Layer::B24 => self.bands.b24.map(f64::from),
            Layer::B5 => self.bands.b5.map(f64::from),
            Layer::B6 => self.bands.b6.map(f64::from),
        }
    }
}

impl Measurement {
    /// Platzierte Punkte mit Wert für die Heatmap
    pub fn heat_points(&self, layer: Layer) -> Vec<([f32; 2], f64)> {
        self.points.iter().filter_map(|p| Some((p.pos?, p.value_for(layer)?))).collect()
    }

    /// Ebenen, für die es Werte gibt
    pub fn layers_with_data(&self) -> Vec<Layer> {
        Layer::ALL.into_iter().filter(|l| self.points.iter().any(|p| p.pos.is_some() && p.value_for(*l).is_some())).collect()
    }
}

/// Was die App beim Erfassen eines Punktes aus dem aktuellen Scan beisteuert
#[derive(Default)]
pub struct ScanContext {
    /// stärkster AP der SSID: BSSID, RSSI, Kanal
    pub best: Option<([u8; 6], i32, u32)>,
    pub aps_visible: usize,
    /// stärkster AP der SSID je Band
    pub bands: BandValues,
}

#[derive(Serialize, Deserialize)]
struct SurveyFile {
    app: String,
    format: u32,
    /// Grundriss-Messung (sonst Rundgang) – steht bewusst weit vorne, damit die Dateiliste
    /// die Art anhand der ersten Bytes erkennt, ohne den ganzen Grundriss zu lesen
    #[serde(default)]
    plan_mode: bool,
    app_version: String,
    ssid: String,
    /// RFC 3339
    started: String,
    duration: f64,
    conn: Vec<ConnSample>,
    best: Vec<BestSample>,
    events: Vec<Event>,
    #[serde(default)]
    points: Vec<MeasPoint>,
    #[serde(default)]
    plan: Option<FloorPlan>,
}

#[derive(Default)]
pub struct Stats {
    pub duration: f64,
    pub samples: usize,
    pub avg: Option<f64>,
    pub min: Option<i32>,
    pub max: Option<i32>,
    /// Anteile der Messwerte (0..1): >= -67 dBm, -75..-67, < -75
    pub good: f64,
    pub fair: f64,
    pub bad: f64,
    pub roams: usize,
    pub disconnects: usize,
    pub sticky: usize,
    pub aps_used: usize,
    /// Werte stammen aus dem Scan (bester AP), weil nicht mit der SSID verbunden
    pub from_scan: bool,
}

pub struct Measurement {
    pub ssid: String,
    pub running: bool,
    pub started_wall: DateTime<Local>,
    /// Programmzeit (s) bei Messbeginn – alle Zeiten der Messung sind relativ dazu
    pub t0: f64,
    pub t_end: Option<f64>,
    pub conn: Vec<ConnSample>,
    pub best: Vec<BestSample>,
    pub events: Vec<Event>,
    pub points: Vec<MeasPoint>,
    pub plan: Option<FloorPlan>,
    /// Grundriss-Messung (Klick in den Plan = Messpunkt) statt Rundgang mit Verlauf
    pub plan_mode: bool,
    connected: bool,
    last: Option<ConnSample>,
    sticky_since: Option<f64>,
    sticky_reported: bool,
}

impl Measurement {
    pub fn new(ssid: String, now_t: f64) -> Self {
        Self {
            ssid,
            running: true,
            started_wall: Local::now(),
            t0: now_t,
            t_end: None,
            conn: Vec::new(),
            best: Vec::new(),
            events: vec![Event { t: 0.0, kind: EventKind::Start }],
            points: Vec::new(),
            plan: None,
            plan_mode: false,
            connected: false,
            last: None,
            sticky_since: None,
            sticky_reported: false,
        }
    }

    fn rel(&self, now_t: f64) -> f64 {
        now_t - self.t0
    }

    pub fn elapsed(&self, now_t: f64) -> f64 {
        self.t_end.unwrap_or_else(|| self.rel(now_t))
    }

    pub fn stop(&mut self, now_t: f64) {
        if self.running {
            let t = self.rel(now_t);
            self.running = false;
            self.t_end = Some(t);
            self.events.push(Event { t, kind: EventKind::Stop });
        }
    }

    #[cfg(test)]
    pub fn add_marker(&mut self, now_t: f64, text: &str) {
        let text = text.trim();
        let text = if text.is_empty() { crate::tf!("Markierung {}", "Marker {}", self.marker_count() + 1) } else { text.to_string() };
        self.events.push(Event { t: self.rel(now_t), kind: EventKind::Marker(text) });
    }

    /// Messpunkt speichern. `since_t` = Programmzeit, seit der der Benutzer an der Stelle steht
    /// (Dialog geöffnet). Gemittelt werden die Werte ab kurz davor; waren es zu wenige,
    /// die letzten paar Werte.
    pub fn add_point(&mut self, now_t: f64, since_t: f64, name: &str, note: &str, pos: Option<[f32; 2]>, scan: &ScanContext) -> &MeasPoint {
        let t = self.rel(now_t);
        let since = self.rel(since_t) - 1.0;
        let name = name.trim();
        let name = if name.is_empty() { crate::tf!("Messpunkt {}", "Point {}", self.points.len() + 1) } else { name.to_string() };
        let mut p = MeasPoint {
            name: name.clone(),
            note: note.trim().to_string(),
            t,
            rssi: None,
            min: None,
            max: None,
            samples: 0,
            from_scan: false,
            bssid: None,
            channel: 0,
            tx_mbps: 0.0,
            best_bssid: scan.best.map(|b| b.0),
            best_rssi: scan.best.map(|b| b.1),
            aps_visible: scan.aps_visible,
            bands: scan.bands,
            id: self.points.iter().map(|p| p.id).max().unwrap_or(0) + 1,
            pos,
            net: None,
        };

        let recent_conn: Vec<&ConnSample> = self.conn.iter().filter(|c| t - c.t < 30.0).collect();
        let mut conn: Vec<&ConnSample> = recent_conn.iter().copied().filter(|c| c.t >= since).collect();
        if conn.is_empty() {
            conn = recent_conn.last().copied().into_iter().collect();
        }
        let vals: Vec<i32> = if self.connected && !conn.is_empty() {
            let last = conn[conn.len() - 1];
            p.bssid = Some(last.bssid);
            p.channel = last.channel;
            p.tx_mbps = conn.iter().map(|c| c.tx_mbps).sum::<f64>() / conn.len() as f64;
            conn.iter().map(|c| c.rssi).collect()
        } else {
            // ohne Verbindung: Scan-Werte des stärksten APs (mindestens der letzte Scan)
            p.from_scan = true;
            let recent: Vec<&BestSample> = self.best.iter().filter(|b| t - b.t < 30.0).collect();
            let mut best: Vec<&BestSample> = recent.iter().copied().filter(|b| b.t >= since).collect();
            if best.is_empty() {
                best = recent.last().copied().into_iter().collect();
            }
            if let Some(last) = best.last() {
                p.bssid = Some(last.bssid);
                p.channel = last.channel;
                best.iter().map(|b| b.rssi).collect()
            } else if let Some((b, r, ch)) = scan.best {
                p.bssid = Some(b);
                p.channel = ch;
                vec![r]
            } else {
                Vec::new()
            }
        };
        if !vals.is_empty() {
            p.rssi = Some(vals.iter().map(|v| *v as f64).sum::<f64>() / vals.len() as f64);
            p.min = vals.iter().copied().min();
            p.max = vals.iter().copied().max();
            p.samples = vals.len();
        }
        self.events.push(Event { t, kind: EventKind::Marker(name) });
        self.points.push(p);
        self.points.last().expect("eben eingefügt")
    }

    pub fn remove_point(&mut self, idx: usize) {
        if idx < self.points.len() {
            let p = self.points.remove(idx);
            // zugehörige Markierung im Verlauf ebenfalls entfernen
            if let Some(i) = self.marker_index(p.t) {
                self.events.remove(i);
            }
        }
    }

    pub fn point_mut(&mut self, id: u64) -> Option<&mut MeasPoint> {
        self.points.iter_mut().find(|p| p.id == id)
    }

    /// Ältere Dateien ohne Nummern: eindeutige Nummern vergeben
    fn ensure_point_ids(&mut self) {
        let mut seen = std::collections::HashSet::new();
        let mut next = self.points.iter().map(|p| p.id).max().unwrap_or(0) + 1;
        for p in &mut self.points {
            if p.id == 0 || !seen.insert(p.id) {
                p.id = next;
                seen.insert(next);
                next += 1;
            }
        }
    }

    /// Markierung eines Messpunkts im Verlauf (gleicher Zeitpunkt)
    fn marker_index(&self, t: f64) -> Option<usize> {
        self.events.iter().position(|e| e.t == t && matches!(e.kind, EventKind::Marker(_)))
    }

    /// Nach dem Umbenennen eines Messpunkts die Beschriftung im Verlauf anpassen
    pub fn sync_point_marker(&mut self, idx: usize) {
        let Some(p) = self.points.get(idx) else { return };
        let (t, name) = (p.t, p.name.clone());
        if let Some(i) = self.marker_index(t) {
            self.events[i].kind = EventKind::Marker(name);
        }
    }

    /// Messung als JSON (Dateiendung .wifisurvey)
    pub fn to_file(&self, now_t: f64) -> String {
        let f = SurveyFile {
            app: "WiFi Analyzer".into(),
            format: 1,
            plan_mode: self.plan_mode,
            app_version: env!("CARGO_PKG_VERSION").into(),
            ssid: self.ssid.clone(),
            started: self.started_wall.to_rfc3339(),
            duration: self.elapsed(now_t),
            conn: self.conn.clone(),
            best: self.best.clone(),
            events: self.events.clone(),
            points: self.points.clone(),
            plan: self.plan.clone(),
        };
        serde_json::to_string_pretty(&f).unwrap_or_default()
    }

    /// Gespeicherte Messung öffnen (immer beendet – zum Ansehen, Ergänzen der Notizen und Exportieren)
    pub fn from_file(text: &str) -> Result<Self, String> {
        let f: SurveyFile = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
        if f.app != "WiFi Analyzer" {
            return Err(crate::t!("Keine WiFi-Analyzer-Messung", "Not a WiFi Analyzer survey").into());
        }
        let started = DateTime::parse_from_rfc3339(&f.started).map(|d| d.with_timezone(&Local)).unwrap_or_else(|_| Local::now());
        let mut m = Self {
            ssid: f.ssid,
            running: false,
            started_wall: started,
            t0: 0.0,
            t_end: Some(f.duration),
            conn: f.conn,
            best: f.best,
            events: f.events,
            points: f.points,
            plan_mode: f.plan_mode || f.plan.is_some(),
            plan: f.plan,
            connected: false,
            last: None,
            sticky_since: None,
            sticky_reported: false,
        };
        m.ensure_point_ids();
        Ok(m)
    }

    #[cfg(test)]
    pub fn marker_count(&self) -> usize {
        self.events.iter().filter(|e| matches!(e.kind, EventKind::Marker(_))).count()
    }

    /// Schnelle Verbindungsabfrage (1×/s)
    pub fn on_conn(&mut self, now_t: f64, conn: Option<&ConnInfo>) {
        if !self.running {
            return;
        }
        let t = self.rel(now_t);
        match conn.filter(|c| c.ssid == self.ssid) {
            Some(c) => {
                // ohne RSSI-Opcode aus der Signalqualität schätzen (0..100 % -> -100..-50 dBm)
                let rssi = c.rssi.unwrap_or(c.quality as i32 / 2 - 100);
                let sample = ConnSample {
                    t,
                    rssi,
                    bssid: c.bssid,
                    channel: c.channel.unwrap_or(0),
                    tx_mbps: c.tx_mbps,
                    rx_mbps: c.rx_mbps,
                };
                match (&self.last, self.connected) {
                    (Some(prev), true) if prev.bssid != c.bssid => {
                        self.events.push(Event {
                            t,
                            kind: EventKind::Roam {
                                from: prev.bssid,
                                to: c.bssid,
                                rssi_from: prev.rssi,
                                rssi_to: rssi,
                                ch_from: prev.channel,
                                ch_to: sample.channel,
                            },
                        });
                        self.sticky_since = None;
                        self.sticky_reported = false;
                    }
                    (_, false) if !self.conn.is_empty() || t > 2.0 => {
                        self.events.push(Event { t, kind: EventKind::Connect { bssid: c.bssid, rssi } });
                    }
                    _ => {}
                }
                self.connected = true;
                self.last = Some(sample.clone());
                self.conn.push(sample);
            }
            None => {
                if self.connected {
                    let bssid = self.last.as_ref().map(|l| l.bssid).unwrap_or_default();
                    self.events.push(Event { t, kind: EventKind::Disconnect { bssid } });
                }
                self.connected = false;
            }
        }
    }

    /// Ergebnis eines Scans: stärksten AP der SSID merken, Sticky-Client prüfen
    pub fn on_scan(&mut self, now_t: f64, list: &[BssInfo]) {
        if !self.running {
            return;
        }
        let t = self.rel(now_t);
        let Some(best) = list.iter().filter(|b| !b.hidden && b.ssid == self.ssid).max_by_key(|b| b.rssi) else {
            return;
        };
        self.best.push(BestSample { t, rssi: best.rssi, bssid: best.bssid, channel: best.channel });

        // Sticky Client: verbunden mit einem AP, der deutlich schwächer ist als der beste verfügbare
        let current = self.last.as_ref().filter(|l| self.connected && t - l.t < 3.0);
        match current {
            Some(cur) if best.bssid != cur.bssid && best.rssi - cur.rssi >= STICKY_DB => {
                let since = *self.sticky_since.get_or_insert(t);
                if t - since >= STICKY_SECS && !self.sticky_reported {
                    self.sticky_reported = true;
                    self.events.push(Event {
                        t,
                        kind: EventKind::Sticky {
                            conn: cur.bssid,
                            conn_rssi: cur.rssi,
                            best: best.bssid,
                            best_rssi: best.rssi,
                        },
                    });
                }
            }
            _ => {
                self.sticky_since = None;
                self.sticky_reported = false;
            }
        }
    }

    /// Verbunden und gerade in einer Sticky-Phase (für die Live-Anzeige)
    pub fn is_sticky(&self) -> bool {
        self.sticky_since.is_some()
    }

    pub fn stats(&self, now_t: f64) -> Stats {
        // Verbindungswerte bevorzugt; ohne Verbindung die Scan-Werte des besten APs
        let from_scan = self.conn.is_empty();
        let vals: Vec<(i32, [u8; 6])> = if from_scan {
            self.best.iter().map(|b| (b.rssi, b.bssid)).collect()
        } else {
            self.conn.iter().map(|c| (c.rssi, c.bssid)).collect()
        };
        let mut s = Stats { duration: self.elapsed(now_t), samples: vals.len(), from_scan, ..Default::default() };
        if !vals.is_empty() {
            let n = vals.len() as f64;
            let sum: i64 = vals.iter().map(|v| v.0 as i64).sum();
            s.avg = Some(sum as f64 / n);
            s.min = vals.iter().map(|v| v.0).min();
            s.max = vals.iter().map(|v| v.0).max();
            s.good = vals.iter().filter(|v| v.0 >= -67).count() as f64 / n;
            s.bad = vals.iter().filter(|v| v.0 < -75).count() as f64 / n;
            s.fair = (1.0 - s.good - s.bad).max(0.0);
            s.aps_used = vals.iter().map(|v| v.1).collect::<HashSet<_>>().len();
        }
        for e in &self.events {
            match e.kind {
                EventKind::Roam { .. } => s.roams += 1,
                EventKind::Disconnect { .. } => s.disconnects += 1,
                EventKind::Sticky { .. } => s.sticky += 1,
                _ => {}
            }
        }
        s
    }

    /// CSV (Semikolon, Excel-tauglich): alle Messwerte und Ereignisse zeitlich sortiert
    pub fn to_csv(&self) -> String {
        let wall = |t: f64| (self.started_wall + chrono::Duration::milliseconds((t * 1000.0) as i64)).format("%H:%M:%S").to_string();
        // Dezimalkomma, damit Excel (deutsch) Zeiten nicht als Datum liest
        let dec = |t: f64| format!("{t:.1}").replace('.', ",");
        let mut rows: Vec<(f64, String)> = Vec::new();
        for c in &self.conn {
            rows.push((
                c.t,
                format!(
                    "{};{};{};{};{};{};{:.0};{:.0};",
                    dec(c.t),
                    wall(c.t),
                    crate::t!("Verbindung", "Connection"),
                    c.rssi,
                    fmt_mac(&c.bssid),
                    c.channel,
                    c.tx_mbps,
                    c.rx_mbps
                ),
            ));
        }
        for b in &self.best {
            let kind = crate::t!("Bester AP", "Best AP");
            rows.push((b.t, format!("{};{};{kind};{};{};{};;;", dec(b.t), wall(b.t), b.rssi, fmt_mac(&b.bssid), b.channel)));
        }
        for e in &self.events {
            let details = e.details().replace(';', ",");
            rows.push((e.t, format!("{};{};{};;;;;;{}", dec(e.t), wall(e.t), e.title(), details)));
        }
        rows.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut s = crate::tf!(
            "\u{feff}# WLAN-Messung SSID \"{}\" gestartet {} (WiFi Analyzer {})\nZeit (s);Uhrzeit;Typ;RSSI (dBm);BSSID;Kanal;Tx (Mbit/s);Rx (Mbit/s);Details\n",
            "\u{feff}# Wi-Fi measurement SSID \"{}\" started {} (WiFi Analyzer {})\nTime (s);Clock;Type;RSSI (dBm);BSSID;Channel;Tx (Mbit/s);Rx (Mbit/s);Details\n",
            self.ssid,
            self.started_wall.format("%Y-%m-%d %H:%M:%S"),
            env!("CARGO_PKG_VERSION")
        );
        for (_, r) in rows {
            s.push_str(&r);
            s.push('\n');
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ie::build_bss;

    const A: [u8; 6] = [0x02, 0, 0, 0, 0, 0xA];
    const B: [u8; 6] = [0x02, 0, 0, 0, 0, 0xB];

    fn conn(ssid: &str, bssid: [u8; 6], rssi: i32) -> ConnInfo {
        ConnInfo {
            profile: ssid.into(),
            ssid: ssid.into(),
            bssid,
            phy: "802.11ax".into(),
            quality: 80,
            rx_mbps: 600.0,
            tx_mbps: 600.0,
            auth: "WPA2-PSK".into(),
            cipher: "CCMP".into(),
            channel: Some(36),
            rssi: Some(rssi),
        }
    }

    fn bss(ssid: &str, bssid: [u8; 6], rssi: i32) -> BssInfo {
        build_bss(ssid.as_bytes(), bssid, rssi, 80, 5_180_000, 10, 100, 0, 0x11, &[], true, &[])
    }

    fn count(m: &Measurement, f: fn(&EventKind) -> bool) -> usize {
        m.events.iter().filter(|e| f(&e.kind)).count()
    }

    #[test]
    fn roam_disconnect_reconnect() {
        let mut m = Measurement::new("Net".into(), 100.0);
        m.on_conn(101.0, Some(&conn("Net", A, -60)));
        m.on_conn(102.0, Some(&conn("Net", A, -62)));
        m.on_conn(103.0, Some(&conn("Net", B, -55))); // Roam A -> B
        m.on_conn(104.0, None); // Abbruch
        m.on_conn(106.0, Some(&conn("Net", B, -54))); // wieder verbunden
        m.on_conn(107.0, Some(&conn("Andere", A, -40))); // andere SSID zählt als Abbruch

        assert_eq!(count(&m, |k| matches!(k, EventKind::Roam { .. })), 1);
        assert_eq!(count(&m, |k| matches!(k, EventKind::Disconnect { .. })), 2);
        assert_eq!(count(&m, |k| matches!(k, EventKind::Connect { .. })), 1);
        let roam = m.events.iter().find(|e| matches!(e.kind, EventKind::Roam { .. })).unwrap();
        assert_eq!(roam.t, 3.0);
        if let EventKind::Roam { from, to, rssi_from, rssi_to, .. } = roam.kind {
            assert_eq!((from, to, rssi_from, rssi_to), (A, B, -62, -55));
        }
        let s = m.stats(110.0);
        assert_eq!((s.roams, s.disconnects, s.aps_used, s.samples), (1, 2, 2, 4));
        assert!(!s.from_scan);
    }

    #[test]
    fn sticky_client_reported_once() {
        let mut m = Measurement::new("Net".into(), 0.0);
        for t in 1..=30 {
            let t = t as f64;
            m.on_conn(t, Some(&conn("Net", A, -78)));
            if t as u32 % 4 == 0 {
                m.on_scan(t, &[bss("Net", A, -78), bss("Net", B, -60)]);
            }
        }
        assert_eq!(count(&m, |k| matches!(k, EventKind::Sticky { .. })), 1);
        // nach dem Roam zum besseren AP ist der Zustand vorbei
        m.on_conn(31.0, Some(&conn("Net", B, -60)));
        m.on_scan(32.0, &[bss("Net", B, -60)]);
        assert!(!m.is_sticky());
    }

    #[test]
    fn stats_fall_back_to_scan_and_stop_freezes() {
        let mut m = Measurement::new("Net".into(), 0.0);
        m.on_scan(2.0, &[bss("Net", A, -60), bss("Fremd", B, -30)]);
        m.on_scan(4.0, &[bss("Net", A, -80)]);
        m.add_marker(5.0, "  Raum 1.03 ");
        m.stop(6.0);
        m.on_conn(7.0, Some(&conn("Net", A, -50))); // nach Stopp ignoriert
        let s = m.stats(100.0);
        assert!(s.from_scan);
        assert_eq!((s.samples, s.min, s.max), (2, Some(-80), Some(-60)));
        assert_eq!(s.duration, 6.0);
        assert!(m.events.iter().any(|e| matches!(&e.kind, EventKind::Marker(t) if t == "Raum 1.03")));
        assert!(m.to_csv().contains("Messpunkt"));
    }

    #[test]
    fn point_averages_samples_at_the_spot_and_roundtrips() {
        let mut m = Measurement::new("Net".into(), 0.0);
        let ctx = ScanContext { best: Some((B, -50, 36)), aps_visible: 3, ..Default::default() };
        // unterwegs schwach, am Punkt (Dialog ab t=10) stark
        for t in 1..=9 {
            m.on_conn(t as f64, Some(&conn("Net", A, -80)));
        }
        for t in 10..=14 {
            m.on_conn(t as f64, Some(&conn("Net", A, -60)));
        }
        let p = m.add_point(14.0, 10.0, " Büro 1.03 ", "", Some([0.25, 0.5]), &ctx).clone();
        assert_eq!(p.name, "Büro 1.03");
        assert!(!p.from_scan);
        // Werte ab 9 s (1 s Vorlauf): einmal -80, dann 5× -60
        assert_eq!(p.samples, 6);
        assert!((p.rssi.unwrap() - (-63.333)).abs() < 0.01);
        assert_eq!((p.min, p.max, p.aps_visible, p.best_rssi), (Some(-80), Some(-60), 3, Some(-50)));
        assert_eq!(p.quality(), Some(Quality::Good));
        assert_eq!(m.marker_count(), 1);

        m.stop(20.0);
        let back = Measurement::from_file(&m.to_file(20.0)).unwrap();
        assert_eq!(back.points.len(), 1);
        assert_eq!(back.points[0].name, "Büro 1.03");
        assert_eq!(back.conn.len(), m.conn.len());
        assert!(!back.running);
        assert_eq!(back.elapsed(999.0), 20.0);
        assert_eq!(back.points[0].pos, Some([0.25, 0.5]));
        assert_eq!(back.points[0].id, 1);

        m.remove_point(0);
        assert!(m.points.is_empty());
        assert_eq!(m.marker_count(), 0);
    }

    #[test]
    fn csv_columns_in_header_order_and_rename_keeps_marker() {
        crate::i18n::set(crate::i18n::Lang::De);
        let mut m = Measurement::new("Net".into(), 0.0);
        m.on_conn(1.5, Some(&conn("Net", A, -60)));
        let ctx = ScanContext { best: None, aps_visible: 1, ..Default::default() };
        m.add_point(2.0, 1.0, "Raum A", "", None, &ctx);
        let csv = m.to_csv();
        // Zeit (s);Uhrzeit;Typ;... – Dezimalkomma, Typ in Spalte 3
        let row = csv.lines().find(|l| l.contains("Verbindung")).unwrap();
        let cols: Vec<&str> = row.split(';').collect();
        assert_eq!(cols[0], "1,5");
        assert_eq!(cols[1].len(), 8); // hh:mm:ss
        assert_eq!(cols[2], "Verbindung");
        assert_eq!(cols[3], "-60");

        m.points[0].name = "Büro".into();
        m.sync_point_marker(0);
        assert!(m.events.iter().any(|e| matches!(&e.kind, EventKind::Marker(n) if n == "Büro")));
        m.remove_point(0);
        assert_eq!(m.marker_count(), 0);
    }

    #[test]
    fn point_without_connection_uses_scan() {
        crate::i18n::set(crate::i18n::Lang::De);
        let mut m = Measurement::new("Net".into(), 0.0);
        m.on_scan(2.0, &[bss("Net", A, -70)]);
        m.on_scan(4.0, &[bss("Net", A, -72)]);
        let ctx = ScanContext { best: Some((A, -72, 36)), aps_visible: 1, ..Default::default() };
        let p = m.add_point(5.0, 4.5, "", "", None, &ctx).clone();
        assert!(p.from_scan);
        assert_eq!(p.name, "Messpunkt 1");
        assert_eq!(p.rssi, Some(-72.0));
        assert_eq!(p.quality(), Some(Quality::Fair));
    }
}
