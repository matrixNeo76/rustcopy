<#
.SYNOPSIS
  Read-only diagnostics for a failed or suspicious rustcopy install (F92, ROADMAP.md).

.DESCRIPTION
  Collects, in one run, the facts that decide why rustcopy installs or starts on one Windows machine
  and not on another: OS build and SKU, Visual C++ runtime and Universal CRT files, WebView2, what
  the installer actually left on disk and in the registry, whether each rustcopy binary can be
  loaded at all (and with which Win32 error), execution policies that can block an unsigned binary
  (Defender, AppLocker, Code Integrity), the Inno Setup logs, recent Application event log errors,
  and pending-reboot flags.

  It changes nothing: no registration, no install, no service, no setting. The only thing it loads
  into memory is each rustcopy DLL, to read the loader's error code, and it never calls
  DllRegisterServer. It runs on Windows PowerShell 5.1 (Windows Server 2016 and later) and writes a
  folder plus a .zip on the Desktop. Review the report before sharing it: it contains the machine
  name and file paths.

.PARAMETER OutDir
  Where the report is written. Default: <Desktop>\rustcopy-diagnostics-<date-time>. A folder that
  already has content is never reused or cleared: a time-stamped sibling is created instead.

.PARAMETER InstallerPath
  Optional path of the setup .exe that failed; its download mark (Zone.Identifier) and SHA-256 are
  added to the report.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File .\collect-install-diagnostics.ps1 -InstallerPath C:\Temp\rustcopy-7.6.1-setup.exe
#>
[CmdletBinding()]
param(
  [string]$OutDir = '',
  [string]$InstallerPath = ''
)

# A 32-bit PowerShell on 64-bit Windows sees a redirected System32 and the Wow6432Node registry view,
# which would make a runtime that is present look absent. Re-run in the native 64-bit host instead
# of reasoning about both views throughout this script.
if ([Environment]::Is64BitOperatingSystem -and -not [Environment]::Is64BitProcess) {
  $native = Join-Path $env:SystemRoot 'Sysnative\WindowsPowerShell\v1.0\powershell.exe'
  $forward = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $PSCommandPath)
  if ($OutDir)        { $forward += @('-OutDir', $OutDir) }
  if ($InstallerPath) { $forward += @('-InstallerPath', $InstallerPath) }
  & $native @forward
  exit $LASTEXITCODE
}

$ErrorActionPreference = 'Continue'
# Every run writes to a fresh folder: never clear, overwrite or mix with files already there.
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
if (-not $OutDir) {
  $OutDir = Join-Path ([Environment]::GetFolderPath('Desktop')) "rustcopy-diagnostics-$stamp"
} elseif ((Test-Path $OutDir) -and @(Get-ChildItem $OutDir -Force -ErrorAction SilentlyContinue).Count -gt 0) {
  $OutDir = "$OutDir-$stamp"
}
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$report = Join-Path $OutDir 'report.txt'
Set-Content -Path $report -Value ("rustcopy install diagnostics - " + (Get-Date -Format 'yyyy-MM-dd HH:mm:ss')) -Encoding UTF8

function Section([string]$Title) { Add-Content -Path $report -Value ("`r`n==== " + $Title + " ====") -Encoding UTF8 }
function Line([string]$Text)     { Add-Content -Path $report -Value $Text -Encoding UTF8 }
function Capture([string]$Name, [scriptblock]$Block) {
  try {
    $text = & $Block 2>&1 | Out-String -Width 250
    if ($text.Trim().Length -eq 0) { $text = '  (nessun risultato)' }
    Add-Content -Path $report -Value $text.TrimEnd() -Encoding UTF8
  } catch {
    Line ("  [errore in $Name] " + $_.Exception.Message)
  }
}

# Get-WinEvent reports "no matching events" as an error. That is an empty result, not a failure:
# recognise it by its locale-independent error id and let every other error (access denied, log
# missing) surface in the report instead of masquerading as "nothing found".
function Get-EventsOrEmpty([hashtable]$Filter) {
  try { Get-WinEvent -FilterHashtable $Filter -ErrorAction Stop }
  catch {
    if ($_.FullyQualifiedErrorId -like 'NoMatchingEventsFound*') { @() } else { throw }
  }
}

