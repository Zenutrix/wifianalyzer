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

### Grundriss-Messung (neu)
- Eigene Seite **Grundriss-Messung**, getrennt von der normalen Messung (Rundgang): Grundriss laden (PNG/JPG oder PDF), an der eigenen Position in den Plan klicken – dort wird gemessen.
- **Vorbereitung vor dem Start**: Pläne laden und je Stockwerk den Maßstab setzen (z. B. eine Tür abmessen), bevor die Messung läuft. Eine Leiste zeigt die drei Schritte; nach dem Laden öffnet sich das Maßstab-Werkzeug automatisch und springt zum nächsten Stockwerk ohne Maßstab. Die Grundrisse bleiben auch nach einem Neustart erhalten.
- **Kein Signal = grau**: Bereiche schwächer als die unterste Farbschwelle und Messpunkte ganz ohne Empfang werden grau statt rot dargestellt, mit weichem Übergang. Farbe einstellbar oder durchsichtig (Farbskala).
- **Mehrere Stockwerke** pro Messung: je Stockwerk ein Grundriss mit eigenen Messpunkten und eigener Heatmap. Reiter zum Wechseln, Rechtsklick zum Umbenennen (z. B. „EG“) oder Entfernen, „+ Stockwerk“ zum Hinzufügen. Bericht (PDF/HTML) und CSV enthalten alle Stockwerke.
- **PDF-Pläne**: Bei mehrseitigen PDFs erscheint eine Seitenauswahl mit Vorschaubildern; jede gewählte Seite wird ein Stockwerk. Gerendert wird mit dem in Windows eingebauten PDF-Renderer, ohne zusätzliche Bibliothek.
- **PDF-Pläne bleiben beim Zoomen scharf**: Der sichtbare Ausschnitt wird direkt aus dem PDF nachgerendert, solange die PDF-Datei am ursprünglichen Ort liegt. Die Messungsdatei merkt sich dafür den Pfad des PDFs.
- **Heatmap** der Ausleuchtung zwischen den Messpunkten; Punkte lassen sich verschieben, schwache Stellen sind sofort sichtbar.
- Der Grundriss wird in der Messungsdatei gespeichert und für die nächste Grundriss-Messung übernommen.
- Gespeicherte Messungen erscheinen jeweils bei ihrer Messart.
- **Maßstab**: zwei Punkte einer bekannten Strecke anklicken und die Länge in Metern eingeben – Heatmap und Maßstabsleiste rechnen in echten Metern.
- **Heatmap wie bei professionellen Survey-Tools**: Messpunkte beeinflussen sich gegenseitig (Interpolation), abseits der Punkte nimmt das Signal mit dem Pfadverlust ab; schwächer als die unterste Schwelle bleibt frei. Geschätzter Wert unter dem Mauszeiger.
- **Farbskala einstellbar**: dBm-Schwellen und Farben frei wählbar (Standard angelehnt an gängige Tools), wird gespeichert.
- **2,4 / 5 / 6 GHz**: pro Messpunkt wird der stärkste AP je Band gespeichert; Heatmap umschaltbar zwischen Messwert und den drei Bändern. Bericht (PDF/HTML) enthält eine Karte je Band, CSV die Werte.
- **Zoom und Verschieben** im Plan: Mausrad zoomt zum Mauszeiger, Ziehen auf freier Fläche (oder mit rechter/mittlerer Maustaste) verschiebt, Taste 0 passt wieder ein.
- **Deckkraft der Heatmap** per Regler, damit Raumnamen lesbar bleiben (wird gespeichert); Farblegende direkt im Plan.
- **Messpunkt-Liste neben dem Plan** auf breiten Bildschirmen (Klick wählt aus, Rechtsklick löscht); volle Tabelle per Umschalter.
- **Rückgängig (Strg+Z)** für erfasste, gesetzte, verschobene und gelöschte Messpunkte.
- Stockwerk entfernen nur noch über das Rechtsklick-Menü des Reiters, mit Rückfrage.

### Latenz und Durchsatz pro Messpunkt
- Ping zum Router und ins Internet (Ø, Min/Max, Jitter, Verlust) – läuft schon, während der Raumname eingegeben wird.
- Optionaler **Speedtest** (Download/Upload über speed.cloudflare.com), nur bei Verbindung mit der gemessenen SSID.

