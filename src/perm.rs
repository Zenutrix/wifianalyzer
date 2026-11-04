//! Berechtigungen für den WLAN-Scan.
//!
//! Seit Windows 11 (24H2) liefert die WLAN-API BSSIDs nur noch, wenn die App auf den Standort
//! zugreifen darf. Wir lösen die offizielle Windows-Abfrage aus (WiFiAdapter/Geolocator
//! RequestAccessAsync) und lesen zusätzlich die Datenschutz-Schalter aus der Registry,
//! um dem Benutzer genau sagen zu können, welcher Schalter fehlt.

use std::ptr::null_mut;

use windows::Devices::Geolocation::{GeolocationAccessStatus, Geolocator};
use windows::Devices::WiFi::{WiFiAccessStatus, WiFiAdapter};
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Access {
    Allowed,
    DeniedByUser,
    DeniedBySystem,
    Unknown,
}

#[derive(Clone, Debug)]
pub struct PermStatus {
    /// Ergebnis der Windows-Abfrage
    pub access: Access,
    /// "Standortdienste" (gerätweit, HKLM)
    pub location_device: Option<bool>,
    /// "Apps den Zugriff auf Ihren Standort erlauben" (Benutzer)
    pub location_apps: Option<bool>,
    /// "Desktop-Apps den Zugriff auf Ihren Standort erlauben"
    pub location_desktop: Option<bool>,
}

impl PermStatus {
    /// Alle bekannten Schalter stehen auf "erlaubt"
    pub fn ok(&self) -> bool {
        self.access != Access::DeniedByUser
            && self.access != Access::DeniedBySystem
            && self.location_device != Some(false)
            && self.location_apps != Some(false)
            && self.location_desktop != Some(false)
    }

    /// Liste der Punkte für den Berechtigungs-Dialog: (erfüllt, Text)
    pub fn checklist(&self) -> Vec<(Option<bool>, &'static str)> {
        vec![
            (self.location_device, crate::t!("Standortdienste sind eingeschaltet (Gerät)", "Location services are on (device)")),
            (self.location_apps, crate::t!("Apps dürfen auf den Standort zugreifen", "Apps may access the location")),
            (self.location_desktop, crate::t!("Desktop-Apps dürfen auf den Standort zugreifen", "Desktop apps may access the location")),
            (
                match self.access {
                    Access::Allowed => Some(true),
                    Access::DeniedByUser | Access::DeniedBySystem => Some(false),
                    Access::Unknown => None,
                },
                crate::t!("WiFi Analyzer hat WLAN-/Standortzugriff", "WiFi Analyzer has Wi-Fi/location access"),
            ),
        ]
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn reg_string(root: HKEY, path: &str, value: &str) -> Option<String> {
    let (p, v) = (wide(path), wide(value));
    let mut buf = [0u16; 64];
    let mut size = (buf.len() * 2) as u32;
    let r = unsafe {
        RegGetValueW(root, p.as_ptr(), v.as_ptr(), RRF_RT_REG_SZ, null_mut(), buf.as_mut_ptr().cast(), &mut size)
    };
    if r != 0 {
        return None;
    }
    let n = (size as usize / 2).saturating_sub(1).min(buf.len());
    Some(String::from_utf16_lossy(&buf[..n]))
}

fn consent(root: HKEY, path: &str) -> Option<bool> {
    reg_string(root, path, "Value").map(|v| !v.eq_ignore_ascii_case("Deny"))
}

const LOCATION_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\location";

fn read_registry() -> (Option<bool>, Option<bool>, Option<bool>) {
    (
        consent(HKEY_LOCAL_MACHINE, LOCATION_KEY),
        consent(HKEY_CURRENT_USER, LOCATION_KEY),
        consent(HKEY_CURRENT_USER, &format!(r"{LOCATION_KEY}\NonPackaged")),
    )
}

/// Löst die Windows-Berechtigungsabfrage aus (blockiert, bis der Benutzer geantwortet hat).
fn request_access() -> Access {
    unsafe {
        let _ = RoInitialize(RO_INIT_MULTITHREADED);
    }
    let wifi = WiFiAdapter::RequestAccessAsync().and_then(|op| op.get());
    let access = match wifi {
        Ok(s) if s == WiFiAccessStatus::Allowed => Access::Allowed,
        Ok(s) if s == WiFiAccessStatus::DeniedByUser => Access::DeniedByUser,
        Ok(s) if s == WiFiAccessStatus::DeniedBySystem => Access::DeniedBySystem,
        _ => Access::Unknown,
    };
    if access == Access::Allowed {
        return access;
    }
    // Zusätzlich die Standort-Abfrage (zeigt unter 24H2 den Zustimmungsdialog für Desktop-Apps)
    match Geolocator::RequestAccessAsync().and_then(|op| op.get()) {
        Ok(s) if s == GeolocationAccessStatus::Allowed => Access::Allowed,
        Ok(s) if s == GeolocationAccessStatus::Denied => Access::DeniedByUser,
        _ => access,
    }
}

/// Prüft die Berechtigungen. Mit `request = true` wird die Windows-Abfrage ausgelöst.
pub fn check(request: bool) -> PermStatus {
    let access = if request { request_access() } else { Access::Unknown };
    let (location_device, location_apps, location_desktop) = read_registry();
    PermStatus { access, location_device, location_apps, location_desktop }
}

pub fn open_location_settings() {
    let _ = std::process::Command::new("explorer").arg("ms-settings:privacy-location").spawn();
}

/// Hält eine Standort-Sitzung dauerhaft offen.
///
/// Windows 11 zeigt das Standort-Symbol in der Taskleiste bei jedem einzelnen WLAN-Scan kurz an,
/// was bei Scans alle paar Sekunden zu einem ständig springenden Infobereich führt. Mit einem
/// aktiven Geolocator-Abo gilt die App durchgehend als „Standort in Verwendung“ – das Symbol
/// bleibt ruhig stehen, statt zu blinken. Ohne Standortfreigabe passiert einfach nichts.
pub fn hold_location_session() {
    let _ = std::thread::Builder::new().name("location-hold".into()).spawn(|| {
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }
        let Ok(geo) = Geolocator::new() else { return };
        let _ = geo.SetReportInterval(60_000);
        let _ = geo.SetMovementThreshold(1_000.0);
        let handler = windows::Foundation::TypedEventHandler::<Geolocator, windows::Devices::Geolocation::PositionChangedEventArgs>::new(|_, _| Ok(()));
        if geo.PositionChanged(&handler).is_err() {
            return;
        }
        // Geolocator für die Laufzeit der App am Leben halten
        loop {
            std::thread::park();
        }
    });
}
