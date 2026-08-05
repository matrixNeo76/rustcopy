---
name: molecule-5-restore
version: 1.0.0
category: molecule
parent: rustcopy-flow
tags: [rustcopy, restore, disaster-recovery, decrypt]
description: "Restore da un report JSON di backup precedente, opzionale decifratura. Scenario 3, autonoma (non passa da Plan/Dry-run generiche)."
steps: 5
max_steps: 6
---

# Molecola 5: Restore — Disaster Recovery (Scenario 3)

## Input
- Richiesta utente di ripristino (es. "ho perso dei file", "restore da backup di ieri")
- Path del binario (Molecola 0)

## Steps

1. **Localizza il report JSON del backup da ripristinare**
   - Chiedi all'utente il path, o cerca in `_ops_reports/`/`report_path` noti per pattern data/nome
   - Verifica che il file sia un report valido di rustcopy (`schema_version` presente, campo
     `source`/`dest` leggibili) — un `schema_version` inferiore all'attuale è comunque
     supportato (i campi mancanti hanno default), non serve rifiutarlo

2. **Determina se serve `--decrypt`**
   - Se il backup originale usava `--encrypt-aes256`, il restore richiede `--decrypt <STESSA_KEY>`
     — chiedi come recuperare la key (`env:NAME`, `file:PATH`, o passphrase manuale) SENZA farla
     scrivere in chiaro nella conversazione se evitabile (preferisci `env:`/`file:` a un
     letterale visibile in chat/process list)
   - Se non sei sicuro che il backup fosse cifrato, prova prima senza `--decrypt`: file cifrati
     letti senza decifratura risultano illeggibili/corrotti in modo evidente, non un rischio
     silenzioso

3. **Costruisci il comando**
   - `<bin> --restore-from <report.json> [--decrypt <KEY>] [--dry-run]`
   - **Nota importante**: `--restore-from` inverte automaticamente source/dest rispetto al
     backup originale (scrive nella sorgente originale) — qualunque altro flag digitato insieme
     a `--restore-from` (incluso `--dry-run`, `--log-path`, `--webhook-url`) sopravvive invariato,
     quindi PUOI e DOVRESTI usare `--dry-run` qui allo stesso modo delle Molecole 1-2, anche se
     questo scenario non passa dalla Molecola 2 generica
   - Esegui prima con `--dry-run`, mostra cosa verrebbe scritto/sovrascritto, chiedi conferma

4. **Checkpoint umano OBBLIGATORIO prima dell'esecuzione reale**
   - Un restore scrive potenzialmente sopra dati esistenti nella directory sorgente originale —
     conferma esplicita, non implicita da un "procedi" generico dato per altro
   - Mostra: directory di destinazione del restore (= source originale), numero di file previsti
     dal dry-run, se `--decrypt` è attivo

5. **Esegui il restore reale, poi passa a Molecola 7**
   - Dopo il restore, se il backup originale usava `--verify-integrity`, consiglia di rilanciare
     una verifica indipendente (nuovo `--verify-integrity` sulla directory appena ripristinata)
     per confermare l'integrità post-restore, non fidarsi solo dell'exit code del restore

## Output Finale
- Directory sorgente originale ripristinata (o dry-run che ne conferma il contenuto atteso)
- Report JSON del restore
- Raccomandazione di verifica integrità post-restore

## Failure Modes
- **Report JSON non trovato/illeggibile**: chiedi un path alternativo, non tentare di indovinare
- **Key di decifratura sbagliata/mancante**: il restore produce file corrotti in modo evidente,
  non un errore esplicito immediato — se il contenuto ripristinato è illeggibile, sospetta prima
  la key sbagliata
- **Directory sorgente originale non più vuota/con dati nuovi nel frattempo**: il restore
  sovrascrive per-file (non pulisce prima) — se serve un ripristino "pulito", valuta con
  l'utente se svuotare prima la destinazione (azione distruttiva separata, da confermare a parte)
