<#
.SYNOPSIS
    Compares robocopy_ingest.exe's --threads values on a COLD bulk write - a fresh, never-touched
    destination subfolder per value - unlike scripts\benchmark-threads.ps1, which deliberately
    measures the steady-state (already-aligned) case.

.DESCRIPTION
    scripts\benchmark-threads.ps1 warms up the destination once and then measures every --threads
    value against that same, now-aligned destination - correct for its purpose (a scheduled
    incremental job, where the destination is normally already in sync), but wrong for isolating
    a throughput collapse on a bulk cold copy: after the warm-up, every measured run mostly
    re-scans and skips already-matching files, so --threads stops mattering by construction. This
    script exists for the other case: "84 GB / 184,000 files, first write, throughput fell from
    85 MB/s to 6 MB/s" is a cold-write question, and needs a genuinely untouched destination for
    every --threads value or the second and later values in the sweep would just be measuring a
    rescan, not a real write.

    Deliberately extends the existing tooling rather than forking it: reuses --report-path JSON
    for timing/throughput (same fields benchmark-threads.ps1 and analyze-runs.ps1 already read),
    and scripts\generate-synthetic-tree.ps1 for the dataset - single-responsibility scripts, not
    one script doing generation + sweep + reporting.

    Each --threads value gets its own destination subfolder, named with the thread count and a
    run timestamp, so nothing is ever reused across the sweep. Add -Cleanup to remove each
    destination subfolder immediately after its report is written - default off, since seeing
    what actually landed can matter more than the disk space, but real NAS space is finite and a
    full 184,000-file / 84 GB sweep at several --threads values multiplies that by the number of
    values tested.

    Distinguishing the two hypotheses this tooling was built to separate (see ANALYSIS.md/
    ROADMAP.md F84's calibration follow-up for the full write-up):
      - Throughput flat across every --threads value -> per-file SMB/metadata overhead dominant,
        not thread contention (--threads isn't the lever to pull).
      - Throughput rises then falls (a real peak at an intermediate --threads) -> genuine I/O
        contention on the destination's physical media - there is a real optimum to find.
      - Throughput still rising at the highest --threads tested -> the ceiling wasn't reached;
        widen -Threads and test again before concluding anything.
    This script prints the raw numbers; it does not classify them the way the (currently
    unimplemented, proposed-only) storage_profile.rs module would - read the table yourself and
    decide whether the pattern justifies building that module for real (see CLAUDE.md's note
    against speculative code without a real caller, D8).

.PARAMETER Dest
    Real destination root to test against (e.g. a NAS share). Each --threads value writes to
    "$Dest\cold-bench-mt<N>-<timestamp>" - never an existing subfolder.

.PARAMETER SourceDir
    Reuse an already-generated synthetic tree instead of generating a new one (useful for
    repeating a sweep against a different -Dest with identical source data). Must already exist.

.PARAMETER FileCount / -AverageFileSizeKB
    Only used when -SourceDir is omitted (a fresh tree is generated). Defaults match this
    tooling's quick-trial size, not the real 184,000-file/84 GB case - pass the real figures
    explicitly once the small run's mechanics are confirmed working end to end; note that a full
    84 GB run at several --threads values needs that many multiples of 84 GB of free space at
    -Dest unless -Cleanup is passed.

.PARAMETER Threads
    --threads values to sweep, in the order tested (not sorted - test order can matter if the
    NAS itself has any state that persists across runs, e.g. its own cache warming).

.PARAMETER Cleanup
    Remove each destination subfolder immediately after its report is captured, instead of
    leaving every run's output on disk for manual inspection.

.EXAMPLE
    .\scripts\benchmark-threads-cold.ps1 -Dest "\\192.168.1.187\datas01\bench" -FileCount 5000 -AverageFileSizeKB 456
    .\scripts\benchmark-threads-cold.ps1 -Dest "\\192.168.1.187\datas01\bench" -SourceDir _ops_reports\benchmark\cold\synth-source -Threads 4,8,16,32,48 -Cleanup
#>

param(
    [Parameter(Mandatory = $true)]
    [string]$Dest,
    [string]$SourceDir,
    [int]$FileCount = 2000,
    [double]$AverageFileSizeKB = 456,
    [int[]]$Threads = @(4, 8, 16, 32, 48),
    [switch]$Cleanup
)

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent $PSScriptRoot
$Exe = Join-Path $RepoRoot "target\release\robocopy_ingest.exe"

if (-not (Test-Path $Exe)) {
    Write-Host "Binary not found at $Exe - build it first with: cargo build --release" -ForegroundColor Yellow
    exit 1
}
if ($Threads.Count -lt 2) {
    Write-Host "-Threads needs at least 2 distinct values to compare (got $($Threads.Count))." -ForegroundColor Red
    exit 1
}

$RunTimestamp = Get-Date -Format "yyyyMMdd_HHmmss"
$ReportsDir = Join-Path $RepoRoot "_ops_reports\benchmark\cold"
New-Item -ItemType Directory -Force -Path $ReportsDir | Out-Null

# Source dataset: generated once, reused for every --threads value in the sweep (only the
# destination must be fresh per value - the source is read-only from robocopy's point of view).
$ownsSource = $false
if ($SourceDir) {
    if (-not (Test-Path $SourceDir)) {
        Write-Host "-SourceDir $SourceDir does not exist." -ForegroundColor Red
        exit 1
    }
    Write-Host "Reusing existing source dataset at $SourceDir" -ForegroundColor Cyan
}
else {
    $SourceDir = Join-Path $ReportsDir "synth-source-$RunTimestamp"
    Write-Host "Generating synthetic source dataset at $SourceDir ..." -ForegroundColor Cyan
    & (Join-Path $PSScriptRoot "generate-synthetic-tree.ps1") -TargetDir $SourceDir -FileCount $FileCount -AverageFileSizeKB $AverageFileSizeKB -Seed 42
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Dataset generation failed - aborting." -ForegroundColor Red
        exit 1
    }
    $ownsSource = $true
}

function Invoke-ColdRun {
    param([int]$ThreadCount)

    $destSubfolder = Join-Path $Dest "cold-bench-mt${ThreadCount}-$RunTimestamp"
    if (Test-Path $destSubfolder) {
        # Extremely unlikely (timestamp + thread count collision) but never silently reuse -
        # same discipline as example_workspace.rs's overwrite refusal.
        Write-Host "$destSubfolder already exists - refusing to reuse it for a cold run." -ForegroundColor Red
        return $null
    }

    $reportPath = Join-Path $ReportsDir "cold_mt${ThreadCount}_$RunTimestamp.json"
    Write-Host "Running --threads $ThreadCount against a fresh destination ($destSubfolder) ..."

    & $Exe --source $SourceDir --dest $destSubfolder --threads $ThreadCount --report-path $reportPath | Out-Null
    $exitCode = $LASTEXITCODE

    if (-not (Test-Path $reportPath)) {
        Write-Host "[/MT:$ThreadCount] No report produced (exit $exitCode) - skipping." -ForegroundColor Red
        return $null
    }

    $report = Get-Content $reportPath -Raw | ConvertFrom-Json
    $robocopySeconds = $report.robocopy_transfer.elapsed_seconds
    $filesPerSecond = if ($robocopySeconds -gt 0) { $report.robocopy_transfer.files_copied / $robocopySeconds } else { 0 }

    $result = [PSCustomObject]@{
        Threads         = $ThreadCount
        ExitCode        = $exitCode
        TotalSeconds    = [Math]::Round($report.phase_timing.total_seconds, 1)
        RobocopySeconds = [Math]::Round($robocopySeconds, 1)
        FilesCopied     = $report.robocopy_transfer.files_copied
        ThroughputMBps  = [Math]::Round($report.robocopy_transfer.throughput_mbps, 2)
        FilesPerSecond  = [Math]::Round($filesPerSecond, 2)
        ReportPath      = $reportPath
        DestSubfolder   = $destSubfolder
    }

    if ($Cleanup) {
        Write-Host "  Removing $destSubfolder (-Cleanup) ..."
        Remove-Item -Recurse -Force $destSubfolder -ErrorAction SilentlyContinue
    }

    $result
}

Write-Host ""
Write-Host "=== Cold sweep: $($Threads.Count) --threads values, each against a fresh destination ===" -ForegroundColor Cyan
$results = foreach ($t in $Threads) { Invoke-ColdRun -ThreadCount $t }
$results = $results | Where-Object { $_ -ne $null }

if ($ownsSource) {
    Write-Host ""
    Write-Host "Synthetic source dataset kept at $SourceDir for re-use in a later sweep (not deleted)." -ForegroundColor DarkGray
}

if ($results.Count -eq 0) {
    Write-Host "No successful runs to compare." -ForegroundColor Red
    exit 1
}

Write-Host ""
$results | Select-Object Threads, TotalSeconds, RobocopySeconds, FilesCopied, ThroughputMBps, FilesPerSecond | Format-Table -AutoSize

$maxThroughput = ($results | Measure-Object -Property ThroughputMBps -Maximum).Maximum
$minThroughput = ($results | Measure-Object -Property ThroughputMBps -Minimum).Minimum
$variationPercent = if ($maxThroughput -gt 0) { ($maxThroughput - $minThroughput) / $maxThroughput * 100 } else { 0 }
$peak = $results | Sort-Object -Property ThroughputMBps -Descending | Select-Object -First 1
$last = $results | Select-Object -Last 1

Write-Host ""
Write-Host ("Throughput range: {0:N2}-{1:N2} MB/s ({2:N0}% relative variation)" -f $minThroughput, $maxThroughput, $variationPercent) -ForegroundColor Cyan
if ($variationPercent -lt 15) {
    Write-Host "Reading: roughly flat across every --threads value tested - consistent with per-file protocol overhead dominating, not thread contention. See scripts/backup-nas-qnap.ps1's own measured note for a real precedent of this pattern." -ForegroundColor Yellow
}
elseif ($peak.Threads -eq $last.Threads) {
    Write-Host "Reading: still rising at the highest --threads tested ($($last.Threads)) - the ceiling wasn't found yet. Widen -Threads and run again before concluding anything." -ForegroundColor Yellow
}
else {
    Write-Host "Reading: throughput peaks at --threads $($peak.Threads) ($($peak.ThroughputMBps) MB/s) and falls off on either side - consistent with real I/O contention at higher thread counts." -ForegroundColor Yellow
}
Write-Host ""
Write-Host "Full reports saved under: $ReportsDir (readable by scripts\analyze-runs.ps1 -ReportsDir $ReportsDir -Recurse)"
