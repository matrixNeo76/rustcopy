---
name: molecule-2-dryrun
version: 1.0.0
category: molecule
parent: rustcopy-flow
tags: [rustcopy, dry-run, validation, report-json]
description: "Esegue sempre il piano con --dry-run prima di un'esecuzione reale, legge il report JSON, chiede conferma. Usata dagli Scenari 1 e 2."
steps: 4
max_steps: 5
---

# Molecola 2: Dry-run — Validazione Prima dell'Esecuzione Reale

## Input
- Comando/TOML costruito nella Molecola 1
- Path del binario (Molecola 0)

## Steps

1. **Esegui il comando con `--dry-run` aggiunto**
   - Se il comando usa già `--config <toml>`, aggiungi semplicemente `--dry-run` come flag CLI
     complementare (non serve nel TOML)
   - Aggiungi anche `--report-path` a un path temporaneo/dedicato se non già specificato, così il
     report è leggibile subito dopo (es. `_ops_reports/<timestamp>-dryrun.json`)
   - **Se l'albero è molto grande (milioni di file)**: valuta di lanciare il comando in
     background (se l'ambiente lo supporta) invece di bloccare la sessione — il dry-run include
     comunque il prescan completo, che su alberi enormi può richiedere minuti
   - Output: `<report>-dryrun.json`, log del dry-run

2. **Leggi il report JSON prodotto**
   - Campi chiave: `total_files`, `total_bytes`, `robocopy_transfer.elapsed_seconds`,
     `robocopy_transfer.throughput_mbps`, `phase_timing.inventory_seconds`,
     `configuration` (per confermare che i flag applicati corrispondano al piano)
   - Verifica che `total_files`/`total_bytes` siano coerenti con l'aspettativa dell'utente (un
     numero molto più basso o molto più alto del previsto indica un'esclusione sbagliata o un
     pattern troppo/poco permissivo)
   - Output: riepilogo leggibile (file, byte, tempo di inventario)

3. **Verifica assenza di errori nel dry-run**
   - `exit_code` dovrebbe essere `0` anche in dry-run; un codice diverso (es. `2` per un errore
     di validazione, `3` se `--mirror` ha abortito la safety-check) va investigato PRIMA di
     proporre l'esecuzione reale
   - Se `--verify-integrity` era attivo: il dry-run la salta sempre ("nothing was copied in
     dry-run mode") — non è un errore, è comportamento atteso

4. **Checkpoint umano**
   - Mostra il riepilogo del punto 2 + eventuali warning del punto 3
   - Chiedi: "Il dry-run conferma il piano ({N} file, {X} GB). Procedo con l'esecuzione reale?"
   - Se l'utente vuole modificare qualcosa (esclusioni, thread, ecc.): torna alla Molecola 1
   - Se confermato: passa alla molecola di esecuzione dello scenario (3 o 4)

## Output Finale
- `<report>-dryrun.json` (conservato per confronto post-esecuzione, se utile)
- Conferma esplicita dell'utente per procedere

## Failure Modes
- **`exit_code` non-zero nel dry-run**: non proporre l'esecuzione reale, mostra l'errore e torna
  alla pianificazione
- **`total_files`/`total_bytes` molto diversi dall'atteso**: segnala esplicitamente prima di
  chiedere conferma, non lasciare che l'utente confermi un piano probabilmente sbagliato
- **Dry-run troppo lento** (albero enorme, nessun `--no-prescan`): se l'utente ha fretta e accetta
  di rinunciare al conteggio totale/verify-integrity, proponi `--no-prescan` solo per QUESTA
  richiesta esplicita — mai di default, e mai insieme a `--mirror` (vedi Molecola 1, Step 3)
