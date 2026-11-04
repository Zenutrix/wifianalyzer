//! Zweisprachige Oberfläche (Deutsch / Englisch).
//!
//! Texte stehen direkt im Code nebeneinander:
//! `t!("Netzwerke", "Networks")` bzw. `tf!("{n} Netze", "{n} networks")` für Formatstrings.

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    De,
    En,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::De => "de",
            Lang::En => "en",
        }
    }
    pub fn from_code(s: &str) -> Option<Self> {
        match s {
            "de" => Some(Lang::De),
            "en" => Some(Lang::En),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Lang::De => "Deutsch",
            Lang::En => "English",
        }
    }
}

static LANG: AtomicU8 = AtomicU8::new(0);

pub fn set(l: Lang) {
    LANG.store(l as u8, Ordering::Relaxed);
}

pub fn get() -> Lang {
    if LANG.load(Ordering::Relaxed) == Lang::En as u8 {
        Lang::En
    } else {
        Lang::De
    }
}

pub fn is_en() -> bool {
    get() == Lang::En
}

/// Sprache der Windows-Oberfläche: Deutsch, sonst Englisch
pub fn system_default() -> Lang {
    // Primäre Sprach-ID 0x07 = Deutsch (de-DE, de-AT, de-CH …)
    let id = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() };
    if id & 0x3FF == 0x07 {
        Lang::De
    } else {
        Lang::En
    }
}

/// Text in der aktuellen Sprache: `t!("Deutsch", "English")`
#[macro_export]
macro_rules! t {
    ($de:expr, $en:expr $(,)?) => {
        if $crate::i18n::is_en() {
            $en
        } else {
            $de
        }
    };
}

/// Formatierter Text in der aktuellen Sprache: `tf!("{n} Netze", "{n} networks")`
#[macro_export]
macro_rules! tf {
    ($de:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        if $crate::i18n::is_en() {
            format!($en $(, $arg)*)
        } else {
            format!($de $(, $arg)*)
        }
    };
}
