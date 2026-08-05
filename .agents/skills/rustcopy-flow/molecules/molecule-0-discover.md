---
name: molecule-0-discover
version: 1.0.0
category: molecule
parent: rustcopy-flow
tags: [rustcopy, discovery, binary-resolution, cross-cli]
description: "Localizza il binario rustcopy, ne verifica la versione, individua config TOML riutilizzabili. Nessuna dipendenza da MCP: solo shell."
steps: 4
max_steps: 6
---

# Molecola 0: Discover — Binario, Versione, Config Esistenti

## Input
- Richiesta utente (può contenere un path esplicito al binario o al repo)

## Steps

1. **Determina l'OS/shell disponibile**
   - Se l'agente ha un tool Bash → sintassi POSIX/git-bash (`./target/release/robocopy_ingest.exe` o `robocopy_ingest` su Linux/macOS)
   - Se l'agente ha solo PowerShell → `.\target\release\robocopy_ingest.exe`
   - Output: nessun file, solo la scelta di sintassi per gli step successivi

2. **Risolvi il path del binario**, in quest'ordine (fermati al primo che esiste):
   1. Variabile d'ambiente `RUSTCOPY_BIN` (se impostata dall'utente/ambiente)
   2. `<cwd>/target/release/robocopy_ingest.exe` (o senza `.exe` su non-Windows) — funziona se
      la sessione è già dentro il repo `robocopy-ingest-cli`
   3. Repo noto sulla macchina dell'utente: `C:\Users\<utente>\repos\robocopy-ingest-cli\target\release\robocopy_ingest.exe`
      (chiedi conferma dell'username se non deducibile dall'ambiente)
   4. **Solo su Windows con Everything disponibile** (`http://127.0.0.1:80`, vedi preferenze
      globali dell'utente): cerca per nome file
      ```powershell
      $result = Invoke-RestMethod -Uri "http://127.0.0.1:80/?s=robocopy_ingest.exe&j=1&count=20&path_column=1" -TimeoutSec 8
      $result.results | Select-Object type, name, path
      ```
   5. Se nessuno trovato: chiedi all'utente il path, oppure se va compilato ora
      (`cargo build --release` nel repo, opzionalmente `--features notify-server` se serve anche
      `notify-server`)
   - Output: path assoluto del binario, usato da tutte le molecole successive

3. **Verifica versione e capacità**
   - Esegui `<bin> --version` e `<bin> --help` (solo la prima riga, per confermare che risponda)
   - Se il comando fallisce (permessi, architettura sbagliata, binario corrotto): segnala e
     interrompi la skill
   - **Nota piattaforma**: se l'OS corrente non è Windows, i trasferimenti reali via
     `robocopy.exe` non sono possibili — segnalalo subito e limita gli scenari disponibili a
     pianificazione/dry-run/lettura report (il motore di confronto e la logica di verifica sono
     cross-platform, il trasferimento reale no)
   - Output metric: binario risponde, versione registrata

4. **Individua config TOML riutilizzabili**
   - Cerca file `examples/*.toml` e `examples/*.local.toml` nella cartella del repo (se
     individuata) — questi contengono spesso source/dest/esclusioni già validati in sessioni
     precedenti (es. `full-profile-test.local.toml`, `smb-nas-mirror.toml`)
   - Se l'intento utente sembra corrispondere a un TOML esistente (stessa coppia source/dest, o
     stesso caso d'uso), proponilo come punto di partenza nella Molecola 1 invece di ricostruire
     tutto da zero
   - Output: lista di TOML trovati (path + una riga di descrizione presa dal commento in testa al
     file, se presente)

## Output Finale
- Path assoluto del binario risolto (da riusare in tutte le molecole successive)
- Conferma versione/funzionamento
- Lista di eventuali TOML riutilizzabili

## Failure Modes
- **Binario non trovato e non compilabile** (nessun toolchain Rust): chiedi all'utente
  un'installazione pre-compilata o un path manuale
- **OS non Windows**: prosegui solo con pianificazione/dry-run/lettura report, segnala il limite
  esplicitamente prima di proporre qualunque scenario
- **Everything non raggiungibile** (`http://127.0.0.1:80` non risponde): salta silenziosamente
  questo fallback, non è un errore bloccante
- **Più binari trovati** (build debug e release, o più repo): preferisci sempre `release`;
  se ambiguo, chiedi conferma
