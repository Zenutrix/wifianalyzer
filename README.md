# WiFi Analyzer (Rust, Windows)

WLAN-Analyse-Tool für Techniker. Nutzt die Windows Native WiFi API (`wlanapi.dll`) und eine egui-Oberfläche.
Die fertige EXE ist eigenständig und braucht nur Windows-System-DLLs.

## Starten

```
target\release\wifi-analyzer.exe
```

Unter Windows 11 gibt Windows WLAN-Scans nur an Apps mit **Standortzugriff** heraus. Die App kümmert sich selbst darum:
- Beim Start löst sie die offizielle Windows-Berechtigungsabfrage aus (`WiFiAdapter`/`Geolocator.RequestAccessAsync`).
- Fehlt etwas, erklärt ein Dialog, welcher Schalter aus ist: Standortdienste, Apps oder Desktop-Apps (gelesen aus der Registry).
  Ein Button öffnet die passende Einstellungsseite. Sobald alles erlaubt ist, schließt sich der Dialog von selbst und der Scan startet.

## Bauen

```
build.cmd
```

Voraussetzungen: Rust-Toolchain `stable-x86_64-pc-windows-gnu` (rustup) und MinGW-w64 (winget `BrechtSanders.WinLibs.POSIX.UCRT`).

## Funktionen

- **Netzwerke**: Tabelle mit SSID, BSSID, Hersteller, Signal (dBm-Balken), Kanal, Band, Breite, Standard (Wi-Fi 4–7), Sicherheit,
  max. PHY-Rate, Clients und Kanalauslastung (BSS Load). Sortierbar, mit Filter.
  - **Nach SSID gruppieren**: eine Zeile je SSID mit dem stärksten AP. Ein Klick klappt alle BSSIDs der SSID auf.
  - ☆ markiert APs für den Signalverlauf.
- **Details** je AP: Chipsatz, AP-Name (Cisco/Aruba-IE), Uptime (TSF), Land, Sendeleistung (TPC), BSS Color, Streams,
  Beacon/DTIM, AKM/Cipher/PMF, WPS-Geräteinfo (Hersteller/Modell/Seriennummer), 802.11k/r/v, MBO, Hotspot 2.0, MLO, rohe IEs.
- **Kanäle**: Spektrum-Diagramm für 2,4, 5 und 6 GHz mit echter Kanalbreite (20–320 MHz) und DFS-Bereich. Ein Klick wählt einen AP aus.
- **Signalverlauf**: RSSI über Zeit (1–60 min) für markierte APs plus die aktuelle Verbindung.
- **Kanalbewertung**: Störpegel je Kanal und Empfehlung (1/6/11, ohne/mit DFS, bester 80-MHz-Block, 6-GHz-PSC).
- **Analyse**: Warnungen zu offenen Netzen, WEP/WPA1/TKIP, aktivem WPS, 40 MHz im 2,4-GHz-Band, Überlappkanälen,
  Co-Channel-Interferenz, hoher Auslastung und gleicher SSID mit unterschiedlicher Sicherheit (Evil Twin).
- **Messung (Walk-Survey)**: SSID wählen, „Messung starten“, Fläche mit dem Laptop ablaufen.
  - Die Verbindung wird jede Sekunde gemessen. Der Verlauf ist pro AP eingefärbt, dazu kommt der stärkste AP der SSID als Vergleichslinie.
  - Roams, Abbrüche und **Sticky Client** werden als Ereignis erkannt und erscheinen als Linien im Diagramm.
    Sticky Client heißt: Der Laptop bleibt ≥ 10 dB unter dem besten AP, und das ≥ 15 s lang.
  - Markierungen („Raum 1.03“) können per Enter gesetzt werden. Die Statistik zeigt Ø/Min/Max, Anteil ≥ -67 / < -75 dBm, Roams und Abbrüche.
  - Export als CSV. Ohne Verbindung zur SSID wird der stärkste AP aus dem Scan gemessen.
- **Signal-Finder**: große Anzeige mit optionalem „Geigerzähler“-Piepton zum Orten eines APs.
- **Ereignisse**: neue oder verschwundene APs, Roaming, Verbindungswechsel.
- **Hersteller-DB**: Beim ersten Start wird die vollständige OUI-Liste (Wireshark/IEEE) geladen und unter
  `%APPDATA%\WifiAnalyzer\oui.txt` gespeichert. Virtuelle BSSIDs (LAA) erben den Hersteller ihrer physischen Radio-BSSID.
- Startet mit einem kleinen, animierten Lade-Popup (Berechtigungen, Adapter, Hersteller-DB, erster Scan).
  Danach geht es ins maximierte Hauptfenster.
- Eigenes App-Icon, eingebettet in die EXE (`assets/icon.ico`). Neu erzeugen mit `python tools/make_icon.py`.
- **CSV-Export** (Semikolon, Excel-tauglich) und Hell-/Dunkel-Modus.
