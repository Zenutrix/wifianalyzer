//! Zugriff auf die Windows Native WiFi API (wlanapi.dll) in einem eigenen Scanner-Thread.

use std::ffi::c_void;
use std::ptr::{null, null_mut};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;
use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::NetworkManagement::WiFi::*;

use crate::ie::{build_bss, BssInfo};

const NOTIFICATION_SOURCE_ACM: u32 = 0x08;
const ACM_SCAN_COMPLETE: u32 = 7;
const ACM_SCAN_FAIL: u32 = 8;
const OPCODE_CURRENT_CONNECTION: i32 = 7;
const OPCODE_CHANNEL_NUMBER: i32 = 8;
const OPCODE_RSSI: i32 = 0x1000_0102;
const BSS_TYPE_ANY: i32 = 3;

pub const ERROR_ACCESS_DENIED: u32 = 5;
/// ERROR_NDIS_DOT11_POWER_STATE_INVALID: Funk ist aus (Flugmodus / WLAN deaktiviert)
pub const ERROR_RADIO_OFF: u32 = 0x8034_2002;
const ERROR_SERVICE_NOT_ACTIVE: u32 = 1062;

static SCAN_DONE: Mutex<u64> = Mutex::new(0);
static SCAN_CV: Condvar = Condvar::new();

unsafe extern "system" fn notification_cb(data: *mut L2_NOTIFICATION_DATA, _ctx: *mut c_void) {
    if data.is_null() {
        return;
    }
    let d = &*data;
    if d.NotificationSource as u32 == NOTIFICATION_SOURCE_ACM
        && (d.NotificationCode == ACM_SCAN_COMPLETE || d.NotificationCode == ACM_SCAN_FAIL)
    {
        let mut c = SCAN_DONE.lock().unwrap_or_else(|e| e.into_inner());
        *c += 1;
        SCAN_CV.notify_all();
    }
}

pub fn guid_to_u128(g: &GUID) -> u128 {
    ((g.data1 as u128) << 96)
        | ((g.data2 as u128) << 80)
        | ((g.data3 as u128) << 64)
        | u64::from_be_bytes(g.data4) as u128
}

fn u128_to_guid(v: u128) -> GUID {
    GUID {
        data1: (v >> 96) as u32,
        data2: (v >> 80) as u16,
        data3: (v >> 64) as u16,
        data4: (v as u64).to_be_bytes(),
    }
}

fn wide_to_string(w: &[u16]) -> String {
    let end = w.iter().position(|c| *c == 0).unwrap_or(w.len());
    String::from_utf16_lossy(&w[..end])
}

#[derive(Clone, Debug)]
pub struct IfaceInfo {
    pub guid: u128,
    pub description: String,
    pub state: String,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct ConnInfo {
    pub profile: String,
    pub ssid: String,
    pub bssid: [u8; 6],
    pub phy: String,
    pub quality: u32,
    pub rx_mbps: f64,
    pub tx_mbps: f64,
    pub auth: String,
    pub cipher: String,
    pub channel: Option<u32>,
    pub rssi: Option<i32>,
}

pub enum ScanCmd {
    SetInterface(u128),
    SetInterval(f32),
    Pause(bool),
    ScanNow,
}

pub struct ScanEvent {
    pub interfaces: Vec<IfaceInfo>,
    pub iface: Option<u128>,
    pub bss: Option<Vec<BssInfo>>,
    pub error: Option<String>,
    pub error_code: u32,
}

pub enum ScanMsg {
    /// Ergebnis eines vollständigen Scans
    Scan(ScanEvent),
    /// Schnelle Abfrage der aktuellen Verbindung (jede Sekunde, auch während Scans/Pause)
    Conn(Option<ConnInfo>),
}

pub struct ScannerHandle {
    pub tx: Sender<ScanCmd>,
    pub rx: Receiver<ScanMsg>,
}

/// Abstand der schnellen Verbindungsabfragen
const CONN_POLL: Duration = Duration::from_secs(1);

fn state_name(s: i32) -> &'static str {
    match s {
        0 => crate::t!("nicht bereit", "not ready"),
        1 => crate::t!("verbunden", "connected"),
        2 => "Ad-hoc",
        3 => crate::t!("trennt", "disconnecting"),
        4 => crate::t!("getrennt", "disconnected"),
        5 => crate::t!("assoziiert", "associating"),
        6 => crate::t!("sucht", "discovering"),
        7 => crate::t!("authentifiziert", "authenticating"),
        _ => "?",
    }
}

