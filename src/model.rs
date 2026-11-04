use std::collections::VecDeque;
use std::time::Instant;

use eframe::egui::{ecolor::Hsva, Color32};

use crate::ie::BssInfo;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum Band {
    B24,
    B5,
    B6,
    Other,
}

impl Band {
    pub fn label(&self) -> &'static str {
        match self {
            Band::B24 => "2,4 GHz",
            Band::B5 => "5 GHz",
            Band::B6 => "6 GHz",
            Band::Other => "?",
        }
    }
}

/// Mittenfrequenz (MHz) -> (Kanalnummer, Band)
pub fn freq_to_channel(mhz: u32) -> (u32, Band) {
    match mhz {
        2484 => (14, Band::B24),
        2412..=2483 => ((mhz - 2407) / 5, Band::B24),
        5935 => (2, Band::B6),
        5950..=7125 => ((mhz - 5950) / 5, Band::B6),
        4900..=5949 => ((mhz.saturating_sub(5000)) / 5, Band::B5),
        _ => (0, Band::Other),
    }
}

pub fn is_dfs(ch: u32) -> bool {
    (52..=144).contains(&ch)
}

/// Prefered Scanning Channels im 6-GHz-Band
pub fn is_psc(ch: u32) -> bool {
    ch >= 5 && (ch - 5) % 16 == 0
}

pub fn signal_color(rssi: i32) -> Color32 {
    match rssi {
        r if r >= -55 => Color32::from_rgb(46, 204, 113),
        r if r >= -67 => Color32::from_rgb(163, 217, 0),
        r if r >= -75 => Color32::from_rgb(241, 196, 15),
        r if r >= -82 => Color32::from_rgb(230, 126, 34),
        _ => Color32::from_rgb(231, 76, 60),
    }
}

pub fn signal_text(rssi: i32) -> &'static str {
    match rssi {
        r if r >= -55 => "Exzellent",
        r if r >= -67 => "Gut",
        r if r >= -75 => "Mittel",
        r if r >= -82 => "Schwach",
        _ => "Sehr schwach",
    }
}

pub fn signal_frac(rssi: i32) -> f32 {
    ((rssi as f32 + 100.0) / 70.0).clamp(0.0, 1.0)
}

/// Stabile Farbe pro BSSID
pub fn ap_color(bssid: &[u8; 6]) -> Color32 {
    let mut h: u32 = 2166136261;
    for b in bssid {
        h ^= *b as u32;
        h = h.wrapping_mul(16777619);
    }
    h ^= h >> 16;
    h = h.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 13;
    let hue = (h % 360) as f32 / 360.0;
    Hsva::new(hue, 0.70, 0.95, 1.0).into()
}

pub fn fmt_mac(m: &[u8; 6]) -> String {
    format!(
        "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        m[0], m[1], m[2], m[3], m[4], m[5]
    )
}

pub fn fmt_duration(secs: u64) -> String {
    let d = secs / 86400;
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if d > 0 {
        format!("{d}d {h:02}:{m:02}:{s:02}")
    } else {
        format!("{h:02}:{m:02}:{s:02}")
    }
}

pub fn fmt_ago(secs: f32) -> String {
    if secs < 1.5 {
        "jetzt".into()
    } else if secs < 90.0 {
        format!("vor {:.0} s", secs)
    } else {
        format!("vor {:.0} min", secs / 60.0)
    }
}

#[allow(dead_code)]
pub struct ApRecord {
    pub info: BssInfo,
    pub vendor: String,
    pub vendor_virtual: bool,
    pub first_seen: Instant,
    pub last_seen: Instant,
    pub first_seen_wall: chrono::DateTime<chrono::Local>,
    /// (Sekunden seit Programmstart, RSSI)
    pub history: VecDeque<(f64, f64)>,
    pub rssi_min: i32,
    pub rssi_max: i32,
    rssi_sum: i64,
    pub samples: u64,
}

impl ApRecord {
    pub fn new(info: BssInfo, vendor: (String, bool), now: Instant, t: f64) -> Self {
        let mut history = VecDeque::new();
        history.push_back((t, info.rssi as f64));
        Self {
            rssi_min: info.rssi,
            rssi_max: info.rssi,
            rssi_sum: info.rssi as i64,
            samples: 1,
            info,
            vendor: vendor.0,
            vendor_virtual: vendor.1,
            first_seen: now,
            last_seen: now,
            first_seen_wall: chrono::Local::now(),
            history,
        }
    }

    pub fn update(&mut self, info: BssInfo, now: Instant, t: f64) {
        self.rssi_min = self.rssi_min.min(info.rssi);
        self.rssi_max = self.rssi_max.max(info.rssi);
        self.rssi_sum += info.rssi as i64;
        self.samples += 1;
        self.history.push_back((t, info.rssi as f64));
        while self.history.len() > 4000 {
            self.history.pop_front();
        }
        self.last_seen = now;
        self.info = info;
    }

    pub fn rssi_avg(&self) -> f64 {
        self.rssi_sum as f64 / self.samples.max(1) as f64
    }

    pub fn age(&self, now: Instant) -> f32 {
        now.duration_since(self.last_seen).as_secs_f32()
    }
}
