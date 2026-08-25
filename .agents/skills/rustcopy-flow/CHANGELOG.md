# Changelog — rustcopy-flow

## v1.1.0 (2026-08-25)

Due molecole nuove, Scenari 5 e 6.

- **Molecola 8 — Diagnose** (Scenario 5): risponde a domande sullo *storico* dei backup
  interrogando `robocopy_ingest --advise`. Copre "quanto durano di solito", "quando è fallito",
  "ogni quanto posso schedularlo", "quanto spazio per N generazioni". Sola lettura: se emerge
  un'azione, la propone e si ferma.
- **Molecola 9 — Notify** (Scenario 6): `--webhook-url` e `notify-server`. Colma il gap rilevato
  in `VALUTAZIONE_AI.md` §2.2 — degli agenti specializzati *scan, backup, verify, restore,
  notify*, gli altri quattro avevano già una molecola e questo no.

Nessuna modifica alle molecole 0-7 e nessun cambio ai checkpoint esistenti.

I suggerimenti della Molecola 8 sono **deterministici**: statistica sulle run passate, nessun
modello linguistico coinvolto, evidenze numeriche sempre mostrate. Dipende da
`.rustcopy_history.jsonl`, che le versioni di rustcopy dalla Fase 0 in poi scrivono accanto ai
report a ogni run conclusa.

## v1.0.0 (2026-08-05)

Prima versione. Adattata dalla struttura compound+molecole di `structured-memory-flow`
(craft-skills-flow) al progetto `robocopy-ingest-cli` (rustcopy).

Differenze principali rispetto al modello di riferimento:
- Zero dipendenze da tool MCP proprietari (niente `remember()`, `search_memory()`,
  `spawn_session()` craft-memory-style) — l'unico canale è l'esecuzione shell del binario
  rustcopy, per garantire portabilità su Claude Code, OpenCode e altre CLI di coding.
- Sub-agenti resi opzionali (usati solo se l'ambiente li offre e la fase è pesante), non
  obbligatori per ogni fase.
- 4 scenari coprono le funzionalità reali di rustcopy: backup rapido/mirror, backup a
  generazioni + retention, restore/disaster recovery, automazione (Task Scheduler/servizio).
- Ogni molecola incorpora i pitfall noti già documentati in `CLAUDE.md`/`ANALYSIS.md` del
  progetto (es. `--exclude-junctions` insieme a `--exclude-dirs`, `--no-prescan`+`--mirror`,
  eccezioni CLI-only nel TOML, semantica di retention per cicli).

Copie distribuite:
- `robocopy-ingest-cli/.agents/skills/rustcopy-flow/` (fonte di verità, versionata con git)
- `~/.claude/skills/rustcopy-flow/` (copia globale, per l'uso da qualunque directory/progetto)
