<p align="center">
  <img src="assets/icon.png" width="112" alt="WiFi Analyzer">
</p>

<h1 align="center">WiFi Analyzer</h1>

<p align="center">
  <b>WLAN-Analyse für Techniker unter Windows</b> · <i>Wi-Fi analysis for technicians on Windows</i><br>
  <a href="https://wifianalyzer.schoepf-tirol.at">wifianalyzer.schoepf-tirol.at</a> ·
  <a href="https://github.com/zenutrix/wifianalyzer/releases/latest">Download</a> ·
  <a href="#english">English</a>
</p>

<p align="center">
  <a href="https://github.com/zenutrix/wifianalyzer/releases/latest"><img src="https://img.shields.io/github/v/release/zenutrix/wifianalyzer?label=Version" alt="Version"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/Lizenz-GPL--3.0-blue" alt="GPL-3.0"></a>
  <img src="https://img.shields.io/badge/Windows-10%20%7C%2011-0078d4" alt="Windows 10 | 11">
</p>

---

## Deutsch

WiFi Analyzer ist ein kostenloses, quelloffenes Werkzeug für die Analyse von WLAN-Umgebungen.
Es zeigt alle Netze und Access Points mit technischen Details, bewertet Kanäle, findet
Konfigurations- und Sicherheitsprobleme und misst bei einem Rundgang die Verbindung inklusive
Roaming. Eine einzelne EXE ohne Installation.

### Funktionen

| Bereich | Was es kann |
|---|---|
| **Übersicht** | Kennzahlen, aktuelle Verbindung, stärkste Netze, wichtigste Befunde, Verteilung nach Standard, Sicherheit und Hersteller |
| **Netzwerke** | Tabelle aller BSSIDs, nach SSID gruppiert. Spalten: Signal, Kanal/Band, Breite, Wi-Fi-Generation (4–7), Sicherheit, PHY-Rate, Clients und Auslastung (BSS Load). Mit Suche, Filtern, Rechtsklick-Menü und CSV-Export |
| **Details** | Hersteller (OUI, auch für virtuelle BSSIDs), Chipsatz, AP-Name, Laufzeit, Land, TPC, BSS Color, Streams, AKM/Cipher/PMF, WPS-Geräteinfo, 802.11k/r/v, MBO, Hotspot 2.0, MLO, rohe Information Elements |
| **Kanäle** | Spektrum für 2,4 / 5 / 6 GHz mit echter Kanalbreite (20–320 MHz) und DFS-Bereich |
| **Kanalbewertung** | Störpegel je Kanal mit Empfehlung: 1/6/11, 5 GHz mit/ohne DFS, bester 80-MHz-Block, 6-GHz-PSC |
| **Befunde** | Offene Netze, WEP/WPA1/TKIP, WPS, 40 MHz im 2,4-GHz-Band, Überlappung, Co-Channel, Auslastung, fehlendes PMF oder 802.11k/v, Evil Twin. Jeweils mit **Empfehlung** |
| **Messung** | Walk-Survey: Verbindung jede Sekunde, Verlauf je AP eingefärbt. Erkennt **Roams**, Abbrüche und **Sticky Clients**. Markierungen setzen, Statistik, CSV-Export |
| **AP-Finder** | Einen AP über die Signalstärke orten, optional mit „Geigerzähler“-Ton |
| **Signalverlauf / Ereignisse** | Verlauf markierter APs und Protokoll (neue Netze, Roaming, Fehler) |

Weitere Merkmale:
- Deutsch und Englisch
- helles und dunkles Design
- Einstellungen werden gespeichert
- Tastenkürzel: F5, Strg+F, Strg+E, Strg+1…9
- Update-Prüfung über GitHub

### Download & Start

