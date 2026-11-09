//! Netzwerktest pro Messpunkt: Latenz (Ping zum Router und ins Internet) und optional
//! Durchsatz (Download/Upload über HTTPS).
//!
//! Alles über Windows-Bordmittel: ICMP über `iphlpapi`, HTTP über WinHTTP (inkl. Proxy-Erkennung
//! und TLS des Systems) – keine zusätzlichen Bibliotheken.

use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use windows_sys::Win32::NetworkManagement::IpHelper::{GetBestRoute, IcmpCloseHandle, IcmpCreateFile, IcmpSendEcho, ICMP_ECHO_REPLY, MIB_IPFORWARDROW};
use windows_sys::Win32::Networking::WinHttp::*;

/// Ziel im Internet für den Ping (Cloudflare-DNS, antwortet zuverlässig auf ICMP)
pub const INTERNET_HOST: Ipv4Addr = Ipv4Addr::new(1, 1, 1, 1);
/// Server für den Durchsatztest
pub const SPEED_HOST: &str = "speed.cloudflare.com";
const DOWN_BYTES: u64 = 50_000_000;
const UP_BYTES: u32 = 20_000_000;
const DOWN_SECS: f64 = 6.0;
const UP_SECS: f64 = 5.0;

/// Ergebnis eines Ping-Laufs
#[derive(Clone, Default, Debug, Serialize, Deserialize, PartialEq)]
pub struct PingStats {
    /// Ø Antwortzeit in ms (nur beantwortete Pakete)
    pub avg_ms: Option<f64>,
    pub min_ms: Option<f64>,
    pub max_ms: Option<f64>,
    /// mittlere Abweichung aufeinanderfolgender Antwortzeiten (ms)
    pub jitter_ms: Option<f64>,
    /// Anteil verlorener Pakete 0..1
    pub loss: f64,
    pub sent: u32,
}

/// Netzwerkwerte eines Messpunkts
#[derive(Clone, Default, Debug, Serialize, Deserialize, PartialEq)]
pub struct NetTest {
    /// IPv4 des Standard-Gateways (Router)
    #[serde(default)]
    pub gateway: Option<String>,
    #[serde(default)]
    pub gw: Option<PingStats>,
    #[serde(default)]
    pub inet: Option<PingStats>,
    #[serde(default)]
    pub down_mbps: Option<f64>,
    #[serde(default)]
    pub up_mbps: Option<f64>,
    /// Hinweis, falls etwas nicht gemessen werden konnte
    #[serde(default)]
    pub note: Option<String>,
}

impl NetTest {
    pub fn is_empty(&self) -> bool {
        self.gw.is_none() && self.inet.is_none() && self.down_mbps.is_none() && self.up_mbps.is_none()
    }
}

/// Was gemessen werden soll
#[derive(Clone, Copy)]
pub struct NetTestPlan {
    pub ping: bool,
    pub speed: bool,
}

/// Kompletter Test (blockierend, ca. 3 s Ping, mit Speedtest ca. 15 s) – im Hintergrund aufrufen
pub fn run(plan: NetTestPlan) -> NetTest {
    let mut r = NetTest::default();
    let gw = default_gateway();
    r.gateway = gw.map(|g| g.to_string());
    if plan.ping {
        match gw {
            Some(g) => r.gw = Some(ping(g, 6, 1000)),
            None => r.note = Some(crate::t!("Kein Gateway gefunden", "No gateway found").into()),
        }
        r.inet = Some(ping(INTERNET_HOST, 5, 1500));
    }
    if plan.speed {
        match download(SPEED_HOST, DOWN_BYTES, DOWN_SECS) {
            Ok(v) => r.down_mbps = Some(v),
            Err(e) => r.note = Some(crate::tf!("Download-Test: {e}", "Download test: {e}")),
        }
        match upload(SPEED_HOST, UP_BYTES, UP_SECS) {
            Ok(v) => r.up_mbps = Some(v),
            Err(e) => {
                let msg = crate::tf!("Upload-Test: {e}", "Upload test: {e}");
                r.note = Some(match r.note.take() {
                    Some(n) => format!("{n}; {msg}"),
                    None => msg,
                });
            }
        }
    }
    r
}

/// Standard-Gateway der Route ins Internet
pub fn default_gateway() -> Option<Ipv4Addr> {
    let mut row: MIB_IPFORWARDROW = unsafe { std::mem::zeroed() };
    let dest = u32::from_ne_bytes(INTERNET_HOST.octets());
    let rc = unsafe { GetBestRoute(dest, 0, &mut row) };
    if rc != 0 {
        return None;
    }
    let gw = Ipv4Addr::from(row.dwForwardNextHop.to_ne_bytes());
    (!gw.is_unspecified()).then_some(gw)
}