fn phy_name(p: i32) -> &'static str {
    match p {
        2 => "802.11 (DSSS)",
        4 => "802.11a",
        5 => "802.11b",
        6 => "802.11g",
        7 => "802.11n",
        8 => "802.11ac",
        9 => "802.11ad",
        10 => "802.11ax",
        11 => "802.11be",
        _ => crate::t!("unbekannt", "unknown"),
    }
}

fn auth_name(a: i32) -> &'static str {
    match a {
        1 => crate::t!("Offen", "Open"),
        2 => "Shared Key",
        3 => "WPA-Enterprise",
        4 => "WPA-PSK",
        6 => "WPA2-Enterprise",
        7 => "WPA2-PSK",
        8 => "WPA3-Enterprise 192",
        9 => "WPA3-SAE",
        10 => "OWE",
        11 => "WPA3-Enterprise",
        _ => "?",
    }
}

fn cipher_name(c: i32) -> &'static str {
    match c {
        0 => crate::t!("keine", "none"),
        1 => "WEP-40",
        2 => "TKIP",
        4 => "CCMP (AES)",
        5 => "WEP-104",
        8 => "GCMP-128",
        9 => "GCMP-256",
        10 => "CCMP-256",
        0x100 => "WEP",
        _ => "?",
    }
}

pub fn error_text(code: u32) -> String {
    match code {
        ERROR_ACCESS_DENIED => crate::t!("Zugriff verweigert – Windows erlaubt den WLAN-Scan nur mit Standortzugriff für Desktop-Apps.", "Access denied – Windows only allows Wi-Fi scanning when desktop apps may access the location.").into(),
        ERROR_SERVICE_NOT_ACTIVE => crate::t!("Der Dienst \"WLAN-AutoKonfig\" (WlanSvc) läuft nicht.", "The \"WLAN AutoConfig\" service (WlanSvc) is not running.").into(),
        1168 => crate::t!("WLAN-Adapter nicht gefunden.", "Wi-Fi adapter not found.").into(),
        ERROR_RADIO_OFF => crate::t!("WLAN ist ausgeschaltet (Flugmodus oder WLAN-Schalter).", "Wi-Fi is turned off (airplane mode or Wi-Fi switch).").into(),
        5023 => crate::t!("WLAN-Adapter ist in einem ungültigen Zustand (ausgeschaltet?).", "Wi-Fi adapter is in an invalid state (switched off?).").into(),
        c => crate::tf!("WLAN-API Fehler {c}", "Wi-Fi API error {c}"),
    }
}

struct Wlan {
    h: HANDLE,
}

impl Drop for Wlan {
    fn drop(&mut self) {
        unsafe {
            WlanCloseHandle(self.h, null());
        }
    }
}

impl Wlan {
    fn open() -> Result<Self, u32> {
        let mut ver = 0u32;
        let mut h: HANDLE = null_mut();
        let r = unsafe { WlanOpenHandle(2, null(), &mut ver, &mut h) };
        if r != 0 {
            return Err(r);
        }
        unsafe {
            WlanRegisterNotification(
                h,
                NOTIFICATION_SOURCE_ACM,
                1,
                Some(notification_cb),
                null(),
                null(),
                null_mut(),
            );
        }
        Ok(Self { h })
    }

    fn interfaces(&self) -> Vec<IfaceInfo> {
        let mut out = Vec::new();
        unsafe {
            let mut p: *mut WLAN_INTERFACE_INFO_LIST = null_mut();
            if WlanEnumInterfaces(self.h, null(), &mut p) != 0 || p.is_null() {
                return out;
            }
            let list = &*p;
            let items = std::slice::from_raw_parts(
                list.InterfaceInfo.as_ptr(),
                list.dwNumberOfItems as usize,
            );
            for i in items {
                out.push(IfaceInfo {
                    guid: guid_to_u128(&i.InterfaceGuid),
                    description: wide_to_string(&i.strInterfaceDescription),
                    state: state_name(i.isState).to_string(),
                });
            }
            WlanFreeMemory(p as *const c_void);
        }
        out
    }

