; WiFi Analyzer – Installer (Inno Setup 6.7+)
;
; Bauen:   tools\package.ps1  (ruft ISCC automatisch auf)
;   oder   ISCC.exe /DAppVersion=2.0.0 /DSourceExe=..\target\release\wifi-analyzer.exe installer\WiFiAnalyzer.iss
;
; Standard: Installation nur für den aktuellen Benutzer (%LOCALAPPDATA%\Programs\WiFi Analyzer),
; ohne Administratorrechte. Im Setup kann alternativ „für alle Benutzer“ gewählt werden.
;
; Updates aus der App heraus laufen still:  Setup.exe /SILENT /SP- /SUPPRESSMSGBOXES /RELAUNCH

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceExe
  #define SourceExe "..\target\release\wifi-analyzer.exe"
#endif
#ifndef OutputDir
  #define OutputDir "..\dist"
#endif

#define AppName      "WiFi Analyzer"
#define AppExe       "WiFiAnalyzer.exe"
#define AppPublisher "Thomas Schöpf"
#define AppWebsite   "https://wifianalyzer.schoepf-tirol.at"
#define AppRepo      "https://github.com/zenutrix/wifianalyzer"

[Setup]
; Feste ID – nie ändern, sonst erkennt Windows Updates nicht als dieselbe App
AppId={{6F1C2B7E-4D8A-4E3B-9A51-2C7D0E5B8F14}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL={#AppWebsite}
AppSupportURL={#AppRepo}/issues
AppUpdatesURL={#AppRepo}/releases
AppCopyright=© 2026 {#AppPublisher} · GPL-3.0
VersionInfoVersion={#AppVersion}
VersionInfoDescription={#AppName} Setup
VersionInfoCompany={#AppPublisher}

; Pro Benutzer ohne Adminrechte; Auswahl „für alle Benutzer“ im Setup möglich
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
DefaultDirName={autopf}\{#AppName}
DisableProgramGroupPage=yes
UsePreviousAppDir=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0

; laufende App vor dem Kopieren schließen
CloseApplications=force
RestartApplications=no

UninstallDisplayName={#AppName}
UninstallDisplayIcon={app}\{#AppExe}
SetupIconFile=..\assets\icon.ico

OutputDir={#OutputDir}
OutputBaseFilename=WiFiAnalyzer-Setup
Compression=lzma2/max
SolidCompression=yes

; Optik: dunkel wie die App
WizardStyle=modern dark hidebevels
WizardBackColor=#0d1117
WizardBackImageFile=art\back.png
WizardImageFile=art\side-202.png,art\side-269.png,art\side-336.png,art\side-430.png,art\side-534.png
WizardSmallImageFile=art\small-58.png,art\small-77.png,art\small-97.png,art\small-124.png,art\small-159.png

ShowLanguageDialog=no
LanguageDetectionMethod=uilanguage
DisableWelcomePage=no
DisableReadyPage=no

[Languages]
Name: "de"; MessagesFile: "compiler:Languages\German.isl"
Name: "en"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
de.Tagline=WLAN-Analyse für Techniker unter Windows
en.Tagline=Wi-Fi analysis for technicians on Windows
de.DesktopIcon=Verknüpfung auf dem Desktop erstellen
en.DesktopIcon=Create a desktop shortcut
de.Shortcuts=Verknüpfungen:
en.Shortcuts=Shortcuts:
de.LaunchApp=WiFi Analyzer jetzt starten
en.LaunchApp=Launch WiFi Analyzer now
de.RemoveSettings=Sollen auch Ihre Einstellungen und die Hersteller-Datenbank entfernt werden?%n%n(%APPDATA%\WifiAnalyzer)
en.RemoveSettings=Also remove your settings and the vendor database?%n%n(%APPDATA%\WifiAnalyzer)
de.WebsiteLink=Website & Hilfe
en.WebsiteLink=Website & help

[Messages]
de.WelcomeLabel2=Dieser Assistent installiert [name/ver] auf Ihrem Computer.%n%nWLAN-Netze sehen, Kanäle bewerten, Ausleuchtung messen und Access Points orten – übersichtlich in einer App.
en.WelcomeLabel2=This wizard will install [name/ver] on your computer.%n%nSee Wi-Fi networks, rate channels, survey coverage and locate access points – all in one clean app.
de.FinishedLabel=[name] wurde erfolgreich installiert.%n%nSie finden die App im Startmenü unter „WiFi Analyzer“.
en.FinishedLabel=[name] has been installed successfully.%n%nYou will find it in the Start menu under "WiFi Analyzer".

[Tasks]
Name: "desktopicon"; Description: "{cm:DesktopIcon}"; GroupDescription: "{cm:Shortcuts}"

[Files]
Source: "{#SourceExe}";           DestDir: "{app}"; DestName: "{#AppExe}"; Flags: ignoreversion
Source: "..\assets\icon.ico";     DestDir: "{app}"; Flags: ignoreversion
Source: "..\assets\README.txt";   DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE";             DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}";   Filename: "{app}\{#AppExe}"; Comment: "{cm:Tagline}"; AppUserModelID: "SchoepfTirol.WiFiAnalyzer"
Name: "{autodesktop}\{#AppName}";    Filename: "{app}\{#AppExe}"; Comment: "{cm:Tagline}"; Tasks: desktopicon

[Run]
; normale Installation: Häkchen „jetzt starten“ auf der letzten Seite
Filename: "{app}\{#AppExe}"; Description: "{cm:LaunchApp}"; Flags: nowait postinstall skipifsilent
; Update aus der App heraus (still): danach automatisch neu starten
Filename: "{app}\{#AppExe}"; Flags: nowait; Check: ShouldRelaunch

[UninstallDelete]
Type: files; Name: "{app}\{#AppExe}.old"

[Code]
function ShouldRelaunch: Boolean;
var
  I: Integer;
begin
  Result := False;
  for I := 1 to ParamCount do
    if CompareText(ParamStr(I), '/RELAUNCH') = 0 then
      Result := True;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Dir: String;
begin
  if CurUninstallStep = usPostUninstall then
  begin
    Dir := ExpandConstant('{userappdata}\WifiAnalyzer');
    if DirExists(Dir) and not UninstallSilent then
      if SuppressibleMsgBox(CustomMessage('RemoveSettings'), mbConfirmation, MB_YESNO or MB_DEFBUTTON2, IDNO) = IDYES then
        DelTree(Dir, True, True, True);
  end;
end;
