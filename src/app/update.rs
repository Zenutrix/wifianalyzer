//! Updates: Prüfung über GitHub-Releases, Update-Dialog mit Release-Text, Selbst-Update
//! (Download + SHA256-Prüfung + Austausch der EXE + Neustart) und „Was ist neu“.

use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui::{self, Align, Align2, Layout, Margin, RichText, Ui, Vec2};

use super::{ReleaseInfo, Sev, WifiApp, REPO, VERSION};
use crate::theme::pal;
use crate::widgets::primary_button;
use crate::{t, tf};

/// Eingebautes Änderungsprotokoll (Quelle für „Was ist neu“)
const CHANGELOG: &str = include_str!("../../CHANGELOG.md");

/// Versionsvergleich "1.10.0" > "1.9.2"
pub fn is_newer(latest: &str, current: &str) -> bool {
    let parse = |s: &str| -> Vec<u32> { s.trim_start_matches('v').split('.').map(|p| p.trim().parse().unwrap_or(0)).collect() };
    parse(latest) > parse(current)
}

/// Abschnitt einer Version aus dem CHANGELOG ("## [x.y.z] …" bis zum nächsten "## [")
pub fn changelog_section(version: &str) -> Option<String> {
    let start = CHANGELOG.find(&format!("## [{version}]"))?;
    let rest = &CHANGELOG[start..];
    let body = &rest[rest.find('\n').map(|i| i + 1).unwrap_or(rest.len())..];
    let end = body.find("\n## [").unwrap_or(body.len());
    Some(body[..end].trim().to_string())
}

/// PowerShell-Befehl unsichtbar ausführen, Ausgabe als UTF-8
fn powershell(script: &str) -> Result<String, String> {
    let full = format!(
        "$ProgressPreference='SilentlyContinue'; [Console]::OutputEncoding=[Text.Encoding]::UTF8; \
         [Net.ServicePointManager]::SecurityProtocol='Tls12,Tls13'; {script}"
    );
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &full])
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(err.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("error").to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Neuestes Release von GitHub (blockierend; im Hintergrund aufrufen). Ok(None) = noch keines veröffentlicht.
fn fetch_latest_release() -> Result<Option<ReleaseInfo>, String> {
    let api = REPO.replace("https://github.com/", "https://api.github.com/repos/") + "/releases/latest";
    let out = powershell(&format!(
        "try {{ $r = Invoke-RestMethod -UseBasicParsing -TimeoutSec 15 -Headers @{{'User-Agent'='WifiAnalyzer-UpdateCheck'}} -Uri '{api}' }} \
         catch {{ if ($_.Exception.Response -and [int]$_.Exception.Response.StatusCode -eq 404) {{ 'NONE'; exit 0 }} else {{ throw }} }}; \
         'TAG=' + $r.tag_name; 'URL=' + $r.html_url; 'DATE=' + ([datetime]$r.published_at).ToString('yyyy-MM-dd'); \
         foreach ($a in $r.assets) {{ 'ASSET=' + $a.name + '|' + $a.browser_download_url }}; \
         '---BODY---'; $r.body"
    ))?;
    if out.trim() == "NONE" {
        return Ok(None);
    }
    let mut rel = ReleaseInfo::default();
    let mut lines = out.lines();
    for line in lines.by_ref() {
        let line = line.trim_end();
        if line == "---BODY---" {
            break;
        }
        if let Some(v) = line.strip_prefix("TAG=") {
            rel.version = v.trim().trim_start_matches('v').to_string();
        } else if let Some(v) = line.strip_prefix("URL=") {
            rel.url = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("DATE=") {
            rel.date = v.trim().to_string();
        } else if let Some((name, url)) = line.strip_prefix("ASSET=").and_then(|a| a.split_once('|')) {
            match name.trim() {
                "WiFiAnalyzer.exe" => rel.exe_url = Some(url.trim().to_string()),
                "SHA256SUMS.txt" => rel.sums_url = Some(url.trim().to_string()),
                n if n.starts_with("WiFiAnalyzer-Setup") && n.ends_with(".exe") => rel.setup_url = Some((n.to_string(), url.trim().to_string())),
                _ => {}
            }
        }
    }
    rel.notes = lines.collect::<Vec<_>>().join("\n").trim().to_string();
    if rel.version.is_empty() {
        return Err(t!("Ungültige Antwort von GitHub", "Invalid response from GitHub").into());
    }
    Ok(Some(rel))
}

/// Wurde die App mit dem Installer eingerichtet? (Deinstallationsprogramm liegt daneben)
pub fn is_installed() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("unins000.exe").exists()))
        .unwrap_or(false)
}

/// Heruntergeladenes Update: entweder eine neue EXE (portable) oder der Installer
pub(super) enum Download {
    Exe(PathBuf),
    Setup(PathBuf),
}

/// Update herunterladen und gegen SHA256SUMS.txt prüfen (blockierend).
/// Installierte App -> Installer, portable App -> einzelne EXE.
fn download_release(rel: &ReleaseInfo) -> Result<Download, String> {
    let (name, url, setup) = match (&rel.setup_url, is_installed()) {
        (Some((n, u)), true) => (n.clone(), u.clone(), true),
        _ => {
            let u = rel.exe_url.clone().ok_or_else(|| t!("Das Release enthält keine WiFiAnalyzer.exe", "The release contains no WiFiAnalyzer.exe").to_string())?;
            ("WiFiAnalyzer.exe".to_string(), u, false)
        }
    };
    let dir = std::env::temp_dir().join("WifiAnalyzer-update");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let target = dir.join(&name);
    let _ = std::fs::remove_file(&target);
    let tp = target.display().to_string().replace('\'', "''");
    let pat = regex_escape(&name);
    let mut script = format!("Invoke-WebRequest -UseBasicParsing -TimeoutSec 300 -Uri '{url}' -OutFile '{tp}'; ");
    if let Some(sums) = &rel.sums_url {
        script.push_str(&format!(
            "$c = (Invoke-WebRequest -UseBasicParsing -TimeoutSec 30 -Uri '{sums}').Content; \
             if ($c -is [byte[]]) {{ $c = [Text.Encoding]::UTF8.GetString($c) }}; \
             $line = $c -split '\\r?\\n' | Where-Object {{ $_ -match '\\s{pat}\\s*$' }} | Select-Object -First 1; \
             if (-not $line) {{ throw 'SHA256SUMS: {name} fehlt / missing' }}; \
             $want = ($line.Trim() -split '\\s+')[0].ToLower(); \
             $have = (Get-FileHash -Algorithm SHA256 -LiteralPath '{tp}').Hash.ToLower(); \
             if ($want -ne $have) {{ throw 'SHA256 stimmt nicht / checksum mismatch' }}; "
        ));
    }
    script.push_str("'OK'");
    powershell(&script)?;
    let size = std::fs::metadata(&target).map(|m| m.len()).unwrap_or(0);
    if size < 1_000_000 {
        return Err(t!("Download unvollständig", "Download incomplete").into());
    }
    Ok(if setup { Download::Setup(target) } else { Download::Exe(target) })
}

fn regex_escape(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c.to_string() } else { format!("\\{c}") }).collect()
}

/// Installer still ausführen; er schließt die App, aktualisiert sie und startet sie neu (/RELAUNCH).
/// Eine Installation unter „Programme“ fragt dabei nach Administratorrechten.
fn run_setup(setup: &PathBuf) -> Result<(), String> {
    let cur = std::env::current_exe().map_err(|e| e.to_string())?;
    let per_user = std::env::var_os("LOCALAPPDATA").map_or(true, |l| cur.starts_with(PathBuf::from(l)));
    let mode = if per_user { "/CURRENTUSER" } else { "/ALLUSERS" };
    std::process::Command::new(setup)
        .args(["/SILENT", "/SP-", "/SUPPRESSMSGBOXES", "/NORESTART", "/CLOSEAPPLICATIONS", "/RELAUNCH", mode])
        .spawn()
        .map_err(|e| tf!("Installer konnte nicht gestartet werden ({e})", "Could not start the installer ({e})"))?;
    Ok(())
}

