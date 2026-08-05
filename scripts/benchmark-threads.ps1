<#
.SYNOPSIS
    Compares robocopy_ingest.exe's run time across different --threads (/MT) values, to find the
    best value for a specific source/destination (e.g. an SMB share on a QNAP NAS).

.DESCRIPTION
    Deliberately reuses the existing --report-path JSON reporting instead of introducing a new
    log format: for each --threads value it runs a real transfer and reads timing/throughput back
    from that run's report (phase_timing / robocopy_transfer), then prints a comparison table.

    The first --threads value is always run twice: once as an unmeasured "warm-up", then once
    measured along with the rest. A cold run against a not-yet-aligned destination spends most of
    its time actually copying every file, which would swamp the comparison — what actually matters
    for a scheduled incremental job (source barely changed since last run) is the STEADY-STATE
    case, where robocopy mostly just re-scans and skips already-matching files. The warm-up run
    brings the destination to that steady state before the timed comparison starts.

    Reports from every run are kept under _ops_reports\benchmark\ (gitignored, like the existing
    _ops_reports folder used by run-ingest-claude-code.ps1) so a run can be inspected in full
    afterwards, not just via the summary table.

.EXAMPLE
    .\scripts\benchmark-threads.ps1 -Source "\\NAS\share" -Dest "D:\backup" -Threads 8,16,32,48
    .\scripts\benchmark-threads.ps1 -ConfigPath .\rustcopy.toml -Threads 8,16,32
    .\scripts\benchmark-threads.ps1 -Source "\\NAS\share" -Dest "D:\backup" -Mirror
#>

param(
    [string]$Source,
    [string]$Dest,
    [string]$ConfigPath,
    [int[]]$Threads = @(8, 16, 32, 48),
    [switch]$Mirror
)

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent $PSScriptRoot
$Exe = Join-Path $RepoRoot "target\release\robocopy_ingest.exe"

if (-not (Test-Path $Exe)) {
    Write-Host "Binary not found at $Exe - build it first with: cargo build --release" -ForegroundColor Yellow
    exit 1
}

if (-not $ConfigPath -and (-not $Source -or -not $Dest)) {
    Write-Host "Specify either -ConfigPath, or both -Source and -Dest." -ForegroundColor Red
    exit 1
}

if ($Mirror -and $ConfigPath) {
    Write-Host "-Mirror only applies to -Source/-Dest runs; put mirror = true in the config file instead." -ForegroundColor Red
    exit 1
}

$ReportsDir = Join-Path $RepoRoot "_ops_reports\benchmark"
New-Item -ItemType Directory -Force -Path $ReportsDir | Out-Null
$RunTimestamp = Get-Date -Format "yyyyMMdd_HHmmss"

function Invoke-TimedRun {
    param([int]$ThreadCount, [string]$Label)

    $reportPath = Join-Path $ReportsDir "bench_${Label}_mt${ThreadCount}_$RunTimestamp.json"
    $argList = @("--threads", "$ThreadCount", "--report-path", $reportPath)
    if ($ConfigPath) {
        $argList += @("--config", $ConfigPath)
    }
    else {
        $argList += @("--source", $Source, "--dest", $Dest)
        if ($Mirror) { $argList += @("--mirror", "--force-purge") }
    }

    & $Exe @argList | Out-Null
    $exitCode = $LASTEXITCODE

    if (-not (Test-Path $reportPath)) {
        Write-Host "[/MT:$ThreadCount] No report produced (exit $exitCode) - skipping." -ForegroundColor Red
        return $null
    }

    $report = Get-Content $reportPath -Raw | ConvertFrom-Json
    $robocopySeconds = $report.robocopy_transfer.elapsed_seconds
    $filesPerSecond = if ($robocopySeconds -gt 0) { $report.robocopy_transfer.files_copied / $robocopySeconds } else { 0 }

    [PSCustomObject]@{
        Threads         = $ThreadCount
        ExitCode        = $exitCode
        TotalSeconds    = [Math]::Round($report.phase_timing.total_seconds, 1)
        RobocopySeconds = [Math]::Round($robocopySeconds, 1)
        FilesCopied     = $report.robocopy_transfer.files_copied
        BytesCopied     = $report.robocopy_transfer.bytes_copied
        ThroughputMBps  = [Math]::Round($report.robocopy_transfer.throughput_mbps, 2)
        FilesPerSecond  = [Math]::Round($filesPerSecond, 2)
        ReportPath      = $reportPath
    }
}

Write-Host "=== Warm-up run (/MT:$($Threads[0]), not measured) ===" -ForegroundColor Yellow
Invoke-TimedRun -ThreadCount $Threads[0] -Label "warmup" | Out-Null

Write-Host ""
Write-Host "=== Measured runs (steady-state) ===" -ForegroundColor Cyan
$results = foreach ($t in $Threads) {
    Write-Host "Running /MT:$t ..."
    Invoke-TimedRun -ThreadCount $t -Label "measured"
}
$results = $results | Where-Object { $_ -ne $null }

if ($results.Count -eq 0) {
    Write-Host "No successful runs to compare." -ForegroundColor Red
    exit 1
}

Write-Host ""
$sorted = $results | Sort-Object -Property FilesPerSecond -Descending
$sorted | Select-Object Threads, TotalSeconds, RobocopySeconds, FilesCopied, ThroughputMBps, FilesPerSecond | Format-Table -AutoSize

$best = $sorted[0]
Write-Host ""
Write-Host "Optimal so far: /MT:$($best.Threads) ($($best.FilesPerSecond) files/s, $($best.ThroughputMBps) MB/s)" -ForegroundColor Green
Write-Host "Full reports saved under: $ReportsDir"
Write-Host ""
Write-Host "Note: on a mostly-idle destination (few changed files, the steady-state case this" -ForegroundColor DarkGray
Write-Host "script targets), robocopy's own SMB metadata scan dominates the run time far more" -ForegroundColor DarkGray
Write-Host "than --threads does - don't expect --threads alone to change RobocopySeconds much" -ForegroundColor DarkGray
Write-Host "once the destination is already aligned." -ForegroundColor DarkGray
