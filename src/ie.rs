//! Auswertung der 802.11 Information Elements aus Beacon/Probe-Response.

use crate::model::{freq_to_channel, Band};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Standard {
    B,
    A,
    G,
    N,
    Ac,
    Ax,
    Be,
}

impl Standard {
    pub fn ieee(&self) -> &'static str {
        match self {
            Standard::B => "802.11b",
            Standard::A => "802.11a",
            Standard::G => "802.11g",
            Standard::N => "802.11n",
            Standard::Ac => "802.11ac",
            Standard::Ax => "802.11ax",
            Standard::Be => "802.11be",
        }
    }
    pub fn generation(&self, band: Band) -> &'static str {
        match self {
            Standard::B => "Wi-Fi 1",
            Standard::A => "Wi-Fi 2",
            Standard::G => "Wi-Fi 3",
            Standard::N => "Wi-Fi 4",
            Standard::Ac => "Wi-Fi 5",
            Standard::Ax if band == Band::B6 => "Wi-Fi 6E",
            Standard::Ax => "Wi-Fi 6",
            Standard::Be => "Wi-Fi 7",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum SecLevel {
    Open,
    Weak,
    Ok,
    Strong,
}

/// Verschlüsselungsverfahren (sprachneutral; Text über `label()`)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SecKind {
    Open,
    OpenOweTransition,
    Owe,
    Wep,
    Wpa,
    WpaEnt,
    WpaWpa2,
    Wpa2,
    Wpa2Other,
    Wpa2Ent,
    Wpa2Wpa3,
    Wpa3,
    Wpa3Ent,
    Wpa3Ent192,
}

impl SecKind {
    pub fn label(self) -> String {
        match self {
            SecKind::Open => crate::t!("Offen", "Open").into(),
            SecKind::OpenOweTransition => crate::t!("Offen (OWE-Transition)", "Open (OWE transition)").into(),
            SecKind::Owe => "OWE (Enhanced Open)".into(),
            SecKind::Wep => "WEP".into(),
            SecKind::Wpa => "WPA-Personal".into(),
            SecKind::WpaEnt => "WPA-Enterprise".into(),
            SecKind::WpaWpa2 => "WPA/WPA2-Personal".into(),
            SecKind::Wpa2 => "WPA2-Personal".into(),
            SecKind::Wpa2Other => "WPA2".into(),
            SecKind::Wpa2Ent => "WPA2-Enterprise".into(),
            SecKind::Wpa2Wpa3 => "WPA2/WPA3-Personal".into(),
            SecKind::Wpa3 => "WPA3-Personal".into(),
            SecKind::Wpa3Ent => "WPA3-Enterprise".into(),
            SecKind::Wpa3Ent192 => "WPA3-Enterprise 192".into(),
        }
    }

    pub fn level(self) -> SecLevel {
        match self {
            SecKind::Open | SecKind::OpenOweTransition => SecLevel::Open,
            SecKind::Wep | SecKind::Wpa | SecKind::WpaEnt | SecKind::WpaWpa2 => SecLevel::Weak,
            SecKind::Wpa2 | SecKind::Wpa2Other | SecKind::Wpa2Ent | SecKind::Wpa2Wpa3 => SecLevel::Ok,
            SecKind::Owe | SecKind::Wpa3 | SecKind::Wpa3Ent | SecKind::Wpa3Ent192 => SecLevel::Strong,
        }
    }

    pub fn is_open(self) -> bool {
        matches!(self, SecKind::Open | SecKind::OpenOweTransition)
    }

    /// WPA2 ohne WPA3-Anteil (für den PMF-Hinweis)
    pub fn is_wpa2_only(self) -> bool {
        matches!(self, SecKind::Wpa2 | SecKind::Wpa2Other | SecKind::Wpa2Ent)
    }
}

/// Protected Management Frames (802.11w)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pmf {
    No,
    Optional,
    Required,
}

