<#
.SYNOPSIS
  Installs the real rustcopy installer silently, checks the result, uninstalls it, checks again.

.DESCRIPTION
  F92, wave 2. Runs in CI on a Windows Server 2022 runner (.github/workflows/installer-smoke.yml)
  and can be run by hand from an elevated PowerShell on any machine that does NOT already have
  rustcopy installed (it refuses to run over an existing install: it would uninstall it).

  What it proves: the installer exits 0, the files land, the Explorer Shell extension is
  registered (COM class + the two drag-and-drop handlers), the install report says COMPLETATA,
  the installed CLI starts, and the uninstaller removes all of it again.

  What it does NOT prove: that a machine WITHOUT the Visual C++ runtime installs cleanly. A CI
  runner has Visual Studio on it. That property is covered separately by check-static-crt.ps1,
  which fails when a binary imports the dynamic C runtime again.

.PARAMETER InstallerPath
  The rustcopy-<version>-setup.exe to test.

.PARAMETER ExpectedVersion
  The version the installed CLI must report (X.Y.Z).

.PARAMETER ReportDir
  Where the install report and Setup log are written (passed to the installer as /ReportDir).
#>
param(
  [Parameter(Mandatory = $true)][string]$InstallerPath,
  [Parameter(Mandatory = $true)][string]$ExpectedVersion,
  [string]$ReportDir = (Join-Path $env:TEMP 'rustcopy-install-smoke')
)

$ErrorActionPreference = 'Stop'
$AppDir = Join-Path $env:ProgramFiles 'rustcopy'
$Clsid = '{59139F3E-0F3D-443E-BFC6-A5CE7CB466FC}'
$Keys = @(
  "HKLM:\SOFTWARE\Classes\CLSID\$Clsid\InprocServer32",
  'HKLM:\SOFTWARE\Classes\Directory\shellex\DragDropHandlers\RustCopy',
  'HKLM:\SOFTWARE\Classes\Drive\shellex\DragDropHandlers\RustCopy'
)
$Failures = New-Object System.Collections.Generic.List[string]

function Check([bool]$Condition, [string]$Message) {
  if ($Condition) { Write-Host "  ok    $Message" }
  else { Write-Host "  FAIL  $Message"; $Failures.Add($Message) }
}

function Summary([string]$Line) {
  if ($env:GITHUB_STEP_SUMMARY) { Add-Content -Path $env:GITHUB_STEP_SUMMARY -Value $Line -Encoding utf8 }
}

if (-not (Test-Path -LiteralPath $InstallerPath)) { throw "Installer not found: $InstallerPath" }
$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
  throw 'Run from an elevated PowerShell: the installer needs Administrator.'
}
if (Test-Path -LiteralPath $AppDir) {
  throw "$AppDir already exists. This script installs and then uninstalls; refusing to touch an existing install."
}

New-Item -ItemType Directory -Force -Path $ReportDir | Out-Null
$SetupLog = Join-Path $ReportDir 'setup-inno.log'

Write-Host "== install ($InstallerPath)"
$install = Start-Process -FilePath $InstallerPath -Wait -PassThru -ArgumentList @(
  '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', "/LOG=$SetupLog", "/ReportDir=$ReportDir"
)
Check ($install.ExitCode -eq 0) "installer exit code is 0 (was $($install.ExitCode))"

Write-Host '== installed files'
foreach ($name in 'robocopy_ingest.exe', 'rustcopy-gui.exe', 'rustcopy_shell.dll') {
  Check (Test-Path -LiteralPath (Join-Path $AppDir $name)) "$name present"
}

Write-Host '== Shell extension registration'
foreach ($key in $Keys) { Check (Test-Path -LiteralPath $key) "registry key $key" }
$server = (Get-ItemProperty -LiteralPath $Keys[0] -ErrorAction SilentlyContinue).'(default)'
Check ($server -eq (Join-Path $AppDir 'rustcopy_shell.dll')) "InprocServer32 points at the installed DLL ($server)"

Write-Host '== install report'
$report = Get-ChildItem -LiteralPath $ReportDir -Filter 'install-*.txt' -ErrorAction SilentlyContinue |
  Sort-Object LastWriteTime | Select-Object -Last 1
Check ($null -ne $report) 'an install-*.txt report was written'
if ($report) {
  $text = Get-Content -LiteralPath $report.FullName -Raw
  Check ($text -match 'COMPLETATA') 'report outcome is COMPLETATA'
  Check ($text -match 'regsvr32 codice 0') 'report says regsvr32 returned 0'
}

Write-Host '== installed CLI'
$cli = Join-Path $AppDir 'robocopy_ingest.exe'
$version = & $cli --version 2>&1 | Out-String
Check ($version -match [regex]::Escape($ExpectedVersion)) "robocopy_ingest --version contains $ExpectedVersion ($($version.Trim()))"

Write-Host '== uninstall'
$uninstaller = Join-Path $AppDir 'unins000.exe'
Check (Test-Path -LiteralPath $uninstaller) 'unins000.exe present'
if (Test-Path -LiteralPath $uninstaller) {
  $remove = Start-Process -FilePath $uninstaller -Wait -PassThru -ArgumentList @(
    '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART'
  )
  Check ($remove.ExitCode -eq 0) "uninstaller exit code is 0 (was $($remove.ExitCode))"
  # The uninstaller finishes by spawning a helper that deletes its own folder.
  for ($i = 0; $i -lt 20 -and (Test-Path -LiteralPath $AppDir); $i++) { Start-Sleep -Milliseconds 500 }
  Check (-not (Test-Path -LiteralPath $cli)) 'robocopy_ingest.exe removed'
  foreach ($key in $Keys) { Check (-not (Test-Path -LiteralPath $key)) "registry key removed: $key" }
}

if ($Failures.Count -gt 0) {
  Summary '### Installer smoke test: FAILED'
  foreach ($f in $Failures) { Summary "- $f" }
  Write-Host "`n$($Failures.Count) check(s) failed."
  exit 1
}
Summary '### Installer smoke test: passed'
Summary "Install, Shell extension registration, report, CLI start and uninstall verified on $([Environment]::OSVersion.VersionString)."
Write-Host "`nAll checks passed."
