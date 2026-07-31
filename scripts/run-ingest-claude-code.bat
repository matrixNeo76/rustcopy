@echo off
REM Thin wrapper: launches the PowerShell script from a plain double-click or cmd.exe prompt.
REM Runs two legs: FILESERV01\provarust2 first, then the QNAP NAS at 192.168.1.187\datas01
REM (credentials read from scripts\nas2-credentials.local.ps1, not committed to git).
REM
REM Usage:
REM   run-ingest-claude-code.bat                    normal run, both destinations
REM   run-ingest-claude-code.bat /dryrun             simulate, no files copied
REM   run-ingest-claude-code.bat /skipsecond          only run the first destination
setlocal
set EXTRA_FLAGS=
if /I "%~1"=="/dryrun" set EXTRA_FLAGS=-DryRun
if /I "%~1"=="/skipsecond" set EXTRA_FLAGS=-SkipSecondDestination

powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0run-ingest-claude-code.ps1" %EXTRA_FLAGS%
exit /b %ERRORLEVEL%
