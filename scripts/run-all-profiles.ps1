<#
.SYNOPSIS
    Runs every profile in scripts\profiles.json in sequence via rustcopy-launcher.ps1's batch
    mode (PIANO_MIGLIORAMENTI.md, Pilastro D).

.DESCRIPTION
    A single entry point for "run all configured backups tonight" (e.g. one Task Scheduler
    trigger instead of one per destination script). Exit codes are not ordered by severity --
    AGENTS.md's exit-code table (0 success, 1 transfer failed, 2 usage error, 3 mirror-purge
    abort, 4 integrity mismatch, 5 retention-purge abort) is a set of distinct failure
    categories, not a scale -- so this does not report a single "worst" code. Instead it prints
    every failing profile with its own exit code and exits with the last non-zero one seen (0 if
    every profile succeeded), giving a scheduler a clear non-zero signal without implying a false
    ordering between failure kinds.

.EXAMPLE
    .\scripts\run-all-profiles.ps1
    .\scripts\run-all-profiles.ps1 -DryRun
#>

param(
    [switch]$DryRun,
    [string]$ProfilesPath
)

$ErrorActionPreference = "Stop"

. (Join-Path $PSScriptRoot "_profiles-common.ps1")

$profilesFile = if ($ProfilesPath) { $ProfilesPath } else { Join-Path $PSScriptRoot "profiles.json" }
$profiles = @(Import-RustcopyProfiles -Path $profilesFile)

if ($profiles.Count -eq 0) {
    Write-Host "No profiles found in $profilesFile - nothing to run." -ForegroundColor Yellow
    exit 0
}

$launcher = Join-Path $PSScriptRoot "rustcopy-launcher.ps1"
$failures = @()

foreach ($p in $profiles) {
    Write-Host ""
    Write-Host "##### Running profile '$($p.name)' #####" -ForegroundColor Magenta
    & $launcher -Profile $p.name -DryRun:$DryRun -ProfilesPath $profilesFile
    $code = $LASTEXITCODE
    if ($code -ne 0) {
        $failures += [PSCustomObject]@{ name = $p.name; exitCode = $code }
    }
}

Write-Host ""
if ($failures.Count -gt 0) {
    Write-Host "##### $($failures.Count) of $($profiles.Count) profile(s) failed #####" -ForegroundColor Red
    foreach ($f in $failures) {
        Write-Host ("  {0}: exit code {1}" -f $f.name, $f.exitCode) -ForegroundColor Red
    }
    exit $failures[-1].exitCode
}

Write-Host "##### All $($profiles.Count) profile(s) succeeded #####" -ForegroundColor Green
exit 0
