<#
.SYNOPSIS
  Repeatable footprint measurement for a desktop GUI executable (memory, processes, start-up time).

.DESCRIPTION
  Launches the executable N times, each in a fresh process, and reports for every run:
    - time to the main window handle (T_window_s);
    - time to the first readable content (T_content_s): the main window exposes at least
      -MinUiaElements UI Automation descendants -- the window being on screen is not enough, a web
      view paints an empty frame first;
    - number of processes in the whole process tree, private memory and working set of that tree
      (sampled -SettleSeconds after the content appeared);
    - size of the executable.
  The median of the runs is printed last. Written for the Slint evaluation (PIANO_GUI_SLINT.md M0),
  so the same method is used for the current Tauri console, the Slint prototype and any reference
  program. It never touches the program's settings and always stops the processes it started.

.PARAMETER ExePath
  Executable to launch (full path).
.PARAMETER Runs
  How many fresh launches to measure (default 3).
.PARAMETER SettleSeconds
  Seconds to wait after the content appeared before sampling memory (default 5).
.PARAMETER MinUiaElements
  UI Automation descendants required to call the content "readable" (default 8).
.PARAMETER EnvVars
  Optional environment variables for the launched process, e.g.
  @{ WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--renderer-process-limit=1 --disable-gpu' }.
.PARAMETER Label
  Free text shown in the output.

.EXAMPLE
  .\scripts\measure-ui-footprint.ps1 -ExePath 'C:\Program Files\rustcopy\rustcopy-gui.exe' -Label tauri-default
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$ExePath,
    [int]$Runs = 3,
    [int]$SettleSeconds = 5,
    [int]$MinUiaElements = 8,
    [hashtable]$EnvVars = @{},
    [string]$Label = ''
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

if (-not (Test-Path -LiteralPath $ExePath)) { throw "Executable not found: $ExePath" }
$exeName = [IO.Path]::GetFileNameWithoutExtension($ExePath)

function Get-ProcessTree([int]$RootId) {
    $all = Get-CimInstance Win32_Process
    $ids = New-Object 'System.Collections.Generic.HashSet[int]'
    [void]$ids.Add($RootId)
    do {
        $before = $ids.Count
        foreach ($p in $all) {
            if ($ids.Contains([int]$p.ParentProcessId)) { [void]$ids.Add([int]$p.ProcessId) }
        }
    } while ($ids.Count -ne $before)
    Get-Process -Id ([int[]]$ids) -ErrorAction SilentlyContinue
}

function Get-UiaCount([int]$ProcessId) {
    try {
        $cond = New-Object Windows.Automation.PropertyCondition([Windows.Automation.AutomationElement]::ProcessIdProperty, $ProcessId)
        $top = [Windows.Automation.AutomationElement]::RootElement.FindFirst([Windows.Automation.TreeScope]::Children, $cond)
        if ($null -eq $top) { return 0 }
        return $top.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition).Count
    } catch { return 0 }
}

function Stop-Started([int]$RootId) {
    foreach ($proc in @(Get-ProcessTree $RootId)) { Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue }
}

$saved = @{}
foreach ($k in $EnvVars.Keys) {
    $saved[$k] = [Environment]::GetEnvironmentVariable($k, 'Process')
    [Environment]::SetEnvironmentVariable($k, [string]$EnvVars[$k], 'Process')
}

$results = @()
try {
    for ($i = 1; $i -le $Runs; $i++) {
        $sw = [Diagnostics.Stopwatch]::StartNew()
        $proc = Start-Process -FilePath $ExePath -PassThru
        $tWindow = $null; $tContent = $null
        while ($sw.Elapsed.TotalSeconds -lt 30) {
            Start-Sleep -Milliseconds 50
            $proc.Refresh()
            if ($null -eq $tWindow -and $proc.MainWindowHandle -ne 0) { $tWindow = $sw.Elapsed.TotalSeconds }
            if ($null -ne $tWindow -and (Get-UiaCount $proc.Id) -ge $MinUiaElements) { $tContent = $sw.Elapsed.TotalSeconds; break }
        }
        Start-Sleep -Seconds $SettleSeconds
        $tree = @(Get-ProcessTree $proc.Id)
        $results += [pscustomobject]@{
            Run          = $i
            T_window_s   = if ($tWindow) { [math]::Round($tWindow, 2) } else { $null }
            T_content_s  = if ($tContent) { [math]::Round($tContent, 2) } else { $null }
            Processes    = $tree.Count
            PrivateMB    = [math]::Round((($tree | Measure-Object PrivateMemorySize64 -Sum).Sum) / 1MB)
            WorkingSetMB = [math]::Round((($tree | Measure-Object WorkingSet64 -Sum).Sum) / 1MB)
        }
        Stop-Started $proc.Id
        Start-Sleep -Seconds 2
    }
}
finally {
    foreach ($k in $saved.Keys) { [Environment]::SetEnvironmentVariable($k, $saved[$k], 'Process') }
}

function Get-Median($values) {
    $v = @($values | Where-Object { $null -ne $_ } | Sort-Object)
    if ($v.Count -eq 0) { return $null }
    $v[[int][math]::Floor(($v.Count - 1) / 2)]
}

$exeMb = [math]::Round((Get-Item -LiteralPath $ExePath).Length / 1MB, 1)
"== $Label  ($exeName, exe $exeMb MB, $Runs runs) =="
$results | Format-Table -AutoSize | Out-String | Write-Output
$median = [pscustomobject]@{
    Label        = $Label
    Exe          = $exeName
    ExeMB        = $exeMb
    T_window_s   = Get-Median ($results.T_window_s)
    T_content_s  = Get-Median ($results.T_content_s)
    Processes    = Get-Median ($results.Processes)
    PrivateMB    = Get-Median ($results.PrivateMB)
    WorkingSetMB = Get-Median ($results.WorkingSetMB)
}
"MEDIAN"
$median | Format-List | Out-String | Write-Output
