//! Befunde: automatische Prüfung auf Sicherheits- und Konfigurationsprobleme mit Empfehlung.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::time::Instant;

use eframe::egui::{self, Margin, RichText, Sense, Stroke, Ui};

use crate::app::{Sev, WifiApp};
use crate::ie::SecKind;
use crate::model::*;
use crate::theme::pal;
use crate::widgets::{badge, page_header};
use crate::{t, tf};

pub(crate) struct Finding {
    pub sev: Sev,
    pub category: &'static str,
    pub title: String,
    pub detail: String,
    pub advice: &'static str,
    /// Beispiel-AP zum Anspringen
    pub bssid: Option<[u8; 6]>,
}

/// Regeln, die pro Access Point geprüft und dann je SSID zusammengefasst werden
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Rule {
    Wep,
    Open,
    Wpa1,
    Tkip,
    Wps,
    Wide24,
    Overlap24,
    HighUtil,
    ManyClients,
    NoPmf,
    Beacon,
}

impl Rule {
    fn sev(self) -> Sev {
        match self {
            Rule::Wep | Rule::Open => Sev::Critical,
            Rule::Wpa1 | Rule::Tkip | Rule::Wps | Rule::Wide24 | Rule::Overlap24 | Rule::HighUtil => Sev::Warning,
            Rule::ManyClients | Rule::NoPmf | Rule::Beacon => Sev::Info,
        }
    }

    fn category(self) -> &'static str {
        match self {
            Rule::Wep | Rule::Open | Rule::Wpa1 | Rule::Tkip | Rule::Wps | Rule::NoPmf => t!("Sicherheit", "Security"),
            Rule::Wide24 | Rule::Overlap24 => t!("Kanalplanung", "Channel planning"),
            Rule::HighUtil | Rule::ManyClients => t!("Auslastung", "Load"),
            Rule::Beacon => t!("Konfiguration", "Configuration"),
        }
    }

    fn title(self) -> &'static str {
        match self {
            Rule::Wep => t!("WEP-Verschlüsselung", "WEP encryption"),
            Rule::Open => t!("Offenes Netz ohne Verschlüsselung", "Open network without encryption"),
            Rule::Wpa1 => t!("Veraltetes WPA (WPA1) aktiv", "Legacy WPA (WPA1) enabled"),
            Rule::Tkip => t!("TKIP-Verschlüsselung aktiv", "TKIP cipher enabled"),
            Rule::Wps => t!("WPS ist aktiv", "WPS is enabled"),
            Rule::Wide24 => t!("40 MHz im 2,4-GHz-Band", "40 MHz in the 2.4 GHz band"),
            Rule::Overlap24 => t!("Überlappender 2,4-GHz-Kanal", "Overlapping 2.4 GHz channel"),
            Rule::HighUtil => t!("Hohe Kanalauslastung", "High channel utilization"),
            Rule::ManyClients => t!("Viele Clients an einem AP", "Many clients on one AP"),
            Rule::NoPmf => t!("PMF (802.11w) nicht aktiv", "PMF (802.11w) not enabled"),
            Rule::Beacon => t!("Ungewöhnliches Beacon-Intervall", "Unusual beacon interval"),
        }
    }

    fn advice(self) -> &'static str {
        match self {
            Rule::Wep => t!(
                "Sofort auf WPA2/WPA3 umstellen – WEP lässt sich in Minuten knacken.",
                "Switch to WPA2/WPA3 immediately – WEP can be cracked in minutes."
            ),
            Rule::Open => t!(
                "WPA2/WPA3 oder OWE (Enhanced Open) aktivieren; Gastnetze mit Client-Isolation betreiben.",
                "Enable WPA2/WPA3 or OWE (Enhanced Open); run guest networks with client isolation."
            ),
            Rule::Wpa1 => t!("WPA1 deaktivieren und nur WPA2/WPA3 zulassen.", "Disable WPA1 and allow WPA2/WPA3 only."),
            Rule::Tkip => t!(
                "Nur AES/CCMP verwenden – TKIP ist unsicher und begrenzt auf 54 Mbit/s.",
                "Use AES/CCMP only – TKIP is insecure and limits throughput to 54 Mbit/s."
            ),
            Rule::Wps => t!("WPS deaktivieren – die PIN ist per Brute-Force angreifbar.", "Disable WPS – the PIN is vulnerable to brute force."),
            Rule::Wide24 => t!(
                "Im 2,4-GHz-Band 20 MHz verwenden, sonst werden Nachbarkanäle gestört.",
                "Use 20 MHz in the 2.4 GHz band, otherwise neighbouring channels are disturbed."
            ),
            Rule::Overlap24 => t!(
                "Nur die überlappungsfreien Kanäle 1, 6 oder 11 verwenden.",
                "Use only the non-overlapping channels 1, 6 or 11."
            ),
            Rule::HighUtil => t!(
                "Clients auf 5/6 GHz verlagern, Kanal wechseln oder niedrige Basisraten abschalten.",
                "Move clients to 5/6 GHz, change channel or disable low basic rates."
            ),
            Rule::ManyClients => t!(
                "Zusätzlichen AP einplanen oder Band-Steering / Lastverteilung aktivieren.",
                "Plan an additional AP or enable band steering / load balancing."
            ),
            Rule::NoPmf => t!(
                "PMF auf „optional“ oder „erforderlich“ stellen (Schutz vor Deauth-Angriffen).",
                "Set PMF to \"optional\" or \"required\" (protects against deauth attacks)."
            ),
            Rule::Beacon => t!(
                "Standard ist 100 TU (102,4 ms); abweichende Werte prüfen.",
                "The default is 100 TU (102.4 ms); check deviating values."
            ),
        }
    }
}

