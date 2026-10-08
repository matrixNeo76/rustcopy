<#
.SYNOPSIS
    Generates a synthetic file tree for benchmarking rustcopy against a real destination, with a
    non-uniform file size distribution around a target average (not every file the same size).

.DESCRIPTION
    Single-responsibility generator, deliberately its own script rather than inlined into
    scripts\benchmark-threads-cold.ps1: this file answers "build a tree with these
    characteristics", the benchmark script answers "sweep --threads against a fresh destination
    per value" - two different concerns that shouldn't share one file (same reasoning
    analyze-runs.ps1/benchmark-threads.ps1 already follow as two scripts, not one).

    File sizes are drawn from a log-normal distribution (Box-Muller transform), not sampled
    uniformly: a real-world tree (see the QNAP/fileserver captures already under _ops_reports\)
    is a mix of many small files and a few large ones, not files all clustered at the same size.
    Individual sizes are clamped to [-MinFileSizeKB, -MaxFileSizeKB] so the tail doesn't produce a
    pathological single file that dwarfs the whole tree or a zero-byte file. The realized total
    size will therefore differ slightly from FileCount * AverageFileSizeKB - the script reports
    the actual total generated, don't assume it matches the target exactly.

    Content is pseudo-random bytes drawn from one shared in-memory buffer (regenerated per
    -Seed), sliced per file - fast to generate at 184k-file scale, but NOT independently random
    per file the way real user data would be. Declared limitation: if the real destination or an
    intermediate hop does content-aware deduplication/compression, this dataset will behave more
    favourably than real data would. For measuring per-file protocol overhead (SMB handle
    open/close, metadata round-trips) rather than raw byte throughput, this doesn't matter - it's
    the same reasoning /BYTES-based throughput math in this project already relies on: the
    interesting number here is elapsed time and file count, not entropy.

.PARAMETER TargetDir
    Where to create the tree. Must not already exist (same overwrite-refusal discipline as
    example_workspace.rs's create_example_workspace - never silently merge into an existing
    directory a caller might not expect touched).

.PARAMETER FileCount
    How many files to generate. Default is a quick-trial size, not the real-world 184,000-file
    case this tooling was built to investigate - pass a larger value once the small run's
    mechanics are confirmed working end to end.

.PARAMETER AverageFileSizeKB
    Target mean file size. Default 456 KB, matching the real case that motivated this tooling
    (184,000 files / 84 GB observed in production).

.PARAMETER FilesPerDirectory
    Files per leaf subdirectory, to avoid one directory with hundreds of thousands of entries
    (unrealistic, and slow to enumerate on both NTFS and over SMB).

.PARAMETER Seed
    Random seed, for a reproducible tree across repeated runs (useful when comparing --threads
    values against otherwise-identical data). Omit for a different tree each time.

.EXAMPLE
    .\scripts\generate-synthetic-tree.ps1 -TargetDir _ops_reports\benchmark\cold\synth-source
    .\scripts\generate-synthetic-tree.ps1 -TargetDir D:\bench-src -FileCount 184000 -AverageFileSizeKB 456 -Seed 42
#>

param(
    [Parameter(Mandatory = $true)]
    [string]$TargetDir,
    [int]$FileCount = 2000,
    [double]$AverageFileSizeKB = 456,
    [int]$MinFileSizeKB = 1,
    [int]$MaxFileSizeKB = 0,   # 0 = derived below (20x the average, a generous long tail)
    [int]$FilesPerDirectory = 500,
    [int]$Seed = 0             # 0 = unseeded (a different tree each run)
)

$ErrorActionPreference = "Stop"

if (Test-Path $TargetDir) {
    Write-Host "$TargetDir already exists - refusing to write into it (pass a fresh path)." -ForegroundColor Red
    exit 1
}
if ($FileCount -le 0) {
    Write-Host "-FileCount must be positive." -ForegroundColor Red
    exit 1
}
if ($MaxFileSizeKB -eq 0) {
    $MaxFileSizeKB = [Math]::Max([int]($AverageFileSizeKB * 20), $MinFileSizeKB + 1)
}

$rng = if ($Seed -ne 0) { [System.Random]::new($Seed) } else { [System.Random]::new() }

