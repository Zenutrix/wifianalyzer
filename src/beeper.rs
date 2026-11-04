//! Akustischer Signal-Finder ("Geigerzähler"): je stärker das Signal, desto höher und schneller der Ton.

use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use windows_sys::Win32::System::Diagnostics::Debug::Beep;

pub struct Beeper {
    enabled: Arc<AtomicBool>,
    rssi: Arc<AtomicI32>,
}

impl Beeper {
    pub fn new() -> Self {
        let enabled = Arc::new(AtomicBool::new(false));
        let rssi = Arc::new(AtomicI32::new(-100));
        let (e, r) = (enabled.clone(), rssi.clone());
        std::thread::Builder::new()
            .name("beeper".into())
            .spawn(move || loop {
                if e.load(Ordering::Relaxed) {
                    let q = ((r.load(Ordering::Relaxed) as f32 + 95.0) / 60.0).clamp(0.0, 1.0);
                    let freq = 400.0 + q * 1800.0;
                    unsafe {
                        Beep(freq as u32, 45);
                    }
                    std::thread::sleep(Duration::from_millis((1000.0 - q * 900.0) as u64));
                } else {
                    std::thread::sleep(Duration::from_millis(200));
                }
            })
            .ok();
        Self { enabled, rssi }
    }

    pub fn set(&self, on: bool, rssi: i32) {
        self.enabled.store(on, Ordering::Relaxed);
        self.rssi.store(rssi, Ordering::Relaxed);
    }
}