/// Laufende EXE gegen die neue tauschen und neu starten (portable Version).
/// Windows erlaubt das Umbenennen einer laufenden EXE; die alte wird beim nächsten Start gelöscht.
fn replace_and_restart(new_exe: &PathBuf) -> Result<(), String> {
    let cur = std::env::current_exe().map_err(|e| e.to_string())?;
    let old = cur.with_extension("exe.old");
    let _ = std::fs::remove_file(&old);
    std::fs::rename(&cur, &old).map_err(|e| tf!("EXE kann nicht ersetzt werden ({e})", "Cannot replace the EXE ({e})"))?;
    if let Err(e) = std::fs::copy(new_exe, &cur) {
        let _ = std::fs::rename(&old, &cur);
        return Err(e.to_string());
    }
    std::process::Command::new(&cur).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

/// Reste eines früheren Updates entfernen (die alte EXE ist evtl. noch kurz gesperrt)
pub fn cleanup_old_exe() {
    std::thread::spawn(|| {
        let Ok(cur) = std::env::current_exe() else { return };
        let old = cur.with_extension("exe.old");
        for _ in 0..20 {
            if !old.exists() || std::fs::remove_file(&old).is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        let _ = std::fs::remove_dir_all(std::env::temp_dir().join("WifiAnalyzer-update"));
    });
}

/// Release-Text (Markdown aus CHANGELOG/GitHub) schlicht und lesbar darstellen
pub fn render_notes(ui: &mut Ui, notes: &str) {
    let p = pal(ui);
    let clean = |s: &str| -> String {
        // **fett**, `code` und [Text](Link) vereinfachen
        let mut out = s.replace("**", "").replace('`', "");
        while let (Some(a), Some(b)) = (out.find("]("), out.find(')')) {
            let Some(open) = out[..a].rfind('[') else { break };
            if b < a {
                break;
            }
            out = format!("{}{}{}", &out[..open], &out[open + 1..a], &out[b + 1..]);
        }
        out
    };
    for raw in notes.lines() {
        let indent = raw.len() - raw.trim_start().len();
        let line = raw.trim();
        if line.is_empty() {
            ui.add_space(4.0);
        } else if let Some(h) = line.strip_prefix("### ").or_else(|| line.strip_prefix("## ")) {
            ui.add_space(6.0);
            ui.label(RichText::new(clean(h)).strong().size(15.0).color(p.text));
        } else if let Some(item) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
            ui.horizontal_top(|ui| {
                ui.add_space(4.0 + indent as f32 * 6.0);
                ui.label(RichText::new(if indent > 0 { "–" } else { "•" }).color(p.accent));
                ui.add(egui::Label::new(RichText::new(clean(item)).color(p.text)).wrap());
            });
        } else {
            ui.add(egui::Label::new(RichText::new(clean(line)).color(p.text)).wrap());
        }
    }
}

impl WifiApp {
    pub(super) fn start_update_check(&mut self, ctx: &egui::Context, manual: bool) {
        if self.update.rx.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let c = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(fetch_latest_release());
            c.request_repaint();
        });
        self.update.rx = Some(rx);
        self.update.manual = manual;
        self.update.checked = true;
        self.update.last_check = Some(Instant::now());
    }

    pub(super) fn maybe_auto_update_check(&mut self, ctx: &egui::Context) {
        // beim Start (nach 5 s) und danach alle 6 Stunden, falls aktiviert
        let due = match self.update.last_check {
            None => self.start.elapsed() > Duration::from_secs(5),
            Some(t) => t.elapsed() > Duration::from_secs(6 * 3600),
        };
        if self.settings.check_updates && due {
            self.start_update_check(ctx, false);
        }
    }

    pub(super) fn poll_update(&mut self) {
        let Some(rx) = &self.update.rx else { return };
        let Ok(res) = rx.try_recv() else { return };
        self.update.rx = None;
        match res {
            Ok(rel) => {
                self.update.error = None;
                self.update.latest = rel;
                if let Some(v) = self.update.available().map(|r| r.version.clone()) {
                    self.toast(Sev::Info, tf!("Neue Version {v} verfügbar – oben auf „Update“ klicken", "New version {v} available – click \"Update\" at the top"));
                    if self.update.manual {
                        self.show_update = true;
                    }
                } else if self.update.manual {
                    let msg = if self.update.latest.is_some() {
                        tf!("WiFi Analyzer {VERSION} ist aktuell", "WiFi Analyzer {VERSION} is up to date")
                    } else {
                        t!("Noch keine Version auf GitHub veröffentlicht", "No release published on GitHub yet").into()
                    };
                    self.toast(Sev::Info, msg);
                }
            }
            Err(e) => {
                if self.update.manual {
                    self.toast(Sev::Warning, tf!("Update-Prüfung fehlgeschlagen: {e}", "Update check failed: {e}"));
                }
                self.update.error = Some(e);
            }
        }
    }

    fn start_install(&mut self, ctx: &egui::Context) {
        let Some(rel) = self.update.available().cloned() else { return };
        if self.update.install_rx.is_some() {
            return;
        }
        self.update.install_error = None;
        let (tx, rx) = std::sync::mpsc::channel();
        let c = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(download_release(&rel));
            c.request_repaint();
        });
        self.update.install_rx = Some(rx);
    }

    pub(super) fn poll_install(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.update.install_rx else { return };
        let Ok(res) = rx.try_recv() else { return };
        self.update.install_rx = None;
        match res.and_then(|d| match d {
            Download::Exe(p) => replace_and_restart(&p),
            Download::Setup(p) => run_setup(&p),
        }) {
            Ok(()) => {
                // Einstellungen sichern, dann beenden – die neue Version startet bereits
                self.sync_settings();
                self.save_settings();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Err(e) => self.update.install_error = Some(e),
        }
    }

    /// Update-Dialog: neue Version, Release-Text, „Jetzt aktualisieren“
    pub(super) fn update_window(&mut self, ctx: &egui::Context) {
        if !self.show_update {
            return;
        }
        let Some(rel) = self.update.available().cloned() else {
            self.show_update = false;
            return;
        };
        let mut open = true;
        let mut install = false;
        let mut later = false;
        let installing = self.update.install_rx.is_some();
        egui::Window::new(RichText::new(t!("⬆ Update verfügbar", "⬆ Update available")).strong())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .min_width(560.0)
            .max_width(560.0)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                let p = pal(ui);
                ui.horizontal(|ui| {
                    if let Some(tex) = &self.icon_tex {
                        ui.add(egui::Image::new((tex.id(), Vec2::splat(56.0))));
                    }
                    ui.vertical(|ui| {
                        ui.label(RichText::new(tf!("WiFi Analyzer {}", "WiFi Analyzer {}", rel.version)).size(21.0).strong().color(p.text));
                        let date = if rel.date.is_empty() { String::new() } else { tf!(" · veröffentlicht am {}", " · released {}", rel.date) };
                        ui.label(RichText::new(tf!("Installiert: {VERSION}{date}", "Installed: {VERSION}{date}")).color(p.weak));
                    });
                });
                ui.add_space(8.0);
                ui.label(RichText::new(t!("Was ist neu", "What's new")).strong().color(p.text));
                egui::Frame::none().fill(p.surface2).rounding(8.0).inner_margin(Margin::same(12.0)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    egui::ScrollArea::vertical().max_height(300.0).min_scrolled_height(160.0).show(ui, |ui| {
                        if rel.notes.trim().is_empty() {
                            ui.label(RichText::new(t!("Keine Beschreibung vorhanden.", "No description available.")).color(p.weak));
                        } else {
                            render_notes(ui, &rel.notes);
                        }
                    });
                });
                ui.add_space(8.0);
                if let Some(e) = &self.update.install_error {
                    ui.label(RichText::new(tf!("⚠ Update fehlgeschlagen: {e}", "⚠ Update failed: {e}")).color(p.danger));
                    ui.label(RichText::new(t!("Du kannst die neue Version auch manuell von GitHub laden.", "You can also download the new version manually from GitHub.")).color(p.weak));
                }
                if installing {
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new());
                        ui.label(t!("Lade und prüfe die neue Version …", "Downloading and verifying the new version …"));
                    });
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let can = rel.exe_url.is_some() && !installing;
                    ui.add_enabled_ui(can, |ui| {
                        if primary_button(ui, t!("⬆ Jetzt aktualisieren", "⬆ Update now"), p.accent).clicked() {
                            install = true;
                        }
                    });
                    ui.hyperlink_to(t!("Auf GitHub ansehen", "View on GitHub"), if rel.url.is_empty() { format!("{REPO}/releases/latest") } else { rel.url.clone() });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add_enabled(!installing, egui::Button::new(t!("Später", "Later"))).clicked() {
                            later = true;
                        }
                    });
                });
                ui.label(
                    RichText::new(t!(
                        "Die neue Version wird geladen, per SHA256 geprüft und startet danach automatisch. Einstellungen bleiben erhalten.",
                        "The new version is downloaded, verified via SHA256 and then starts automatically. Settings are kept."
                    ))
                    .size(12.0)
                    .color(p.weak),
                );
            });
        if install {
            self.start_install(ctx);
        }
        if !open || later {
            self.show_update = false;
        }
    }

    /// Einmalig nach einem Update: Neuerungen der laufenden Version (aus dem eingebauten CHANGELOG)
    pub(super) fn whats_new_window(&mut self, ctx: &egui::Context) {
        if !self.show_whats_new {
            return;
        }
        let notes = changelog_section(VERSION).unwrap_or_default();
        let mut open = true;
        let mut close = false;
        egui::Window::new(RichText::new(tf!("Neu in Version {VERSION}", "New in version {VERSION}")).strong())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .min_width(560.0)
            .max_width(560.0)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                let p = pal(ui);
                ui.horizontal(|ui| {
                    if let Some(tex) = &self.icon_tex {
                        ui.add(egui::Image::new((tex.id(), Vec2::splat(48.0))));
                    }
                    ui.vertical(|ui| {
                        if self.whats_new_upgraded {
                            ui.label(RichText::new(t!("Update erfolgreich installiert", "Update installed successfully")).size(18.0).strong().color(p.success));
                        } else {
                            ui.label(RichText::new(t!("Was ist neu", "What's new")).size(18.0).strong());
                        }
                        ui.label(RichText::new(tf!("WiFi Analyzer {VERSION}", "WiFi Analyzer {VERSION}")).color(p.weak));
                    });
                });
                ui.add_space(8.0);
                egui::Frame::none().fill(p.surface2).rounding(8.0).inner_margin(Margin::same(12.0)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    egui::ScrollArea::vertical().max_height(340.0).min_scrolled_height(200.0).show(ui, |ui| render_notes(ui, &notes));
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if primary_button(ui, t!("Los geht's", "Let's go"), p.accent).clicked() {
                        close = true;
                    }
                    ui.hyperlink_to(t!("Alle Änderungen", "Full changelog"), format!("{REPO}/blob/main/CHANGELOG.md"));
                });
            });
        if !open || close {
            self.show_whats_new = false;
            self.whats_new_upgraded = false;
        }
    }
}
