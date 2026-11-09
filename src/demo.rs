//! Demo-Modus (`wifi-analyzer.exe --demo`): erfundene WLAN-Umgebung statt echter Scans.
//!
//! Gedacht für Präsentationen, Screenshots (Website, Handbuch) und die Entwicklung ohne WLAN.
//! Alle SSIDs sind frei erfunden; die Hersteller ergeben sich aus echten OUI-Präfixen.
//! Die Signale schwanken realistisch, zwei Office-APs simulieren einen Rundgang mit Roaming.

use std::sync::mpsc::{channel, RecvTimeoutError};
use std::time::{Duration, Instant};

use eframe::egui;

use crate::ie::{build_bss, BssInfo};
use crate::wlan::{ConnInfo, IfaceInfo, ScanCmd, ScanEvent, ScanMsg, ScannerHandle};

/// Demo: Signalstärke an einer Stelle im Grundriss (0..1), zwei virtuelle Access Points
pub fn rssi_at(pos: [f32; 2]) -> i32 {
    const APS: [([f32; 2], f32); 2] = [([0.24, 0.30], -36.0), ([0.70, 0.28], -38.0)];
    APS.iter()
        .map(|(ap, tx)| {
            let (dx, dy) = ((pos[0] - ap[0]) * 1.6, pos[1] - ap[1]);
            let d = (dx * dx + dy * dy).sqrt();
            // Freiraumdämpfung + Wände (grob)
            (tx - 40.0 * (1.0 + d * 9.0).log10() - if pos[1] > 0.6 { 8.0 } else { 0.0 }) as i32
        })
        .max()
        .unwrap_or(-80)
        .clamp(-92, -30)
}

/// Demo: Signal je Band – 2,4 GHz reicht weiter, 6 GHz dämpft am stärksten
pub fn bands_at(pos: [f32; 2]) -> crate::measure::BandValues {
    let base = rssi_at(pos);
    let walls = if pos[1] > 0.6 { 1 } else { 0 };
    crate::measure::BandValues { b24: Some((base + 6).min(-30)), b5: Some(base), b6: Some(base - 7 - 4 * walls).filter(|v| *v > -95) }
}

pub fn enabled() -> bool {
    std::env::args().any(|a| a == "--demo") || std::env::var_os("WIFIANALYZER_DEMO").is_some()
}

const IFACE: u128 = 0xD3_40D3_40D3_40;

#[derive(Clone, Copy)]
enum Sec {
    Open,
    Owe,
    Wep,
    WpaTkip,
    Wpa2,
    Wpa2Wpa3,
    Wpa3,
    Enterprise,
}

struct Net {
    ssid: &'static str,
    bssid: [u8; 6],
    ch: u32,
    /// 2 = 2,4 GHz, 5 = 5 GHz, 6 = 6 GHz
    band: u8,
    base: f64,
    sec: Sec,
    /// Wi-Fi 4/5/6/7
    gen: u8,
    width80: bool,
    stations: u16,
    util: f64,
    krv: bool,
}

fn mac(oui: [u8; 3], a: u8, b: u8, c: u8) -> [u8; 6] {
    [oui[0], oui[1], oui[2], a, b, c]
}

// echte Hersteller-Präfixe (OUI), damit die Herstelleranzeige etwas zeigt
const UBNT: [u8; 3] = [0x24, 0x5A, 0x4C];
const AVM: [u8; 3] = [0x3C, 0xA6, 0x2F];
const TPLINK: [u8; 3] = [0x50, 0xC7, 0xBF];
const ARUBA: [u8; 3] = [0x00, 0x0B, 0x86];
const LANCOM: [u8; 3] = [0x00, 0xA0, 0x57];

