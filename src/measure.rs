//! WLAN-Messung ("Walk-Survey"): zeichnet für eine SSID das Signal der Verbindung (1×/s),
//! den jeweils stärksten AP der SSID (pro Scan) sowie Roams, Abbrüche, Sticky-Client-Phasen
//! und frei gesetzte Markierungen auf.

use std::collections::HashSet;

use chrono::{DateTime, Local};

use crate::ie::BssInfo;
use crate::model::fmt_mac;
use crate::wlan::ConnInfo;

/// Ab diesem Abstand (dB) zwischen bestem und verbundenem AP gilt der Client als "klebend"
const STICKY_DB: i32 = 10;
/// ... wenn der Zustand mindestens so lange (s) anhält
const STICKY_SECS: f64 = 15.0;

#[derive(Clone)]
pub struct ConnSample {
    pub t: f64,
    pub rssi: i32,
    pub bssid: [u8; 6],
    pub channel: u32,
    pub tx_mbps: f64,
    pub rx_mbps: f64,
}

#[derive(Clone)]
pub struct BestSample {
    pub t: f64,
    pub rssi: i32,
    pub bssid: [u8; 6],
    pub channel: u32,
}

#[derive(Clone)]
pub enum EventKind {
    Start,
    Stop,
    Roam { from: [u8; 6], to: [u8; 6], rssi_from: i32, rssi_to: i32, ch_from: u32, ch_to: u32 },
    Disconnect { bssid: [u8; 6] },
    Connect { bssid: [u8; 6], rssi: i32 },
    Sticky { conn: [u8; 6], conn_rssi: i32, best: [u8; 6], best_rssi: i32 },
    Marker(String),
}

#[derive(Clone)]
pub struct Event {
    /// Sekunden seit Messbeginn
    pub t: f64,
    pub kind: EventKind,
}

impl Event {
    pub fn title(&self) -> &'static str {
        match self.kind {
            EventKind::Start => "Start",
            EventKind::Stop => "Stopp",
            EventKind::Roam { .. } => "Roam",
            EventKind::Disconnect { .. } => "Abbruch",
            EventKind::Connect { .. } => "Verbunden",
            EventKind::Sticky { .. } => "Sticky Client",
            EventKind::Marker(_) => "Markierung",
        }
    }

    pub fn details(&self) -> String {
        match &self.kind {
            EventKind::Start => "Messung gestartet".into(),
            EventKind::Stop => "Messung beendet".into(),
            EventKind::Roam { from, to, rssi_from, rssi_to, ch_from, ch_to } => format!(
                "{} (Kanal {ch_from}, {rssi_from} dBm) > {} (Kanal {ch_to}, {rssi_to} dBm)",
                fmt_mac(from),
                fmt_mac(to)
            ),
            EventKind::Disconnect { bssid } => format!("Verbindung zu {} verloren", fmt_mac(bssid)),
            EventKind::Connect { bssid, rssi } => format!("Verbunden mit {} ({rssi} dBm)", fmt_mac(bssid)),
            EventKind::Sticky { conn, conn_rssi, best, best_rssi } => format!(
                "bleibt an {} ({conn_rssi} dBm), obwohl {} mit {best_rssi} dBm verfügbar ist",
                fmt_mac(conn),
                fmt_mac(best)
            ),
            EventKind::Marker(text) => text.clone(),
        }
    }
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

    pub fn add_marker(&mut self, now_t: f64, text: &str) {
        let text = text.trim();
        let text = if text.is_empty() { format!("Markierung {}", self.marker_count() + 1) } else { text.to_string() };
        self.events.push(Event { t: self.rel(now_t), kind: EventKind::Marker(text) });
    }

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
        let mut rows: Vec<(f64, String)> = Vec::new();
        for c in &self.conn {
            rows.push((
                c.t,
                format!(
                    "{:.1};{};Verbindung;{};{};{};{:.0};{:.0};",
                    c.t,
                    wall(c.t),
                    c.rssi,
                    fmt_mac(&c.bssid),
                    c.channel,
                    c.tx_mbps,
                    c.rx_mbps
                ),
            ));
        }
        for b in &self.best {
            rows.push((b.t, format!("{:.1};{};Bester AP;{};{};{};;;", b.t, wall(b.t), b.rssi, fmt_mac(&b.bssid), b.channel)));
        }
        for e in &self.events {
            let details = e.details().replace(';', ",");
            rows.push((e.t, format!("{:.1};{};{};;;;;;{}", e.t, wall(e.t), e.title(), details)));
        }
        rows.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut s = format!(
            "\u{feff}# WLAN-Messung SSID \"{}\" gestartet {}\nZeit (s);Uhrzeit;Typ;RSSI (dBm);BSSID;Kanal;Tx (Mbit/s);Rx (Mbit/s);Details\n",
            self.ssid,
            self.started_wall.format("%Y-%m-%d %H:%M:%S")
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
        assert!(m.to_csv().contains("Markierung"));
    }
}