fn rules_for(r: &ApRecord) -> Vec<Rule> {
    let i = &r.info;
    let mut v = Vec::new();
    match i.sec_kind {
        SecKind::Wep => v.push(Rule::Wep),
        k if k.is_open() => v.push(Rule::Open),
        SecKind::Wpa | SecKind::WpaEnt | SecKind::WpaWpa2 => v.push(Rule::Wpa1),
        _ => {}
    }
    if i.pairwise.iter().any(|c| c == "TKIP") {
        v.push(Rule::Tkip);
    }
    if i.wps.as_ref().is_some_and(|w| !w.locked) {
        v.push(Rule::Wps);
    }
    if i.band == Band::B24 && i.width >= 40 {
        v.push(Rule::Wide24);
    }
    if i.band == Band::B24 && ![1, 5, 6, 9, 11, 13].contains(&i.channel) {
        v.push(Rule::Overlap24);
    }
    if i.ch_util.is_some_and(|u| u > 60.0) {
        v.push(Rule::HighUtil);
    }
    if i.stations.is_some_and(|s| s > 40) {
        v.push(Rule::ManyClients);
    }
    if i.sec_kind.is_wpa2_only() && i.pmf == crate::ie::Pmf::No {
        v.push(Rule::NoPmf);
    }
    if i.beacon_ms > 150.0 || i.beacon_ms < 90.0 {
        v.push(Rule::Beacon);
    }
    v
}

