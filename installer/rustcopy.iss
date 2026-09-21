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
#define MyAppVersion "7.5.0"
#define MyAppPublisher "matrixNeo76"
#define MyAppURL "https://github.com/matrixNeo76/rustcopy"
#define MyAppExeName "robocopy_ingest.exe"
#define MyGuiExeName "rustcopy-gui.exe"
#define MyNotifyExeName "notify-server.exe"
#define MyShellDllName "rustcopy_shell.dll"

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
;
; "shell" is a subcomponent of "gui", not a sibling: rustcopy-shell's InvokeCommand locates
; rustcopy-gui.exe with runner::gui_beside(own_dll_path()) -- beside the DLL itself (F85,
; Milestone 3's console-handoff design) -- so the extension is inert without the console actually
; installed next to it. The "gui\shell" nesting keeps that dependency visible in the wizard
; instead of relying on an operator to notice it; NextButtonClick below is the actual backstop.
; "Check" hides an entry from the wizard entirely rather than just leaving it unchecked -- on
; Server Core there is no explorer.exe/desktop shell at all, so neither WebView2 (the console)
; nor a shell extension could ever run: offering them would just self-register a COM DLL nothing
; loads (F90, ROADMAP.md).
[Components]
Name: "cli"; Description: "CLI e notify-server"; Types: full cli custom; Flags: fixed
Name: "gui"; Description: "Console grafica (richiede WebView2)"; Types: full; Check: not IsServerCore
Name: "gui\shell"; Description: "Estensione Shell per Explorer (drag & drop, ""Copia con RustCopy"")"; Types: full; Check: not IsServerCore

[Tasks]
Name: "addtopath"; Description: "Aggiungi rustcopy al PATH di sistema (consigliato)"; GroupDescription: "Opzioni aggiuntive:"; Components: cli

[Files]
Source: "..\target\release\{#MyAppExeName}"; DestDir: "{app}"; Components: cli; Flags: ignoreversion
Source: "..\target\release\{#MyNotifyExeName}"; DestDir: "{app}"; Components: cli; Flags: ignoreversion skipifsourcedoesntexist
; The console carries its frontend inside the executable (Tauri embeds ui/dist), so there is no
; web asset directory to install beside it.
Source: "..\target\release\{#MyGuiExeName}"; DestDir: "{app}"; Components: gui; Flags: ignoreversion
; regserver calls DllRegisterServer/DllUnregisterServer automatically at install/uninstall --
; the DLL is self-registering (registry.rs), so no separate [Registry] section is needed here.
Source: "..\target\release\{#MyShellDllName}"; DestDir: "{app}"; Components: gui\shell; Flags: ignoreversion regserver
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

// --- Server/Server Core detection (F90, ROADMAP.md) -------------------------------------------
//
// This project's only real-machine verification so far (including the D29 incident) has been on
// Windows 11 client -- never a Server SKU. ProductType from GetWindowsVersionEx is the documented
// way to tell a Server (or domain controller) apart from a workstation; it says nothing about
// Server Core specifically, which is still ProductType = Server but has no explorer.exe/desktop
// shell at all -- that distinction only exists in the InstallationType registry value.
function IsServerSku(): Boolean;
var
  Version: TWindowsVersion;
begin
  GetWindowsVersionEx(Version);
  Result := Version.ProductType <> VER_NT_WORKSTATION;
end;

function IsServerCore(): Boolean;
var
  installType: string;
begin
  if not RegQueryStringValue(HKLM, 'SOFTWARE\Microsoft\Windows NT\CurrentVersion', 'InstallationType', installType) then
    installType := 'Client'; // undetectable: assume desktop capability rather than hide components wrongly
  Result := installType = 'Server Core';
end;

// docs/installation.md already documents "Windows 10 1607+/Server 2016+" as the real requirement
// (Universal CRT, present by default only from there on) but nothing enforced it before this --
// an older Server would accept the install and only fail later, at first launch, with a cryptic
// "this app can't run on your PC"-style error instead of a clear message here. Same warn-not-block
// treatment as IsVCRedistInstalled/IsWebView2Installed below: a hard MinVersion block is a bigger,
// separate decision, not made here.
function IsOsVersionSupported(): Boolean;
var
  Version: TWindowsVersion;
begin
  GetWindowsVersionEx(Version);
  // Windows 11 still reports NT major version 10 (same as Windows 10 and Server 2016+) -- only
  // the build number tells 1607+ apart from an older 10.0 release (1507/1511) that predates the
  // Universal CRT. 14393 is Windows 10 1607 / Windows Server 2016's build number.
  Result := (Version.Major > 10) or ((Version.Major = 10) and (Version.Build >= 14393));
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

// Backstop for the gui\shell nesting above: Inno's component tree unchecks/grays out a child
// when its parent is unchecked, but does not stop a *parent* from being deselected while a
// child selection from a "full"-type default is still logically pending on the same page (and a
// custom install can reach odd intermediate states while clicking around). Checked explicitly
// rather than trusted to the tree UI alone, since rustcopy-shell is genuinely non-functional
// without rustcopy-gui.exe beside it (see the [Components] comment above).
function NextButtonClick(CurPageID: Integer): Boolean;
begin
  Result := True;
  if (CurPageID = wpSelectComponents) and WizardIsComponentSelected('gui\shell')
    and not WizardIsComponentSelected('gui') then
  begin
    MsgBox(
      'L''estensione Shell richiede la console grafica: da sola non avvierebbe mai una copia, ' +
      'perche'' cerca rustcopy-gui.exe accanto a se''.' + #13#10 + #13#10 +
      'Seleziona anche "Console grafica" oppure deseleziona "Estensione Shell per Explorer".',
      mbError, MB_OK);
    Result := False;
  end;
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
  // SuppressibleMsgBox (not MsgBox) on every warning from here down: /SUPPRESSMSGBOXES does NOT
  // suppress a script-authored MsgBox (only Setup's own built-in prompts) -- verified against
  // Inno Setup's own docs, found by CodeRabbit reviewing this PR. Without this, an unattended
  // /VERYSILENT /SUPPRESSMSGBOXES install would hang waiting for a click nobody is there to give.
  if WizardIsComponentSelected('gui') and not IsWebView2Installed() then
    SuppressibleMsgBox(
      'La console grafica richiede il runtime WebView2 (Microsoft), non rilevato su questo ' +
      'sistema.' + #13#10 + #13#10 +
      'La CLI funziona comunque: e'' solo la finestra della console che non si aprirebbe. ' +
      'Scarica il runtime da:' + #13#10 +
      WEBVIEW2_URL,
      mbInformation, MB_OK, IDOK);

  // F90 (ROADMAP.md): Windows Server 2016/2019/2022 with Desktop Experience can run the shell
  // extension, unlike Server Core (already excluded from selection above) -- but on a Remote
  // Desktop Session Host, common on those SKUs, it loads into *every* signed-in user's
  // explorer.exe at once, not one personal desktop. Informational only, same as every other
  // warning in this script: never blocks setup.
  if WizardIsComponentSelected('gui\shell') and IsServerSku() then
    SuppressibleMsgBox(
      'Questo sistema e'' una SKU Windows Server. Su un Remote Desktop Session Host, comune su ' +
      'Server 2016/2019/2022, l''estensione Shell per Explorer si carica nella sessione di ' +
      'OGNI utente collegato contemporaneamente, non di un singolo desktop personale.' + #13#10 + #13#10 +
      'Setup continuera'' comunque -- valuta se installarla davvero su un host multi-utente.',
      mbInformation, MB_OK, IDOK);
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
    SuppressibleMsgBox(
      'rustcopy richiede il Visual C++ Redistributable x64 (Microsoft), non rilevato su questo ' +
      'sistema.' + #13#10 + #13#10 +
      'Il programma potrebbe non avviarsi senza. Scaricalo da:' + #13#10 +
      VC_REDIST_URL + #13#10 + #13#10 +
      'Setup continuera comunque.',
      mbInformation, MB_OK, IDOK);

  // F90 (ROADMAP.md): the Universal CRT rustcopy relies on ships by default only from Windows 10
  // 1607+ / Server 2016+ (already documented in docs/installation.md, never enforced before this).
  // Checked here, unlike the WebView2 warning below, because it applies to every component --
  // components are not chosen yet at InitializeSetup, but this warning does not depend on them.
  if not IsOsVersionSupported() then
    SuppressibleMsgBox(
      'Questa versione di Windows/Windows Server e'' precedente a quella richiesta ' +
      '(Windows 10 1607+ / Windows Server 2016+).' + #13#10 + #13#10 +
      'Il programma potrebbe non avviarsi, con un errore di sistema poco chiaro invece di ' +
      'questo avviso.' + #13#10 + #13#10 +
      'Setup continuera comunque.',
      mbInformation, MB_OK, IDOK);
end;
