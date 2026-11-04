//! Dauerhafte Einstellungen unter %APPDATA%\WifiAnalyzer\settings.ini (einfaches key=value).

use std::path::PathBuf;

use crate::i18n::{self, Lang};
use crate::model::fmt_mac;

#[derive(Clone, PartialEq)]
pub struct Settings {
    pub lang: Lang,
    pub dark: bool,
    pub interval: f32,
    pub stale_after: f32,
    pub remove_after: f32,
    pub group_ssid: bool,
    pub show_hidden: bool,
    pub check_updates: bool,
    pub pinned: Vec<[u8; 6]>,
    /// zuletzt gestartete Version (für den Dialog "Was ist neu")
    pub last_version: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            lang: i18n::system_default(),
            dark: true,
            interval: 4.0,
            stale_after: 15.0,
            remove_after: 180.0,
            group_ssid: true,
            show_hidden: true,
            check_updates: true,
            pinned: Vec::new(),
            last_version: String::new(),
        }
    }
}

pub fn app_dir() -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".")).join("WifiAnalyzer")
}

fn path() -> PathBuf {
    app_dir().join("settings.ini")
}

fn parse_mac(s: &str) -> Option<[u8; 6]> {
    let parts: Vec<u8> = s.split(':').filter_map(|p| u8::from_str_radix(p, 16).ok()).collect();
    parts.try_into().ok()
}

impl Settings {
    pub fn load() -> Self {
        let mut s = Self::default();
        let Ok(text) = std::fs::read_to_string(path()) else { return s };
        // Editor/PowerShell schreiben gern eine UTF-8-BOM an den Anfang
        for line in text.trim_start_matches('\u{feff}').lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim());
            let b = v == "1" || v.eq_ignore_ascii_case("true");
            match k {
                "lang" => s.lang = Lang::from_code(v).unwrap_or(s.lang),
                "dark" => s.dark = b,
                "interval" => s.interval = v.parse().unwrap_or(s.interval).clamp(2.0, 60.0),
                "stale_after" => s.stale_after = v.parse().unwrap_or(s.stale_after).clamp(5.0, 300.0),
                "remove_after" => s.remove_after = v.parse().unwrap_or(s.remove_after).clamp(20.0, 3600.0),
                "group_ssid" => s.group_ssid = b,
                "show_hidden" => s.show_hidden = b,
                "check_updates" => s.check_updates = b,
                "last_version" => s.last_version = v.to_string(),
                "pinned" => s.pinned = v.split(',').filter_map(|m| parse_mac(m.trim())).collect(),
                _ => {}
            }
        }
        s
    }

    pub fn save(&self) {
        let pinned: Vec<String> = self.pinned.iter().map(fmt_mac).collect();
        let text = format!(
            "# WiFi Analyzer – Einstellungen / settings\nlang={}\ndark={}\ninterval={}\nstale_after={}\nremove_after={}\ngroup_ssid={}\nshow_hidden={}\ncheck_updates={}\npinned={}\nlast_version={}\n",
            self.lang.code(),
            self.dark as u8,
            self.interval,
            self.stale_after,
            self.remove_after,
            self.group_ssid as u8,
            self.show_hidden as u8,
            self.check_updates as u8,
            pinned.join(","),
            self.last_version
        );
        let _ = std::fs::create_dir_all(app_dir());
        let _ = std::fs::write(path(), text);
    }
}
