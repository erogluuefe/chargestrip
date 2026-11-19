; Chargestrip Windows kurulumu (Inno Setup 6).
; Derleme:  ISCC.exe /DAppVersion=0.2.0 installer\chargestrip.iss   → dist\Chargestrip-Setup-0.2.0.exe
; Önce:     cargo build --release
;
; Yönetici izni istemez; kullanıcının kendi klasörüne (%LOCALAPPDATA%\Programs\Chargestrip) kurar.
; Açılışta başlatma, uygulamanın sağ tık menüsüyle aynı kayıt defteri değerini kullanır.

#define AppName "Chargestrip"
#define AppExe "chargestrip.exe"
#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
; Kimlik sabit kalmalı: güncellemeler eski kurulumun üstüne yazılır.
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
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}

[Languages]
Name: "turkish"; MessagesFile: "compiler:Languages\Turkish.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

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
; Kutucuk işaretliyse açılışta başlat.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; \
  ValueName: "{#AppName}"; ValueData: """{app}\{#AppExe}"""; Tasks: autostart
; Kaldırırken değeri her durumda sil (uygulamanın menüsünden açılmış olsa da).
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; \
  ValueName: "{#AppName}"; Flags: uninsdeletevalue

[Run]
Filename: "{app}\{#AppExe}"; Description: "{cm:LaunchNow}"; Flags: nowait postinstall skipifsilent
; Uygulamanın kendi "Güncelle" komutu sessiz kurar; bitince uygulamayı yeniden açar.
Filename: "{app}\{#AppExe}"; Flags: nowait skipifnotsilent

[UninstallRun]
Filename: "{sys}\taskkill.exe"; Parameters: "/F /IM {#AppExe}"; Flags: runhidden; RunOnceId: "StopApp"

[Code]
// Güncelleme/yeniden kurulumda çalışan kopya dosyayı kilitlemesin diye önce kapatılır.
function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Code: Integer;
begin
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM {#AppExe}', '', SW_HIDE, ewWaitUntilTerminated, Code);
  Result := '';
end;
