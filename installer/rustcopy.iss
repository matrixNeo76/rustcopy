; Inno Setup script for robocopy-ingest-cli (rustcopy).
;
; Packages the CLI, the notify-server and — as an OPTIONAL component — the desktop console
; (F60). Build artifacts only, no source changes.
;
; One installer, not two: two separate installers would mean two version streams, two SmartScreen
; reputations to build from zero, and two things to keep in sync.
;
; The console is the Slint one (crates/rustcopy-ui, GUI Slint plan phase 5c): one process, no web
; runtime, nothing to install first. Its executable is built as rustcopy-ui.exe and INSTALLED as
; rustcopy-gui.exe, the name the Shell extension (runner::gui_beside), the Start menu and the
; scripts already look for -- renaming it at install time keeps all of them working untouched.
; The previous Tauri console (crates/rustcopy-gui) is no longer packaged; its removal is a separate
; step.
;
; Build:
;   1. cargo build --release -p rustcopy-cli -p rustcopy-shell -p rustcopy-ui --features rustcopy-cli/notify-server
;   2. "C:\Users\<you>\AppData\Local\Programs\Inno Setup 6\ISCC.exe" installer\rustcopy.iss
;   Output: installer-output\rustcopy-<version>-setup.exe
;
; The VERSION #define below must match Cargo.toml's [workspace.package].version. That is no
; longer left to memory: scripts/check-versions.sh fails CI when the declarations
; (Cargo.toml, this file, tauri.conf.json, ui/package.json) disagree — version drift had bitten
; this repo before, and the previous wording of this comment admitted it without preventing it.

#define MyAppName "rustcopy (robocopy-ingest-cli)"
#define MyAppVersion "7.8.1"
#define MyAppPublisher "matrixNeo76"
#define MyAppURL "https://github.com/matrixNeo76/rustcopy"
#define MyAppExeName "robocopy_ingest.exe"
#define MyGuiExeName "rustcopy-gui.exe"
; What cargo builds; installed under MyGuiExeName (see the header).
#define MyGuiSourceName "rustcopy-ui.exe"
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
; Server Core there is no explorer.exe/desktop shell at all, so neither the console
; nor a shell extension could ever run: offering them would just self-register a COM DLL nothing
; loads (F90, ROADMAP.md).
[Components]
Name: "cli"; Description: "CLI e notify-server"; Types: full cli custom; Flags: fixed
Name: "gui"; Description: "Console grafica"; Types: full; Check: not IsServerCore
Name: "gui\shell"; Description: "Estensione Shell per Explorer (drag & drop, ""Copia con RustCopy"")"; Types: full; Check: not IsServerCore

[Tasks]
Name: "addtopath"; Description: "Aggiungi rustcopy al PATH di sistema (consigliato)"; GroupDescription: "Opzioni aggiuntive:"; Components: cli

[Files]
Source: "..\target\release\{#MyAppExeName}"; DestDir: "{app}"; Components: cli; Flags: ignoreversion
Source: "..\target\release\{#MyNotifyExeName}"; DestDir: "{app}"; Components: cli; Flags: ignoreversion skipifsourcedoesntexist
; One self-contained executable (the interface is compiled in): no asset directory beside it.
Source: "..\target\release\{#MyGuiSourceName}"; DestDir: "{app}"; DestName: "{#MyGuiExeName}"; Components: gui; Flags: ignoreversion
; No regserver flag (F92 / D30): with it, a DLL that cannot register -- e.g. a missing runtime -- made
; Setup roll the WHOLE install back and exit with code 5. Registration is done from [Code]
; (RegisterShellExtension, install-report.pas) where a failure is reported and the rest installs.
Source: "..\target\release\{#MyShellDllName}"; DestDir: "{app}"; Components: gui\shell; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Components: cli; Flags: ignoreversion isreadme
Source: "..\RUNBOOK.md"; DestDir: "{app}"; Components: cli; Flags: ignoreversion
Source: "..\CLAUDE.md"; DestDir: "{app}"; DestName: "NOTES.md"; Components: cli; Flags: ignoreversion

; The AppUserModelID is what lets Windows show the console's end-of-copy notification: a toast from a
; desktop program is accepted only for an identity that has a Start menu shortcut carrying it (the same
; string as toast::APP_USER_MODEL_ID in crates/rustcopy-ui) and, below, a registered display name.
[Registry]
Root: HKLM; Subkey: "SOFTWARE\Classes\AppUserModelId\rustcopy.console"; ValueType: string; ValueName: "DisplayName"; ValueData: "rustcopy"; Flags: uninsdeletekey; Components: gui

