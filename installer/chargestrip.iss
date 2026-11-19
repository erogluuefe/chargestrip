; Chargestrip installer for Windows (Inno Setup 6).
; Build:    ISCC.exe /DAppVersion=0.2.0 installer\chargestrip.iss   → dist\Chargestrip-Setup-0.2.0.exe
; First:    cargo build --release
;
; No administrator rights: installs for the current user (%LOCALAPPDATA%\Programs\Chargestrip).
; "Start with Windows" uses the same registry value as the app's own menu entry.

#define AppName "Chargestrip"
#define AppExe "chargestrip.exe"
#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
; Must never change: updates install over the previous installation.
AppId={{6B0F3C2E-8D4A-4F7B-9C1E-2A5D7E9F0B13}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=erogluuefe
AppPublisherURL=https://github.com/erogluuefe/chargestrip
PrivilegesRequired=lowest
DefaultDirName={autopf}\{#AppName}
DisableDirPage=yes
DisableProgramGroupPage=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir=..\dist
OutputBaseFilename=Chargestrip-Setup-{#AppVersion}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ShowLanguageDialog=no
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}

[Languages]
; The first language (English) is used when the system language isn't listed.
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "turkish"; MessagesFile: "compiler:Languages\Turkish.isl"

[CustomMessages]
turkish.Autostart=Windows açılışında başlat
english.Autostart=Start with Windows
turkish.LaunchNow=Chargestrip'i şimdi başlat
english.LaunchNow=Launch Chargestrip now

[Tasks]
Name: "autostart"; Description: "{cm:Autostart}"

[Files]
Source: "..\target\release\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"

[Registry]
; Start with Windows if the box is ticked.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; \
  ValueName: "{#AppName}"; ValueData: """{app}\{#AppExe}"""; Tasks: autostart
; Always remove the value on uninstall (it may also have been set from the app's menu).
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; \
  ValueName: "{#AppName}"; Flags: uninsdeletevalue

[Run]
Filename: "{app}\{#AppExe}"; Description: "{cm:LaunchNow}"; Flags: nowait postinstall skipifsilent
; The in-app "Update" installs silently; relaunch the app when done.
Filename: "{app}\{#AppExe}"; Flags: nowait skipifnotsilent

[UninstallRun]
Filename: "{sys}\taskkill.exe"; Parameters: "/F /IM {#AppExe}"; Flags: runhidden; RunOnceId: "StopApp"

[Code]
// Close a running copy first so it doesn't lock the file during an update or reinstall.
function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Code: Integer;
begin
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM {#AppExe}', '', SW_HIDE, ewWaitUntilTerminated, Code);
  Result := '';
end;
