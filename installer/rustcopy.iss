; Inno Setup script for robocopy-ingest-cli (rustcopy).
;
; Packages the CLI, the notify-server and — as an OPTIONAL component — the desktop console
; (F60). Build artifacts only, no source changes.
;
; One installer, not two. Measured on 2 Set 2026: the console is 8.9 MB against a 13.7 MB CLI
; install, because Tauri renders through the system WebView2 instead of shipping a browser
; engine. Two separate installers would mean two version streams, two SmartScreen reputations to
; build from zero, and two things to keep in sync — a poor trade for 8.9 MB. For the same reason
; Tauri's own bundler stays off ("bundle.active": false in tauri.conf.json): it would produce a
; second MSI/NSIS for the console alone, which is precisely the split this avoids.
;
; Build:
;   1. npm --prefix crates/rustcopy-gui/ui ci          (only when packaging the console)
;   2. npm --prefix crates/rustcopy-gui/ui run build   (only when packaging the console)
;   3. cargo build --release --workspace --features rustcopy-cli/notify-server
;   4. "C:\Users\<you>\AppData\Local\Programs\Inno Setup 6\ISCC.exe" installer\rustcopy.iss
;   Output: installer-output\rustcopy-<version>-setup.exe
;
; The VERSION #define below must match Cargo.toml's [workspace.package].version. That is no
; longer left to memory: scripts/check-versions.sh fails CI when the four declarations
; (Cargo.toml, this file, tauri.conf.json, ui/package.json) disagree — version drift had bitten
; this repo before, and the previous wording of this comment admitted it without preventing it.

#define MyAppName "rustcopy (robocopy-ingest-cli)"
#define MyAppVersion "6.0.0"
#define MyAppPublisher "matrixNeo76"
#define MyAppURL "https://github.com/matrixNeo76/rustcopy"
#define MyAppExeName "robocopy_ingest.exe"
#define MyGuiExeName "rustcopy-gui.exe"
#define MyNotifyExeName "notify-server.exe"

