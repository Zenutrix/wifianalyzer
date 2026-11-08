# Website wifianalyzer.schoepf-tirol.at

Statische Landingpage ohne Build-Schritt, ohne externe Skripte und ohne Google Fonts
(keine Datenübertragung an Dritte – DSGVO-freundlich).

**Veröffentlichen:** den kompletten Ordner (`index.html`, `icon.png`, `img/`) in das Web-Verzeichnis
von `wifianalyzer.schoepf-tirol.at` hochladen. Fertig.

- Die Download-Buttons zeigen auf `…/releases/latest/download/WiFiAnalyzer-Setup.exe` bzw. `WiFiAnalyzer.exe`
  und liefern damit immer die neueste Version, ohne dass die Seite angepasst werden muss.
- Die Versionsnummer im Download-Bereich wird beim Aufruf von der GitHub-API geholt (fehlt sie, bleibt der Hinweistext stehen).
- Die Sprache (Deutsch/Englisch) folgt der Browsersprache und lässt sich oben rechts umschalten.
- `img/og.png` ist das Vorschaubild für Social Media und Messenger.
- **Impressum:** Im Footer ist ein Platzhalter-Link `#impressum`. Dort den Link zu deinem Impressum eintragen.
  In Österreich ist es Pflicht (§ 5 ECG / § 25 MedienG).

## Screenshots erneuern

Die Bilder in `img/` stammen aus dem Demo-Modus der App (erfundene Netze, keine echten Daten):

```
wifi-analyzer.exe --demo
```

Screenshots als PNG aufnehmen, Windows-Titelleiste abschneiden, auf 1600 px Breite verkleinern und als WebP
(Qualität ~84) unter `img/<seite>.webp` speichern.

## Lokal ansehen

```
python -m http.server 8765 --directory website
```

Dann http://localhost:8765 öffnen.