### PDF-Bericht direkt aus der App
- Eigener PDF-Erzeuger ohne Fremdbibliothek: Kennzahlen, Grundriss mit Heatmap, Signalverlauf, Tabelle mit Bewertung, Latenz und Durchsatz, Legende, Hinweise, Seitenzahlen.
- HTML-Bericht und CSV enthalten ebenfalls Grundriss bzw. Netzwerkwerte.

### Messpunkte pro Raum
- Während einer Messung **Enter** drücken, Raum/Ort (und optional eine Notiz) eingeben, Enter. Der Wert dieser Stelle wird gemittelt, solange das Fenster offen ist, und als Messpunkt mit Bewertung (Sehr gut / Gut / Ausreichend / Schwach) gespeichert.
- Liste der Messpunkte unter dem Verlauf: Signal, Bewertung, AP, Kanal, Datenrate, Notiz. Name und Notiz lassen sich nachträglich ändern.
- Messungen werden automatisch in `Dokumente\WiFi Analyzer` gespeichert und lassen sich wieder öffnen (Liste „Gespeicherte Messungen“).
- Export als **Bericht für Kunden** (HTML, im Browser als PDF druckbar) mit Zusammenfassung, Verlauf, Tabelle und Hinweisen, als **Liste der Messpunkte** (CSV/Excel) oder mit allen Messwerten.

### Oberfläche
- Professionellerer Look: Windows-Schrift Segoe UI (Semibold für Überschriften und Kennzahlen) und ein einheitliches Linien-Icon-Set (Phosphor) statt bunter Emojis.
- Seitenleiste aufgeräumt: Filter nur auf Analyse-Seiten, seltene Filter eingeklappt, kompakte Verbindungskarte.
- Diagramme mit ruhigem Hintergrund und dezentem Gitter, Achsen auch bei hoher Bildschirmskalierung beschriftet.
- Einheitlich hohe Kacheln, AP-Anzahl als Abzeichen in der Netzwerkliste, Schnellauswahl im AP-Finder.
- Heller Modus: besser lesbare AP-Farben.
- Messung: Beschriftungen im Diagramm gestaffelt statt übereinander, Legende über dem Diagramm, kleinerer Tacho, Messpunkte und Ereignisse als Reiter.
- Kennzahlen passen ihre Schriftgröße an, statt abgeschnitten zu werden; kürzere Kacheltexte in der Übersicht und „Alle anzeigen »“ im Kartenkopf.
- Netzwerkliste: Spaltenbreiten werden gespeichert (Rechtsklick auf Spaltenkopf = zurücksetzen), abgeschnittene Texte zeigen den vollen Inhalt beim Überfahren, Erklärung des Farbstreifens; darunter ein Vergleich aller APs des gewählten Netzes mit Signalverlauf.
- Kopfleiste entschlackt: Design, Updates, Tastenkürzel und Über im ⋯-Menü (Update-Knopf erscheint nur bei neuer Version).
- Neue Tastenkürzel: Leertaste (Scan anhalten), M (Messpunkt), Strg+Z (Rückgängig), F1 (Übersicht aller Tastenkürzel).
- **Neues Logo**: ein „W“ als Linie im Verlauf Cyan → Violett, aus dessen mittlerer Spitze WLAN-Wellen aufsteigen. Im **Ladefenster baut es sich auf** (Kachel, das W zeichnet sich, die Wellen ploppen auf und senden weiter), ebenso auf der Website.
- **Deinstallieren** direkt in der App (Einstellungen → Programm) mit Rückfrage; die portable Version zeigt, wo die EXE liegt. Daneben ein Link zu den installierten Apps in Windows.
- **Farben wie im Logo**: Akzent in Indigo, Hauptaktionen mit dem Verlauf des Logos, Markierung der aktiven Seite in Cyan → Violett. Pro Seite ist nur noch die Hauptaktion farbig, „Messung stoppen“ ist rot umrandet.
- **Dialoge** ohne farbigen Titelbalken, mit größeren Ecken und weichem Schatten; der Hintergrund dunkelt ab, solange ein Dialog offen ist. Meldungen erscheinen oben mittig und verdecken keine Inhalte mehr.
- **Übersicht** mit **WLAN-Bewertung** (0–100, Urteil und die wichtigsten Gründe); Frequenzbänder und Sicherheit in einer Karte „Umgebung“.
- **Grundriss-Seite mit mehr Platz für den Plan**: Live-Wert in der Messpunkt-Zeile, Stockwerke und Hinweis in einer Zeile, Werkzeuge und Zoom schweben auf dem Plan. **Vollbild** für die Begehung mit F11.
- **Seitenleiste einklappbar** auf Symbole (Knopf unten oder Strg+B), wird gespeichert.
- **Anzeigegröße** einstellbar (Strg + / Strg − oder Einstellungen, 70–160 %), wird gespeichert.
- Leere Messseite mit großem Startknopf, sanftes Einblenden beim Seitenwechsel, dBm-Werte überall in derselben Schrift.
- **Berichte im neuen Look**: PDF mit Verlaufslinie und Logo im Kopf, HTML-Bericht mit Logo und neuen Farben.
- **Demo-Modus** (`--demo`) mit erfundener WLAN-Umgebung für Präsentationen und Screenshots.