/// ICMP-Echo: `count` Pakete im Abstand von ca. 200 ms
pub fn ping(addr: Ipv4Addr, count: u32, timeout_ms: u32) -> PingStats {
    let mut st = PingStats { sent: count, ..Default::default() };
    let handle = unsafe { IcmpCreateFile() };
    if handle.is_null() || handle as isize == -1 {
        st.loss = 1.0;
        return st;
    }
    let data = [0x57u8; 32];
    let mut buf = vec![0u8; std::mem::size_of::<ICMP_ECHO_REPLY>() + data.len() + 64];
    let dest = u32::from_ne_bytes(addr.octets());
    let mut times: Vec<f64> = Vec::new();
    for i in 0..count {
        let n = unsafe {
            IcmpSendEcho(handle, dest, data.as_ptr().cast(), data.len() as u16, std::ptr::null(), buf.as_mut_ptr().cast(), buf.len() as u32, timeout_ms)
        };
        if n > 0 {
            let reply = unsafe { std::ptr::read_unaligned(buf.as_ptr() as *const ICMP_ECHO_REPLY) };
            if reply.Status == 0 {
                times.push(reply.RoundTripTime as f64);
            }
        }
        if i + 1 < count {
            std::thread::sleep(Duration::from_millis(200));
        }
    }
    unsafe { IcmpCloseHandle(handle) };
    stats_from(&times, count, &mut st);
    st
}

fn stats_from(times: &[f64], count: u32, st: &mut PingStats) {
    st.loss = if count == 0 { 0.0 } else { 1.0 - times.len() as f64 / count as f64 };
    if times.is_empty() {
        return;
    }
    st.avg_ms = Some(times.iter().sum::<f64>() / times.len() as f64);
    st.min_ms = times.iter().copied().reduce(f64::min);
    st.max_ms = times.iter().copied().reduce(f64::max);
    if times.len() > 1 {
        let j: f64 = times.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f64>() / (times.len() - 1) as f64;
        st.jitter_ms = Some(j);
    }
}

// ------------------------------------------------------------------ WinHTTP

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// WinHTTP-Handle, das beim Verlassen geschlossen wird
struct H(*mut core::ffi::c_void);
impl Drop for H {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { WinHttpCloseHandle(self.0) };
        }
    }
}

fn last_error() -> String {
    let code = std::io::Error::last_os_error();
    crate::tf!("Verbindung fehlgeschlagen ({code})", "Connection failed ({code})")
}

/// Session + Verbindung + Anfrage öffnen
fn open_request(host: &str, verb: &str, path: &str) -> Result<(H, H, H), String> {
    let agent = wide(concat!("WiFiAnalyzer/", env!("CARGO_PKG_VERSION")));
    let session = H(unsafe { WinHttpOpen(agent.as_ptr(), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, std::ptr::null(), std::ptr::null(), 0) });
    if session.0.is_null() {
        return Err(last_error());
    }
    unsafe { WinHttpSetTimeouts(session.0, 8000, 8000, 15000, 15000) };
    let h = wide(host);
    let conn = H(unsafe { WinHttpConnect(session.0, h.as_ptr(), 443, 0) });
    if conn.0.is_null() {
        return Err(last_error());
    }
    let (v, p) = (wide(verb), wide(path));
    let req = H(unsafe { WinHttpOpenRequest(conn.0, v.as_ptr(), p.as_ptr(), std::ptr::null(), std::ptr::null(), std::ptr::null(), WINHTTP_FLAG_SECURE) });
    if req.0.is_null() {
        return Err(last_error());
    }
    Ok((session, conn, req))
}

fn status_code(req: &H) -> u32 {
    let mut code: u32 = 0;
    let mut len = std::mem::size_of::<u32>() as u32;
    unsafe {
        WinHttpQueryHeaders(req.0, WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER, std::ptr::null(), (&mut code as *mut u32).cast(), &mut len, std::ptr::null_mut());
    }
    code
}

/// Download-Durchsatz in Mbit/s (bricht nach `max_secs` ab)
pub fn download(host: &str, bytes: u64, max_secs: f64) -> Result<f64, String> {
    let (_s, _c, req) = open_request(host, "GET", &format!("/__down?bytes={bytes}"))?;
    if unsafe { WinHttpSendRequest(req.0, std::ptr::null(), 0, std::ptr::null(), 0, 0, 0) } == 0 || unsafe { WinHttpReceiveResponse(req.0, std::ptr::null_mut()) } == 0 {
        return Err(last_error());
    }
    let code = status_code(&req);
    if code != 200 {
        return Err(format!("HTTP {code}"));
    }
    let mut buf = vec![0u8; 256 * 1024];
    let mut total: u64 = 0;
    // erste Daten abwarten, ab dann messen (Verbindungsaufbau zählt nicht zum Durchsatz)
    let mut start: Option<Instant> = None;
    loop {
        let mut read: u32 = 0;
        if unsafe { WinHttpReadData(req.0, buf.as_mut_ptr().cast(), buf.len() as u32, &mut read) } == 0 {
            if total == 0 {
                return Err(last_error());
            }
            break;
        }
        if read == 0 {
            break;
        }
        let t0 = *start.get_or_insert_with(Instant::now);
        total += read as u64;
        if t0.elapsed().as_secs_f64() >= max_secs {
            break;
        }
    }
    let secs = start.map(|s| s.elapsed().as_secs_f64()).unwrap_or(0.0);
    if secs < 0.2 || total < 100_000 {
        return Err(crate::t!("zu wenig Daten", "too little data").into());
    }
    Ok(total as f64 * 8.0 / secs / 1e6)
}

