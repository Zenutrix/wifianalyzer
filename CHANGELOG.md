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
- **Update-Button** in der Kopfzeile: erkennt neue Versionen auf GitHub, zeigt den Release-Text und installiert das Update auf Wunsch selbst (Download mit SHA-256-Prüfung, Neustart).
- Nach einem Update zeigt ein Dialog einmalig, was neu ist.
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
- Code in übersichtliche Module aufgeteilt (`app/shell`, `app/pages/*`, `app/dialogs`, `app/inspector`, `app/settings_ui`, `app/update`).
- **Einstellungen** aufgeräumt (Allgemein / Scan / Daten), das Scan-Intervall ist jetzt nur noch dort einstellbar.
- **Übersicht** neu gestaltet: gleich große Kennzahl-Kacheln, Ringdiagramme für Frequenzbänder und Sicherheit, einheitliches Raster.
- Signal-Finder heißt jetzt **AP-Finder**.
- Unruhige Zeitanzeigen entfernt (Intervall-Auswahl in der Kopfzeile, Spalte „Gesehen“, Scan-Hinweis in der Messung).

### Installation
- **Installer** (`WiFiAnalyzer-Setup.exe`) im dunklen App-Design. Er legt Programmordner, Startmenü- und Desktop-Verknüpfung sowie einen Eintrag unter „Apps“ an.
- Installation ohne Adminrechte für den aktuellen Benutzer, optional für alle Benutzer.
- Updates laufen bei installierter App über den Installer (still, mit Neustart).
- Die portable `WiFiAnalyzer.exe` gibt es weiterhin.

### Messpunkte pro Raum
- Während einer Messung **Enter** drücken, Raum/Ort (und optional eine Notiz) eingeben, Enter. Der Wert dieser Stelle wird gemittelt, solange das Fenster offen ist, und als Messpunkt mit Bewertung (Sehr gut / Gut / Ausreichend / Schwach) gespeichert.
- Liste der Messpunkte unter dem Verlauf: Signal, Bewertung, AP, Kanal, Datenrate, Notiz. Name und Notiz lassen sich nachträglich ändern.
- Messungen werden automatisch in `Dokumente\WiFi Analyzer` gespeichert und lassen sich wieder öffnen (Liste „Gespeicherte Messungen“).
- Export als **Bericht für Kunden** (HTML, im Browser als PDF druckbar) mit Zusammenfassung, Verlauf, Tabelle und Hinweisen, als **Liste der Messpunkte** (CSV/Excel) oder mit allen Messwerten.

### Messung (Walk-Survey) überarbeitet
- Der Verlauf läuft flüssig mit: Die Linie reicht bis „jetzt“, ein Live-Punkt markiert den aktuellen Wert, Neuzeichnen 10× pro Sekunde während der Messung.
- Die Zeitachse wächst in festen Stufen (1 / 2 / 5 / 10 min …) statt jede Sekunde neu zu skalieren.
- Scan-Werte werden als Stufenlinie gezeigt (ein Wert gilt bis zum nächsten Scan).
- Während einer Messung wird alle 2 s gescannt, der erste Wert kommt sofort.
- Hinweis, ob mit Verbindung (jede Sekunde) oder nur per Scan gemessen wird; „Warte auf den ersten Messwert …“.

### Behoben
- Einstellungsdateien mit UTF-8-BOM wurden nicht korrekt gelesen.
- Unverständlicher Fehlercode bei ausgeschaltetem WLAN.
- Das Standort-Symbol in der Taskleiste blinkte bei jedem Scan auf, wodurch die Taskleiste ständig sprang. Es bleibt jetzt ruhig stehen, solange die App läuft.

## [1.3.0] – 2026-10-30
- Walk-Survey-Messung mit Roaming-, Abbruch- und Sticky-Client-Erkennung, Verbindung jede Sekunde.

## [1.2.0] – 2026-10-30
- UI aufgeräumt, stabile Sortierungen, Leerlauf-CPU von 34 % auf ca. 1 % eines Kerns gesenkt.

## [1.1.0] – 2026-10-29
- Standort-Berechtigungsabfrage, Lade-Popup, App-Icon.

## [1.0.0] – 2026-10-29
- Erste Version: Netzwerkliste, Kanäle, Kanalbewertung, Analyse, Signal-Finder, Hersteller-Erkennung.
