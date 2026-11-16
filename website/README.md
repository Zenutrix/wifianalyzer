# Website schoepf-tirol.at/Project/WifiAnalyzer

Statische Landingpage ohne Build-Schritt, ohne externe Skripte und ohne Google Fonts
(keine Datenübertragung an Dritte – DSGVO-freundlich).

**Veröffentlichen:** den kompletten Inhalt (`index.html`, `logo.svg`, `icon.png`, `img/`) in das Verzeichnis
`/Project/WifiAnalyzer/` auf `schoepf-tirol.at` hochladen. Fertig.

- Die Download-Buttons zeigen auf `…/releases/latest/download/WiFiAnalyzer-Setup.exe` bzw. `WiFiAnalyzer.exe`
  und liefern damit immer die neueste Version, ohne dass die Seite angepasst werden muss.
- Die Seite lädt nichts von fremden Servern (keine Schriften, keine Skripte, keine API-Abfragen). Erst der Klick auf einen Download führt zu GitHub.
- Screenshots gibt es in 1600 und 2400 px (`srcset`), ein Klick öffnet sie groß (Lightbox).
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
(Qualität ~84) unter `img/<seite>.webp` speichern, zusätzlich in 2400 px als `img/<seite>-2400.webp`.

## Lokal ansehen

```
python -m http.server 8765 --directory website
```

Dann http://localhost:8765 öffnen.
