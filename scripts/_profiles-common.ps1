<#
.SYNOPSIS
    Shared profile-storage helpers dot-sourced by scripts/rustcopy-launcher.ps1 and by the
    launcher's own thin wrappers (backup-fileserv01.ps1, backup-nas-qnap.ps1, run-all-profiles.ps1).

.DESCRIPTION
    Not meant to be run directly - it only defines functions. Dot-source it:
        . (Join-Path $PSScriptRoot "_profiles-common.ps1")

    Split out of rustcopy-launcher.ps1 (Pilastro D's "Refactor script -> wrapper" follow-up,
    PIANO_MIGLIORAMENTI.md) specifically so backup-fileserv01.ps1/backup-nas-qnap.ps1 can read and
    self-seed their own profile in scripts\profiles.json without dot-sourcing the whole launcher
    script (which would also pull in its interactive menu / batch-mode top-level logic).
#>

function Import-RustcopyProfiles {
    param([Parameter(Mandatory)][string]$Path)

    if (-not (Test-Path $Path)) { return @() }
    $raw = Get-Content -LiteralPath $Path -Raw -ErrorAction Stop
    if ([string]::IsNullOrWhiteSpace($raw)) { return @() }

    $parsed = $raw | ConvertFrom-Json
    if ($null -eq $parsed) { return @() }
    # ConvertFrom-Json returns a single PSCustomObject (not a 1-element array) when the JSON's
    # top-level array has exactly one element -- wrap so callers can always treat the result as
    # a collection regardless of how many profiles are stored.
    if ($parsed -is [System.Array]) { return $parsed }
    return @($parsed)
}

function Save-RustcopyProfiles {
    param(
        [Parameter(Mandatory)][string]$Path,
        [AllowEmptyCollection()][array]$Profiles
    )

    if ($null -eq $Profiles) { $Profiles = @() }
    # Passed via -InputObject (never piped): piping an array into ConvertTo-Json enumerates it
    # element by element, which loses the array wrapper entirely for a 0- or 1-element array
    # (silently producing "" or a bare {...} instead of "[]" / "[{...}]") -- -InputObject binds
    # the whole array as one parameter value instead, which serializes correctly at every count.
    $json = ConvertTo-Json -InputObject $Profiles -Depth 6
    Set-Content -LiteralPath $Path -Value $json -Encoding utf8
}

function Get-UncShareRoot {
    param([Parameter(Mandatory)][string]$UncPath)
    $match = [regex]::Match($UncPath, '^\\\\[^\\]+\\[^\\]+')
    if (-not $match.Success) { return $null }
    return $match.Value
}

<#
.SYNOPSIS
    Returns the named profile from $Path, creating it from $Defaults on first run if missing.

.DESCRIPTION
    Non-interactive counterpart to rustcopy-launcher.ps1's New-RustcopyProfileInteractive wizard,
    used by the thin wrapper scripts to self-seed the one profile they need instead of requiring
    a human to run the launcher's interactive menu once before the wrapper works. Idempotent: a
    profile that already exists (created by this function on an earlier run, or by hand via the
    launcher) is returned as-is, never overwritten -- so any edits made later via the launcher's
    own [E]dit menu are not clobbered by the wrapper's hardcoded defaults on a subsequent run.
#>
function Confirm-RustcopyProfile {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][PSCustomObject]$Defaults
    )

    $profiles = @(Import-RustcopyProfiles -Path $Path)
    $existing = $profiles | Where-Object { $_.name -eq $Defaults.name } | Select-Object -First 1
    if ($existing) { return $existing }

    $profiles = @($profiles) + $Defaults
    Save-RustcopyProfiles -Path $Path -Profiles $profiles
    return $Defaults
}
