---
name: molecule-7-verify-report
version: 1.0.0
category: molecule
parent: rustcopy-flow
tags: [rustcopy, report, exit-codes, verification, summary]
description: "Interpreta exit code e report JSON/HTML dopo qualunque esecuzione reale, riepiloga in italiano all'utente. Usata da tutti gli scenari con esecuzione reale (1, 2, 3)."
steps: 4
max_steps: 5
---

# Molecola 7: Verify & Report — Riepilogo Finale

## Input
- Exit code dell'esecuzione reale
- Path del report JSON (ed eventuale HTML) prodotto

## Steps

1. **Interpreta l'exit code**
   | Codice | Significato | Azione consigliata |
   |---|---|---|
   | `0` | successo completo | riepiloga e chiudi |
   | `1` | trasferimento fallito su alcuni file (retry esauriti) | mostra quali file dal log, proponi un secondo run mirato |
   | `2` | errore di utilizzo/validazione (prima ancora di copiare) | mostra il messaggio d'errore, torna alla pianificazione |
   | `3` | `--mirror` abortito dalla safety-check | NON riprovare con `--force-purge` automaticamente, mostra il dettaglio e chiedi conferma esplicita |
   | `4` | copia riuscita ma `--verify-integrity` ha trovato mismatch | mostra i mismatch dal report, valuta se sono transitori (log/tmp: `--ignore-transient-missing` alla prossima run) |
   | `5` | generazione salvata ma rotazione retention abortita | la nuova generazione è al sicuro; serve un secondo run con conferma per liberare spazio |

2. **Leggi il report JSON e estrai i campi rilevanti**
   - Sempre: `total_files`, `total_bytes`, `robocopy_transfer.elapsed_seconds`,
     `robocopy_transfer.throughput_mbps`, `robocopy_transfer.exit_code_meaning`
   - Se `verify_integrity` era attivo: cerca `integrity`/`mismatches`/`missing_in_dest` (nomi
     esatti dei campi possono variare per `schema_version`, verifica quello effettivo nel file)
   - Se `--backup-type` era attivo: conferma in quale sottocartella `<dest>/<timestamp>_<type>/`
     è finita la nuova generazione
   - Se presente, apri/segnala il path del report HTML per una lettura visuale più comoda

3. **Riepiloga in italiano all'utente**, in modo compatto:
   - Cosa è stato fatto (source → dest, tipo di operazione)
   - Quanti file/byte, in quanto tempo, a che throughput
   - Esito (successo pieno / parziale / fallito) con l'exit code e il suo significato
   - Eventuali azioni di follow-up consigliate (secondo run, verifica manuale, rotazione da
     confermare)

4. **Se pertinente, aggiorna la documentazione del progetto**
   - Solo se questa esecuzione ha rivelato qualcosa di nuovo e generalizzabile (non specifico
     alla singola run) — es. un bug nel comportamento di rustcopy, un limite di performance
     misurato — segui la stessa disciplina di documentazione già stabilita nel repo
     (`ANALYSIS.md`/`CLAUDE.md`/`NEXT_SESSION_PROMPT.md`), non per ogni singola esecuzione di
     routine

## Output Finale
- Riepilogo testuale mostrato all'utente
- (Opzionale) aggiornamento di documentazione, solo se l'esecuzione ha prodotto un apprendimento
  generalizzabile

## Failure Modes
- **Report JSON non trovato** (es. crash prima della scrittura): riepiloga dal solo exit code +
  log, segnala esplicitamente l'assenza del report invece di inventare numeri
- **Mismatch di integrità su pattern noti come transitori** (`.log`, `.tmp`,
  `.git/objects/`): non trattarli come fallimento grave, ma segnala comunque che
  `--ignore-transient-missing` alla prossima run li filtrerebbe automaticamente
- **Utente si aspettava un formato di riepilogo diverso** (es. tabella, solo numeri): adattati
  alla richiesta, il formato sopra è un default ragionevole non un obbligo