# --- 1. System -----------------------------------------------------------------------------------
Section 'Sistema'
Capture 'os' {
  $os = Get-CimInstance Win32_OperatingSystem
  $cv = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
  [pscustomobject]@{
    Caption          = $os.Caption
    Version          = $os.Version
    Build            = "$($cv.CurrentBuild).$($cv.UBR)"
    DisplayVersion   = $cv.DisplayVersion
    ProductType      = $os.ProductType   # 1 = workstation, 2 = domain controller, 3 = server
    InstallationType = $cv.InstallationType  # Client / Server / Server Core
    Architecture     = $os.OSArchitecture
    Language         = (Get-Culture).Name
    PowerShell       = $PSVersionTable.PSVersion.ToString()
    IsAdmin          = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    UAC_EnableLUA    = (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System' -ErrorAction SilentlyContinue).EnableLUA
    PartOfDomain     = (Get-CimInstance Win32_ComputerSystem).PartOfDomain
    Machine          = $env:COMPUTERNAME
    TempDir          = $env:TEMP
    FreeGB_SystemDrv = [math]::Round((Get-PSDrive ($env:SystemDrive.TrimEnd(':'))).Free / 1GB, 1)
  } | Format-List
}

# --- 2. Visual C++ runtime and Universal CRT ------------------------------------------------------
Section 'Visual C++ Redistributable e CRT'
Capture 'vcreg' {
  foreach ($k in 'HKLM:\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\X64',
                 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\VisualStudio\14.0\VC\Runtimes\X64') {
    if (Test-Path $k) {
      $p = Get-ItemProperty $k
      "{0}`r`n  Installed={1} Version={2} Bld={3}" -f $k, $p.Installed, $p.Version, $p.Bld
    } else { "$k : chiave assente" }
  }
}
Capture 'vcfiles' {
  foreach ($f in 'vcruntime140.dll', 'vcruntime140_1.dll', 'msvcp140.dll', 'ucrtbase.dll') {
    $p = Join-Path $env:SystemRoot "System32\$f"
    if (Test-Path $p) { "{0,-20} presente  versione {1}" -f $f, (Get-Item $p).VersionInfo.FileVersion }
    else              { "{0,-20} ASSENTE" -f $f }
  }
}
Capture 'vcuninstall' {
  $keys = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*',
          'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*'
  Get-ItemProperty $keys -ErrorAction SilentlyContinue |
    Where-Object { $_.DisplayName -like 'Microsoft Visual C++ 20*' } |
    Select-Object DisplayName, DisplayVersion | Sort-Object DisplayName | Format-Table -AutoSize
}

# --- 3. WebView2 ---------------------------------------------------------------------------------
Section 'WebView2 (richiesto solo dalla console grafica)'
Capture 'webview2' {
  $client = '{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
  foreach ($k in "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\$client",
                 "HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\$client",
                 "HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients\$client") {
    if (Test-Path $k) { "{0} -> pv={1}" -f $k, (Get-ItemProperty $k).pv } else { "$k : assente" }
  }
  $dir = Join-Path ${env:ProgramFiles(x86)} 'Microsoft\EdgeWebView\Application'
  if (Test-Path $dir) { "Cartella runtime: " + ((Get-ChildItem $dir -Directory | Select-Object -ExpandProperty Name) -join ', ') }
  else { "Cartella runtime assente: $dir" }
}

# --- 4. What the installer left behind -----------------------------------------------------------
Section 'Installazione rustcopy'
$installDir = $null
Capture 'uninstall' {
  $appId = '{7B1E5C2A-2D8F-4A6B-9E3C-1F5A6D2B8C90}_is1'
  foreach ($root in 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall',
                    'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall') {
    $k = Join-Path $root $appId
    if (Test-Path $k) {
      $p = Get-ItemProperty $k
      "{0}`r`n  DisplayVersion={1} InstallLocation={2}" -f $k, $p.DisplayVersion, $p.InstallLocation
    }
  }
}
foreach ($root in 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall',
                  'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall') {
  $k = Join-Path $root '{7B1E5C2A-2D8F-4A6B-9E3C-1F5A6D2B8C90}_is1'
  if (-not $installDir -and (Test-Path $k)) { $installDir = (Get-ItemProperty $k).InstallLocation }
}
if (-not $installDir) { $installDir = Join-Path $env:ProgramFiles 'rustcopy' }
Line ("Cartella considerata: $installDir  (esiste: " + (Test-Path $installDir) + ')')
Capture 'files' {
  if (Test-Path $installDir) {
    Get-ChildItem $installDir -File | Select-Object Name, Length, LastWriteTime,
      @{n = 'FileVersion'; e = { $_.VersionInfo.FileVersion }} | Format-Table -AutoSize
  }
}
Capture 'path' {
  $machinePath = [Environment]::GetEnvironmentVariable('Path', 'Machine')
  "Cartella in PATH di sistema: " + ($machinePath -split ';' -contains $installDir.TrimEnd('\'))
}
Capture 'com' {
  $clsid = '{59139F3E-0F3D-443E-BFC6-A5CE7CB466FC}'
  $k = "HKLM:\SOFTWARE\Classes\CLSID\$clsid\InprocServer32"
  if (Test-Path $k) {
    $dll = (Get-ItemProperty $k).'(default)'
    "Estensione Shell registrata: $dll  (file presente: $(Test-Path $dll))"
  } else { "Estensione Shell NON registrata ($k assente)" }
  foreach ($h in 'Directory', 'Drive') {
    $hk = "HKLM:\SOFTWARE\Classes\$h\shellex\DragDropHandlers\RustCopy"
    "{0}: {1}" -f $hk, $(if (Test-Path $hk) { 'presente' } else { 'assente' })
  }
}

# --- 5. Can each binary even be loaded? ------------------------------------------------------------
Section 'Prova di caricamento dei binari (nessuna registrazione)'
Add-Type -Namespace Native -Name K32 -MemberDefinition @'
[DllImport("kernel32.dll", SetLastError=true, CharSet=CharSet.Unicode)]
public static extern System.IntPtr LoadLibraryEx(string file, System.IntPtr hFile, uint flags);
[DllImport("kernel32.dll", SetLastError=true)]
public static extern bool FreeLibrary(System.IntPtr module);
'@ -ErrorAction SilentlyContinue
Capture 'loadshell' {
  $dll = Join-Path $installDir 'rustcopy_shell.dll'
  if (-not (Test-Path $dll)) { "rustcopy_shell.dll non presente in $installDir"; return }
  $h = [Native.K32]::LoadLibraryEx($dll, [IntPtr]::Zero, 0)
  if ($h -eq [IntPtr]::Zero) {
    $err = [Runtime.InteropServices.Marshal]::GetLastWin32Error()
    "rustcopy_shell.dll: CARICAMENTO FALLITO, errore Win32 $err  (126 = modulo o dipendenza mancante, es. VCRUNTIME140.dll; 193 = immagine non valida; 1260 = bloccato da criterio)"
  } else {
    [void][Native.K32]::FreeLibrary($h)
    "rustcopy_shell.dll: caricamento riuscito"
  }
}
foreach ($exe in 'robocopy_ingest.exe', 'notify-server.exe') {
  Capture "run-$exe" {
    $p = Join-Path $installDir $exe
    if (-not (Test-Path $p)) { "$exe non presente"; return }
    $out = & $p --version 2>&1 | Out-String
    $code = $LASTEXITCODE
    $hint = if ($code -eq -1073741515 -or $code -eq 3221225781) { '  <-- 0xC0000135 / -1073741515 = DLL mancante (VCRUNTIME140.dll?)' } else { '' }
    "{0} --version -> exit {1}{2}  output: {3}" -f $exe, $code, $hint, $out.Trim()
  }
}
Capture 'gui' {
  $g = Join-Path $installDir 'rustcopy-gui.exe'
  if (Test-Path $g) { "rustcopy-gui.exe presente ($((Get-Item $g).VersionInfo.ProductVersion)); non viene avviato da questo script" }
  else { "rustcopy-gui.exe non presente (componente console non installato)" }
}

# --- 6. Policies that can block an unsigned binary -----------------------------------------------
Section 'Criteri di sicurezza'
Capture 'defender' {
  $s = Get-MpComputerStatus -ErrorAction Stop
  [pscustomobject]@{
    AMServiceEnabled = $s.AMServiceEnabled; RealTimeProtection = $s.RealTimeProtectionEnabled
    TamperProtection = $s.IsTamperProtected; AntivirusSignature = $s.AntivirusSignatureVersion
  } | Format-List
}
Capture 'applocker' {
  $p = Get-AppLockerPolicy -Effective -ErrorAction Stop
  if ($p.RuleCollections.Count -eq 0) { 'AppLocker: nessuna regola effettiva' }
  else { $p.RuleCollections | ForEach-Object { "{0}: {1} regole" -f $_.RuleCollectionType, @($_).Count } }
}
Capture 'deviceguard' {
  Get-CimInstance -ClassName Win32_DeviceGuard -Namespace root\Microsoft\Windows\DeviceGuard -ErrorAction Stop |
    Select-Object CodeIntegrityPolicyEnforcementStatus, UsermodeCodeIntegrityPolicyEnforcementStatus,
                  SecurityServicesRunning, VirtualizationBasedSecurityStatus | Format-List
}
Capture 'codeintegrity' {
  $since = (Get-Date).AddDays(-3)
  $ev = @(Get-EventsOrEmpty @{ LogName = 'Microsoft-Windows-CodeIntegrity/Operational'; StartTime = $since; Id = 3033, 3034, 3076, 3077 })
  if ($ev.Count -eq 0) { 'Code Integrity: nessun blocco negli ultimi 3 giorni'; return }
  "Code Integrity: $(@($ev).Count) eventi di blocco/audit negli ultimi 3 giorni (di qualunque programma)"
  $mine = $ev | Where-Object { $_.Message -match 'rustcopy|robocopy_ingest|notify-server' }
  if ($mine) { $mine | Select-Object -First 10 TimeCreated, Id, @{n = 'Msg'; e = { $_.Message -replace '\s+', ' ' }} | Format-List }
  else { 'Nessuno riguarda rustcopy.' }
}
if ($InstallerPath -and (Test-Path $InstallerPath)) {
  Capture 'installer' {
    "Installer: $InstallerPath"
    "SHA-256  : " + (Get-FileHash $InstallerPath -Algorithm SHA256).Hash.ToLower()
    $z = Get-Content -Path $InstallerPath -Stream Zone.Identifier -ErrorAction SilentlyContinue
    if ($z) { "Zone.Identifier (file scaricato, puo essere bloccato):"; $z } else { 'Zone.Identifier: assente' }
    "Firma digitale: " + (Get-AuthenticodeSignature $InstallerPath).Status
  }
}

# --- 7. Inno Setup logs ---------------------------------------------------------------------------
Section 'Log di Inno Setup'
Capture 'setuplogs' {
  $dirs = @($env:TEMP, (Join-Path $env:SystemRoot 'Temp')) | Select-Object -Unique
  $logs = foreach ($d in $dirs) { Get-ChildItem $d -Filter 'Setup Log*.txt' -ErrorAction SilentlyContinue }
  $logs = $logs | Sort-Object LastWriteTime -Descending | Select-Object -First 3
  if (-not $logs) { 'Nessun "Setup Log *.txt" trovato in %TEMP% o C:\Windows\Temp (se hai lanciato il setup come altro utente, cercalo nel suo %TEMP%).'; return }
  foreach ($l in $logs) {
    Copy-Item $l.FullName -Destination $OutDir -ErrorAction SilentlyContinue
    "Copiato: " + $l.FullName + "  (" + $l.LastWriteTime + ")"
  }
  "--- ultime 40 righe del piu recente ---"
  Get-Content $logs[0].FullName -Tail 40
}

# --- 8. Event log -----------------------------------------------------------------------------------
Section 'Registro eventi Applicazione (ultimi 3 giorni)'
Capture 'events' {
  $since = (Get-Date).AddDays(-3)
  $ev = @(Get-EventsOrEmpty @{ LogName = 'Application'; StartTime = $since; Level = 1, 2 } |
    Where-Object { $_.Message -match 'rustcopy|robocopy_ingest|rustcopy_shell|rustcopy-gui|notify-server|VCRUNTIME|regsvr|setup' -or $_.ProviderName -in 'SideBySide', 'MsiInstaller' } |
    Select-Object -First 15)
  if ($ev.Count -gt 0) { $ev | Select-Object TimeCreated, ProviderName, Id, @{n = 'Msg'; e = { ($_.Message -replace '\s+', ' ').Substring(0, [math]::Min(300, $_.Message.Length)) }} | Format-List }
  else { 'Nessun evento di errore pertinente' }
}

# --- 9. Pending reboot ----------------------------------------------------------------------------
Section 'Riavvio in sospeso'
Capture 'reboot' {
  [pscustomobject]@{
    ComponentServicing = Test-Path 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending'
    WindowsUpdate      = Test-Path 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired'
    PendingFileRename  = $null -ne (Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager' -ErrorAction SilentlyContinue).PendingFileRenameOperations
  } | Format-List
}

$zip = "$OutDir.zip"
if (Test-Path $zip) { $zip = "$OutDir-$stamp.zip" }   # never replace an archive that already exists
try {
  Compress-Archive -Path (Join-Path $OutDir '*') -DestinationPath $zip -ErrorAction Stop
  if (-not (Test-Path $zip)) { throw 'archivio non trovato dopo la creazione' }
} catch { $zip = '(zip non creato: ' + $_.Exception.Message + ')' }
Write-Host ''
Write-Host 'Fatto. Nessuna modifica e'' stata apportata al sistema.'
Write-Host "Report : $report"
Write-Host "Archivio: $zip"
Write-Host 'Controlla il contenuto prima di condividerlo: contiene il nome della macchina e dei percorsi.'