1. Unter [Releases](https://github.com/zenutrix/wifianalyzer/releases/latest) **`WiFiAnalyzer-Setup.exe`** laden und starten.
   Das Setup legt den Programmordner (`%LOCALAPPDATA%\Programs\WiFi Analyzer`), Startmenü- und Desktop-Verknüpfung
   sowie einen Eintrag unter „Apps“ zum Deinstallieren an – ohne Administratorrechte. Optional „für alle Benutzer“.
2. Alternativ portable: `WiFiAnalyzer.exe` (oder die ZIP-Datei) laden und direkt starten.

Updates installiert die App auf Wunsch selbst (Installer- oder portable Variante).

**Voraussetzungen:** Windows 10 oder 11 (64 Bit) mit WLAN-Adapter.

**Standortzugriff:** Windows gibt WLAN-Scans nur an Apps heraus, die auf den Standort zugreifen dürfen.
Beim ersten Start fragt Windows danach. Bitte mit „Ja“ bestätigen. Fehlt die Freigabe, zeigt die App,
welcher Schalter in den Einstellungen aus ist, und bietet einen Button direkt zur richtigen Seite.

> Hinweis: Die EXE ist (noch) nicht digital signiert. Windows SmartScreen kann beim ersten Start warnen:
> „Weitere Informationen“ → „Trotzdem ausführen“.

### Datenschutz

Alle Messwerte bleiben auf dem PC. Netzwerkzugriffe gibt es nur für zwei Dinge:
- die Hersteller-Datenbank (Wireshark `manuf` bzw. IEEE-OUI-Register), einmalig beim ersten Start
- die Update-Prüfung bei GitHub (in den Einstellungen abschaltbar)

Standortdaten werden nicht verwendet. Einstellungen und Cache liegen unter `%APPDATA%\WifiAnalyzer`.

### Selbst bauen

```powershell
# Rust installieren: https://rustup.rs  (Standard-Toolchain MSVC genügt)
git clone https://github.com/zenutrix/wifianalyzer.git
cd wifianalyzer
cargo build --release          # -> target\release\wifi-analyzer.exe
cargo test --release
./tools/package.ps1            # -> dist\WiFiAnalyzer-<version>-win64.zip
```

Alternativ mit der GNU-Toolchain: `rustup default stable-x86_64-pc-windows-gnu` plus MinGW-w64 (z. B. WinLibs) im `PATH`. Siehe `build.cmd`.

**Demo-Modus:** `wifi-analyzer.exe --demo` zeigt eine erfundene WLAN-Umgebung (mit Roaming) statt echter Scans – für Präsentationen, Screenshots und Entwicklung ohne WLAN.

### Projektaufbau

```
src/
  main.rs          Start, Fenster
  wlan.rs          Windows Native WiFi API (Scan-Thread, Verbindungsabfrage)
  ie.rs            Auswertung der 802.11 Information Elements
  measure.rs       Walk-Survey: Roaming, Abbrüche, Sticky Client (mit Tests)
  oui.rs           Hersteller-Erkennung
  perm.rs          Standort-Berechtigung (WinRT + Registry)
  i18n.rs          Deutsch/Englisch (t!/tf!)
  settings.rs      gespeicherte Einstellungen
  theme.rs         Farben/Stil
  widgets.rs       UI-Bausteine
  app/             Hauptfenster: shell, inspector, dialogs, pages/*
assets/            Icon, Ressourcen, Manifest
tools/             Icon-Generator, Paket-Skript
```

Fehler und Ideen gerne als [Issue](https://github.com/zenutrix/wifianalyzer/issues). Pull Requests sind willkommen.

---

## English

WiFi Analyzer is a free, open-source tool for analysing Wi-Fi environments on Windows. It lists all
networks and access points with technical details, rates channels, detects configuration and security
issues, and measures your connection, including roaming, during a walk survey. It is a single EXE with
no installation.

**Highlights**
- **Overview dashboard** with key figures
- **Network table**, grouped by SSID:
  - signal, channel, width, Wi-Fi generation, security, PHY rate and BSS load
  - vendor detection, also for virtual BSSIDs
  - detailed inspector with all information elements
- **Spectrum view** for 2.4, 5 and 6 GHz, plus a **channel rating** with recommendations (incl. DFS and PSC)
- **Findings** with concrete recommendations:
  - security: open networks, WEP/WPA1/TKIP, WPS
  - channel planning: overlapping channels, co-channel interference
  - configuration: missing PMF or 802.11k/v
  - evil twin
- **Walk survey**:
  - connection sampled every second
  - roam, disconnect and **sticky client** detection
  - markers, statistics and CSV export
- **AP finder** with an optional Geiger counter beep
- **General**:
  - German and English UI
  - light and dark theme
  - persisted settings
  - keyboard shortcuts
  - update check

**Requirements:** Windows 10/11 (64-bit) with a Wi-Fi adapter. Windows only allows Wi-Fi scans for
apps with **location access**. The app asks for it on first start and explains which setting is missing.

**Download:** [latest release](https://github.com/zenutrix/wifianalyzer/releases/latest). Run `WiFiAnalyzer-Setup.exe` (no admin rights needed, creates Start menu/desktop shortcuts and an uninstaller) or use the portable `WiFiAnalyzer.exe`.

**Build:** `cargo build --release` (Rust stable, MSVC or GNU toolchain). Package with `tools/package.ps1` (installer needs [Inno Setup](https://jrsoftware.org/isinfo.php) 6.7+).

**Privacy:** all measurements stay on your PC. The only network access is the vendor database download
(Wireshark/IEEE) and the optional GitHub update check.

---

## Lizenz / License

Copyright © 2026 **Thomas Schöpf** · [wifianalyzer.schoepf-tirol.at](https://wifianalyzer.schoepf-tirol.at)

Dieses Programm ist freie Software: Du kannst es unter den Bedingungen der **GNU General Public License**
Version 3 oder (nach deiner Wahl) jeder späteren Version weitergeben und/oder ändern.
Es wird **ohne jede Gewährleistung** bereitgestellt. Siehe [LICENSE](LICENSE).

This program is free software under the GNU GPL v3 or later, without any warranty. See [LICENSE](LICENSE).

Verwendete Bibliotheken (Rust-Crates, u. a. egui/eframe, windows-rs) stehen unter MIT- bzw. Apache-2.0-Lizenzen.
Die mitgelieferten Schriften von egui stehen unter der Ubuntu Font Licence, der OFL bzw. der Apache-2.0-Lizenz.
