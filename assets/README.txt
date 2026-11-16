WiFi Analyzer
=============
WLAN-Analyse fuer Techniker unter Windows / Wi-Fi analysis for technicians on Windows

(c) 2026 Thomas Schoepf - https://schoepf-tirol.at/Project/WifiAnalyzer
Quellcode / Source: https://github.com/zenutrix/wifianalyzer
Lizenz / License: GNU GPL v3 or later (siehe/see LICENSE.txt) - ohne Gewaehrleistung / without warranty


DEUTSCH
-------
Starten: "WiFiAnalyzer.exe" doppelklicken. Keine Installation, keine Adminrechte noetig.

Voraussetzungen
- Windows 10 oder 11 (64 Bit) mit WLAN-Adapter
- Standortzugriff: Windows gibt WLAN-Scans nur an Apps mit Standortzugriff heraus.
  Beim ersten Start fragt Windows danach - bitte mit "Ja" bestaetigen. Fehlt die Freigabe,
  zeigt die App, welcher Schalter in den Einstellungen fehlt.

Hinweise
- Die EXE ist nicht digital signiert. SmartScreen kann warnen:
  "Weitere Informationen" -> "Trotzdem ausfuehren".
- Beim ersten Start wird die Hersteller-Datenbank einmalig geladen (Wireshark/IEEE).
- Einstellungen und Cache: %APPDATA%\WifiAnalyzer  (zum Entfernen einfach loeschen)


ENGLISH
-------
Run "WiFiAnalyzer.exe". No installation and no admin rights required.

Requirements
- Windows 10 or 11 (64-bit) with a Wi-Fi adapter
- Location access: Windows only allows Wi-Fi scans for apps with location access.
  Windows asks on first start - please confirm. If it is missing, the app shows which
  setting has to be turned on.

Notes
- The EXE is not code-signed. SmartScreen may warn: "More info" -> "Run anyway".
- The vendor database is downloaded once on first start (Wireshark/IEEE).
- Settings and cache: %APPDATA%\WifiAnalyzer  (delete to remove)