impl WifiApp {
    /// Alle Befunde für die aktuell sichtbaren, nicht veralteten APs (stabil sortiert)
    pub(crate) fn findings(&self) -> Vec<Finding> {
        let now = Instant::now();
        let vis: Vec<&ApRecord> = self.visible_records().into_iter().filter(|r| !self.is_stale(r, now)).collect();
        let mut out = Vec::new();

        // Regeln je AP, zusammengefasst pro (Regel, SSID)
        let mut groups: BTreeMap<(Rule, String), Vec<&ApRecord>> = BTreeMap::new();
        for r in &vis {
            for rule in rules_for(r) {
                groups.entry((rule, Self::display_ssid(r))).or_default().push(r);
            }
        }
        for ((rule, ssid), aps) in groups {
            let chans: BTreeSet<u32> = aps.iter().map(|r| r.info.channel).collect();
            let chans = chans.iter().map(|c| c.to_string()).collect::<Vec<_>>().join(", ");
            let detail = if aps.len() == 1 {
                let r = aps[0];
                let mut d = tf!("{} · Kanal {} · {} dBm", "{} · channel {} · {} dBm", fmt_mac(&r.info.bssid), r.info.channel, r.info.rssi);
                match rule {
                    Rule::HighUtil => d.push_str(&format!(" · {:.0} %", r.info.ch_util.unwrap_or(0.0))),
                    Rule::ManyClients => d.push_str(&tf!(" · {} Clients", " · {} clients", r.info.stations.unwrap_or(0))),
                    Rule::Beacon => d.push_str(&format!(" · {:.0} ms", r.info.beacon_ms)),
                    _ => {}
                }
                d
            } else {
                tf!("{} Access Points · Kanäle {}", "{} access points · channels {}", aps.len(), chans)
            };
            out.push(Finding {
                sev: rule.sev(),
                category: rule.category(),
                title: format!("{ssid}: {}", rule.title()),
                detail,
                advice: rule.advice(),
                bssid: aps.iter().max_by_key(|r| r.info.rssi).map(|r| r.info.bssid),
            });
        }

        // Co-Channel-Interferenz
        let mut per_ch: BTreeMap<(Band, u32), usize> = BTreeMap::new();
        for r in vis.iter().filter(|r| r.info.rssi >= -75) {
            *per_ch.entry((r.info.band, r.info.channel)).or_default() += 1;
        }
        for ((band, ch), n) in per_ch.into_iter().filter(|(_, n)| *n >= 4) {
            out.push(Finding {
                sev: Sev::Warning,
                category: t!("Kanalplanung", "Channel planning"),
                title: tf!("{} Kanal {ch}: Co-Channel-Interferenz", "{} channel {ch}: co-channel interference", band.label()),
                detail: tf!("{n} BSSIDs mit ≥ -75 dBm auf demselben Kanal", "{n} BSSIDs at ≥ -75 dBm on the same channel"),
                advice: t!(
                    "Sendeleistung reduzieren, Kanäle neu verteilen oder SSIDs zusammenlegen.",
                    "Reduce transmit power, redistribute channels or consolidate SSIDs."
                ),
                bssid: None,
            });
        }

        // Gleiche SSID mit unterschiedlicher Sicherheit (Fehlkonfiguration / Evil Twin)
        let mut ssid_sec: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for r in vis.iter().filter(|r| !r.info.hidden) {
            ssid_sec.entry(r.info.ssid.as_str()).or_default().insert(r.info.security.as_str());
        }
        for (s, secs) in ssid_sec.into_iter().filter(|(_, v)| v.len() > 1) {
            out.push(Finding {
                sev: Sev::Warning,
                category: t!("Sicherheit", "Security"),
                title: tf!("{s}: unterschiedliche Sicherheit", "{s}: inconsistent security"),
                detail: secs.into_iter().collect::<Vec<_>>().join(" / "),
                advice: t!(
                    "Konfiguration aller APs vereinheitlichen; unbekannte APs mit dem AP-Finder orten (möglicher Evil Twin).",
                    "Align the configuration of all APs; locate unknown APs with the AP finder (possible evil twin)."
                ),
                bssid: None,
            });
        }

        // Mehrere APs einer SSID ohne Roaming-Unterstützung
        let mut by_ssid: HashMap<&str, Vec<&ApRecord>> = HashMap::new();
        for r in vis.iter().filter(|r| !r.info.hidden) {
            by_ssid.entry(r.info.ssid.as_str()).or_default().push(r);
        }
        let mut no_roam: Vec<(&str, usize)> = by_ssid
            .iter()
            .filter(|(_, aps)| aps.len() >= 3 && aps.iter().all(|r| !r.info.k_rrm && !r.info.v_btm))
            .map(|(s, aps)| (*s, aps.iter().map(|r| r.info.bssid).collect::<HashSet<_>>().len()))
            .collect();
        no_roam.sort();
        for (s, n) in no_roam {
            out.push(Finding {
                sev: Sev::Info,
                category: "Roaming",
                title: tf!("{s}: kein 802.11k/v", "{s}: no 802.11k/v"),
                detail: tf!("{n} Access Points ohne Roaming-Unterstützung", "{n} access points without roaming assistance"),
                advice: t!(
                    "802.11k (Neighbor Reports) und 802.11v (BSS Transition) aktivieren – Clients roamen dann schneller und zuverlässiger.",
                    "Enable 802.11k (neighbor reports) and 802.11v (BSS transition) – clients then roam faster and more reliably."
                ),
                bssid: None,
            });
        }

        out.sort_by(|a, b| a.sev.cmp(&b.sev).then_with(|| a.title.cmp(&b.title)));
        out
    }

