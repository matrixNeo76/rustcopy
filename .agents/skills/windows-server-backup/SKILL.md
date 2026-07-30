---
name: windows-server-backup
version: 1.0.0
category: infrastructure
tags: [windows-server, backup, disaster-recovery, system-state, wbadmin, windows-server-backup, restore]
triggers:
  - "backup Windows Server"
  - "system state backup"
  - "disaster recovery Windows"
  - "wbadmin"
  - "Windows Server Backup installazione"
  - "backup AD"
  - "programma backup server"
  - "restore Windows"
  - "DR Windows Server"

depends_on:
  - windows-server-admin

description: "Backup e Disaster Recovery Windows Server: system state (AD), backup automatico, Windows Server Backup, recovery. Sub-skill di windows-server-admin. Pre-check destinazione backup obbligatorio."
status: active
last_improved: 2026-05-06
improvement_log: "v1.0.0: Creata da refactoring di windows-server-admin v1.1.0. Pre-check destinazione, backup system state prima di modifiche AD, programmazione automatica."
---

# Windows Server Backup & Disaster Recovery

> **⚠️ WARNING**: MAI eseguire operazioni AD senza system state backup PRIMA.
> Verifica SEMPRE che la destinazione backup sia raggiungibile prima di iniziare.
> Testare il ripristino almeno trimestralmente (ISO 27001 A.17).

## Quick Start

```markdown
# Backup system state AD (prima di modifiche!)
1. wbadmin start systemstatebackup
2. Get-WBSummary → verifica
3. wbadmin enable backup → programmazione automatica
```

## System State Backup (AD)

```powershell
try {
    Install-WindowsFeature -Name Windows-Server-Backup

    # PRECHECK: destinazione raggiungibile
    $target = "\\backupsrv\WindowsBackups"
    if (-not (Test-Path $target)) { throw "Destinazione $target non raggiungibile." }

    # Backup system state (OBBLIGATORIO prima di modifiche AD)
    wbadmin start systemstatebackup -backuptarget:$target -quiet
    Get-WBSummary

    # Programmazione automatica
    wbadmin enable backup -addtarget:$target -include:C:,D: -schedule:01:00,13:00 -quiet
} catch { Write-Error "Backup fallito: $_" }
```

## Backup Manuale PowerShell

```powershell
$policy = New-WBPolicy
$volume = Get-WBVolume -VolumePath "C:"
$target = New-WBBackupTarget -NetworkPath "\\backupsrv\WindowsBackups\DC01"
Add-WBVolume -Policy $policy -Volume $volume
Add-WBBackupTarget -Policy $policy -Target $target
Start-WBBackup -Policy $policy
```

## Riferimenti

- Hub: `.agents/skills/windows-server-admin/`
- ISO 27001 A.17: backup policy, retention 30gg, restore test trimestrali
