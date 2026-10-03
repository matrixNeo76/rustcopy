// Install report and non-fatal Shell-extension registration (F92 / D30, ROADMAP.md).
//
// Included into the [Code] section of rustcopy.iss with ISPP's include directive, and into a
// throwaway test installer when its logic is exercised without admin rights.
//
// Why this exists. On 2 Ott 2026 the install failed on Windows Server 2016 and 2022 and worked on
// 2019 and Windows 11. A test installer showed the mechanism: a [Files] entry with the regserver flag
// whose DLL cannot load makes Setup log "RegSvr32 failed with exit code 0x3", default to Abort when
// message boxes are suppressed, roll the whole install back and exit with code 5 -- an optional
// component taking everything else down. So registration is done here instead, and a failure is
// reported rather than fatal.
//
// The report is written on every run -- completed, failed, cancelled, silent or not -- because
// DeinitializeSetup runs in all of them (verified with that same test installer). It lives outside
// {app} so a rollback does not remove it. It is plain Pascal Script on purpose: no PowerShell is
// started, since an unsigned installer launching powershell -ExecutionPolicy Bypass is exactly what
// an EDR on a server blocks.

var
  ReportLines: TStringList;
  ReportReady: Boolean;
  InstallBegan: Boolean;
  InstallCompleted: Boolean;
  ReportPath: string;

procedure ReportAdd(const Text: string);
begin
  Log('report: ' + Text);
  if ReportLines <> nil then
    ReportLines.Add(Text);
end;

function YesNo(Value: Boolean): string;
begin
  if Value then
    Result := 'si'
  else
    Result := 'no';
end;

function FileVersionText(const Path: string): string;
var
  Version: string;
begin
  if not FileExists(Path) then
    Result := 'ASSENTE'
  else if GetVersionNumbersString(Path, Version) then
    Result := 'presente, versione ' + Version
  else
    Result := 'presente, versione non leggibile';
end;

function RegValueText(const SubKey, ValueName: string): string;
var
  Value: string;
begin
  if RegQueryStringValue(HKLM, SubKey, ValueName, Value) then
    Result := Value
  else
    Result := '(assente)';
end;

function RegSvrMeaning(Code: Integer): string;
begin
  case Code of
    0: Result := 'riuscita';
    1: Result := 'argomenti non validi';
    2: Result := 'OleInitialize non riuscita';
    3: Result := 'LoadLibrary non riuscita: la DLL o una sua dipendenza non si carica';
    4: Result := 'DllRegisterServer non trovata nella DLL';
    5: Result := 'DllRegisterServer ha restituito un errore';
  else
    Result := 'codice non documentato';
  end;
end;

// Registers the Shell extension without ever failing the install. Returns False when it could not be
// registered; the caller tells the operator, and the report records why.
function RegisterShellExtension(const DllPath: string): Boolean;
var
  Regsvr: string;
  Code: Integer;
begin
  Result := False;
  Regsvr := ExpandConstant('{sys}\regsvr32.exe');
  if not FileExists(DllPath) then
  begin
    ReportAdd('Estensione Shell: DLL non trovata in ' + DllPath);
    exit;
  end;
  if not Exec(Regsvr, '/s "' + DllPath + '"', '', SW_HIDE, ewWaitUntilTerminated, Code) then
  begin
    ReportAdd('Estensione Shell: impossibile avviare regsvr32 (' + SysErrorMessage(Code) + ')');
    exit;
  end;
  ReportAdd('Estensione Shell: regsvr32 codice ' + IntToStr(Code) + ' - ' + RegSvrMeaning(Code));
  Result := (Code = 0);
  if not Result then
  begin
    // A registration that failed half way must not be left pointing at a DLL Explorer cannot load.
    Exec(Regsvr, '/u /s "' + DllPath + '"', '', SW_HIDE, ewWaitUntilTerminated, Code);
    ReportAdd('Estensione Shell: rimozione di una registrazione parziale, regsvr32 /u codice ' + IntToStr(Code));
  end;
end;

procedure UnregisterShellExtension(const DllPath: string);
var
  Code: Integer;
begin
  if FileExists(DllPath) then
    Exec(ExpandConstant('{sys}\regsvr32.exe'), '/u /s "' + DllPath + '"', '', SW_HIDE, ewWaitUntilTerminated, Code);
end;

function DefaultReportDirectory(): string;
begin
  Result := ExpandConstant('{commonappdata}\rustcopy\install-reports');
end;

// /ReportDir=<folder> on the Setup command line overrides where the report goes (an admin who wants
// it on a share, and the way this logic is tested without writing under C:\ProgramData).
function ReportDirectory(): string;
begin
  Result := ExpandConstant('{param:ReportDir|{commonappdata}\rustcopy\install-reports}');
end;