    pub(crate) fn page_findings(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        let all = self.findings();
        let count = |s: Sev| all.iter().filter(|f| f.sev == s).count();
        let (c, w, i) = (count(Sev::Critical), count(Sev::Warning), count(Sev::Info));
        page_header(
            ui,
            t!("Befunde", "Findings"),
            &tf!(
                "Automatische Prüfung der sichtbaren Netze · {c} kritisch · {w} Warnungen · {i} Hinweise",
                "Automatic check of visible networks · {c} critical · {w} warnings · {i} info"
            ),
            |_| {},
        );
        ui.horizontal(|ui| {
            for (idx, sev, col, n) in [(0, Sev::Critical, p.danger, c), (1, Sev::Warning, p.warning, w), (2, Sev::Info, p.accent, i)] {
                let on = self.findings_filter[idx];
                let txt = RichText::new(format!("{}  {n}", sev.label())).color(if on { col } else { p.weak });
                if ui.selectable_label(on, txt).clicked() {
                    self.findings_filter[idx] = !on;
                }
            }
        });
        ui.add_space(8.0);

        let shown: Vec<&Finding> = all
            .iter()
            .filter(|f| self.findings_filter[match f.sev {
                Sev::Critical => 0,
                Sev::Warning => 1,
                Sev::Info => 2,
            }])
            .collect();
        if shown.is_empty() {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("✔").size(40.0).color(p.success));
                ui.label(RichText::new(t!("Keine Befunde", "No findings")).size(18.0).color(p.text));
                ui.label(RichText::new(t!("Alle sichtbaren Netze sind unauffällig.", "All visible networks look fine.")).color(p.weak));
            });
            return;
        }
        let mut pick = None;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            for f in shown {
                let col = match f.sev {
                    Sev::Critical => p.danger,
                    Sev::Warning => p.warning,
                    Sev::Info => p.accent,
                };
                let resp = egui::Frame::none()
                    .fill(p.surface)
                    .stroke(Stroke::new(1.0f32, p.border))
                    .rounding(8.0)
                    .inner_margin(Margin::symmetric(14.0, 10.0))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            // farbiger Balken links
                            let (r, _) = ui.allocate_exact_size(egui::vec2(4.0, 46.0), Sense::hover());
                            ui.painter().rect_filled(r, 2.0, col);
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    badge(ui, f.sev.label(), col);
                                    ui.label(RichText::new(f.category).size(12.0).color(p.weak));
                                });
                                ui.label(RichText::new(&f.title).strong().color(p.text));
                                ui.label(RichText::new(&f.detail).size(12.5).color(p.weak));
                                ui.label(RichText::new(format!("💡 {}", f.advice)).size(12.5).color(p.text.gamma_multiply(0.9)));
                            });
                        });
                    })
                    .response;
                if let Some(b) = f.bssid {
                    let resp = resp.interact(Sense::click()).on_hover_text(t!("Klicken: Access Point anzeigen", "Click: show access point"));
                    if resp.clicked() {
                        pick = Some(b);
                    }
                }
                ui.add_space(6.0);
            }
        });
        if pick.is_some() {
            self.selected = pick;
        }
    }
}
