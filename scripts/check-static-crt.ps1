<#
.SYNOPSIS
  Fails if a Windows binary imports the dynamic C runtime (F92 / D30, ROADMAP.md).

.DESCRIPTION
  rustcopy links the C runtime statically (.cargo/config.toml) so that no Visual C++
  Redistributable is needed. A binary that imports VCRUNTIME*, MSVCP*, CONCRT*, ucrtbase.dll or an
  api-ms-win-crt-* forwarder is back on the dynamic runtime -- which is how an install fails on a
  machine without the matching redistributable. Used by CI (ci.yml, job `static-crt`) and run by
  hand before publishing a release.

.EXAMPLE
  powershell -File scripts\check-static-crt.ps1 -Path target\release\robocopy_ingest.exe, target\release\rustcopy_shell.dll
#>
param(
  [Parameter(Mandatory = $true)][string[]]$Path
)

$ErrorActionPreference = 'Stop'

$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
if (-not (Test-Path $vswhere)) { throw "vswhere.exe not found at $vswhere (Visual Studio Build Tools needed for dumpbin)" }
$dumpbin = & $vswhere -latest -products * -find 'VC\Tools\MSVC\**\bin\Hostx64\x64\dumpbin.exe' | Select-Object -Last 1
if (-not $dumpbin) { throw 'dumpbin.exe not found: install the MSVC build tools' }

$forbidden = '^(vcruntime|msvcp|concrt|vccorlib|ucrtbase|api-ms-win-crt-)'
$failed = $false

foreach ($file in $Path) {
  if (-not (Test-Path $file)) { throw "File not found: $file" }
  $imports = & $dumpbin /nologo /dependents $file |
    Where-Object { $_ -match '^\s+\S+\.dll\s*$' } |
    ForEach-Object { $_.Trim() }
  $bad = @($imports | Where-Object { $_ -match $forbidden })
  if ($bad.Count -gt 0) {
    Write-Host ("FAIL  {0}: imports the dynamic C runtime -> {1}" -f $file, ($bad -join ', '))
    $failed = $true
  } else {
    Write-Host ("ok    {0}: no dynamic C runtime import ({1} DLL dependencies)" -f $file, @($imports).Count)
  }
}

if ($failed) {
  Write-Host ''
  Write-Host 'The static CRT setting in .cargo/config.toml was lost, overridden (RUSTFLAGS replaces it) or bypassed.'
  exit 1
}