    /// Startet einen Scan und wartet (max. 6 s) auf "fertig". `tick` wird währenddessen
    /// etwa jede Sekunde aufgerufen, damit die Verbindungsabfrage nicht pausiert.
    fn trigger_scan_and_wait(&self, guid: &GUID, mut tick: impl FnMut()) {
        let before = *SCAN_DONE.lock().unwrap_or_else(|e| e.into_inner());
        let r = unsafe { WlanScan(self.h, guid, null(), null(), null()) };
        if r != 0 {
            return;
        }
        let deadline = Instant::now() + Duration::from_secs(6);
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return;
            }
            let g = SCAN_DONE.lock().unwrap_or_else(|e| e.into_inner());
            let (g, _) = SCAN_CV
                .wait_timeout_while(g, left.min(CONN_POLL), |c| *c == before)
                .unwrap_or_else(|e| e.into_inner());
            if *g != before {
                return;
            }
            drop(g);
            tick();
        }
    }

    fn bss_list(&self, guid: &GUID) -> Result<Vec<BssInfo>, u32> {
        let mut out = Vec::new();
        unsafe {
            let mut p: *mut WLAN_BSS_LIST = null_mut();
            let r = WlanGetNetworkBssList(self.h, guid, null(), BSS_TYPE_ANY, 0, null(), &mut p);
            if r != 0 {
                return Err(r);
            }
            if p.is_null() {
                return Ok(out);
            }
            let list = &*p;
            let entries = std::slice::from_raw_parts(
                list.wlanBssEntries.as_ptr(),
                list.dwNumberOfItems as usize,
            );
            for e in entries {
                let base = e as *const WLAN_BSS_ENTRY as *const u8;
                let ies = if e.ulIeSize > 0 {
                    std::slice::from_raw_parts(base.add(e.ulIeOffset as usize), e.ulIeSize as usize)
                } else {
                    &[]
                };
                let ssid_len = (e.dot11Ssid.uSSIDLength as usize).min(32);
                let rate_n = ((e.wlanRateSet.uRateSetLength as usize) / 2).min(126);
                out.push(build_bss(
                    &e.dot11Ssid.ucSSID[..ssid_len],
                    e.dot11Bssid,
                    e.lRssi,
                    e.uLinkQuality,
                    e.ulChCenterFrequency,
                    e.dot11BssPhyType,
                    e.usBeaconPeriod,
                    e.ullTimestamp,
                    e.usCapabilityInformation,
                    &e.wlanRateSet.usRateSet[..rate_n],
                    e.bInRegDomain != 0,
                    ies,
                ));
            }
            WlanFreeMemory(p as *const c_void);
        }
        Ok(out)
    }

    fn query<T: Copy>(&self, guid: &GUID, opcode: i32) -> Option<T> {
        unsafe {
            let mut size = 0u32;
            let mut data: *mut c_void = null_mut();
            let r = WlanQueryInterface(self.h, guid, opcode, null(), &mut size, &mut data, null_mut());
            if r != 0 || data.is_null() {
                return None;
            }
            let v = if size as usize >= std::mem::size_of::<T>() {
                Some(*(data as *const T))
            } else {
                None
            };
            WlanFreeMemory(data);
            v
        }
    }

    fn connection(&self, guid: &GUID) -> Option<ConnInfo> {
        let a: WLAN_CONNECTION_ATTRIBUTES = self.query(guid, OPCODE_CURRENT_CONNECTION)?;
        if a.isState != 1 {
            return None;
        }
        let assoc = &a.wlanAssociationAttributes;
        let sl = (assoc.dot11Ssid.uSSIDLength as usize).min(32);
        Some(ConnInfo {
            profile: wide_to_string(&a.strProfileName),
            ssid: String::from_utf8_lossy(&assoc.dot11Ssid.ucSSID[..sl]).to_string(),
            bssid: assoc.dot11Bssid,
            phy: phy_name(assoc.dot11PhyType).to_string(),
            quality: assoc.wlanSignalQuality,
            rx_mbps: assoc.ulRxRate as f64 / 1000.0,
            tx_mbps: assoc.ulTxRate as f64 / 1000.0,
            auth: auth_name(a.wlanSecurityAttributes.dot11AuthAlgorithm).to_string(),
            cipher: cipher_name(a.wlanSecurityAttributes.dot11CipherAlgorithm).to_string(),
            channel: self.query::<u32>(guid, OPCODE_CHANNEL_NUMBER),
            rssi: self.query::<i32>(guid, OPCODE_RSSI),
        })
    }
}