[Icons]
Name: "{group}\rustcopy - console"; Filename: "{app}\{#MyGuiExeName}"; Components: gui; AppUserModelID: "rustcopy.console"
Name: "{group}\Disinstalla rustcopy"; Filename: "{uninstallexe}"

[Code]
const
  // Kept only because the install report (install-report.pas) still says whether the runtime is
  // there: the console no longer needs it, and the line is informational.
  WEBVIEW2_CLIENT = '{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}';

// (There is no check for the Visual C++ Redistributable or for WebView2 any more: from 7.7.0 every
// binary links the C runtime statically -- .cargo/config.toml -- and the console draws itself,
// so neither is a requirement.)

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

// Rust's x86_64-pc-windows-msvc target requires Windows 10 or Windows Server 2016 and later (its
// official platform-support page), which docs/installation.md documents as the real requirement.
// An older system would accept the install and only fail at first launch, with a cryptic "this app
// can't run on your PC"-style error instead of a clear message here. Same warn-not-block treatment
// as every other warning here: a hard MinVersion block is a bigger, separate decision, not made here.
function IsOsVersionSupported(): Boolean;
var
  Version: TWindowsVersion;
begin
  GetWindowsVersionEx(Version);
  // Windows 11 still reports NT major version 10 (same as Windows 10 and Server 2016+) -- only
  // the build number tells 1607+ apart from an older 10.0 release (1507/1511). 14393 is Windows 10
  // 1607 / Windows Server 2016's build number.
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

#include "install-report.pas"

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
  if CurStep = ssInstall then
  begin
    ReportSelection();
    exit;
  end;
  if CurStep = ssDone then
  begin
    // Only reached after a successful install: DeinitializeSetup uses it to tell "completed" from
    // "failed or cancelled" when it writes the report.
    InstallCompleted := True;
    exit;
  end;
  if CurStep <> ssPostInstall then
    exit;

  ReportInstalledFiles();

  if WizardIsTaskSelected('addtopath') then
    EnvAddPath(ExpandConstant('{app}'));

  // The Shell extension is optional: if it cannot be registered the rest of rustcopy is already
  // installed and stays that way. The reason (regsvr32's exit code and what it means) is in the
  // install report.
  if WizardIsComponentSelected('gui\shell') then
    if not RegisterShellExtension(ExpandConstant('{app}\{#MyShellDllName}')) then
      SuppressibleMsgBox(
        'L''estensione Shell per Explorer non e'' stata registrata: resta disattivata.' + #13#10 + #13#10 +
        'Il resto di rustcopy (CLI e console) e'' installato regolarmente. Il motivo e'' nel rapporto di ' +
        'installazione, in ' + ReportDirectory() + '.',
        mbInformation, MB_OK, IDOK);

  // SuppressibleMsgBox (not MsgBox) on every warning from here down: /SUPPRESSMSGBOXES does NOT
  // suppress a script-authored MsgBox (only Setup's own built-in prompts) -- verified against
  // Inno Setup's own docs, found by CodeRabbit reviewing this PR. Without this, an unattended
  // /VERYSILENT /SUPPRESSMSGBOXES install would hang waiting for a click nobody is there to give.

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
  begin
    // What the regserver flag used to do on uninstall. Done before the files are removed.
    UnregisterShellExtension(ExpandConstant('{app}\{#MyShellDllName}'));
    EnvRemovePath(ExpandConstant('{app}'));
  end;
end;

function InitializeSetup(): Boolean;
begin
  Result := True;
  ReportStart();

  // F90 (ROADMAP.md): Rust's Windows target needs Windows 10 / Server 2016 or later (already
  // documented in docs/installation.md, never enforced before this). Checked here, rather than
  // after the components are chosen, because it applies to every component -- components are not chosen yet at
  // InitializeSetup, but this warning does not depend on them.
  if not IsOsVersionSupported() then
    SuppressibleMsgBox(
      'Questa versione di Windows/Windows Server e'' precedente a quella richiesta ' +
      '(Windows 10 1607+ / Windows Server 2016+).' + #13#10 + #13#10 +
      'Il programma potrebbe non avviarsi, con un errore di sistema poco chiaro invece di ' +
      'questo avviso.' + #13#10 + #13#10 +
      'Setup continuera comunque.',
      mbInformation, MB_OK, IDOK);
end;

procedure DeinitializeSetup();
begin
  ReportFinish();
end;