[Setup]
AppId={{7B1E5C2A-2D8F-4A6B-9E3C-1F5A6D2B8C90}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
DefaultDirName={autopf}\rustcopy
DefaultGroupName=rustcopy
DisableProgramGroupPage=yes
DisableWelcomePage=no
OutputDir=..\installer-output
OutputBaseFilename=rustcopy-{#MyAppVersion}-setup
Compression=lzma2
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=admin
UninstallDisplayIcon={app}\{#MyAppExeName}
WizardStyle=modern
SetupLogging=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "italian"; MessagesFile: "compiler:Languages\Italian.isl"

[Types]
Name: "full"; Description: "CLI e console grafica"
Name: "cli"; Description: "Solo CLI (backup non presidiati)"
Name: "custom"; Description: "Scelta manuale"; Flags: iscustom

; The console is optional on purpose: a server that only runs scheduled backups has no use for a
; desktop window, and the CLI is the component that has to keep working unattended.
[Components]
Name: "cli"; Description: "CLI e notify-server"; Types: full cli custom; Flags: fixed
Name: "gui"; Description: "Console grafica (richiede WebView2)"; Types: full

[Tasks]
Name: "addtopath"; Description: "Aggiungi rustcopy al PATH di sistema (consigliato)"; GroupDescription: "Opzioni aggiuntive:"; Components: cli

[Files]
Source: "..\target\release\{#MyAppExeName}"; DestDir: "{app}"; Components: cli; Flags: ignoreversion
Source: "..\target\release\{#MyNotifyExeName}"; DestDir: "{app}"; Components: cli; Flags: ignoreversion skipifsourcedoesntexist
; The console carries its frontend inside the executable (Tauri embeds ui/dist), so there is no
; web asset directory to install beside it.
Source: "..\target\release\{#MyGuiExeName}"; DestDir: "{app}"; Components: gui; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Components: cli; Flags: ignoreversion isreadme
Source: "..\RUNBOOK.md"; DestDir: "{app}"; Components: cli; Flags: ignoreversion
Source: "..\CLAUDE.md"; DestDir: "{app}"; DestName: "NOTES.md"; Components: cli; Flags: ignoreversion

[Icons]
Name: "{group}\rustcopy - console"; Filename: "{app}\{#MyGuiExeName}"; Components: gui
Name: "{group}\Disinstalla rustcopy"; Filename: "{uninstallexe}"

[Code]
const
  VC_REDIST_URL = 'https://aka.ms/vs/17/release/vc_redist.x64.exe';
  WEBVIEW2_URL = 'https://developer.microsoft.com/microsoft-edge/webview2/';
  // The Evergreen WebView2 Runtime registers itself under this fixed client id.
  WEBVIEW2_CLIENT = '{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}';

// robocopy_ingest.exe is a Rust windows-msvc binary: it dynamically links VCRUNTIME140.dll,
// which does NOT ship with a clean Windows install (unlike the Universal CRT, present by
// default on Windows 10 1607+/11). Detect it via the registry key the VC++ Redistributable
// itself installs, rather than bundling a ~25 MB redistributable installer inside this setup
// (bundling/auto-downloading a second installer wasn't something to decide unilaterally here —
// flagging it clearly to the user at the end of setup is the safer default).
function IsVCRedistInstalled(): Boolean;
var
  installed: Cardinal;
begin
  Result :=
    (RegQueryDWordValue(HKLM, 'SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\X64', 'Installed', installed) and (installed = 1)) or
    (RegQueryDWordValue(HKLM, 'SOFTWARE\WOW6432Node\Microsoft\VisualStudio\14.0\VC\Runtimes\X64', 'Installed', installed) and (installed = 1));
end;

// The console renders through the system WebView2 Runtime rather than shipping a browser engine
// — which is why it costs 8.9 MB instead of ~150 — so that runtime has to be present. It ships
// with Windows 11 and reaches most updated Windows 10 machines through Windows Update, but LTSC
// and offline images can lack it. Detected and reported the same way as the VC++ redistributable
// above: warn, do not bundle a second installer, and never block setup.
function IsWebView2Installed(): Boolean;
var
  version: string;
begin
  Result :=
    (RegQueryStringValue(HKLM, 'SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\' + WEBVIEW2_CLIENT, 'pv', version) and (version <> '') and (version <> '0.0.0.0')) or
    (RegQueryStringValue(HKLM, 'SOFTWARE\Microsoft\EdgeUpdate\Clients\' + WEBVIEW2_CLIENT, 'pv', version) and (version <> '') and (version <> '0.0.0.0')) or
    (RegQueryStringValue(HKCU, 'SOFTWARE\Microsoft\EdgeUpdate\Clients\' + WEBVIEW2_CLIENT, 'pv', version) and (version <> '') and (version <> '0.0.0.0'));
end;

// --- Add/remove {app} from the system PATH (classic Inno Setup snippet, adapted) -------------
procedure EnvAddPath(Path: string);
var
  Paths: string;
begin
  if not RegQueryStringValue(HKEY_LOCAL_MACHINE,
    'SYSTEM\CurrentControlSet\Control\Session Manager\Environment', 'Path', Paths)
  then Paths := '';

  if Paths = '' then
    Paths := Path
  else if Pos(';' + Uppercase(Path) + ';', ';' + Uppercase(Paths) + ';') = 0 then
    Paths := Paths + ';' + Path
  else
    exit; // already present

  if not RegWriteExpandStringValue(HKEY_LOCAL_MACHINE,
    'SYSTEM\CurrentControlSet\Control\Session Manager\Environment', 'Path', Paths)
  then
    Log('EnvAddPath: could not write to the registry (needs admin).');
end;

procedure EnvRemovePath(Path: string);
var
  Paths: string;
  P: Integer;
begin
  if not RegQueryStringValue(HKEY_LOCAL_MACHINE,
    'SYSTEM\CurrentControlSet\Control\Session Manager\Environment', 'Path', Paths)
  then exit;

  P := Pos(';' + Uppercase(Path) + ';', ';' + Uppercase(Paths) + ';');
  if P = 0 then exit;

  Delete(Paths, P - 1, Length(Path) + 1);
  RegWriteExpandStringValue(HKEY_LOCAL_MACHINE,
    'SYSTEM\CurrentControlSet\Control\Session Manager\Environment', 'Path', Paths);
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep <> ssPostInstall then
    exit;

  if WizardIsTaskSelected('addtopath') then
    EnvAddPath(ExpandConstant('{app}'));

  // Checked here rather than in InitializeSetup because components are not chosen yet at that
  // point: warning about WebView2 on a CLI-only install would be noise about a runtime nothing
  // installed is going to use.
  if WizardIsComponentSelected('gui') and not IsWebView2Installed() then
    MsgBox(
      'La console grafica richiede il runtime WebView2 (Microsoft), non rilevato su questo ' +
      'sistema.' + #13#10 + #13#10 +
      'La CLI funziona comunque: e'' solo la finestra della console che non si aprirebbe. ' +
      'Scarica il runtime da:' + #13#10 +
      WEBVIEW2_URL,
      mbInformation, MB_OK);
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
    EnvRemovePath(ExpandConstant('{app}'));
end;

function InitializeSetup(): Boolean;
begin
  Result := True;
  if not IsVCRedistInstalled() then
    MsgBox(
      'rustcopy richiede il Visual C++ Redistributable x64 (Microsoft), non rilevato su questo ' +
      'sistema.' + #13#10 + #13#10 +
      'Il programma potrebbe non avviarsi senza. Scaricalo da:' + #13#10 +
      VC_REDIST_URL + #13#10 + #13#10 +
      'Setup continuera comunque.',
      mbInformation, MB_OK);
end;
