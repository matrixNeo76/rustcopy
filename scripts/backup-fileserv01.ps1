<#
.SYNOPSIS
    Backs up C:\Users\auresystem\claude-code to \\FILESERV01\dati01\provarust2 (Windows SMB
    share - no NAS-style credential mapping needed).

.DESCRIPTION
    Thin wrapper around scripts\rustcopy-launcher.ps1's "fileserv01" profile (PIANO_MIGLIORAMENTI.md,
    "Refactor script -> wrapper"): all the actual invocation/exit-code/report logic now lives in
    the launcher (via scripts\_ingest-common.ps1's Invoke-Ingest, as before), so a fix there only
    needs to happen once instead of being duplicated per destination script. This script's only
    remaining job is to self-seed its profile into scripts\profiles.json on first run (via
    Confirm-RustcopyProfile) with exactly the values it used to hardcode directly, then delegate.

    See scripts/benchmark-threads.ps1 to validate a --threads value for THIS destination before
    hardcoding one below via -Threads. Measured on the NAS leg (backup-nas-qnap.ps1): ~26 Mbit/s
    realised throughput copying 32k small files, dominated by SMB per-file overhead -- the same
    caution likely applies here.

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

. (Join-Path $PSScriptRoot "_profiles-common.ps1")

$ProfilesPath = Join-Path $PSScriptRoot "profiles.json"
$defaults = [PSCustomObject]@{
    name               = "fileserv01"
    source             = "C:\Users\auresystem\claude-code"
    dest               = "\\FILESERV01\dati01\provarust2"
    threads            = $null
    mirror             = $false
    force_purge        = $false
    verify_integrity   = $true
    hash_algo          = "blake3"
    requires_smb_creds = $false
    creds_file         = $null
}
Confirm-RustcopyProfile -Path $ProfilesPath -Defaults $defaults | Out-Null

& (Join-Path $PSScriptRoot "rustcopy-launcher.ps1") -Profile "fileserv01" -DryRun:$DryRun -Threads $Threads -ProfilesPath $ProfilesPath
exit $LASTEXITCODE
