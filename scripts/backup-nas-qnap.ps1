<#
.SYNOPSIS
    Backs up C:\Users\auresystem\claude-code to the QNAP NAS at \\192.168.1.187\datas01 over
    SMB (credentialed mapping required).

.DESCRIPTION
    Thin wrapper around scripts\rustcopy-launcher.ps1's "nas-qnap" profile (PIANO_MIGLIORAMENTI.md,
    "Refactor script -> wrapper"): all the actual invocation/exit-code/report/SMB-mapping logic
    now lives in the launcher, so a fix there only needs to happen once. This script's remaining
    job is to (1) self-seed its profile on first run, (2) bridge the pre-existing SMB credentials
    file to the shape the launcher expects, and (3) support -Subfolder as a one-off override
    without persisting it into the stored profile.

    Credentials adapter (decision "opzione 2", 18 Ago 2026): the launcher's SMB-credentialed
    profiles expect a creds_file defining $SmbUser/$SmbPassword, a different shape from this
    script's pre-existing scripts\nas2-credentials.local.ps1 ($Nas2User/$Nas2Password). Rather
    than rename the variables in that file directly -- it holds a real password and a human, not
    this script, has to touch it -- Sync-LegacyNasCredentials below regenerates a
    nas-qnap-credentials.local.ps1 adapter file from it on every run, so the two stay in sync
    automatically if the password in the legacy file ever changes. Both files are gitignored
    (scripts/*.local.ps1).

    Measured real-world throughput on this leg: ~26 Mbit/s copying 32k small files (~25 KB
    average), dominated by SMB per-file protocol overhead rather than raw bandwidth - don't
    assume a higher --threads helps without validating it with scripts/benchmark-threads.ps1
    against this actual NAS first.

.EXAMPLE
    .\scripts\backup-nas-qnap.ps1
    .\scripts\backup-nas-qnap.ps1 -DryRun
    .\scripts\backup-nas-qnap.ps1 -Subfolder "backup01"
#>

param(
    [switch]$DryRun,
    [int]$Threads,   # validate with .\scripts\benchmark-threads.ps1 before hardcoding a value here
    # Copy into a subfolder of the share instead of its root (e.g. "backup01" for
    # \\192.168.1.187\datas01\backup01), for this run only -- does not modify the stored profile.
    [string]$Subfolder
)

$ErrorActionPreference = "Stop"

. (Join-Path $PSScriptRoot "_profiles-common.ps1")

function Sync-LegacyNasCredentials {
    param(
        [Parameter(Mandatory)][string]$NewCredsPath,
        [Parameter(Mandatory)][string]$LegacyCredsPath
    )

    # Dot-sourced in a nested scope of this function, not the script's own scope, so
    # $Nas2User/$Nas2Password never leak into the rest of this script.
    . $LegacyCredsPath
    if (-not $Nas2User -or -not $Nas2Password) { return $false }

    # Escaped before interpolation: an unescaped apostrophe in either legacy value would close
    # the single-quoted literal early and corrupt (or inject into) the generated adapter file,
    # which gets dot-sourced by the launcher on every run.
    $nas2UserLiteral = ConvertTo-RustcopySingleQuotedLiteral -Value $Nas2User
    $nas2PasswordLiteral = ConvertTo-RustcopySingleQuotedLiteral -Value $Nas2Password

    $content = @"
<#
    Auto-generated adapter (regenerated on every backup-nas-qnap.ps1 run) - translates
    scripts\nas2-credentials.local.ps1's `$Nas2User/`$Nas2Password into the shape
    scripts\rustcopy-launcher.ps1 profiles expect. Do not edit by hand: edit
    nas2-credentials.local.ps1 instead, this file is overwritten from it every run.
#>

`$SmbUser     = '$nas2UserLiteral'
`$SmbPassword = '$nas2PasswordLiteral'
"@
    Set-Content -LiteralPath $NewCredsPath -Value $content -Encoding utf8
    return $true
}

$ProfilesPath    = Join-Path $PSScriptRoot "profiles.json"
$NewCredsFile    = "nas-qnap-credentials.local.ps1"
$NewCredsPath    = Join-Path $PSScriptRoot $NewCredsFile
$LegacyCredsPath = Join-Path $PSScriptRoot "nas2-credentials.local.ps1"

# Checked separately from Sync-LegacyNasCredentials's own return value so "file missing" and
# "file present but incomplete" get distinct messages instead of both being reported as
# "not found", which would contradict what's actually on disk in the second case.
if (-not (Test-Path -LiteralPath $LegacyCredsPath)) {
    Write-Host "NAS backup skipped: $LegacyCredsPath not found." -ForegroundColor Yellow
    Write-Host "Create it with `$Nas2User and `$Nas2Password to enable this script." -ForegroundColor Yellow
    exit 1
}
if (-not (Sync-LegacyNasCredentials -NewCredsPath $NewCredsPath -LegacyCredsPath $LegacyCredsPath)) {
    Write-Host "NAS backup skipped: $LegacyCredsPath exists but does not define both `$Nas2User and `$Nas2Password." -ForegroundColor Yellow
    exit 1
}

$DestRoot = "\\192.168.1.187\datas01"
$defaults = [PSCustomObject]@{
    name               = "nas-qnap"
    source             = "C:\Users\auresystem\claude-code"
    dest               = $DestRoot
    threads            = $null
    mirror             = $false
    force_purge        = $false
    verify_integrity   = $true
    hash_algo          = "blake3"
    requires_smb_creds = $true
    creds_file         = $NewCredsFile
}
Confirm-RustcopyProfile -Path $ProfilesPath -Defaults $defaults | Out-Null

if ($Subfolder) {
    # One-off override: run against a subfolder of the share without persisting it into the
    # stored profile (which stays pointed at the share root). Reuses -ProfilesPath, the same
    # extension point the launcher already exposes for pointing it at a non-default profiles
    # file, instead of adding a new override mechanism to the launcher itself.
    $override = [PSCustomObject]@{
        name               = $defaults.name
        source             = $defaults.source
        dest               = Join-Path $DestRoot $Subfolder
        threads            = $defaults.threads
        mirror             = $defaults.mirror
        force_purge        = $defaults.force_purge
        verify_integrity   = $defaults.verify_integrity
        hash_algo          = $defaults.hash_algo
        requires_smb_creds = $defaults.requires_smb_creds
        creds_file         = $defaults.creds_file
    }
    $tempProfilesPath = Join-Path ([System.IO.Path]::GetTempPath()) "rustcopy-nas-qnap-subfolder-$PID.json"
    Save-RustcopyProfiles -Path $tempProfilesPath -Profiles @($override)
    try {
        & (Join-Path $PSScriptRoot "rustcopy-launcher.ps1") -Profile $defaults.name -DryRun:$DryRun -Threads $Threads -ProfilesPath $tempProfilesPath
        exit $LASTEXITCODE
    }
    finally {
        Remove-Item -LiteralPath $tempProfilesPath -Force -ErrorAction SilentlyContinue
    }
}

& (Join-Path $PSScriptRoot "rustcopy-launcher.ps1") -Profile $defaults.name -DryRun:$DryRun -Threads $Threads -ProfilesPath $ProfilesPath
exit $LASTEXITCODE
