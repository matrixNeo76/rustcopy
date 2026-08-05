---
name: molecule-4-generations
version: 1.0.0
category: molecule
parent: rustcopy-flow
tags: [rustcopy, backup-type, generations, retention, keep-generations]
description: "Esecuzione di backup versionati (full/incremental/differential) con retention opzionale per cicli. Scenario 2."
steps: 5
max_steps: 6
---

# Molecola 4: Generations & Retention — Backup Versionato (Scenario 2)

## Input
- Comando/piano confermato dal dry-run (Molecola 2), con `--backup-type` impostato
- Path del manifest esistente, se presente: `<dest>/.rustcopy_generations.json`

## Steps

1. **Determina il tipo di generazione corretto**
   - `full`: sempre valido, prima generazione o "punto di riferimento" per i differenziali
     successivi
   - `incremental`: richiede ALMENO una generazione precedente (di qualunque tipo) nel manifest —
     se il manifest non esiste o è vuoto, rustcopy rifiuta con un errore chiaro: proponi `full`
     invece
   - `differential`: richiede ALMENO una generazione `full` precedente (un `incremental` in
     mezzo non conta come riferimento) — stesso controllo, proponi `full` se manca
   - Se il manifest esiste, leggilo per mostrare all'utente la cronologia (`generations[]`: tipo,
     timestamp, N file) prima di scegliere

2. **Se `--keep-generations <N>` è richiesto, spiega la semantica PRIMA di eseguire**
   - La rotazione avviene per **ciclo** (un `full` + tutti gli `incremental`/`differential` che
     lo seguono fino al prossimo `full`), non per singola generazione — questo evita di cancellare
     un `full` ancora referenziato da un `incremental`/`differential` mantenuto
   - Mostra quali cicli verrebbero eliminati (i più vecchi oltre gli `N` richiesti) leggendo il
     manifest, PRIMA di eseguire
   - Come per `--mirror`, la purge richiede `--force-purge` o conferma interattiva — **non
     aggiungere `--force-purge` di iniziativa**, stesso protocollo della Molecola 3, Step 1

3. **Checkpoint esplicito separato per la retention**
   - Anche se il dry-run (Molecola 2) ha già validato source/dest/pattern, la Molecola 2 NON
     valida la logica di retention in modo specifico (il dry-run copre la copia, non la purge
     delle vecchie generazioni) — chiedi conferma dedicata: "Verranno eliminati i cicli: {lista}.
     Confermi?"

4. **Esegui il comando reale**
   - Nota: `--backup-type` non è compatibile con `--mirror` (rustcopy lo rifiuta) — se il piano
     della Molecola 1 conteneva entrambi, correggilo prima di arrivare qui
   - Ricorda che la copia effettiva per incremental/differential NON passa da robocopy ma dal
     motore `naive` interno (per poter selezionare un elenco esplicito di file) — throughput
     atteso più basso rispetto a un mirror/copy pieno via robocopy, non è un'anomalia
   - Monitora l'exit code: oltre ai codici della Molecola 3, `5` = purge di retention abortita
     (la nuova generazione è comunque stata copiata e salvata nel manifest — solo la rotazione
     delle vecchie è stata annullata)

5. **Passa alla Molecola 7 (Verify & Report)**

## Output Finale
- Nuova generazione scritta in `<dest>/<timestamp>_<type>/` + manifest aggiornato
- Report JSON/HTML dell'esecuzione
- Exit code registrato per la Molecola 7

## Failure Modes
- **`incremental`/`differential` senza generazione di riferimento**: rustcopy rifiuta con errore
  esplicito, non un fallback silenzioso a full — proponi `full` esplicitamente all'utente
- **Exit code 5 (retention abortita)**: la nuova generazione è al sicuro, ma il disco continua a
  crescere finché la rotazione non viene confermata — segnala che serve un secondo run con
  conferma esplicita per liberare spazio
- **`--keep-generations` senza `--backup-type`**: rustcopy rifiuta (`KeepGenerationsWithoutBackupType`)
  — verificalo già in Molecola 1, non aspettare l'errore a runtime
