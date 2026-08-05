<#
.SYNOPSIS
    Backs up C:\Users\auresystem\claude-code to \\FILESERV01\dati01\provarust2 (Windows SMB
    share - no NAS-style credential mapping needed).

.DESCRIPTION
    Split out from the old combined run-ingest-claude-code.ps1 (5 August 2026): the SMB-to-
    Windows-share and SMB-to-NAS legs turned out to have different enough performance profiles
    in practice (measured on the NAS leg: ~26 Mbit/s realised throughput copying 32k small files,
    dominated by SMB per-file overhead) that each destination deserves its own --threads tuning
    without a shared script muddying which number applies to which leg. See
    scripts/benchmark-threads.ps1 to validate a value for THIS destination before hardcoding one
    below via -Threads.

    Shares the actual invocation/exit-code logic with backup-nas-qnap.ps1 via
    scripts/_ingest-common.ps1, so a fix there only needs to happen once.

    Report/log/html are written under this repo's own _ops_reports folder, OUTSIDE the source
    tree being copied - this avoids the self-referential integrity "mismatch" that happens when
    the tool's own live log file sits inside the folder it is scanning.

.EXAMPLE
    .\scripts\backup-fileserv01.ps1
    .\scripts\backup-fileserv01.ps1 -DryRun
    .\scripts\backup-fileserv01.ps1 -Threads 16
#>

param(
    [switch]$DryRun,
    [int]$Threads   # validate with .\scripts\benchmark-threads.ps1 before hardcoding a value here
)

$ErrorActionPreference = "Stop"

$RepoRoot   = Split-Path -Parent $PSScriptRoot
$Exe        = Join-Path $RepoRoot "target\release\robocopy_ingest.exe"
$Source     = "C:\Users\auresystem\claude-code"
$Dest       = "\\FILESERV01\dati01\provarust2"
$ReportsDir = Join-Path $RepoRoot "_ops_reports"
$Timestamp  = Get-Date -Format "yyyyMMdd_HHmmss"

New-Item -ItemType Directory -Force -Path $ReportsDir | Out-Null

if (-not (Test-Path $Exe)) {
    Write-Host "Binary not found at $Exe - build it first with:" -ForegroundColor Yellow
    Write-Host "  cargo build --release" -ForegroundColor Yellow
    exit 1
}

. (Join-Path $PSScriptRoot "_ingest-common.ps1")

$exitCode = Invoke-Ingest -Exe $Exe -Label "claude-code_fileserv01" -Source $Source -Dest $Dest `
    -ReportsDir $ReportsDir -Timestamp $Timestamp -Threads $Threads -DryRun:$DryRun

exit $exitCode
