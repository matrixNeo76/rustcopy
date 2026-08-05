<#
.SYNOPSIS
    Runs robocopy_ingest.exe twice: first to back up C:\Users\auresystem\claude-code to
    \\FILESERV01\dati01\provarust2, then (only if the first leg didn't hit an unrecoverable
    error) to a second destination, the QNAP NAS at \\192.168.1.187\datas01. Use -NasOnly to
    skip the first leg entirely and only run the NAS one.

.DESCRIPTION
    Report and log files are written under this repo's own _ops_reports folder, OUTSIDE the
    source tree being copied. This avoids the self-referential integrity "mismatch" that happens
    when the tool's own live log file sits inside the folder it is scanning (its size changes
    between the prescan and the verification pass, which then gets reported as a corrupted file
    even though nothing real changed).

    The second destination requires SMB credentials (user "backup"). Those live in
    scripts\nas2-credentials.local.ps1, which is NOT committed to git (see .gitignore:
    scripts/*.local.ps1) so the password never ends up in source control. The script establishes
    a credentialed SMB mapping to the NAS UNC path before the second copy and removes it
    afterwards, whether the copy succeeded or not.

.EXAMPLE
    .\scripts\run-ingest-claude-code.ps1
    .\scripts\run-ingest-claude-code.ps1 -DryRun
    .\scripts\run-ingest-claude-code.ps1 -SkipSecondDestination
    .\scripts\run-ingest-claude-code.ps1 -NasOnly
    .\scripts\run-ingest-claude-code.ps1 -NasOnly -DryRun -Dest2Subfolder "backup01"
#>

param(
    [switch]$DryRun,
    [switch]$SkipSecondDestination,
    # Skip leg 1 (FILESERV01) entirely and only run leg 2 (the QNAP NAS).
    [switch]$NasOnly,
    # Copy into a subfolder of the NAS share instead of its root (e.g. "backup01" for
    # \\192.168.1.187\datas01\backup01). The SMB credential mapping still targets the share
    # root - only the actual copy destination changes.
    [string]$Dest2Subfolder
)

if ($SkipSecondDestination -and $NasOnly) {
    Write-Host "-SkipSecondDestination and -NasOnly together would run neither destination." -ForegroundColor Red
    exit 1
}

$ErrorActionPreference = "Stop"

$RepoRoot    = Split-Path -Parent $PSScriptRoot
$Exe         = Join-Path $RepoRoot "target\release\robocopy_ingest.exe"
$Source      = "C:\Users\auresystem\claude-code"
$Dest1       = "\\FILESERV01\dati01\provarust2"
$Dest2Root   = "\\192.168.1.187\datas01"
# New-SmbMapping below always authenticates against the share root ($Dest2Root); only the
# actual copy destination changes when -Dest2Subfolder is given.
$Dest2       = if ($Dest2Subfolder) { Join-Path $Dest2Root $Dest2Subfolder } else { $Dest2Root }
$ReportsDir  = Join-Path $RepoRoot "_ops_reports"
$Timestamp   = Get-Date -Format "yyyyMMdd_HHmmss"

New-Item -ItemType Directory -Force -Path $ReportsDir | Out-Null

if (-not (Test-Path $Exe)) {
    Write-Host "Binary not found at $Exe - build it first with:" -ForegroundColor Yellow
    Write-Host "  cargo build --release" -ForegroundColor Yellow
    exit 1
}

function Invoke-Ingest {
    param(
        [string]$Label,
        [string]$Source,
        [string]$Dest
    )

    $reportPath = Join-Path $ReportsDir "claude-code_${Label}_$Timestamp.json"
    $logPath    = Join-Path $ReportsDir "claude-code_${Label}_$Timestamp.log"
    $htmlPath   = Join-Path $ReportsDir "claude-code_${Label}_$Timestamp.html"

    $argList = @(
        "--source", $Source,
        "--dest", $Dest,
        "--verify-integrity",
        "--hash-algo", "blake3",
        # F28: skip re-hashing files whose source size+mtime already match the last clean pass
        # (cached per-destination in <dest>\.ingest_cache). This is a recurring backup script -
        # without this, every run re-hashes the ENTIRE tree with BLAKE3 on both destinations,
        # even files that haven't changed since yesterday.
        "--fast-verify",
        "--report-path", $reportPath,
        "--log-path", $logPath,
        "--html-report-path", $htmlPath
    )
    if ($DryRun) { $argList += "--dry-run" }

    Write-Host ""
    Write-Host "=== [$Label] $Source -> $Dest ===" -ForegroundColor Cyan
    Write-Host "Report : $reportPath"
    Write-Host "Log    : $logPath"
    Write-Host "HTML   : $htmlPath"
    Write-Host ""

    & $Exe @argList
    # $LASTEXITCODE must be read into a script-scoped variable, not returned from this function:
    # if this function's result is captured with "$x = Invoke-Ingest ...", PowerShell folds the
    # external exe's own stdout (already written straight to the console above) into the
    # function's pipeline output too, so a plain `return $exitCode` comes back mixed together
    # with those lines instead of a clean integer.
    $script:LastIngestExitCode = $LASTEXITCODE
    $exitCode = $script:LastIngestExitCode

    Write-Host ""
    Write-Host "[$Label] Exit code: $exitCode" -ForegroundColor $(if ($exitCode -eq 0) { "Green" } else { "Red" })
    switch ($exitCode) {
        0 { Write-Host "[$Label] Success." -ForegroundColor Green }
        1 { Write-Host "[$Label] Completed with problems (see report/log)." -ForegroundColor Yellow }
        2 { Write-Host "[$Label] Unrecoverable error (bad args, missing source, etc.)." -ForegroundColor Red }
        3 { Write-Host "[$Label] Mirror-purge aborted (only relevant if --mirror was used)." -ForegroundColor Red }
        4 { Write-Host "[$Label] Transfer succeeded but --verify-integrity found a mismatch (data landed but doesn't match the source) - check the report's integrity_check section." -ForegroundColor Red }
        5 { Write-Host "[$Label] Retention purge aborted (only relevant if --keep-generations was used)." -ForegroundColor Red }
        default { Write-Host "[$Label] Unrecognised exit code $exitCode - check the CLAUDE.md exit-code list for what's new." -ForegroundColor Red }
    }

    if (Test-Path $htmlPath) {
        Write-Host "[$Label] Opening the HTML report..."
        Start-Process $htmlPath
    }
}

# --- Leg 1: existing FILESERV01 destination ---------------------------------------------------
if ($NasOnly) {
    Write-Host "Skipping the first destination ($Dest1) - -NasOnly." -ForegroundColor Yellow
    $exitCode1 = 0
}
else {
    Invoke-Ingest -Label "dest1-fileserv01" -Source $Source -Dest $Dest1
    $exitCode1 = $script:LastIngestExitCode

    if ($SkipSecondDestination) {
        Write-Host ""
        Write-Host "Skipping the second destination (-SkipSecondDestination)." -ForegroundColor Yellow
        exit $exitCode1
    }

    if ($exitCode1 -ge 2) {
        Write-Host ""
        Write-Host "Leg 1 hit an unrecoverable error (exit $exitCode1) - not attempting the second destination." -ForegroundColor Red
        exit $exitCode1
    }
}

# --- Leg 2: QNAP NAS at 192.168.1.187\datas01 -------------------------------------------------
$credsFile = Join-Path $PSScriptRoot "nas2-credentials.local.ps1"
if (-not (Test-Path $credsFile)) {
    Write-Host ""
    Write-Host "Second destination skipped: $credsFile not found." -ForegroundColor Yellow
    Write-Host "Create it with `$Nas2User and `$Nas2Password to enable the NAS leg." -ForegroundColor Yellow
    exit $exitCode1
}
. $credsFile

$mappingEstablished = $false
try {
    Write-Host ""
    Write-Host "Authenticating to $Dest2Root as $Nas2User ..." -ForegroundColor Cyan
    New-SmbMapping -RemotePath $Dest2Root -UserName $Nas2User -Password $Nas2Password -Persistent $false -ErrorAction Stop | Out-Null
    $mappingEstablished = $true

    # No need to pre-create $Dest2 (subfolder or not): robocopy_ingest's own execute() already
    # creates the destination directory if missing (outside --dry-run) before transferring.

    Invoke-Ingest -Label "dest2-qnap-datas01" -Source $Source -Dest $Dest2
    $exitCode2 = $script:LastIngestExitCode
}
catch {
    Write-Host ""
    Write-Host "Could not reach/authenticate to $Dest2Root : $_" -ForegroundColor Red
    $exitCode2 = 2
}
finally {
    if ($mappingEstablished) {
        Remove-SmbMapping -RemotePath $Dest2Root -Force -ErrorAction SilentlyContinue | Out-Null
    }
    # Credentials only need to live for the duration of this script.
    Remove-Variable -Name Nas2User, Nas2Password -ErrorAction SilentlyContinue
}

Write-Host ""
Write-Host "=== Summary ===" -ForegroundColor Cyan
Write-Host "Leg 1 ($Dest1): $(if ($NasOnly) { 'skipped (-NasOnly)' } else { "exit $exitCode1" })"
Write-Host "Leg 2 ($Dest2): exit $exitCode2"

exit ([Math]::Max($exitCode1, $exitCode2))
