; Inno Setup script for robocopy-ingest-cli (rustcopy).
;
; Packages the CLI as it exists TODAY (no GUI, no service) — build artifacts only, no source
; changes. This does not conflict with the planned 8.0.0 Tauri milestone: that one bundles a
; workspace-split GUI crate via Tauri's own bundler (F60); this one packages the existing
; single-package CLI binaries via Inno Setup, for anyone who wants a normal Windows setup.exe
; before the GUI exists.
;
; Build:
;   1. cargo build --release --features notify-server   (from the repo root)
;   2. "C:\Users\<you>\AppData\Local\Programs\Inno Setup 6\ISCC.exe" installer\rustcopy.iss
;   Output: installer-output\rustcopy-<version>-setup.exe
;
; The VERSION #define below must be kept in sync with Cargo.toml's [package].version — there is
; no automated sync (see the "known limitation" note in ROADMAP.md / AGENTS.md pattern of this
; project: version drift has bitten this repo before).

#define MyAppName "rustcopy (robocopy-ingest-cli)"
#define MyAppVersion "5.4.0"
#define MyAppPublisher "matrixNeo76"
#define MyAppURL "https://github.com/matrixNeo76/rustcopy"
#define MyAppExeName "robocopy_ingest.exe"
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

[Tasks]
Name: "addtopath"; Description: "Aggiungi rustcopy al PATH di sistema (consigliato)"; GroupDescription: "Opzioni aggiuntive:"

[Files]
Source: "..\target\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\target\release\{#MyNotifyExeName}"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion isreadme
Source: "..\RUNBOOK.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\CLAUDE.md"; DestDir: "{app}"; DestName: "NOTES.md"; Flags: ignoreversion

[Icons]
Name: "{group}\Disinstalla rustcopy"; Filename: "{uninstallexe}"

[Code]
const
  VC_REDIST_URL = 'https://aka.ms/vs/17/release/vc_redist.x64.exe';

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
  if (CurStep = ssPostInstall) and WizardIsTaskSelected('addtopath') then
    EnvAddPath(ExpandConstant('{app}'));
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
