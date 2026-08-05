---
name: molecule-6-automation
version: 1.0.0
category: molecule
parent: rustcopy-flow
tags: [rustcopy, task-scheduler, windows-service, automation, schtasks]
description: "Installa/rimuove uno schedule Task Scheduler o un servizio Windows che rilancia rustcopy. Scenario 4, non esegue un backup adesso."
steps: 5
max_steps: 6
---

# Molecola 6: Automation — Schedule & Servizio (Scenario 4)

## Input
- Comando rustcopy già validato (idealmente passato per gli Scenari 1/2, con dry-run già fatto
  manualmente almeno una volta) — l'automazione non deve essere il primo test di un comando mai
  eseguito
- Path del binario (Molecola 0)

## Steps

1. **Determina il meccanismo richiesto**
   - **Task Scheduler** (`--install-schedule`): per backup ricorrenti pianificati (giornaliero,
     orario, settimanale) — nessun processo persistente, Windows stesso richiama il binario al
     trigger
   - **Servizio Windows** (`--install-service`): registra il binario come servizio SCM, ma di
     default parte **idle** (non esegue nulla finché non riceve lavoro reale — vedi limitazioni
     sotto). Non è un sostituto dello scheduler per backup ricorrenti in questa versione di
     rustcopy
   - Se l'utente vuole "un backup ogni notte": è Task Scheduler, non il servizio

2. **Costruisci lo SPEC dello schedule** (se Task Scheduler)
   - Grammatica supportata: `daily@HH:MM`, `hourly@N`, `weekly@DAY,...@HH:MM` (DAY = codice a 3
     lettere, es. `MON`) — non è cron generico, non provare sintassi diversa
   - Il comando registrato in Task Scheduler è **esattamente** l'invocazione reale digitata
     insieme a `--install-schedule` (minus i soli flag di scheduling) — quindi il comando deve
     essere già completo e corretto (source/dest/esclusioni/config) PRIMA di installarlo: se usa
     `--config job.toml`, il file verrà riletto ad ogni trigger, non congelato al momento
     dell'installazione
   - `--schedule-name <NAME>` per dare un nome esplicito (default `rustcopy` se omesso) — utile
     se l'utente prevede più schedule diversi

3. **Checkpoint OBBLIGATORIO prima di installare**
   - Mostra il comando esatto che verrà eseguito ad ogni trigger (Task Scheduler) o all'avvio
     (servizio), e il trigger/nome scelto
   - Ricorda esplicitamente: creare un task Task Scheduler per l'utente corrente NON richiede
     privilegi elevati; `--install-service` SÌ (Service Control Manager) — se il servizio
     fallisce con un errore legato ai permessi, è quasi certamente mancata elevazione
     "Esegui come amministratore", non un bug

4. **Esegui l'installazione/rimozione**
   - Installazione: `<bin> <comando-completo> --install-schedule <SPEC> [--schedule-name <NAME>]`
     oppure `<bin> --install-service`
   - Rimozione: `<bin> --uninstall-schedule <NAME>` oppure `<bin> --uninstall-service`
     (nessuno dei due richiede `--source`/`--dest`)
   - Verifica l'esito: per lo schedule, `schtasks /Query /TN <name>` conferma la creazione (non
     esiste ancora un `--list-schedules` interno); per il servizio, controlla lo stato via
     Gestione Servizi o `Get-Service` con il nome atteso

5. **Riepiloga all'utente**
   - Cosa è stato installato, quando scatterà, come verificarlo manualmente, come rimuoverlo in
     futuro (comando esatto di uninstall da riusare)

## Output Finale
- Task Scheduler entry o servizio Windows installato/rimosso
- Comando di disinstallazione pronto per riferimento futuro

## Failure Modes
- **Servizio installato ma "non fa nulla"**: comportamento atteso in questa versione — il
  servizio idle è solo infrastruttura SCM, non esegue backup automatici da solo. Se l'utente si
  aspettava backup automatici dal servizio, correggi l'aspettativa e proponi Task Scheduler
- **Errore di permessi su `--install-service`**: richiede una sessione realmente elevata
  (Amministratore), non basta "Esegui come amministratore" su un terminale già aperto senza
  elevazione — va riaperta la shell elevata
- **SPEC di schedule non valido**: rustcopy rifiuta con un errore di parsing esplicito — non
  improvvisare varianti della grammatica, ricontrolla `daily@HH:MM`/`hourly@N`/`weekly@DAY,...@HH:MM`
- **Comando con `--restore-from`/`--resume-from` insieme a `--install-schedule`**: sono
  mutuamente esclusivi in rustcopy (`conflicts_with_all`) — non proporre questa combinazione