# Log-normal parameters chosen so the arithmetic mean of the (unclamped) distribution lands on
# -AverageFileSizeKB: for a log-normal, mean = exp(mu + sigma^2/2), so mu = ln(mean) - sigma^2/2.
# Sigma of 0.9 gives a real-world-shaped spread (a handful of files several times the mean, many
# well below it) without needing a measured value from a real dataset - this is an estimate, not
# a fit, same honesty as the thresholds documented in the storage_profile.rs proposal this
# tooling exists to feed data into.
$sigma = 0.9
$meanBytes = $AverageFileSizeKB * 1KB
$mu = [Math]::Log($meanBytes) - ($sigma * $sigma / 2)

function Get-LogNormalSizeBytes {
    param($Rng, $Mu, $Sigma, $MinBytes, $MaxBytes)
    # Box-Muller transform: two independent uniform(0,1) draws -> one standard normal draw.
    $u1 = 1.0 - $Rng.NextDouble()  # (0,1], avoids log(0)
    $u2 = $Rng.NextDouble()
    $standardNormal = [Math]::Sqrt(-2.0 * [Math]::Log($u1)) * [Math]::Cos(2.0 * [Math]::PI * $u2)
    $sizeBytes = [Math]::Exp($Mu + $Sigma * $standardNormal)
    [Math]::Min([Math]::Max([int64]$sizeBytes, $MinBytes), $MaxBytes)
}

# One shared buffer of pseudo-random bytes, sliced per file (see the .DESCRIPTION note on why
# this isn't independently-random content per file). Sized to the clamp ceiling so every file's
# slice fits without regenerating the buffer.
$bufferBytes = New-Object byte[] ($MaxFileSizeKB * 1KB)
$rng.NextBytes($bufferBytes)

New-Item -ItemType Directory -Force -Path $TargetDir | Out-Null

$totalBytesWritten = [int64]0
$currentDir = $null
$filesInCurrentDir = 0
$dirIndex = 0
$stopwatch = [System.Diagnostics.Stopwatch]::StartNew()

for ($i = 0; $i -lt $FileCount; $i++) {
    if ($null -eq $currentDir -or $filesInCurrentDir -ge $FilesPerDirectory) {
        $dirIndex++
        $currentDir = Join-Path $TargetDir ("dir-{0:D5}" -f $dirIndex)
        New-Item -ItemType Directory -Force -Path $currentDir | Out-Null
        $filesInCurrentDir = 0
    }

    $sizeBytes = Get-LogNormalSizeBytes -Rng $rng -Mu $mu -Sigma $sigma -MinBytes ($MinFileSizeKB * 1KB) -MaxBytes ($MaxFileSizeKB * 1KB)
    $filePath = Join-Path $currentDir ("file-{0:D6}.bin" -f $i)

    # A random offset into the shared buffer so adjacent files don't share an identical prefix,
    # without paying for a fresh random fill per file.
    $maxOffset = $bufferBytes.Length - $sizeBytes
    $offset = if ($maxOffset -gt 0) { $rng.Next(0, [int]$maxOffset) } else { 0 }

    $stream = [System.IO.File]::Create($filePath)
    try {
        $stream.Write($bufferBytes, $offset, [int]$sizeBytes)
    }
    finally {
        $stream.Dispose()
    }

    $totalBytesWritten += $sizeBytes
    $filesInCurrentDir++

    if ($i % 5000 -eq 0 -and $i -gt 0) {
        Write-Host ("  {0:N0} / {1:N0} files ({2:N1} GB so far, {3:N0}s elapsed)" -f $i, $FileCount, ($totalBytesWritten / 1GB), $stopwatch.Elapsed.TotalSeconds)
    }
}

$stopwatch.Stop()
$actualAverageKB = ($totalBytesWritten / 1KB) / $FileCount

Write-Host ""
Write-Host "=== Generated $FileCount files under $TargetDir ===" -ForegroundColor Green
Write-Host ("Total size          : {0:N2} GB" -f ($totalBytesWritten / 1GB))
Write-Host ("Actual average size : {0:N1} KB (target was {1:N1} KB)" -f $actualAverageKB, $AverageFileSizeKB)
Write-Host ("Directories         : $dirIndex ({0} files each, last one may be partial)" -f $FilesPerDirectory)
Write-Host ("Generation time     : {0:N0}s" -f $stopwatch.Elapsed.TotalSeconds)

# Explicit exit code: this script never invokes a native .exe, so $LASTEXITCODE would otherwise
# stay unset/stale for a caller checking it (e.g. benchmark-threads-cold.ps1) rather than
# reflecting this script's own success.
exit 0
