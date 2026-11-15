; Per-user installer for xmas-tui (no admin rights needed).
; Built by CI:  iscc /DAppVersion=1.2.3 /DSrc=path\to\stage packaging\windows\xmas-tui.iss
#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef Src
  #define Src "..\..\dist\xmas-tui-windows"
#endif

[Setup]
AppId={{7F3C2A1E-6B0D-4E9A-9C51-5A2D8E4B1C7F}
AppName=xmas-tui
AppVersion={#AppVersion}
AppPublisher=dash1101
AppPublisherURL=https://github.com/dash1101/xmas-tui
DefaultDirName={localappdata}\Programs\xmas-tui
DefaultGroupName=xmas-tui
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
OutputBaseFilename=xmas-tui-setup-x86_64
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesInstallIn64BitMode=x64compatible
ArchitecturesAllowed=x64compatible
UninstallDisplayIcon={app}\xmas-tui.exe
SetupIconFile=..\..\assets\icon\xmas-tui.ico
LicenseFile={#Src}\LICENSE

[Tasks]
Name: "startup"; Description: "Run at startup (as the desktop wallpaper)"
Name: "desktopicon"; Description: "Desktop shortcut to the settings"; Flags: unchecked

[Files]
Source: "{#Src}\xmas-tui.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Src}\README.txt"; DestDir: "{app}"; Flags: ignoreversion isreadme
Source: "{#Src}\config.example.toml"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Src}\LICENSE*"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\xmas-tui (start the wallpaper)"; Filename: "{app}\xmas-tui.exe"
Name: "{group}\xmas-tui settings"; Filename: "{app}\xmas-tui.exe"; Parameters: "--settings-window"
Name: "{group}\Uninstall xmas-tui"; Filename: "{uninstallexe}"
Name: "{userdesktop}\xmas-tui settings"; Filename: "{app}\xmas-tui.exe"; Parameters: "--settings-window"; Tasks: desktopicon

[Registry]
; the same entry the in-app "Run at startup" toggle reads and writes
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "xmas-tui"; ValueData: """{app}\xmas-tui.exe"""; Tasks: startup; Flags: uninsdeletevalue

[Run]
Filename: "{app}\xmas-tui.exe"; Description: "Start the wallpaper now"; Flags: nowait postinstall skipifsilent

[UninstallRun]
Filename: "{sys}\taskkill.exe"; Parameters: "/F /IM xmas-tui.exe"; Flags: runhidden; RunOnceId: "StopXmasTui"

[UninstallDelete]
Type: files; Name: "{app}\xmas-tui.old.exe"
