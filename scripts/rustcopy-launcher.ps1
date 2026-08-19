<#
.SYNOPSIS
    Interactive launcher for robocopy_ingest.exe: manage named backup profiles (source, dest,
    threads, mirror, verify-integrity, hash algorithm, SMB credentials) instead of copying and
    hand-editing a dedicated script per destination.

.DESCRIPTION
    Reads/writes scripts\profiles.json (gitignored - see scripts\profiles.example.json for the
    shape) via scripts\_profiles-common.ps1, shared with the thin wrapper scripts
    (backup-fileserv01.ps1, backup-nas-qnap.ps1, run-all-profiles.ps1) so they can self-seed their
    own profile without dot-sourcing this whole file. Every actual copy still goes through
    scripts\_ingest-common.ps1's Invoke-Ingest, the same shared helper those wrappers used to call
    directly -- this script only builds the profile data and the arguments passed to it; it never
    talks to robocopy_ingest.exe directly and never reimplements any backup logic of its own.

    Two modes:
      - No -Profile given: interactive menu (list/new/edit/delete/run, with a menu of --mirror /
        --threads / --dry-run / --verify-integrity overrides at run time).
      - -Profile <name> given: batch mode, no prompts at all -- for Task Scheduler. Only -DryRun
        and -Threads are accepted as overrides, both optional.

    New-profile and edit-profile use plain Read-Host text prompts with Test-Path validation and
    re-prompting on error, not a Windows Forms folder-browse dialog -- see PIANO_MIGLIORAMENTI.md
    (decision D-Q1) for why: every destination in this project is a UNC path, which a folder
    dialog handles worse than typed input; batch mode must never touch System.Windows.Forms
    (breaks under Task Scheduler's non-interactive session); and powershell.exe 5.1 (STA) /
    pwsh 7 (MTA by default) would need different threading-model workarounds for a dialog to even
    work reliably in both.

    SMB credentials: a profile with requires_smb_creds = true dot-sources its creds_file (relative
    to this script's folder) before mapping the destination's UNC share root, same lifecycle as
    backup-nas-qnap.ps1 used to implement directly (map before the copy, remove after, even on
    failure). The creds file must define $SmbUser and $SmbPassword -- a generic convention for
    launcher-managed profiles, deliberately not the legacy scripts\nas2-credentials.local.ps1's
    $Nas2User/$Nas2Password names. backup-nas-qnap.ps1 is now a thin wrapper around this launcher
    (see PIANO_MIGLIORAMENTI.md, "Refactor script -> wrapper") and keeps working against the
    legacy file via its own Sync-LegacyNasCredentials adapter, which regenerates a
    $SmbUser/$SmbPassword file from it on every run rather than requiring the legacy file to be
    hand-edited. The new-profile wizard offers to generate a fresh creds file directly when a
    profile created some other way needs one.

.EXAMPLE
    .\scripts\rustcopy-launcher.ps1
    .\scripts\rustcopy-launcher.ps1 -Profile fileserv01
    .\scripts\rustcopy-launcher.ps1 -Profile fileserv01 -DryRun
    .\scripts\rustcopy-launcher.ps1 -Profile nas-qnap -Threads 8
#>

param(
    # Batch mode: run this profile non-interactively and exit. Omit for the interactive menu.
    [string]$Profile,
    [switch]$DryRun,
    [int]$Threads,
    # Override for scripts\profiles.json's location -- mainly useful for testing this script
    # itself against a throwaway profiles file instead of the real one.
    [string]$ProfilesPath
)

$ErrorActionPreference = "Stop"

$RepoRoot     = Split-Path -Parent $PSScriptRoot
$Exe          = Join-Path $RepoRoot "target\release\robocopy_ingest.exe"
$ReportsRoot  = Join-Path $RepoRoot "_ops_reports"
$ProfilesFile = if ($ProfilesPath) { $ProfilesPath } else { Join-Path $PSScriptRoot "profiles.json" }

if (-not (Test-Path $Exe)) {
    Write-Host "Binary not found at $Exe - build it first with:" -ForegroundColor Yellow
    Write-Host "  cargo build --release" -ForegroundColor Yellow
    exit 1
}

. (Join-Path $PSScriptRoot "_ingest-common.ps1")
# Import-RustcopyProfiles / Save-RustcopyProfiles / Get-UncShareRoot / Confirm-RustcopyProfile --
# shared with the wrapper scripts (backup-fileserv01.ps1, backup-nas-qnap.ps1, run-all-profiles.ps1).
. (Join-Path $PSScriptRoot "_profiles-common.ps1")

# ---------------------------------------------------------------------------------------------
# Prompt helpers (D-Q1: plain terminal input, validated, re-prompt on error)
# ---------------------------------------------------------------------------------------------

function Read-NonEmpty {
    param([Parameter(Mandatory)][string]$Prompt, [string]$DefaultValue)
    while ($true) {
        $suffix = if ($DefaultValue) { " [$DefaultValue]" } else { "" }
        $value = Read-Host "$Prompt$suffix"
        if ([string]::IsNullOrWhiteSpace($value)) {
            if ($DefaultValue) { return $DefaultValue }
            Write-Host "  This cannot be empty." -ForegroundColor Yellow
            continue
        }
        return $value
    }
}

function Read-RustcopyPath {
    param([Parameter(Mandatory)][string]$Prompt, [switch]$MustExist, [string]$DefaultValue)
    while ($true) {
        $value = Read-NonEmpty -Prompt $Prompt -DefaultValue $DefaultValue
        if ($MustExist -and -not (Test-Path -LiteralPath $value)) {
            Write-Host "  Path not found: $value" -ForegroundColor Yellow
            continue
        }
        return $value
    }
}

function Read-RustcopyInt {
    param([Parameter(Mandatory)][string]$Prompt, [Nullable[int]]$DefaultValue)
    while ($true) {
        $suffix = if ($null -ne $DefaultValue) { " [$DefaultValue]" } else { " (blank = let rustcopy choose automatically)" }
        $raw = Read-Host "$Prompt$suffix"
        if ([string]::IsNullOrWhiteSpace($raw)) { return $DefaultValue }
        $parsed = 0
        if ([int]::TryParse($raw, [ref]$parsed) -and $parsed -gt 0) { return $parsed }
        Write-Host "  Enter a positive whole number, or leave blank." -ForegroundColor Yellow
    }
}

function Read-YesNo {
    param([Parameter(Mandatory)][string]$Prompt, [bool]$DefaultValue = $false)
    $suffix = if ($DefaultValue) { "(Y/n)" } else { "(y/N)" }
    $answer = Read-Host "$Prompt $suffix"
    if ([string]::IsNullOrWhiteSpace($answer)) { return $DefaultValue }
    return $answer -match '^[yY]'
}

# ---------------------------------------------------------------------------------------------
# Profile CRUD (interactive)
# ---------------------------------------------------------------------------------------------

function New-RustcopyProfileInteractive {
    param([array]$ExistingProfiles)

    Write-Host ""
    Write-Host "=== New profile ===" -ForegroundColor Cyan

    $name = $null
    while (-not $name) {
        $candidate = Read-NonEmpty -Prompt "Profile name (e.g. fileserv01)"
        if ($ExistingProfiles | Where-Object { $_.name -eq $candidate }) {
            Write-Host "  A profile named '$candidate' already exists." -ForegroundColor Yellow
            continue
        }
        $name = $candidate
    }

    $source = Read-RustcopyPath -Prompt "Source path" -MustExist
    $dest   = Read-RustcopyPath -Prompt "Destination path (UNC or local; need not exist yet)"
    $threads = Read-RustcopyInt -Prompt "Threads"
    $mirror  = Read-YesNo -Prompt "Mirror (delete files in dest that are not in source)?" -DefaultValue $false
    # Never implied by $mirror alone (AGENTS.md rule 6: --force-purge must never become the
    # default) -- only asked, and only ever set true, when the operator explicitly wants an
    # unattended --mirror run to purge without a terminal confirmation available to answer it.
    $forcePurge = $false
    if ($mirror) {
        $forcePurge = Read-YesNo -Prompt "  Allow this profile to purge WITHOUT confirmation when run unattended (e.g. Task Scheduler)? Leave No to have unattended runs safely abort instead" -DefaultValue $false
    }
    $verifyIntegrity = Read-YesNo -Prompt "Verify integrity after copy?" -DefaultValue $true
    $hashAlgo = Read-NonEmpty -Prompt "Hash algorithm (sha256/blake3/xxh3)" -DefaultValue "blake3"

    $requiresCreds = Read-YesNo -Prompt "Does the destination need SMB credentials?" -DefaultValue $false
    $credsFile = $null
    if ($requiresCreds) {
        $credsFile = "$name-credentials.local.ps1"
        $credsPath = Join-Path $PSScriptRoot $credsFile
        if (Test-Path -LiteralPath $credsPath) {
            Write-Host "  Reusing existing $credsFile" -ForegroundColor Cyan
        }
        else {
            $smbUser = Read-NonEmpty -Prompt "  SMB username"
            # Plaintext in a gitignored *.local.ps1 file, same convention already established by
            # scripts\nas2-credentials.local.ps1 -- not a new/weaker choice introduced here.
            $smbPassword = Read-NonEmpty -Prompt "  SMB password"
            # Escaped before interpolation: an unescaped apostrophe in either value would close
            # the single-quoted literal early and corrupt (or inject into) the generated file,
            # which gets dot-sourced on every run.
            $smbUserLiteral = ConvertTo-RustcopySingleQuotedLiteral -Value $smbUser
            $smbPasswordLiteral = ConvertTo-RustcopySingleQuotedLiteral -Value $smbPassword
            $credsContent = @"
<#
    Local-only SMB credentials for the '$name' rustcopy-launcher profile.
    Excluded from git via .gitignore: scripts/*.local.ps1
#>

`$SmbUser     = '$smbUserLiteral'
`$SmbPassword = '$smbPasswordLiteral'
"@
            Set-Content -LiteralPath $credsPath -Value $credsContent -Encoding utf8
            Write-Host "  Credentials saved to $credsFile (gitignored)." -ForegroundColor Green
        }
    }

    return [PSCustomObject]@{
        name               = $name
        source             = $source
        dest               = $dest
        threads            = $threads
        mirror             = $mirror
        force_purge        = $forcePurge
        verify_integrity   = $verifyIntegrity
        hash_algo          = $hashAlgo
        requires_smb_creds = $requiresCreds
        creds_file         = $credsFile
    }
}

function Edit-RustcopyProfileInteractive {
    param([Parameter(Mandatory)][PSCustomObject]$RustcopyProfile)

    Write-Host ""
    Write-Host "=== Edit profile '$($RustcopyProfile.name)' (Enter keeps the current value) ===" -ForegroundColor Cyan

    $RustcopyProfile.source = Read-RustcopyPath -Prompt "Source path" -MustExist -DefaultValue $RustcopyProfile.source
    $RustcopyProfile.dest   = Read-RustcopyPath -Prompt "Destination path" -DefaultValue $RustcopyProfile.dest
    $RustcopyProfile.threads = Read-RustcopyInt -Prompt "Threads" -DefaultValue $RustcopyProfile.threads
    $RustcopyProfile.mirror = Read-YesNo -Prompt "Mirror?" -DefaultValue ([bool]$RustcopyProfile.mirror)
    # Add-Member -Force, not plain assignment: a profile saved before force_purge existed has no
    # such NoteProperty yet, and assigning to a genuinely missing property on a PSCustomObject
    # throws (verified) -- unlike reading one, which just returns $null. -Force makes this work
    # identically whether the property already exists or not.
    $forcePurge = if ($RustcopyProfile.mirror) {
        Read-YesNo -Prompt "  Allow this profile to purge WITHOUT confirmation when run unattended?" -DefaultValue ([bool]$RustcopyProfile.force_purge)
    }
    else {
        $false
    }
    $RustcopyProfile | Add-Member -NotePropertyName force_purge -NotePropertyValue $forcePurge -Force
    $RustcopyProfile.verify_integrity = Read-YesNo -Prompt "Verify integrity?" -DefaultValue ([bool]$RustcopyProfile.verify_integrity)
    $RustcopyProfile.hash_algo = Read-NonEmpty -Prompt "Hash algorithm" -DefaultValue $RustcopyProfile.hash_algo

    return $RustcopyProfile
}

# ---------------------------------------------------------------------------------------------
# Run a profile
# ---------------------------------------------------------------------------------------------

function Invoke-RustcopyProfile {
    param(
        [Parameter(Mandatory)][PSCustomObject]$RustcopyProfile,
        [Parameter(Mandatory)][string]$Exe,
        [Parameter(Mandatory)][string]$ReportsRoot,
        [switch]$DryRun,
        [int]$ThreadsOverride,
        [switch]$Interactive
    )

    $threads = if ($ThreadsOverride) { $ThreadsOverride } elseif ($RustcopyProfile.threads) { [int]$RustcopyProfile.threads } else { 0 }
    $mirror = [bool]$RustcopyProfile.mirror
    # [bool]$null is $false, so a profile saved before force_purge existed (missing property,
    # reads back as $null) safely defaults to "unattended --mirror aborts on extraneous files"
    # rather than silently gaining purge-without-confirmation behavior.
    $forcePurge = [bool]$RustcopyProfile.force_purge
    $verifyIntegrity = if ($null -eq $RustcopyProfile.verify_integrity) { $true } else { [bool]$RustcopyProfile.verify_integrity }
    $dryRunEffective = [bool]$DryRun

    if ($Interactive) {
        Write-Host ""
        Write-Host "=== Run options for '$($RustcopyProfile.name)' (Enter keeps the profile's saved value) ===" -ForegroundColor Cyan
        $mirror = Read-YesNo -Prompt "  --mirror?" -DefaultValue $mirror
        if ($mirror) {
            $forcePurge = Read-YesNo -Prompt "  --force-purge (skip confirmation on destination-only files)?" -DefaultValue $forcePurge
        }
        else {
            $forcePurge = $false
        }
        $threadsPrompted = Read-RustcopyInt -Prompt "  --threads" -DefaultValue $(if ($threads) { $threads } else { $null })
        if ($threadsPrompted) { $threads = $threadsPrompted }
        $dryRunEffective = Read-YesNo -Prompt "  --dry-run?" -DefaultValue $dryRunEffective
        $verifyIntegrity = Read-YesNo -Prompt "  --verify-integrity?" -DefaultValue $verifyIntegrity
    }

    $timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
    $reportsDir = Join-Path (Join-Path $ReportsRoot $RustcopyProfile.name) $timestamp
    New-Item -ItemType Directory -Force -Path $reportsDir | Out-Null

    $mappingEstablished = $false
    $shareRoot = $null
    $exitCode = 2
    try {
        if ($RustcopyProfile.requires_smb_creds) {
            $credsPath = Join-Path $PSScriptRoot $RustcopyProfile.creds_file
            if (-not (Test-Path -LiteralPath $credsPath)) {
                Write-Host "SMB credentials file not found: $credsPath" -ForegroundColor Red
                Write-Host "Expected shape: `$SmbUser / `$SmbPassword (see scripts\nas2-credentials.local.ps1 for the pattern this follows)." -ForegroundColor Yellow
                return 2
            }
            . $credsPath

            $shareRoot = Get-UncShareRoot -UncPath $RustcopyProfile.dest
            if (-not $shareRoot) {
                Write-Host "Destination '$($RustcopyProfile.dest)' is not a UNC path (\\server\share\...) - cannot map SMB credentials to it." -ForegroundColor Red
                return 2
            }

            Write-Host "Authenticating to $shareRoot as $SmbUser ..." -ForegroundColor Cyan
            New-SmbMapping -RemotePath $shareRoot -UserName $SmbUser -Password $SmbPassword -Persistent $false -ErrorAction Stop | Out-Null
            $mappingEstablished = $true
        }

        # No need to pre-create $RustcopyProfile.dest: robocopy_ingest's own execute() already
        # creates the destination directory if missing (outside --dry-run) before transferring,
        # same as backup-nas-qnap.ps1 relies on for its own Subfolder case.
        $exitCode = Invoke-Ingest -Exe $Exe -Label $RustcopyProfile.name -Source $RustcopyProfile.source -Dest $RustcopyProfile.dest `
            -ReportsDir $reportsDir -Timestamp $timestamp -Threads $threads -HashAlgo $RustcopyProfile.hash_algo `
            -Mirror:$mirror -ForcePurge:$forcePurge -VerifyIntegrity $verifyIntegrity -DryRun:$dryRunEffective
    }
    catch {
        Write-Host "Could not run profile '$($RustcopyProfile.name)': $_" -ForegroundColor Red
        $exitCode = 2
    }
    finally {
        if ($mappingEstablished -and $shareRoot) {
            Remove-SmbMapping -RemotePath $shareRoot -Force -ErrorAction SilentlyContinue | Out-Null
        }
        Remove-Variable -Name SmbUser, SmbPassword -ErrorAction SilentlyContinue
    }

    return $exitCode
}

# ---------------------------------------------------------------------------------------------
# Batch mode: -Profile given, no menu, no prompts (Task Scheduler entry point)
# ---------------------------------------------------------------------------------------------

if ($Profile) {
    $profiles = @(Import-RustcopyProfiles -Path $ProfilesFile)
    $match = $profiles | Where-Object { $_.name -eq $Profile } | Select-Object -First 1
    if (-not $match) {
        Write-Host "No profile named '$Profile' in $ProfilesFile" -ForegroundColor Red
        exit 2
    }
    $exitCode = Invoke-RustcopyProfile -RustcopyProfile $match -Exe $Exe -ReportsRoot $ReportsRoot -DryRun:$DryRun -ThreadsOverride $Threads
    exit $exitCode
}

# ---------------------------------------------------------------------------------------------
# Interactive mode
# ---------------------------------------------------------------------------------------------

function Show-RustcopyMainMenu {
    param([array]$Profiles)

    Write-Host ""
    Write-Host "=== RUSTCOPY LAUNCHER ===" -ForegroundColor Cyan
    if ($Profiles.Count -eq 0) {
        Write-Host "  (no profiles saved yet)"
    }
    else {
        for ($i = 0; $i -lt $Profiles.Count; $i++) {
            $p = $Profiles[$i]
            Write-Host ("  [{0}] {1}   {2} -> {3}" -f ($i + 1), $p.name, $p.source, $p.dest)
        }
    }
    Write-Host ""
    Write-Host "  Enter a number to run that profile."
    Write-Host "  [N] New profile   [E] Edit a profile   [D] Delete a profile   [Q] Quit"
    Write-Host ""
}

$profiles = @(Import-RustcopyProfiles -Path $ProfilesFile)

while ($true) {
    Show-RustcopyMainMenu -Profiles $profiles
    $choice = Read-Host "Choice"

    switch -Regex ($choice) {
        '^[Qq]$' {
            exit 0
        }
        '^[Nn]$' {
            $newProfile = New-RustcopyProfileInteractive -ExistingProfiles $profiles
            $profiles = @($profiles) + $newProfile
            Save-RustcopyProfiles -Path $ProfilesFile -Profiles $profiles
            Write-Host "Profile '$($newProfile.name)' saved." -ForegroundColor Green
        }
        '^[Ee]$' {
            if ($profiles.Count -eq 0) { Write-Host "No profiles to edit." -ForegroundColor Yellow; continue }
            $idxRaw = Read-Host "Edit which profile number?"
            $idx = 0
            if ([int]::TryParse($idxRaw, [ref]$idx) -and $idx -ge 1 -and $idx -le $profiles.Count) {
                $profiles[$idx - 1] = Edit-RustcopyProfileInteractive -RustcopyProfile $profiles[$idx - 1]
                Save-RustcopyProfiles -Path $ProfilesFile -Profiles $profiles
                Write-Host "Profile updated." -ForegroundColor Green
            }
            else {
                Write-Host "No profile #$idxRaw." -ForegroundColor Yellow
            }
        }
        '^[Dd]$' {
            if ($profiles.Count -eq 0) { Write-Host "No profiles to delete." -ForegroundColor Yellow; continue }
            $idxRaw = Read-Host "Delete which profile number?"
            $idx = 0
            if ([int]::TryParse($idxRaw, [ref]$idx) -and $idx -ge 1 -and $idx -le $profiles.Count) {
                $target = $profiles[$idx - 1]
                if (Read-YesNo -Prompt "Delete profile '$($target.name)'?" -DefaultValue $false) {
                    $profiles = @($profiles | Where-Object { $_.name -ne $target.name })
                    Save-RustcopyProfiles -Path $ProfilesFile -Profiles $profiles
                    Write-Host "Profile '$($target.name)' deleted." -ForegroundColor Green
                }
            }
            else {
                Write-Host "No profile #$idxRaw." -ForegroundColor Yellow
            }
        }
        '^\d+$' {
            $idx = [int]$choice
            if ($idx -ge 1 -and $idx -le $profiles.Count) {
                Invoke-RustcopyProfile -RustcopyProfile $profiles[$idx - 1] -Exe $Exe -ReportsRoot $ReportsRoot `
                    -DryRun:$DryRun -ThreadsOverride $Threads -Interactive | Out-Null
            }
            else {
                Write-Host "No profile #$idx." -ForegroundColor Yellow
            }
        }
        default {
            Write-Host "Unrecognised choice." -ForegroundColor Yellow
        }
    }
}
