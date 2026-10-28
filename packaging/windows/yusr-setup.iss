; -- Yusr (يُسر) Windows Self-Contained Installer Script (Inno Setup) --
; Bundles: Core Application + Complete Embedded PostgreSQL + llama.cpp runtime + System DLLs
; Excludes: Local AI Model (Downloaded on-demand post-install or via checkbox)

#define AppName "Yusr"
#define AppVersion "0.2.1"
#define AppPublisher "ZeidexSoft"
#define AppURL "https://zeidexsoft.com"
#define AppExeName "Yusr.exe"

[Setup]
AppId={{6F1B8C2E-4D3A-4E5B-9A7C-2B9D0E4F8A61}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL={#AppURL}
AppSupportURL={#AppURL}
AppUpdatesURL={#AppURL}
DefaultDirName={autopf}\{#AppName}
DisableProgramGroupPage=yes
LicenseFile=..\..\LICENSE
OutputDir=..\..\release-assets
OutputBaseFilename=Yusr-Setup-{#AppVersion}
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=admin

[Languages]
Name: "arabic"; MessagesFile: "compiler:Languages\Arabic.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"
Name: "autostart"; Description: "تشغيل يُسر تلقائياً عند بدء تشغيل ويندوز (Start automatically on Windows boot)"; GroupDescription: "خيارات التشغيل (Startup Options)"
Name: "downloadmodel"; Description: "تنزيل نموذج المستشار الذكي المحلي (~1.3 جيجابايت) الآن (Download Local AI Model Now)"; GroupDescription: "المستشار الذكي (Local AI Advisor)"; Flags: unchecked

[Files]
; Main Executable
Source: "..\..\app\src-tauri\target\release\{#AppExeName}"; DestDir: "{app}"; Flags: ignoreversion

; Embedded PostgreSQL Cluster Binaries, Libs, and Shares
Source: "..\..\app\resources\postgresql\*"; DestDir: "{app}\resources\postgresql"; Flags: ignoreversion recursesubdirs createallsubdirs

; llama.cpp Offline Engine
Source: "..\..\app\resources\llama\*"; DestDir: "{app}\resources\llama"; Flags: ignoreversion recursesubdirs createallsubdirs

; Pre-compiled Web UI Assets
Source: "..\..\app\resources\web\*"; DestDir: "{app}\resources\web"; Flags: ignoreversion recursesubdirs createallsubdirs

; Visual C++ Runtime & Essential System DLLs
Source: "..\..\app\resources\postgresql\bin\vcruntime140.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\app\resources\postgresql\bin\vcruntime140_1.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\app\resources\postgresql\bin\msvcp140.dll"; DestDir: "{app}"; Flags: ignoreversion

; On-demand model downloader script
Source: "download-model.ps1"; DestDir: "{app}\scripts"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExeName}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExeName}"; Tasks: desktopicon

[Registry]
; Windows Boot Autostart Key
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "{#AppName}"; ValueData: """{app}\{#AppExeName}"""; Tasks: autostart; Flags: uninsdeletevalue

[Run]
; Launch on-demand local model download in background if user checked the option
Filename: "powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File ""{app}\scripts\download-model.ps1"""; Tasks: downloadmodel; Flags: runhidden nowait
; Run Application
Filename: "{app}\{#AppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(AppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent
