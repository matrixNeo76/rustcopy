<#
.SYNOPSIS
    Backs up C:\Users\auresystem\claude-code to the QNAP NAS at \\192.168.1.187\datas01 over
    SMB (credentialed mapping required).

.DESCRIPTION
    Split out from the old combined run-ingest-claude-code.ps1 (5 August 2026) - see
    backup-fileserv01.ps1's own note for why. Measured real-world throughput on this leg: ~26
    Mbit/s copying 32k small files (~25 KB average), dominated by SMB per-file protocol overhead
    rather than raw bandwidth - don't assume a higher --threads helps without validating it with
    scripts/benchmark-threads.ps1 against this actual NAS first.

    Requires SMB credentials (user "backup") in scripts\nas2-credentials.local.ps1, which is NOT
    committed to git (see .gitignore: scripts/*.local.ps1) so the password never ends up in
    source control. Establishes a credentialed SMB mapping to the share root before the copy and
    removes it afterwards, whether the copy succeeded or not.

    Shares the actual invocation/exit-code logic with backup-fileserv01.ps1 via
    scripts/_ingest-common.ps1, so a fix there only needs to happen once.

.EXAMPLE
    .\scripts\backup-nas-qnap.ps1
    .\scripts\backup-nas-qnap.ps1 -DryRun
    .\scripts\backup-nas-qnap.ps1 -Subfolder "backup01"
#>

param(
    [switch]$DryRun,
    [int]$Threads,   # validate with .\scripts\benchmark-threads.ps1 before hardcoding a value here
    # Copy into a subfolder of the share instead of its root (e.g. "backup01" for
    # \\192.168.1.187\datas01\backup01). The SMB credential mapping still targets the share root.
    [string]$Subfolder
)

$ErrorActionPreference = "Stop"

$RepoRoot   = Split-Path -Parent $PSScriptRoot
$Exe        = Join-Path $RepoRoot "target\release\robocopy_ingest.exe"
$Source     = "C:\Users\auresystem\claude-code"
$DestRoot   = "\\192.168.1.187\datas01"
$Dest       = if ($Subfolder) { Join-Path $DestRoot $Subfolder } else { $DestRoot }
$ReportsDir = Join-Path $RepoRoot "_ops_reports"
$Timestamp  = Get-Date -Format "yyyyMMdd_HHmmss"

New-Item -ItemType Directory -Force -Path $ReportsDir | Out-Null

if (-not (Test-Path $Exe)) {
    Write-Host "Binary not found at $Exe - build it first with:" -ForegroundColor Yellow
    Write-Host "  cargo build --release" -ForegroundColor Yellow
    exit 1
}

$credsFile = Join-Path $PSScriptRoot "nas2-credentials.local.ps1"
if (-not (Test-Path $credsFile)) {
    Write-Host "NAS backup skipped: $credsFile not found." -ForegroundColor Yellow
    Write-Host "Create it with `$Nas2User and `$Nas2Password to enable this script." -ForegroundColor Yellow
    exit 1
}
. $credsFile

. (Join-Path $PSScriptRoot "_ingest-common.ps1")

$mappingEstablished = $false
try {
    Write-Host "Authenticating to $DestRoot as $Nas2User ..." -ForegroundColor Cyan
    New-SmbMapping -RemotePath $DestRoot -UserName $Nas2User -Password $Nas2Password -Persistent $false -ErrorAction Stop | Out-Null
    $mappingEstablished = $true

    # No need to pre-create $Dest (subfolder or not): robocopy_ingest's own execute() already
    # creates the destination directory if missing (outside --dry-run) before transferring.
    $exitCode = Invoke-Ingest -Exe $Exe -Label "claude-code_nas-qnap" -Source $Source -Dest $Dest `
        -ReportsDir $ReportsDir -Timestamp $Timestamp -Threads $Threads -DryRun:$DryRun
}
catch {
    Write-Host "Could not reach/authenticate to $DestRoot : $_" -ForegroundColor Red
    $exitCode = 2
}
finally {
    if ($mappingEstablished) {
        Remove-SmbMapping -RemotePath $DestRoot -Force -ErrorAction SilentlyContinue | Out-Null
    }
    # Credentials only need to live for the duration of this script.
    Remove-Variable -Name Nas2User, Nas2Password -ErrorAction SilentlyContinue
}

exit $exitCode