impl Pmf {
    pub fn label(self) -> &'static str {
        match self {
            Pmf::No => crate::t!("Nein", "No"),
            Pmf::Optional => crate::t!("Optional", "Optional"),
            Pmf::Required => crate::t!("Erforderlich", "Required"),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct WpsInfo {
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub model_number: Option<String>,
    pub device_name: Option<String>,
    pub serial: Option<String>,
    pub configured: Option<bool>,
    pub locked: bool,
}

#[derive(Clone, Debug)]
pub struct RawIe {
    pub id: u8,
    pub ext: Option<u8>,
    pub data: Vec<u8>,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct BssInfo {
    pub ssid: String,
    pub hidden: bool,
    pub bssid: [u8; 6],
    pub rssi: i32,
    pub quality: u32,
    pub freq_mhz: u32,
    pub channel: u32,
    pub band: Band,
    pub width: u32,
    pub width_label: String,
    /// Mittenkanal des gesamten Kanals (für Diagramme)
    pub center: f64,
    pub standard: Standard,
    pub security: String,
    pub sec_kind: SecKind,
    pub sec_level: SecLevel,
    pub akms: Vec<String>,
    pub pairwise: Vec<String>,
    pub group_cipher: String,
    pub pmf: Pmf,
    pub beacon_ms: f64,
    pub dtim: Option<u8>,
    pub country: Option<String>,
    pub stations: Option<u16>,
    pub ch_util: Option<f32>,
    pub admission_cap: Option<u16>,
    pub streams: u8,
    pub max_rate: f64,
    pub rates: Vec<f32>,
    pub basic_rates: Vec<f32>,
    pub bss_color: Option<u8>,
    pub uptime_s: u64,
    pub wps: Option<WpsInfo>,
    pub ap_name: Option<String>,
    pub chipset: Option<String>,
    pub ie_vendors: Vec<String>,
    pub k_rrm: bool,
    pub r_ft: bool,
    pub v_btm: bool,
    pub wmm: bool,
    pub hs20: bool,
    pub mbo: bool,
    pub mesh: bool,
    pub mlo: bool,
    pub owe_transition: bool,
    pub power_constraint: Option<u8>,
    pub tx_power: Option<i8>,
    pub capability: u16,
    pub in_reg_domain: bool,
    pub ies: Vec<RawIe>,
}

impl BssInfo {
    pub fn std_label(&self) -> String {
        format!("{} ({})", self.standard.generation(self.band), &self.standard.ieee()[5..])
    }
}

pub fn ie_name(id: u8, ext: Option<u8>) -> &'static str {
    if let Some(e) = ext {
        return match e {
            35 => "HE Capabilities",
            36 => "HE Operation",
            38 => "MU EDCA Parameter Set",
            39 => "Spatial Reuse Parameter",
            55 => "BSS Color Change",
            59 => "HE 6 GHz Band Capabilities",
            106 => "EHT Operation",
            107 => "Multi-Link (MLO)",
            108 => "EHT Capabilities",
            _ => "Extension",
        };
    }
    match id {
        0 => "SSID",
        1 => "Supported Rates",
        3 => "DS Parameter Set",
        5 => "TIM",
        7 => "Country",
        11 => "BSS Load",
        32 => "Power Constraint",
        35 => "TPC Report",
        42 => "ERP Information",
        45 => "HT Capabilities",
        48 => "RSN",
        50 => "Extended Supported Rates",
        54 => "Mobility Domain (802.11r)",
        59 => "Supported Operating Classes",
        61 => "HT Operation",
        70 => "RM Enabled Capabilities (802.11k)",
        74 => "Overlapping BSS Scan Parameters",
        107 => "Interworking",
        108 => "Advertisement Protocol",
        111 => "Roaming Consortium",
        113 => "Mesh Configuration",
        114 => "Mesh ID",
        127 => "Extended Capabilities",
        133 => "Cisco CCX (AP-Name)",
        191 => "VHT Capabilities",
        192 => "VHT Operation",
        195 => "VHT Transmit Power Envelope",
        201 => "Reduced Neighbor Report",
        221 => "Vendor Specific",
        _ => "",
    }
}

fn clean_str(b: &[u8]) -> Option<String> {
    let s: String = String::from_utf8_lossy(b)
        .chars()
        .filter(|c| !c.is_control())
        .collect();
    let s = s.trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn cipher_name(t: u8) -> String {
    match t {
        1 => "WEP-40",
        2 => "TKIP",
        4 => "CCMP-128 (AES)",
        5 => "WEP-104",
        6 => "BIP-CMAC-128",
        8 => "GCMP-128",
        9 => "GCMP-256",
        10 => "CCMP-256",
        11 => "BIP-GMAC-128",
        12 => "BIP-GMAC-256",
        13 => "BIP-CMAC-256",
        _ => return format!("Typ {t}"),
    }
    .into()
}

fn akm_name(t: u8) -> String {
    match t {
        1 => "802.1X",
        2 => "PSK",
        3 => "FT-802.1X",
        4 => "FT-PSK",
        5 => "802.1X-SHA256",
        6 => "PSK-SHA256",
        8 => "SAE",
        9 => "FT-SAE",
        11 => "802.1X Suite-B",
        12 => "802.1X Suite-B-192",
        13 => "FT-802.1X-SHA384",
        18 => "OWE",
        24 => "SAE-EXT-KEY",
        25 => "FT-SAE-EXT-KEY",
        _ => return format!("AKM {t}"),
    }
    .into()
}

#[derive(Default)]
struct Rsn {
    group: Option<u8>,
    pairwise: Vec<u8>,
    akms: Vec<u8>,
    mfpc: bool,
    mfpr: bool,
}

/// Parst RSN (IE 48) bzw. WPA1 (Vendor-IE) – Aufbau ist identisch, nur OUI unterscheidet sich.
fn parse_rsn(b: &[u8], oui: [u8; 3]) -> Rsn {
    let mut r = Rsn::default();
    let mut i = 2; // Version
    let suite = |i: usize| -> Option<u8> {
        if i + 4 <= b.len() && b[i..i + 3] == oui {
            Some(b[i + 3])
        } else if i + 4 <= b.len() {
            Some(0xFF)
        } else {
            None
        }
    };
    if let Some(g) = suite(i) {
        r.group = Some(g);
        i += 4;
    } else {
        return r;
    }
    if i + 2 > b.len() {
        return r;
    }
    let n = u16::from_le_bytes([b[i], b[i + 1]]) as usize;
    i += 2;
    for _ in 0..n {
        if let Some(s) = suite(i) {
            r.pairwise.push(s);
        }
        i += 4;
    }
    if i + 2 > b.len() {
        return r;
    }
    let n = u16::from_le_bytes([b[i], b[i + 1]]) as usize;
    i += 2;
    for _ in 0..n {
        if let Some(s) = suite(i) {
            r.akms.push(s);
        }
        i += 4;
    }
    if i + 2 <= b.len() {
        let cap = u16::from_le_bytes([b[i], b[i + 1]]);
        r.mfpr = cap & 0x40 != 0;
        r.mfpc = cap & 0x80 != 0;
    }
    r
}

fn parse_wps(b: &[u8]) -> WpsInfo {
    let mut w = WpsInfo::default();
    let mut i = 0;
    while i + 4 <= b.len() {
        let t = u16::from_be_bytes([b[i], b[i + 1]]);
        let l = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        let start = i + 4;
        let end = (start + l).min(b.len());
        let v = &b[start..end];
        match t {
            0x1021 => w.manufacturer = clean_str(v),
            0x1023 => w.model = clean_str(v),
            0x1024 => w.model_number = clean_str(v),
            0x1011 => w.device_name = clean_str(v),
            0x1042 => w.serial = clean_str(v),
            0x1044 if !v.is_empty() => w.configured = Some(v[0] == 2),
            0x1057 if !v.is_empty() => w.locked = v[0] != 0,
            _ => {}
        }
        i = start + l;
    }
    w
}

fn oui_vendor(o: [u8; 3]) -> Option<&'static str> {
    Some(match o {
        [0x00, 0x50, 0xF2] => "Microsoft (WPA/WMM/WPS)",
        [0x50, 0x6F, 0x9A] => "Wi-Fi Alliance",
        [0x00, 0x10, 0x18] => "Broadcom",
        [0x00, 0x90, 0x4C] => "Broadcom (Epigram)",
        [0x00, 0x03, 0x7F] => "Qualcomm Atheros",
        [0x8C, 0xFD, 0xF0] => "Qualcomm",
        [0x00, 0x0C, 0xE7] => "MediaTek",
        [0x00, 0x0C, 0x43] => "Ralink/MediaTek",
        [0x00, 0xE0, 0x4C] => "Realtek",
        [0x00, 0x26, 0x86] => "Quantenna",
        [0x00, 0x17, 0x35] => "Intel",
        [0x00, 0x40, 0x96] => "Cisco",
        [0x00, 0x0B, 0x86] => "Aruba",
        [0x00, 0x0C, 0x42] => "MikroTik",
        [0x00, 0x13, 0x92] => "Ruckus",
        [0x00, 0x17, 0xF2] => "Apple",
        [0x00, 0x15, 0x6D] | [0x00, 0x27, 0x22] => "Ubiquiti",
        [0x00, 0x18, 0x0A] => "Meraki",
        [0x00, 0xA0, 0x57] => "LANCOM",
        [0x00, 0x1A, 0x11] => "Google",
        [0x00, 0x04, 0x0E] => "AVM",
        [0x00, 0x19, 0x77] => "Aerohive/Extreme",
        [0x00, 0x09, 0x0F] => "Fortinet",
        [0x5C, 0x5B, 0x35] => "Mist (Juniper)",
        _ => return None,
    })
}

fn chipset_of(o: [u8; 3]) -> Option<&'static str> {
    Some(match o {
        [0x00, 0x10, 0x18] | [0x00, 0x90, 0x4C] => "Broadcom",
        [0x00, 0x03, 0x7F] | [0x8C, 0xFD, 0xF0] => "Qualcomm",
        [0x00, 0x0C, 0xE7] | [0x00, 0x0C, 0x43] => "MediaTek",
        [0x00, 0xE0, 0x4C] => "Realtek",
        [0x00, 0x26, 0x86] => "Quantenna",
        [0x00, 0x17, 0x35] => "Intel",
        _ => return None,
    })
}

/// Theoretische maximale PHY-Rate in Mbit/s
fn max_phy_rate(std: Standard, width: u32, ss: u8, sgi: bool, legacy_max: f32) -> f64 {
    let ss = ss.clamp(1, 8) as f64;
    let per_ss = match std {
        Standard::Be => match width {
            320 => 2882.4,
            160 => 1441.2,
            80 => 720.6,
            40 => 344.1,
            _ => 172.1,
        },
        Standard::Ax => match width {
            w if w >= 160 => 1201.0,
            80 => 600.5,
            40 => 286.8,
            _ => 143.4,
        },
        Standard::Ac => match (width, sgi) {
            (w, true) if w >= 160 => 866.7,
            (w, false) if w >= 160 => 780.0,
            (80, true) => 433.3,
            (80, false) => 390.0,
            (40, true) => 200.0,
            (40, false) => 180.0,
            (_, true) => 86.7,
            _ => 78.0,
        },
        Standard::N => match (width >= 40, sgi) {
            (true, true) => 150.0,
            (true, false) => 135.0,
            (false, true) => 72.2,
            (false, false) => 65.0,
        },
        _ => return legacy_max as f64,
    };
    (per_ss * ss * 10.0).round() / 10.0
}

#[allow(clippy::too_many_arguments)]
pub fn build_bss(
    ssid_raw: &[u8],
    bssid: [u8; 6],
    rssi: i32,
    quality: u32,
    freq_khz: u32,
    phy_type: i32,
    beacon_period: u16,
    timestamp: u64,
    capability: u16,
    rate_set: &[u16],
    in_reg_domain: bool,
    ie_bytes: &[u8],
) -> BssInfo {
    let freq_mhz = freq_khz / 1000;
    let (mut channel, band) = freq_to_channel(freq_mhz);

    let hidden = ssid_raw.is_empty() || ssid_raw.iter().all(|b| *b == 0);
    let ssid = if hidden {
        String::new()
    } else {
        // manche APs füllen die SSID mit Null-Bytes auf -> abschneiden, Steuerzeichen entfernen
        String::from_utf8_lossy(ssid_raw)
            .trim_end_matches('\0')
            .chars()
            .filter(|c| !c.is_control())
            .collect()
    };

    let mut rates = Vec::new();
    let mut basic_rates = Vec::new();
    for r in rate_set {
        let mbps = (r & 0x7FFF) as f32 * 0.5;
        if mbps > 0.0 {
            rates.push(mbps);
            if r & 0x8000 != 0 {
                basic_rates.push(mbps);
            }
        }
    }
    rates.sort_by(|a, b| a.partial_cmp(b).unwrap());
    rates.dedup();
    basic_rates.sort_by(|a, b| a.partial_cmp(b).unwrap());
    basic_rates.dedup();

    // Elemente sammeln
    let mut ies = Vec::new();
    let mut i = 0usize;
    while i + 2 <= ie_bytes.len() {
        let id = ie_bytes[i];
        let len = ie_bytes[i + 1] as usize;
        if i + 2 + len > ie_bytes.len() {
            break;
        }
        let body = &ie_bytes[i + 2..i + 2 + len];
        if id == 255 && !body.is_empty() {
            ies.push(RawIe { id, ext: Some(body[0]), data: body[1..].to_vec() });
        } else {
            ies.push(RawIe { id, ext: None, data: body.to_vec() });
        }
        i += 2 + len;
    }

    let mut ds_channel = None;
    let mut dtim = None;
    let mut country = None;
    let mut stations = None;
    let mut ch_util = None;
    let mut admission_cap = None;
    let mut power_constraint = None;
    let mut tx_power = None;
    let mut ht_cap: Option<&[u8]> = None;
    let mut ht_op: Option<&[u8]> = None;
    let mut vht_cap: Option<&[u8]> = None;
    let mut vht_op: Option<&[u8]> = None;
    let mut he_cap: Option<&[u8]> = None;
    let mut he_op: Option<&[u8]> = None;
    let mut eht_cap = false;
    let mut eht_op: Option<&[u8]> = None;
    let mut rsn: Option<Rsn> = None;
    let mut wpa: Option<Rsn> = None;
    let mut wps = None;
    let mut ap_name = None;
    let mut chipset = None;
    let mut ie_vendors: Vec<String> = Vec::new();
    let (mut k_rrm, mut r_ft, mut v_btm, mut wmm, mut hs20, mut mbo, mut mesh, mut mlo, mut owe_tr) =
        (false, false, false, false, false, false, false, false, false);

    for ie in &ies {
        let b = ie.data.as_slice();
        match (ie.id, ie.ext) {
            (3, _) if !b.is_empty() => ds_channel = Some(b[0] as u32),
            (5, _) if b.len() >= 2 => dtim = Some(b[1]),
            (7, _) if b.len() >= 2 => {
                let cc = String::from_utf8_lossy(&b[..2]).to_string();
                let env = match b.get(2) {
                    Some(b'I') => crate::t!(" (Innen)", " (indoor)"),
                    Some(b'O') => crate::t!(" (Außen)", " (outdoor)"),
                    _ => "",
                };
                country = Some(format!("{cc}{env}"));
            }
            (11, _) if b.len() >= 5 => {
                stations = Some(u16::from_le_bytes([b[0], b[1]]));
                ch_util = Some(b[2] as f32 / 255.0 * 100.0);
                admission_cap = Some(u16::from_le_bytes([b[3], b[4]]));
            }
            (32, _) if !b.is_empty() => power_constraint = Some(b[0]),
            (35, _) if !b.is_empty() => tx_power = Some(b[0] as i8),
            (45, _) => ht_cap = Some(b),
            (48, _) => rsn = Some(parse_rsn(b, [0x00, 0x0F, 0xAC])),
            (54, _) => r_ft = true,
            (61, _) => ht_op = Some(b),
            (70, _) => k_rrm = true,
            (114, _) => mesh = true,
            (127, _) => {
                if b.len() >= 3 && b[2] & 0x08 != 0 {
                    v_btm = true;
                }
            }
            (133, _) if b.len() >= 26 => {
                let end = b[10..26].iter().position(|c| *c == 0).map(|p| 10 + p).unwrap_or(26);
                ap_name = clean_str(&b[10..end]);
            }
            (191, _) => vht_cap = Some(b),
            (192, _) => vht_op = Some(b),
            (221, _) if b.len() >= 4 => {
                let oui = [b[0], b[1], b[2]];
                if let Some(v) = oui_vendor(oui) {
                    if !ie_vendors.iter().any(|x| x == v) {
                        ie_vendors.push(v.to_string());
                    }
                } else {
                    let s = format!("{:02X}:{:02X}:{:02X}", b[0], b[1], b[2]);
                    if !ie_vendors.contains(&s) {
                        ie_vendors.push(s);
                    }
                }
                if chipset.is_none() {
                    chipset = chipset_of(oui).map(String::from);
                }
                match (oui, b[3]) {
                    ([0x00, 0x50, 0xF2], 1) => wpa = Some(parse_rsn(&b[4..], [0x00, 0x50, 0xF2])),
                    ([0x00, 0x50, 0xF2], 2) => wmm = true,
                    ([0x00, 0x50, 0xF2], 4) => wps = Some(parse_wps(&b[4..])),
                    ([0x50, 0x6F, 0x9A], 0x10) => hs20 = true,
                    ([0x50, 0x6F, 0x9A], 0x16) => mbo = true,
                    ([0x50, 0x6F, 0x9A], 0x1C) => owe_tr = true,
                    ([0x00, 0x0B, 0x86], 1) if b.len() > 5 && b[4] == 3 => {
                        if ap_name.is_none() {
                            ap_name = clean_str(&b[5..]);
                        }
                    }
                    _ => {}
                }
            }
            (255, Some(35)) => he_cap = Some(b),
            (255, Some(36)) => he_op = Some(b),
            (255, Some(106)) => eht_op = Some(b),
            (255, Some(107)) => mlo = true,
            (255, Some(108)) => eht_cap = true,
            _ => {}
        }
    }

    if band == Band::B24 {
        if let Some(c) = ds_channel {
            channel = c;
        }
    }

    // ---------- Standard ----------
    let legacy_max = rates.iter().cloned().fold(0.0f32, f32::max);
    let mut standard = if eht_cap || eht_op.is_some() {
        Standard::Be
    } else if he_cap.is_some() || he_op.is_some() {
        Standard::Ax
    } else if vht_cap.is_some() || vht_op.is_some() {
        Standard::Ac
    } else if ht_cap.is_some() || ht_op.is_some() {
        Standard::N
    } else if band == Band::B24 {
        if legacy_max > 11.0 {
            Standard::G
        } else {
            Standard::B
        }
    } else {
        Standard::A
    };
    let from_phy = match phy_type {
        11 => Some(Standard::Be),
        10 => Some(Standard::Ax),
        8 => Some(Standard::Ac),
        7 => Some(Standard::N),
        _ => None,
    };
    if let Some(p) = from_phy {
        standard = standard.max(p);
    }

    // ---------- Kanalbreite ----------
    let mut width = 20u32;
    let mut width_label = String::new();
    let mut center = channel as f64;
    if let Some(op) = ht_op {
        if op.len() >= 2 {
            let sec = op[1] & 0x03;
            if op[1] & 0x04 != 0 && (sec == 1 || sec == 3) {
                width = 40;
                center = if sec == 1 { channel as f64 + 2.0 } else { channel as f64 - 2.0 };
            }
        }
    }
    if let Some(op) = vht_op {
        if op.len() >= 3 && op[0] >= 1 {
            let (c0, c1) = (op[1] as i32, op[2] as i32);
            match op[0] {
                1 if c1 != 0 && (c1 - c0).abs() == 8 => {
                    width = 160;
                    center = c1 as f64;
                }
                1 if c1 != 0 && (c1 - c0).abs() > 16 => {
                    width = 160;
                    width_label = "80+80 MHz".into();
                    center = c0 as f64;
                }
                1 => {
                    width = 80;
                    center = c0 as f64;
                }
                2 => {
                    width = 160;
                    center = c0 as f64;
                }
                3 => {
                    width = 160;
                    width_label = "80+80 MHz".into();
                    center = c0 as f64;
                }
                _ => {}
            }
        }
    }
    let mut bss_color = None;
    if let Some(op) = he_op {
        if op.len() >= 6 {
            let params = op[0] as u32 | (op[1] as u32) << 8 | (op[2] as u32) << 16;
            bss_color = Some(op[3] & 0x3F);
            let mut off = 6;
            if params & (1 << 14) != 0 {
                off += 3;
            }
            if params & (1 << 15) != 0 {
                off += 1;
            }
            if params & (1 << 17) != 0 && op.len() >= off + 5 {
                let ctrl = op[off + 1] & 0x03;
                let (c0, c1) = (op[off + 2] as f64, op[off + 3] as f64);
                let (w, c) = match ctrl {
                    0 => (20, c0),
                    1 => (40, c0),
                    2 => (80, c0),
                    _ => (160, if c1 != 0.0 { c1 } else { c0 }),
                };
                if w >= width {
                    width = w;
                    center = c;
                    width_label.clear();
                }
            }
        }
    }
    if let Some(op) = eht_op {
        if op.len() >= 8 && op[0] & 0x01 != 0 {
            let ctrl = op[5] & 0x07;
            let (c0, c1) = (op[6] as f64, op[7] as f64);
            let (w, c) = match ctrl {
                0 => (20, c0),
                1 => (40, c0),
                2 => (80, c0),
                3 => (160, c1),
                _ => (320, c1),
            };
            if w > width && c > 0.0 {
                width = w;
                center = c;
                width_label.clear();
            }
        }
    }
    if width_label.is_empty() {
        width_label = format!("{width} MHz");
    }

    // ---------- Spatial Streams / SGI ----------
    let mut streams = 1u8;
    let mut sgi = false;
    if let Some(c) = ht_cap {
        if c.len() >= 7 {
            let n = c[3..7].iter().filter(|b| **b != 0).count() as u8;
            streams = streams.max(n);
            let info = u16::from_le_bytes([c[0], c[1]]);
            sgi = if width >= 40 { info & 0x40 != 0 } else { info & 0x20 != 0 };
        }
    }
    let mcs_map_streams = |map: u16| -> u8 {
        (0..8).filter(|ss| (map >> (2 * ss)) & 0x3 != 0x3).map(|ss| ss + 1).max().unwrap_or(1) as u8
    };
    if let Some(c) = vht_cap {
        if c.len() >= 6 {
            streams = streams.max(mcs_map_streams(u16::from_le_bytes([c[4], c[5]])));
            let info = u32::from_le_bytes([c[0], c[1], c[2], c[3]]);
            sgi = match width {
                w if w >= 160 => info & 0x40 != 0,
                _ => info & 0x20 != 0,
            };
        }
    }
    if let Some(c) = he_cap {
        if c.len() >= 19 {
            streams = streams.max(mcs_map_streams(u16::from_le_bytes([c[17], c[18]])));
        }
    }

    let max_rate = max_phy_rate(standard, width, streams, sgi, legacy_max);

    // ---------- Sicherheit ----------
    let privacy = capability & 0x0010 != 0;
    let mut akms = Vec::new();
    let mut pairwise = Vec::new();
    let mut group_cipher = String::from("-");
    let mut pmf = Pmf::No;
    let (sec_kind, sec_level) = if let Some(r) = &rsn {
        akms = r.akms.iter().map(|a| akm_name(*a)).collect();
        pairwise = r.pairwise.iter().map(|c| cipher_name(*c)).collect();
        group_cipher = r.group.map(cipher_name).unwrap_or_else(|| "-".into());
        pmf = if r.mfpr {
            Pmf::Required
        } else if r.mfpc {
            Pmf::Optional
        } else {
            Pmf::No
        };
        let has = |l: &[u8]| r.akms.iter().any(|a| l.contains(a));
        let sae = has(&[8, 9, 24, 25]);
        let psk = has(&[2, 4, 6]);
        let ent = has(&[1, 3, 5]);
        let suiteb = has(&[11, 12, 13]);
        let owe = has(&[18]);
        let tkip_only = !r.pairwise.is_empty() && r.pairwise.iter().all(|c| *c == 2);
        let kind = if owe {
            SecKind::Owe
        } else if suiteb {
            SecKind::Wpa3Ent192
        } else if sae && psk {
            SecKind::Wpa2Wpa3
        } else if sae {
            SecKind::Wpa3
        } else if ent && r.mfpr {
            SecKind::Wpa3Ent
        } else if ent {
            SecKind::Wpa2Ent
        } else if psk && wpa.is_some() {
            SecKind::WpaWpa2
        } else if psk {
            SecKind::Wpa2
        } else {
            SecKind::Wpa2Other
        };
        let lvl = if tkip_only { SecLevel::Weak } else { kind.level() };
        (kind, lvl)
    } else if let Some(w) = &wpa {
        akms = w.akms.iter().map(|a| match a {
            1 => "802.1X".to_string(),
            2 => "PSK".to_string(),
            o => format!("AKM {o}"),
        }).collect();
        pairwise = w.pairwise.iter().map(|c| cipher_name(*c)).collect();
        group_cipher = w.group.map(cipher_name).unwrap_or_else(|| "-".into());
        let kind = if w.akms.contains(&1) { SecKind::WpaEnt } else { SecKind::Wpa };
        (kind, SecLevel::Weak)
    } else if privacy {
        (SecKind::Wep, SecLevel::Weak)
    } else if owe_tr {
        (SecKind::OpenOweTransition, SecLevel::Open)
    } else {
        (SecKind::Open, SecLevel::Open)
    };
    let security = sec_kind.label();

    BssInfo {
        ssid,
        hidden,
        bssid,
        rssi,
        quality,
        freq_mhz,
        channel,
        band,
        width,
        width_label,
        center,
        standard,
        security,
        sec_kind,
        sec_level,
        akms,
        pairwise,
        group_cipher,
        pmf,
        beacon_ms: beacon_period as f64 * 1.024,
        dtim,
        country,
        stations,
        ch_util,
        admission_cap,
        streams,
        max_rate,
        rates,
        basic_rates,
        bss_color,
        uptime_s: timestamp / 1_000_000,
        wps,
        ap_name,
        chipset,
        ie_vendors,
        k_rrm,
        r_ft,
        v_btm,
        wmm,
        hs20,
        mbo,
        mesh,
        mlo,
        owe_transition: owe_tr,
        power_constraint,
        tx_power,
        capability,
        in_reg_domain,
        ies,
    }
}