fn networks() -> Vec<Net> {
    use Sec::*;
    let n = |ssid, bssid, ch, band, base, sec, gen, width80, stations, util, krv| Net { ssid, bssid, ch, band, base, sec, gen, width80, stations, util, krv };
    vec![
        // Firmennetz: 3 APs, je 2,4 + 5 GHz, ein AP zusätzlich 6 GHz
        n("Musterfirma", mac(UBNT, 0x10, 0x20, 0x01), 1, 2, -52.0, Wpa2Wpa3, 6, false, 6, 34.0, true),
        n("Musterfirma", mac(UBNT, 0x10, 0x20, 0x02), 36, 5, -46.0, Wpa2Wpa3, 6, true, 11, 22.0, true),
        n("Musterfirma", mac(UBNT, 0x10, 0x20, 0x03), 37, 6, -58.0, Wpa3, 7, true, 2, 4.0, true),
        n("Musterfirma", mac(UBNT, 0x10, 0x31, 0x01), 6, 2, -63.0, Wpa2Wpa3, 6, false, 4, 41.0, true),
        n("Musterfirma", mac(UBNT, 0x10, 0x31, 0x02), 52, 5, -60.0, Wpa2Wpa3, 6, true, 7, 18.0, true),
        n("Musterfirma", mac(UBNT, 0x10, 0x42, 0x01), 11, 2, -74.0, Wpa2Wpa3, 6, false, 3, 29.0, true),
        n("Musterfirma", mac(UBNT, 0x10, 0x42, 0x02), 100, 5, -71.0, Wpa2Wpa3, 6, true, 5, 12.0, true),
        // Gäste
        n("Musterfirma-Gast", mac(UBNT, 0x12, 0x20, 0x01), 1, 2, -53.0, Owe, 6, false, 3, 34.0, false),
        n("Musterfirma-Gast", mac(UBNT, 0x12, 0x20, 0x02), 36, 5, -47.0, Owe, 6, true, 5, 22.0, false),
        n("Musterfirma-Mitarbeiter", mac(ARUBA, 0x5C, 0x11, 0x40), 44, 5, -66.0, Enterprise, 6, true, 9, 27.0, true),
        n("Musterfirma-Mitarbeiter", mac(ARUBA, 0x5C, 0x11, 0x50), 6, 2, -70.0, Enterprise, 6, false, 6, 48.0, true),
        // Nachbarn
        n("Praxis am Markt", mac(LANCOM, 0x7A, 0x01, 0x10), 6, 2, -72.0, Wpa2, 5, false, 8, 52.0, false),
        n("Praxis am Markt", mac(LANCOM, 0x7A, 0x01, 0x11), 116, 5, -78.0, Wpa2, 5, true, 4, 9.0, false),
        n("Cafe Sonnenschein", mac(AVM, 0x8E, 0x41, 0x02), 1, 2, -76.0, Wpa2, 5, false, 12, 61.0, false),
        n("Cafe Sonnenschein", mac(AVM, 0x8E, 0x41, 0x03), 60, 5, -83.0, Wpa2, 6, true, 3, 14.0, false),
        n("WLAN-Wohnung-3", mac(AVM, 0x44, 0x02, 0x9A), 11, 2, -81.0, Wpa2, 5, false, 2, 23.0, false),
        n("Werkstatt", mac(TPLINK, 0x9F, 0x12, 0x30), 3, 2, -79.0, WpaTkip, 4, false, 1, 44.0, false),
        n("Lager-Alt", mac(TPLINK, 0x9F, 0x12, 0x31), 9, 2, -84.0, Wep, 4, false, 1, 17.0, false),
        n("DIRECT-4F-Drucker Empfang", mac([0x9E, 0x5A, 0x31], 0x22, 0x10, 0x01), 6, 2, -61.0, Wpa2, 4, false, 0, 41.0, false),
        n("Besucher-Hotspot", mac(LANCOM, 0x7A, 0x05, 0x20), 48, 5, -80.0, Open, 5, true, 6, 16.0, false),
        n("", mac(UBNT, 0x16, 0x20, 0x02), 36, 5, -47.0, Wpa2, 6, true, 2, 22.0, false),
        n("", mac(AVM, 0x8E, 0x41, 0x05), 1, 2, -77.0, Wpa2, 5, false, 1, 61.0, false),
    ]
}

/// Einfache Pseudozufallszahl (-1..1), deterministisch pro AP und Zeit
fn noise(seed: u64) -> f64 {
    let mut x = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51afd7ed558ccd);
    x ^= x >> 33;
    (x % 2000) as f64 / 1000.0 - 1.0
}

fn freq_khz(band: u8, ch: u32) -> u32 {
    let mhz = match band {
        2 => 2407 + 5 * ch,
        5 => 5000 + 5 * ch,
        _ => 5950 + 5 * ch,
    };
    mhz * 1000
}

fn push_ie(v: &mut Vec<u8>, id: u8, body: &[u8]) {
    v.push(id);
    v.push(body.len() as u8);
    v.extend_from_slice(body);
}