pub fn spawn(ctx: egui::Context) -> ScannerHandle {
    let (cmd_tx, cmd_rx) = channel::<ScanCmd>();
    let (ev_tx, ev_rx) = channel::<ScanMsg>();

    std::thread::Builder::new()
        .name("wlan-scanner".into())
        .spawn(move || {
            // WLAN-Dienst evtl. (noch) nicht gestartet: Fehler melden und alle 3 s erneut versuchen
            let wlan = loop {
                match Wlan::open() {
                    Ok(w) => break w,
                    Err(code) => {
                        let sent = ev_tx.send(ScanMsg::Scan(ScanEvent {
                            interfaces: vec![],
                            iface: None,
                            bss: None,
                            error: Some(error_text(code)),
                            error_code: code,
                        }));
                        ctx.request_repaint();
                        if sent.is_err() {
                            return;
                        }
                        if let Err(RecvTimeoutError::Disconnected) = cmd_rx.recv_timeout(Duration::from_secs(3)) {
                            return;
                        }
                    }
                }
            };
            let mut iface: Option<u128> = wlan.interfaces().first().map(|i| i.guid);
            let mut interval = Duration::from_secs_f32(4.0);
            let mut paused = false;
            let mut next = Instant::now();
            let mut next_conn = Instant::now();

            // Verbindung abfragen und an die UI schicken; false = UI ist weg
            let send_conn = |iface: Option<u128>| -> bool {
                let conn = iface.and_then(|g| wlan.connection(&u128_to_guid(g)));
                let ok = ev_tx.send(ScanMsg::Conn(conn)).is_ok();
                ctx.request_repaint();
                ok
            };

            loop {
                let now = Instant::now();
                if now >= next_conn {
                    if !send_conn(iface) {
                        break;
                    }
                    next_conn = now + CONN_POLL;
                }
                let until = if paused { next_conn } else { next.min(next_conn) };
                let wait = until.saturating_duration_since(Instant::now());
                match cmd_rx.recv_timeout(wait) {
                    Ok(cmd) => {
                        match cmd {
                            ScanCmd::SetInterface(g) => {
                                iface = Some(g);
                                next = Instant::now();
                            }
                            ScanCmd::SetInterval(s) => interval = Duration::from_secs_f32(s.max(1.0)),
                            ScanCmd::Pause(p) => {
                                paused = p;
                                next = Instant::now();
                            }
                            ScanCmd::ScanNow => {
                                paused = false;
                                next = Instant::now();
                            }
                        }
                        continue;
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
                if paused || Instant::now() < next {
                    continue;
                }

                let interfaces = wlan.interfaces();
                if iface.map_or(true, |g| !interfaces.iter().any(|i| i.guid == g)) {
                    iface = interfaces.first().map(|i| i.guid);
                }
                let mut ev = ScanEvent {
                    interfaces,
                    iface,
                    bss: None,
                    error: None,
                    error_code: 0,
                };
                match iface {
                    None => ev.error = Some(crate::t!("Kein WLAN-Adapter gefunden.", "No Wi-Fi adapter found.").into()),
                    Some(g) => {
                        let guid = u128_to_guid(g);
                        wlan.trigger_scan_and_wait(&guid, || {
                            send_conn(Some(g));
                        });
                        next_conn = Instant::now() + CONN_POLL;
                        match wlan.bss_list(&guid) {
                            Ok(list) => ev.bss = Some(list),
                            Err(code) => {
                                ev.error = Some(error_text(code));
                                ev.error_code = code;
                            }
                        }
                    }
                }
                if ev_tx.send(ScanMsg::Scan(ev)).is_err() {
                    break;
                }
                ctx.request_repaint();
                next = Instant::now() + interval;
            }
        })
        .expect("Scanner-Thread konnte nicht gestartet werden");

    ScannerHandle { tx: cmd_tx, rx: ev_rx }
}
