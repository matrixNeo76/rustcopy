---
name: molecule-3-quickcopy
version: 1.0.0
category: molecule
parent: rustcopy-flow
tags: [rustcopy, execute, copy, mirror]
description: "Esecuzione reale di una copia semplice o --mirror, dopo dry-run confermato. Scenario 1."
steps: 3
max_steps: 4
---

# Molecola 3: Quick Execute — Copia Reale (Scenario 1)

## Input
- Comando confermato dalla Molecola 2 (identico al dry-run, meno `--dry-run`)
- Conferma esplicita dell'utente (checkpoint Molecola 2)

## Steps

1. **Gestisci `--mirror` con cautela esplicita, se presente nel piano**
   - Rustcopy stesso chiede conferma interattiva prima di purgare file solo-in-destinazione,
     A MENO che `--force-purge` sia passato
   - **Non aggiungere `--force-purge` di iniziativa.** Se la sessione non è interattiva (l'agente
     non può rispondere a un prompt bloccante del processo figlio), chiedi ESPLICITAMENTE
     all'utente in questo turno se autorizza `--force-purge` per QUESTA esecuzione, spiegando
     cosa verrà cancellato (rustcopy lo mostra nel suo output di safety-check se eseguito con
     `--dry-run` prima, o descrivilo dal confronto source/dest fatto in Molecola 2)
   - Se l'utente non autorizza: lascia che il comando venga eseguito interattivamente (se
     l'ambiente lo permette) o interrompi e segnala che serve conferma manuale

2. **Esegui il comando reale** (senza `--dry-run`)
   - Se l'albero è grande, valuta background execution (come in Molecola 2, Step 1) — un run
     reale multi-milione-file può durare da minuti a ore
   - Monitora l'exit code al termine: `0` successo, `1` trasferimento fallito (retry esauriti su
     alcuni file), `2` errore di utilizzo/validazione, `3` mirror-purge abortito, `4` transfer ok
     ma `--verify-integrity` ha trovato problemi
   - Output: log/report reale (`--report-path`), eventuale `--html-report-path`

3. **Passa alla Molecola 7 (Verify & Report)** con l'exit code e il path del report reale

## Output Finale
- Report JSON/HTML della copia reale
- Exit code registrato per la Molecola 7

## Failure Modes
- **Exit code 1 (retry esauriti su alcuni file)**: non è necessariamente un fallimento totale —
  la Molecola 7 deve distinguere "alcuni file falliti" da "niente copiato"; suggerisci un secondo
  run mirato (stesso comando, robocopy salta i file già corretti) se l'utente vuole ritentare
- **Exit code 3 (mirror abortito)**: safety-check ha rilevato differenze non attese — NON
  ripetere aggiungendo `--force-purge` automaticamente, torna dall'utente con il dettaglio di
  cosa verrebbe cancellato
- **Processo interrotto (Ctrl+C) a metà**: rustcopy scrive un checkpoint
  (`<report>.checkpoint.json`) — segnala all'utente che può riprendere con `--resume-from
  <checkpoint>` invece di ripartire da zero