/// Information Elements wie im echten Beacon
fn ies(n: &Net) -> (Vec<u8>, u16) {
    let mut v = Vec::new();
    let mut cap: u16 = 0x0001 | 0x0400; // ESS, Short Slot
    if n.band == 2 {
        push_ie(&mut v, 3, &[n.ch as u8]);
    }
    push_ie(&mut v, 5, &[0, 1, 0, 0]);
    push_ie(&mut v, 7, &[b'D', b'E', b' ', 1, 13, 20]);
    let util = (n.util / 100.0 * 255.0) as u8;
    push_ie(&mut v, 11, &[(n.stations & 0xFF) as u8, (n.stations >> 8) as u8, util, 0, 0]);

    // Sicherheit
    let suite = |t: u8| [0x00, 0x0F, 0xAC, t];
    let rsn = |akms: &[u8], tkip: bool, caps: u8| {
        let mut b = vec![1, 0];
        b.extend_from_slice(&suite(if tkip { 2 } else { 4 }));
        b.extend_from_slice(&[1, 0]);
        b.extend_from_slice(&suite(if tkip { 2 } else { 4 }));
        b.extend_from_slice(&[akms.len() as u8, 0]);
        for a in akms {
            b.extend_from_slice(&suite(*a));
        }
        b.extend_from_slice(&[caps, 0]);
        b
    };
    match n.sec {
        Sec::Open => {}
        Sec::Owe => {
            cap |= 0x10;
            push_ie(&mut v, 48, &rsn(&[18], false, 0xC0));
        }
        Sec::Wep => cap |= 0x10,
        Sec::WpaTkip => {
            cap |= 0x10;
            push_ie(&mut v, 48, &rsn(&[2], true, 0x00));
        }
        Sec::Wpa2 => {
            cap |= 0x10;
            push_ie(&mut v, 48, &rsn(&[2], false, 0x00));
        }
        Sec::Wpa2Wpa3 => {
            cap |= 0x10;
            push_ie(&mut v, 48, &rsn(&[2, 8], false, 0x80));
        }
        Sec::Wpa3 => {
            cap |= 0x10;
            push_ie(&mut v, 48, &rsn(&[8], false, 0xC0));
        }
        Sec::Enterprise => {
            cap |= 0x10;
            push_ie(&mut v, 48, &rsn(&[1, 3], false, 0x80));
        }
    }

    // HT (Wi-Fi 4): 2 Streams
    let mut ht = [0u8; 26];
    ht[0] = 0x6F;
    ht[3] = 0xFF;
    ht[4] = 0xFF;
    push_ie(&mut v, 45, &ht);
    let mut htop = [0u8; 22];
    htop[0] = n.ch as u8;
    if n.band == 5 && n.width80 {
        htop[1] = 0x05;
    }
    push_ie(&mut v, 61, &htop);
    if n.krv {
        push_ie(&mut v, 70, &[0x73, 0x00, 0x00, 0x00, 0x00]);
        push_ie(&mut v, 54, &[0x4D, 0x46, 0x01]);
    }
    push_ie(&mut v, 127, &[0x04, 0x00, if n.krv { 0x08 } else { 0x00 }, 0x00, 0x00, 0x00, 0x00, 0x40]);
    // VHT (Wi-Fi 5) auf 5 GHz
    if n.band == 5 && n.gen >= 5 {
        push_ie(&mut v, 191, &[0x92, 0x01, 0x80, 0x33, 0xFA, 0xFF, 0x00, 0x00, 0xFA, 0xFF, 0x00, 0x00]);
        let center = match n.ch {
            36..=48 => 42,
            52..=64 => 58,
            100..=112 => 106,
            116..=128 => 122,
            _ => n.ch,
        } as u8;
        push_ie(&mut v, 192, &[if n.width80 { 1 } else { 0 }, center, 0, 0xFC, 0xFF]);
    }
    // HE (Wi-Fi 6) / EHT (Wi-Fi 7)
    if n.gen >= 6 {
        let mut he = vec![35];
        he.extend_from_slice(&[0x01, 0x08, 0x00, 0x12, 0x00, 0x00, 0x20, 0x0E, 0x09, 0x00, 0x00, 0x00, 0xFA, 0xFF, 0xFA, 0xFF]);
        push_ie(&mut v, 255, &he);
        let mut heop = vec![36, 0xF4, 0x3F, 0x00, 0x00, 0xFC, 0xFF];
        if n.band == 6 {
            // 6-GHz-Operation-Info: Kanal, Breite 80 MHz, Mittenfrequenz
            heop[2] |= 0x02;
            heop.extend_from_slice(&[n.ch as u8, 0x02, 39, 0, 0]);
        }
        push_ie(&mut v, 255, &heop);
    }
    if n.gen >= 7 {
        push_ie(&mut v, 255, &[108, 0x00, 0x00, 0x00]);
        push_ie(&mut v, 255, &[106, 0x00, 0x00, 0x00, 0x00, 0x00]);
    }
    // WMM
    push_ie(&mut v, 221, &[0x00, 0x50, 0xF2, 0x02, 0x01, 0x01, 0x00]);
    (v, cap)
}

