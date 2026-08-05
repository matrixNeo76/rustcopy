# Changelog — rustcopy-flow

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