// WebView2 is registered per machine (32-bit view), per machine, or per user depending on how it
// was installed: the console works with any of them, so the report must look at all three.
function WebView2VersionText(): string;
var
  Version: string;
begin
  if RegQueryStringValue(HKLM, 'SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\' + WEBVIEW2_CLIENT, 'pv', Version) then
    Result := Version + ' (per macchina, vista a 32 bit)'
  else if RegQueryStringValue(HKLM, 'SOFTWARE\Microsoft\EdgeUpdate\Clients\' + WEBVIEW2_CLIENT, 'pv', Version) then
    Result := Version + ' (per macchina)'
  else if RegQueryStringValue(HKCU, 'SOFTWARE\Microsoft\EdgeUpdate\Clients\' + WEBVIEW2_CLIENT, 'pv', Version) then
    Result := Version + ' (per utente)'
  else
    Result := '(assente)';
end;

// Facts that need no wizard: called from InitializeSetup.
procedure ReportStart();
var
  Version: TWindowsVersion;
  Ubr: Cardinal;
  Existing, Tail: string;
  FreeMb, TotalMb: Cardinal;
  Zone: AnsiString;
begin
  ReportLines := TStringList.Create;
  ReportReady := True;
  GetWindowsVersionEx(Version);

  ReportAdd('rustcopy - rapporto di installazione');
  ReportAdd('Data: ' + GetDateTimeString('yyyy-mm-dd hh:nn:ss', '-', ':'));
  ReportAdd('Versione installer: {#MyAppVersion}');
  // {srcexe} is the Setup file the operator started; ParamStr(0) is Setup's own temporary copy,
  // which would not carry the download mark below.
  ReportAdd('Installer: ' + ExpandConstant('{srcexe}'));
  Tail := GetCmdTail;
  ReportAdd('Parametri di avvio: ' + Tail);
  ReportAdd('Modalita'' silenziosa: ' + YesNo(WizardSilent));
  if LoadStringFromFile(ExpandConstant('{srcexe}') + ':Zone.Identifier', Zone) then
    ReportAdd('Segno di download (Zone.Identifier): presente, il file arriva da Internet o da un altro computer')
  else
    ReportAdd('Segno di download (Zone.Identifier): assente');

  ReportAdd('');
  ReportAdd('--- Sistema ---');
  ReportAdd('Prodotto: ' + RegValueText('SOFTWARE\Microsoft\Windows NT\CurrentVersion', 'ProductName'));
  ReportAdd('InstallationType: ' + RegValueText('SOFTWARE\Microsoft\Windows NT\CurrentVersion', 'InstallationType'));
  if RegQueryDWordValue(HKLM, 'SOFTWARE\Microsoft\Windows NT\CurrentVersion', 'UBR', Ubr) then
    ReportAdd('Versione: ' + IntToStr(Version.Major) + '.' + IntToStr(Version.Minor) + ' build ' + IntToStr(Version.Build) + '.' + IntToStr(Ubr))
  else
    ReportAdd('Versione: ' + IntToStr(Version.Major) + '.' + IntToStr(Version.Minor) + ' build ' + IntToStr(Version.Build));
  ReportAdd('ProductType: ' + IntToStr(Version.ProductType) + ' (1 = client, 2 = controller di dominio, 3 = server)');
  ReportAdd('Nota: il nome del prodotto dice "Windows 10" anche su Windows 11; conta la build (22000 o piu'' = Windows 11).');
  ReportAdd('Installazione a 64 bit: ' + YesNo(Is64BitInstallMode));
  ReportAdd('Amministratore: ' + YesNo(IsAdmin));
  if GetSpaceOnDisk(ExtractFileDrive(ExpandConstant('{autopf}')) + '\', True, FreeMb, TotalMb) then
    ReportAdd('Spazio libero sul disco di installazione: ' + IntToStr(FreeMb) + ' MB');
  ReportAdd('Riavvio in sospeso: servicing=' +
    YesNo(RegKeyExists(HKLM, 'SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending')) +
    ' windowsupdate=' +
    YesNo(RegKeyExists(HKLM, 'SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired')) +
    ' file-in-sospeso=' +
    YesNo(RegValueExists(HKLM, 'SYSTEM\CurrentControlSet\Control\Session Manager', 'PendingFileRenameOperations')));

  ReportAdd('');
  ReportAdd('--- Runtime (informativo: da 7.7.0 rustcopy collega il runtime C in modo statico) ---');
  ReportAdd('Visual C++ 14.x x64 nel registro: ' +
    RegValueText('SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\X64', 'Version'));
  ReportAdd('vcruntime140.dll: ' + FileVersionText(ExpandConstant('{sys}\vcruntime140.dll')));
  ReportAdd('vcruntime140_1.dll: ' + FileVersionText(ExpandConstant('{sys}\vcruntime140_1.dll')));
  ReportAdd('ucrtbase.dll: ' + FileVersionText(ExpandConstant('{sys}\ucrtbase.dll')));
  ReportAdd('WebView2 (richiesto solo dalla console): ' + WebView2VersionText());

  ReportAdd('');
  ReportAdd('--- Installazione precedente ---');
  Existing := RegValueText('SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\{7B1E5C2A-2D8F-4A6B-9E3C-1F5A6D2B8C90}_is1', 'DisplayVersion');
  ReportAdd('Versione gia'' installata: ' + Existing);
end;

// What the operator chose: needs the wizard, so called when installation begins.
procedure ReportSelection();
begin
  InstallBegan := True;
  ReportAdd('');
  ReportAdd('--- Scelte ---');
  ReportAdd('Cartella: ' + WizardDirValue);
  ReportAdd('Componenti: ' + WizardSelectedComponents(False));
  ReportAdd('Attivita'': ' + WizardSelectedTasks(False));
end;

procedure ReportInstalledFiles();
var
  Names: array of string;
  I, Size: Integer;
  Path: string;
begin
  SetArrayLength(Names, 4);
  Names[0] := '{#MyAppExeName}';
  Names[1] := '{#MyNotifyExeName}';
  Names[2] := '{#MyGuiExeName}';
  Names[3] := '{#MyShellDllName}';
  ReportAdd('');
  ReportAdd('--- File installati ---');
  for I := 0 to GetArrayLength(Names) - 1 do
  begin
    Path := ExpandConstant('{app}\') + Names[I];
    if FileExists(Path) and FileSize(Path, Size) then
      ReportAdd(Names[I] + ': presente, ' + IntToStr(Size) + ' byte')
    else if FileExists(Path) then
      ReportAdd(Names[I] + ': presente')
    else
      ReportAdd(Names[I] + ': non installato');
  end;
end;

// Saves the report (and a copy of Setup's own log) into Dir under a name no earlier or concurrent
// run has used. Returns False when Dir cannot be created or written.
function TrySaveReport(const Dir, Stamp: string): Boolean;
var
  N: Integer;
  Base, SetupLog: string;
begin
  Result := False;
  if not ForceDirectories(Dir) then
    exit;
  N := 0;
  repeat
    if N = 0 then
      Base := Dir + '\install-' + Stamp
    else
      Base := Dir + '\install-' + Stamp + '-' + IntToStr(N);
    N := N + 1;
  until (not FileExists(Base + '.txt') and not FileExists(Base + '-setup.log')) or (N > 99);
  if not SaveStringToFile(Base + '.txt', ReportLines.Text, False) then
    exit;
  ReportPath := Base + '.txt';
  SetupLog := ExpandConstant('{log}');
  if SetupLog <> '' then
    CopyFile(SetupLog, Base + '-setup.log', False);
  Result := True;
end;

// Called from DeinitializeSetup, which runs whether Setup completed, failed or was cancelled.
procedure ReportFinish();
var
  Stamp, Outcome: string;
  Saved: Boolean;
begin
  if not ReportReady then
    exit;
  if InstallCompleted then
    Outcome := 'COMPLETATA'
  else if InstallBegan then
    Outcome := 'NON COMPLETATA (errore o annullata durante l''installazione)'
  else
    Outcome := 'ANNULLATA prima di installare';
  ReportAdd('');
  ReportAdd('--- Esito: ' + Outcome + ' ---');

  Stamp := GetDateTimeString('yyyymmdd-hhnnss', '-', ':');
  // The requested folder first, then the default, then the current user's profile: a report that
  // cannot be saved is exactly the one nobody can read, so a bad /ReportDir must not lose it.
  Saved := TrySaveReport(ReportDirectory(), Stamp);
  if not Saved and (CompareText(ReportDirectory(), DefaultReportDirectory()) <> 0) then
    Saved := TrySaveReport(DefaultReportDirectory(), Stamp);
  if not Saved then
    Saved := TrySaveReport(ExpandConstant('{localappdata}\rustcopy\install-reports'), Stamp);
  if not Saved then
  begin
    Log('report: could not be saved in any folder');
    exit;
  end;

  if InstallBegan and not InstallCompleted and not WizardSilent then
    SuppressibleMsgBox(
      'L''installazione non si e'' completata.' + #13#10 + #13#10 +
      'Ho salvato un rapporto con i dettagli in:' + #13#10 + ReportPath + #13#10 + #13#10 +
      'Se serve aiuto, invia quel file (e il file -setup.log accanto): contiene il sistema operativo, ' +
      'cio'' che e'' installato e dove si e'' fermato Setup.',
      mbInformation, MB_OK, IDOK);
end;
