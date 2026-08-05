<#
.SYNOPSIS
    Manual, elevated verification of both Windows services this crate can register
    (RustcopyIngestService via robocopy_ingest.exe --install-service, F37; RustcopyNotifyServer
    via notify-server.exe --install-service, F41).

.DESCRIPTION
    CreateService/StartService/StopService/DeleteService all require Administrator elevation, so
    this round trip cannot be part of the normal `cargo test` suite (see service.rs's doc comment
    and ROADMAP.md's F37/F41 rows for the declared limitation). This script exists so verifying a
    change to service.rs/main.rs/bin/notify_server.rs is a single repeatable command instead of
    re-typing install/query/start/stop/uninstall by hand every time.

    For each service: install, confirm it's registered (sc query), start it, confirm it's
    running, stop it, confirm it's stopped, uninstall, confirm it's gone. Always attempts cleanup
    (stop + delete) in a finally block, even if an earlier step failed, so a failed run doesn't
    leave a half-installed service behind.

    Must be run from an elevated (Administrator) PowerShell prompt. Requires
    `cargo build --release --features notify-server` to have been run first.

.EXAMPLE
    .\scripts\verify-services.ps1
    .\scripts\verify-services.ps1 -SkipNotifyServer
#>

param(
    [switch]$SkipIngestService,
    [switch]$SkipNotifyServer
)

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent $PSScriptRoot

$currentPrincipal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $currentPrincipal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Host "This script must be run from an elevated (Administrator) PowerShell prompt." -ForegroundColor Red
    Write-Host "Right-click PowerShell -> Run as administrator, then re-run this script." -ForegroundColor Yellow
    exit 1
}

$overallOk = $true

function Test-ServiceRoundTrip {
    param(
        [string]$Label,
        [string]$ExePath,
        [string]$ServiceName
    )

    Write-Host ""
    Write-Host "=== $Label ($ServiceName) ===" -ForegroundColor Cyan

    if (-not (Test-Path $ExePath)) {
        Write-Host "[$Label] Binary not found at $ExePath - build it first." -ForegroundColor Yellow
        return $false
    }

    $ok = $true
    try {
        Write-Host "[$Label] Installing..."
        & $ExePath --install-service
        if ($LASTEXITCODE -ne 0) { throw "install-service exited $LASTEXITCODE" }
        Write-Host "[$Label] Installed." -ForegroundColor Green

        Write-Host "[$Label] Querying (expect registered, stopped)..."
        $queryAfterInstall = sc.exe query $ServiceName
        if ($LASTEXITCODE -ne 0) { throw "sc query could not find the service right after install" }
        Write-Host ($queryAfterInstall -join "`n")

        Write-Host "[$Label] Starting..."
        sc.exe start $ServiceName | Out-Null
        Start-Sleep -Seconds 2
        $queryRunning = sc.exe query $ServiceName
        if ($queryRunning -notmatch "RUNNING") {
            Write-Host ($queryRunning -join "`n")
            throw "service did not reach RUNNING state"
        }
        Write-Host "[$Label] Running." -ForegroundColor Green

        Write-Host "[$Label] Stopping..."
        sc.exe stop $ServiceName | Out-Null
        Start-Sleep -Seconds 2
        $queryStopped = sc.exe query $ServiceName
        if ($queryStopped -notmatch "STOPPED") {
            Write-Host ($queryStopped -join "`n")
            throw "service did not reach STOPPED state"
        }
        Write-Host "[$Label] Stopped." -ForegroundColor Green
    }
    catch {
        Write-Host "[$Label] FAILED: $_" -ForegroundColor Red
        $ok = $false
    }
    finally {
        # Best-effort cleanup regardless of what failed above, mirroring the Drop-based guards
        # in the Rust tests this script complements.
        sc.exe stop $ServiceName 2>&1 | Out-Null
        Start-Sleep -Milliseconds 500
        & $ExePath --uninstall-service 2>&1 | Out-Null
        sc.exe delete $ServiceName 2>&1 | Out-Null
    }

    Write-Host "[$Label] Confirming removal..."
    sc.exe query $ServiceName 2>&1 | Out-Null
    if ($LASTEXITCODE -eq 0) {
        Write-Host "[$Label] Service still present after cleanup - remove manually with: sc.exe delete $ServiceName" -ForegroundColor Red
        $ok = $false
    }
    else {
        Write-Host "[$Label] Confirmed removed." -ForegroundColor Green
    }

    return $ok
}

if (-not $SkipIngestService) {
    $ingestExe = Join-Path $RepoRoot "target\release\robocopy_ingest.exe"
    $overallOk = (Test-ServiceRoundTrip -Label "robocopy_ingest" -ExePath $ingestExe -ServiceName "RustcopyIngestService") -and $overallOk
}

if (-not $SkipNotifyServer) {
    $notifyExe = Join-Path $RepoRoot "target\release\notify-server.exe"
    $overallOk = (Test-ServiceRoundTrip -Label "notify-server" -ExePath $notifyExe -ServiceName "RustcopyNotifyServer") -and $overallOk
}

Write-Host ""
if ($overallOk) {
    Write-Host "=== All service round trips passed ===" -ForegroundColor Green
    exit 0
}
else {
    Write-Host "=== One or more service round trips FAILED - see above ===" -ForegroundColor Red
    exit 1
}
