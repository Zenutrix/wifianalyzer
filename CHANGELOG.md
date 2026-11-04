# Changelog

Alle wichtigen Änderungen an diesem Projekt. Format angelehnt an [Keep a Changelog](https://keepachangelog.com/de/1.1.0/),
Versionen nach [Semantic Versioning](https://semver.org/lang/de/).

## [2.0.0] – 2026-11-04

Erste öffentliche Version.

### Neu
- Neue Oberfläche im Business-Stil:
  - Navigation links
  - **Übersicht** (Dashboard) mit Kennzahlen, Verbindung, stärksten Netzen, Befunden und Verteilungen
  - Statusleiste
- **Deutsch und Englisch**, beim ersten Start automatisch nach Windows-Sprache, umschaltbar in den Einstellungen.
- **Einstellungen** werden gespeichert (`%APPDATA%\WifiAnalyzer\settings.ini`). Dazu gehören Sprache, Design, Intervall, Zeitgrenzen, Gruppierung und Markierungen.
- **Über-Dialog** mit Ersteller, Website, GitHub, Lizenz und Tastenkürzeln.
- **Update-Prüfung** über GitHub-Releases, beim Start oder manuell.
- **Befunde** mit konkreter Empfehlung, pro SSID zusammengefasst. Neuer Check auf fehlendes 802.11k/v.
- Netzwerkliste:
  - Rechtsklick-Menü: Details, Markieren, Orten, Messung starten, kopieren
  - Doppelklick zum Orten
  - Spalten-Tooltips
- Tastenkürzel: F5 (scannen), Strg+F (suchen), Strg+E (Export), Strg+1…9 (Seiten), Esc.
- Hinweis-Banner mit passender Aktion, wenn der Scan nicht funktioniert (z. B. WLAN ausgeschaltet oder fehlende Berechtigung).
- Windows-Manifest (HiDPI, moderne Steuerelemente) und Versionsinfos mit Ersteller in der EXE.
- GitHub Actions: Build und Tests bei jedem Push, automatisches Release bei Versions-Tags.

### Geändert
- Interne Logik ist sprachneutral (Sicherheitsverfahren und PMF als Typen statt Textvergleichen).
- Code in übersichtliche Module aufgeteilt (`app/shell`, `app/pages/*`, `app/dialogs`, `app/inspector`).

### Behoben
- Einstellungsdateien mit UTF-8-BOM wurden nicht korrekt gelesen.
- Unverständlicher Fehlercode bei ausgeschaltetem WLAN.

## [1.3.0] – 2026-10-30
- Walk-Survey-Messung mit Roaming-, Abbruch- und Sticky-Client-Erkennung, Verbindung jede Sekunde.

## [1.2.0] – 2026-10-30
- UI aufgeräumt, stabile Sortierungen, Leerlauf-CPU von 34 % auf ca. 1 % eines Kerns gesenkt.

## [1.1.0] – 2026-10-29
- Standort-Berechtigungsabfrage, Lade-Popup, App-Icon.

## [1.0.0] – 2026-10-29
- Erste Version: Netzwerkliste, Kanäle, Kanalbewertung, Analyse, Signal-Finder, Hersteller-Erkennung.
