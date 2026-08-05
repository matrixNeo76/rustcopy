<#
.SYNOPSIS
    Reads every rustcopy --report-path JSON found under a directory and prints a trend table:
    files copied, throughput, total vs robocopy-only time, wrapper overhead.

.DESCRIPTION
    Deliberately reuses the existing report schema (report.rs: PhaseTiming, TransferReport)
    instead of a new log format - every field this script prints is already written to
    --report-path on every run, so this is pure parsing/aggregation, no new data capture needed.

    rustcopy has no built-in per-run report history: a fixed --report-path (as used by a
    scheduled job) gets overwritten on every run. To build up history for this script to analyze,
    archive a timestamped copy of the report after each scheduled run (e.g. a one-line
    Copy-Item in whatever wraps the scheduled invocation) before the next run overwrites it.
    scripts/benchmark-threads.ps1 already does this correctly (unique --report-path per run,
    under _ops_reports\benchmark\) and is a ready-made source of data for this script.

.EXAMPLE
    .\scripts\analyze-runs.ps1
    .\scripts\analyze-runs.ps1 -ReportsDir _ops_reports\benchmark -Recurse
#>

param(
    [string]$ReportsDir = (Join-Path (Split-Path -Parent $PSScriptRoot) "_ops_reports"),
    [switch]$Recurse
)

if (-not (Test-Path $ReportsDir)) {
    Write-Host "Directory not found: $ReportsDir" -ForegroundColor Red
    exit 1
}

$files = Get-ChildItem -Path $ReportsDir -Filter "*.json" -Recurse:$Recurse
if ($files.Count -eq 0) {
    Write-Host "No .json reports found under $ReportsDir" -ForegroundColor Yellow
    exit 0
}

$rows = @()
foreach ($file in $files) {
    try {
        $r = Get-Content $file.FullName -Raw | ConvertFrom-Json
    }
    catch {
        Write-Warning "Skipping $($file.Name): not valid JSON"
        continue
    }
    # Skip anything that isn't actually a rustcopy report (e.g. a stray file in the directory).
    if (-not $r.robocopy_transfer -or -not $r.phase_timing) {
        Write-Warning "Skipping $($file.Name): missing robocopy_transfer/phase_timing, not a rustcopy report"
        continue
    }

    $filesCopied = $r.robocopy_transfer.files_copied
    $filesSkipped = [Math]::Max(0, $r.total_files - $filesCopied)
    $robocopySeconds = $r.robocopy_transfer.elapsed_seconds
    $totalSeconds = $r.phase_timing.total_seconds

    $rows += [PSCustomObject]@{
        File                   = $file.Name
        Timestamp              = $r.timestamp
        TotalFiles             = $r.total_files
        FilesCopied            = $filesCopied
        FilesSkipped           = $filesSkipped
        BytesCopiedMB          = [Math]::Round($r.robocopy_transfer.bytes_copied / 1MB, 1)
        TotalSeconds           = [Math]::Round($totalSeconds, 1)
        RobocopySeconds        = [Math]::Round($robocopySeconds, 1)
        WrapperOverheadSeconds = [Math]::Round($totalSeconds - $robocopySeconds, 2)
        ExitCode               = $r.robocopy_transfer.exit_code
    }
}

if ($rows.Count -eq 0) {
    Write-Host "No valid rustcopy reports found under $ReportsDir" -ForegroundColor Yellow
    exit 0
}

$sorted = $rows | Sort-Object Timestamp
$sorted | Format-Table -AutoSize

Write-Host ""
Write-Host "=== Summary across $($sorted.Count) run(s) ===" -ForegroundColor Cyan
$avgFilesCopied = ($sorted | Measure-Object -Property FilesCopied -Average).Average
$avgTotalSeconds = ($sorted | Measure-Object -Property TotalSeconds -Average).Average
$avgRobocopySeconds = ($sorted | Measure-Object -Property RobocopySeconds -Average).Average
$failedRuns = ($sorted | Where-Object { $_.ExitCode -ne 0 }).Count

Write-Host ("Average files copied per run : {0:N0}" -f $avgFilesCopied)
Write-Host ("Average total time           : {0:N1}s" -f $avgTotalSeconds)
Write-Host ("Average robocopy time        : {0:N1}s ({1:P0} of total)" -f $avgRobocopySeconds, ($avgRobocopySeconds / [Math]::Max($avgTotalSeconds, 0.001)))
Write-Host ("Runs with non-zero exit code : $failedRuns / $($sorted.Count)") -ForegroundColor $(if ($failedRuns -gt 0) { "Red" } else { "Green" })