/// Signalstärke zum Zeitpunkt t (s). Die zwei Haupt-APs der Musterfirma tauschen langsam die Rollen,
/// wie bei einem Rundgang durchs Büro.
fn rssi(i: usize, n: &Net, t: f64, tick: u64) -> i32 {
    let walk = (t / 45.0).sin();
    let mut v = n.base + 3.0 * (t / (6.0 + i as f64) + i as f64).sin() + 1.5 * noise(tick * 131 + i as u64);
    if n.ssid == "Musterfirma" && n.bssid[4] == 0x20 {
        v -= 9.0 + 9.0 * walk;
    } else if n.ssid == "Musterfirma" && n.bssid[4] == 0x31 {
        v += 9.0 + 9.0 * walk;
    }
    v.round().clamp(-95.0, -25.0) as i32
}

fn scan(nets: &[Net], t: f64, tick: u64) -> Vec<BssInfo> {
    nets.iter()
        .enumerate()
        .map(|(i, n)| {
            let (ie, cap) = ies(n);
            let r = rssi(i, n, t, tick);
            let quality = ((r + 100) * 2).clamp(0, 100) as u32;
            build_bss(n.ssid.as_bytes(), n.bssid, r, quality, freq_khz(n.band, n.ch), 0, 100, (t * 1e6) as u64 + 86_400_000_000 * (3 + i as u64), cap, &[], true, &ie)
        })
        .collect()
}

/// Verbindung des Demo-Laptops: Musterfirma, 5 GHz, roamt zwischen den beiden Haupt-APs
struct Roamer {
    current: usize,
}

impl Roamer {
    fn conn(&mut self, nets: &[Net], t: f64, tick: u64) -> ConnInfo {
        let candidates: Vec<usize> = nets.iter().enumerate().filter(|(_, n)| n.ssid == "Musterfirma" && n.band == 5 && n.bssid[4] != 0x42).map(|(i, _)| i).collect();
        let cur_rssi = rssi(self.current, &nets[self.current], t, tick);
        if let Some(&best) = candidates.iter().max_by_key(|&&i| rssi(i, &nets[i], t, tick)) {
            if best != self.current && rssi(best, &nets[best], t, tick) - cur_rssi >= 8 {
                self.current = best;
            }
        }
        let n = &nets[self.current];
        let r = rssi(self.current, n, t, tick);
        let rate = match r {
            r if r >= -50 => 1201.0,
            r if r >= -58 => 864.0,
            r if r >= -66 => 576.0,
            r if r >= -72 => 288.0,
            _ => 144.0,
        };
        ConnInfo {
            profile: n.ssid.into(),
            ssid: n.ssid.into(),
            bssid: n.bssid,
            phy: "802.11ax".into(),
            quality: ((r + 100) * 2).clamp(0, 100) as u32,
            rx_mbps: rate,
            tx_mbps: rate,
            auth: "WPA3-SAE".into(),
            cipher: "CCMP".into(),
            channel: Some(n.ch),
            rssi: Some(r),
        }
    }
}

pub fn spawn(ctx: egui::Context) -> ScannerHandle {
    let (cmd_tx, cmd_rx) = channel::<ScanCmd>();
    let (ev_tx, ev_rx) = channel::<ScanMsg>();
    std::thread::Builder::new()
        .name("demo-scanner".into())
        .spawn(move || {
            let nets = networks();
            let start = Instant::now();
            let iface = IfaceInfo { guid: IFACE, description: "Intel(R) Wi-Fi 7 BE200 320MHz (Demo)".into(), state: "connected".into() };
            let mut roamer = Roamer { current: 1 };
            let mut interval = Duration::from_secs(4);
            let mut paused = false;
            let mut next = Instant::now() + Duration::from_millis(600);
            let mut next_conn = Instant::now();
            loop {
                let t = start.elapsed().as_secs_f64();
                let tick = t as u64;
                if Instant::now() >= next_conn {
                    if ev_tx.send(ScanMsg::Conn(Some(roamer.conn(&nets, t, tick)))).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                    next_conn = Instant::now() + Duration::from_secs(1);
                }
                if !paused && Instant::now() >= next {
                    let ev = ScanEvent { interfaces: vec![iface.clone()], iface: Some(IFACE), bss: Some(scan(&nets, t, tick)), error: None, error_code: 0 };
                    if ev_tx.send(ScanMsg::Scan(ev)).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                    next = Instant::now() + interval;
                }
                let until = if paused { next_conn } else { next.min(next_conn) };
                match cmd_rx.recv_timeout(until.saturating_duration_since(Instant::now())) {
                    Ok(ScanCmd::SetInterval(s)) => interval = Duration::from_secs_f32(s.max(1.0)),
                    Ok(ScanCmd::Pause(p)) => paused = p,
                    Ok(ScanCmd::ScanNow) => {
                        paused = false;
                        next = Instant::now();
                    }
                    Ok(ScanCmd::SetInterface(_)) => {}
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
        })
        .expect("Demo-Thread konnte nicht gestartet werden");
    ScannerHandle { tx: cmd_tx, rx: ev_rx }
}
