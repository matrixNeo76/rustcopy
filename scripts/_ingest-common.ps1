<#
.SYNOPSIS
    Shared helper dot-sourced by scripts/backup-fileserv01.ps1 and scripts/backup-nas-qnap.ps1:
    the actual robocopy_ingest.exe invocation, report/log/html path naming, and exit-code
    interpretation.

.DESCRIPTION
    Not meant to be run directly - it only defines Invoke-Ingest. Dot-source it:
        . (Join-Path $PSScriptRoot "_ingest-common.ps1")

    Kept in one place, rather than duplicated per destination script, so a fix here only needs to
    happen once - this exit-code switch has already needed two real fixes in one day (the F28
    --fast-verify addition and the F29b/F35 exit-code-4/5 gap) after the two destinations were
    still one combined script; splitting into two scripts without sharing this part would double
    that maintenance cost going forward.
#>

function Invoke-Ingest {
    param(
        [Parameter(Mandatory)] [string]$Exe,
        [Parameter(Mandatory)] [string]$Label,
        [Parameter(Mandatory)] [string]$Source,
        [Parameter(Mandatory)] [string]$Dest,
        [Parameter(Mandatory)] [string]$ReportsDir,
        [Parameter(Mandatory)] [string]$Timestamp,
        [int]$Threads,
        [string]$HashAlgo = "blake3",
        [switch]$DryRun
    )

    $reportPath = Join-Path $ReportsDir "${Label}_$Timestamp.json"
    $logPath    = Join-Path $ReportsDir "${Label}_$Timestamp.log"
    $htmlPath   = Join-Path $ReportsDir "${Label}_$Timestamp.html"

    $argList = @(
        "--source", $Source,
        "--dest", $Dest,
        "--verify-integrity",
        "--hash-algo", $HashAlgo,
        # F28: skip re-hashing files whose source size+mtime already match the last clean pass
        # (cached per-destination in <dest>\.ingest_cache).
        "--fast-verify",
        "--report-path", $reportPath,
        "--log-path", $logPath,
        "--html-report-path", $htmlPath
    )
    if ($Threads) { $argList += @("--threads", "$Threads") }
    if ($DryRun) { $argList += "--dry-run" }

    Write-Host ""
    Write-Host "=== [$Label] $Source -> $Dest ===" -ForegroundColor Cyan
    Write-Host "Report : $reportPath"
    Write-Host "Log    : $logPath"
    Write-Host "HTML   : $htmlPath"
    Write-Host ""

    # Piping the exe's own stdout through Out-Host explicitly terminates the pipeline for this
    # statement, so it still prints live to the console but does NOT also flow into this
    # function's own return value - `$x = Invoke-Ingest ...` gets a clean exit code, not the
    # exe's stdout lines mixed in with it.
    & $Exe @argList | Out-Host
    $exitCode = $LASTEXITCODE

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

    return $exitCode
}