/// Upload-Durchsatz in Mbit/s (bricht nach `max_secs` ab)
pub fn upload(host: &str, bytes: u32, max_secs: f64) -> Result<f64, String> {
    let (_s, _c, req) = open_request(host, "POST", "/__up")?;
    let headers = wide("Content-Type: application/octet-stream\r\n");
    if unsafe { WinHttpSendRequest(req.0, headers.as_ptr(), u32::MAX, std::ptr::null(), 0, bytes, 0) } == 0 {
        return Err(last_error());
    }
    let chunk = vec![0x5Au8; 128 * 1024];
    let mut total: u64 = 0;
    let start = Instant::now();
    while total < bytes as u64 {
        let n = (bytes as u64 - total).min(chunk.len() as u64) as u32;
        let mut written: u32 = 0;
        if unsafe { WinHttpWriteData(req.0, chunk.as_ptr().cast(), n, &mut written) } == 0 {
            if total == 0 {
                return Err(last_error());
            }
            break;
        }
        total += written as u64;
        if start.elapsed().as_secs_f64() >= max_secs {
            break;
        }
    }
    let secs = start.elapsed().as_secs_f64();
    if total as u32 >= bytes {
        // vollständig gesendet: Antwort abholen (sauberer Abschluss)
        unsafe { WinHttpReceiveResponse(req.0, std::ptr::null_mut()) };
    }
    if secs < 0.2 || total < 100_000 {
        return Err(crate::t!("zu wenig Daten", "too little data").into());
    }
    Ok(total as f64 * 8.0 / secs / 1e6)
}

/// Werte für den Demo-Modus, passend zur Signalstärke
pub fn demo(rssi: i32, tx_mbps: f64, plan: NetTestPlan, seed: u64) -> NetTest {
    let n = |k: u64| ((seed.wrapping_mul(2654435761).wrapping_add(k * 97) % 1000) as f64) / 1000.0;
    let weak = ((-rssi - 45).max(0)) as f64;
    let mut r = NetTest { gateway: Some("192.168.10.1".into()), ..Default::default() };
    if plan.ping {
        let gw = 1.5 + weak * 0.35 + n(1) * 1.5;
        r.gw = Some(PingStats { avg_ms: Some(gw), min_ms: Some(gw * 0.7), max_ms: Some(gw * 1.8), jitter_ms: Some(0.4 + weak * 0.08), loss: if rssi < -78 { 0.17 } else { 0.0 }, sent: 6 });
        let inet = 9.0 + gw + n(2) * 3.0;
        r.inet = Some(PingStats { avg_ms: Some(inet), min_ms: Some(inet - 2.0), max_ms: Some(inet + 6.0), jitter_ms: Some(1.1 + weak * 0.1), loss: 0.0, sent: 5 });
    }
    if plan.speed {
        let base = if tx_mbps > 0.0 { tx_mbps } else { 400.0 };
        r.down_mbps = Some((base * (0.48 + n(3) * 0.08)).min(940.0));
        r.up_mbps = Some((base * (0.36 + n(4) * 0.06)).min(900.0));
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_statistics() {
        let mut st = PingStats::default();
        stats_from(&[2.0, 4.0, 3.0], 4, &mut st);
        assert_eq!(st.avg_ms, Some(3.0));
        assert_eq!((st.min_ms, st.max_ms), (Some(2.0), Some(4.0)));
        assert_eq!(st.jitter_ms, Some(1.5));
        assert!((st.loss - 0.25).abs() < 1e-9);

        let mut none = PingStats::default();
        stats_from(&[], 5, &mut none);
        assert_eq!((none.avg_ms, none.loss), (None, 1.0));
    }

    #[test]
    fn demo_values_scale_with_signal() {
        let plan = NetTestPlan { ping: true, speed: true };
        let good = demo(-45, 1201.0, plan, 1);
        let weak = demo(-80, 144.0, plan, 1);
        assert!(good.gw.unwrap().avg_ms.unwrap() < weak.gw.unwrap().avg_ms.unwrap());
        assert!(good.down_mbps.unwrap() > weak.down_mbps.unwrap());
    }
}
