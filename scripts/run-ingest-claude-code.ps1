<#
.SYNOPSIS
    Runs robocopy_ingest.exe to back up C:\Users\auresystem\claude-code to \\FILESERV01\dati01\provarust2.

.DESCRIPTION
    Report and log files are written under this repo's own _ops_reports folder, OUTSIDE the
    source tree being copied. This avoids the self-referential integrity "mismatch" that happens
    when the tool's own live log file sits inside the folder it is scanning (its size changes
    between the prescan and the verification pass, which then gets reported as a corrupted file
    even though nothing real changed).

.EXAMPLE
    .\scripts\run-ingest-claude-code.ps1
    .\scripts\run-ingest-claude-code.ps1 -DryRun
#>

param(
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

$RepoRoot   = Split-Path -Parent $PSScriptRoot
$Exe        = Join-Path $RepoRoot "target\release\robocopy_ingest.exe"
$Source     = "C:\Users\auresystem\claude-code"
$Dest       = "\\FILESERV01\dati01\provarust2"
$ReportsDir = Join-Path $RepoRoot "_ops_reports"
$Timestamp  = Get-Date -Format "yyyyMMdd_HHmmss"

New-Item -ItemType Directory -Force -Path $ReportsDir | Out-Null

$ReportPath = Join-Path $ReportsDir "claude-code_$Timestamp.json"
$LogPath    = Join-Path $ReportsDir "claude-code_$Timestamp.log"
$HtmlPath   = Join-Path $ReportsDir "claude-code_$Timestamp.html"

if (-not (Test-Path $Exe)) {
    Write-Host "Binary not found at $Exe - build it first with:" -ForegroundColor Yellow
    Write-Host "  cargo build --release" -ForegroundColor Yellow
    exit 1
}

$ArgList = @(
    "--source", $Source,
    "--dest", $Dest,
    "--verify-integrity",
    "--hash-algo", "blake3",
    "--report-path", $ReportPath,
    "--log-path", $LogPath,
    "--html-report-path", $HtmlPath
)

if ($DryRun) {
    $ArgList += "--dry-run"
    Write-Host "Running in --dry-run mode: nothing will be copied." -ForegroundColor Cyan
}

Write-Host "Source : $Source"
Write-Host "Dest   : $Dest"
Write-Host "Report : $ReportPath"
Write-Host "Log    : $LogPath"
Write-Host "HTML   : $HtmlPath"
Write-Host ""

& $Exe @ArgList
$ExitCode = $LASTEXITCODE

Write-Host ""
Write-Host "Exit code: $ExitCode" -ForegroundColor $(if ($ExitCode -eq 0) { "Green" } else { "Red" })
switch ($ExitCode) {
    0 { Write-Host "Success." -ForegroundColor Green }
    1 { Write-Host "Completed with problems (see report/log)." -ForegroundColor Yellow }
    2 { Write-Host "Unrecoverable error (bad args, missing source, etc.)." -ForegroundColor Red }
    3 { Write-Host "Mirror-purge aborted (only relevant if --mirror was used)." -ForegroundColor Red }
}

if (Test-Path $HtmlPath) {
    Write-Host "Opening the HTML report..."
    Start-Process $HtmlPath
}

exit $ExitCode
