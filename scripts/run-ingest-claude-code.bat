@echo off
REM Thin wrapper: launches the PowerShell script from a plain double-click or cmd.exe prompt.
REM Pass /dryrun to run in --dry-run mode, e.g.:  run-ingest-claude-code.bat /dryrun
setlocal
set DRYRUN_FLAG=
if /I "%~1"=="/dryrun" set DRYRUN_FLAG=-DryRun

powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0run-ingest-claude-code.ps1" %DRYRUN_FLAG%
exit /b %ERRORLEVEL%