### Website
- Neue Website mit Screenshots, Funktionsübersicht, Detail-Abschnitten, Hell/Dunkel-Vergleich, Bildergalerie mit Großansicht, Technik-Übersicht, Download (Installer und portable) und FAQ – ohne externe Schriften, Skripte oder Abfragen.

### Messung (Walk-Survey) überarbeitet
- Der Verlauf läuft flüssig mit: Die Linie reicht bis „jetzt“, ein Live-Punkt markiert den aktuellen Wert, Neuzeichnen 10× pro Sekunde während der Messung.
- Die Zeitachse wächst in festen Stufen (1 / 2 / 5 / 10 min …) statt jede Sekunde neu zu skalieren.
- Scan-Werte werden als Stufenlinie gezeigt (ein Wert gilt bis zum nächsten Scan).
- Während einer Messung wird alle 2 s gescannt, der erste Wert kommt sofort.
- Hinweis, ob mit Verbindung (jede Sekunde) oder nur per Scan gemessen wird; „Warte auf den ersten Messwert …“.

### Behoben
- Einstellungsdateien mit UTF-8-BOM wurden nicht korrekt gelesen.
- Unverständlicher Fehlercode bei ausgeschaltetem WLAN.
- Messungen mit gleicher SSID innerhalb derselben Minute überschrieben sich gegenseitig – jetzt „(2)“, „(3)“ …
- CSV „Alle Messwerte“: Spalten Uhrzeit/Typ waren vertauscht; Zeiten mit Dezimalkomma für Excel.
- CSV-Export entschärft Formeln in SSIDs (Schutz vor manipulierten Netzwerknamen).
- Umbenannte Messpunkte werden auch im Verlauf umbenannt.
- Der gespeicherte Wert eines Messpunkts entspricht dem im Dialog angezeigten.
- Updates: Prüfsumme ist Pflicht; schlägt der Installer fehl oder wird die Administrator-Abfrage abgebrochen, startet die bisherige Version wieder; Neustart nach dem Update als normaler Benutzer.
- Scanner versucht es erneut, wenn der WLAN-Dienst beim Start noch nicht lief.
- Einstellungen werden sicher (atomar) gespeichert; der Demo-Modus verändert keine Einstellungen.
- Das Standort-Symbol in der Taskleiste blinkte bei jedem Scan auf, wodurch die Taskleiste ständig sprang. Es bleibt jetzt ruhig stehen, solange die App läuft.

## [1.3.0] – 2026-10-30
- Walk-Survey-Messung mit Roaming-, Abbruch- und Sticky-Client-Erkennung, Verbindung jede Sekunde.

## [1.2.0] – 2026-10-30
- UI aufgeräumt, stabile Sortierungen, Leerlauf-CPU von 34 % auf ca. 1 % eines Kerns gesenkt.

## [1.1.0] – 2026-10-29
- Standort-Berechtigungsabfrage, Lade-Popup, App-Icon.

## [1.0.0] – 2026-10-29
- Erste Version: Netzwerkliste, Kanäle, Kanalbewertung, Analyse, Signal-Finder, Hersteller-Erkennung.
