---
type: Concept
title: Prompt per la prossima sessione
description: Handoff di sessione — stato progetto, aree da investigare, convenzioni stabilite. Riscritto ad ogni sessione.
status: draft
generated:
  by: process:claude-code
  at: 2026-08-20T00:00:00Z
---

# Prompt per la prossima sessione — robocopy-ingest-cli (rustcopy)

## Stato del progetto (23 Agosto 2026)

`Cargo.toml` = **6.0.0**. Ultimo lavoro: D21 (23 Agosto 2026, vedi `ANALYSIS.md`) — `ScanSummary::files` è ora `Arc<[ScannedFile]>`: ogni consumatore deve passare dati posseduti a `spawn_blocking` pur solo leggendoli, e con un `Vec` questo significava una copia vera ad ogni passaggio (**quattro copie vive** dentro `verify`, 580 MB contro 145 misurati sul profilo reale). Trovato leggendo il codice per rispondere a una domanda dell'utente, non cercando un difetto — e lo spunto che l'handoff precedente aveva lasciato aperto su `ScanSummary` puntava alla cosa sbagliata (vedi la nota metodologica in `ANALYSIS.md` D21). Prima ancora, D20 (23 Agosto) — chiude la metà in **lettura** del costo che D19 aveva lasciato aperta: `load_or_default` materializzava in RAM l'intera cronologia a tutti e tre i call site di `main.rs`, nessuno dei quali ne aveva bisogno (**580 MB misurati** su 4 generazioni del profilo reale da 1,34M file, contro 145 MB per l'unica generazione davvero usata e ~0 MB per il pruning; `--backup-type full` non legge mai il manifest e ne caricava comunque 580 MB). Ora letture streaming (`load_latest_generation`/`load_latest_full_generation`) e un `GenerationIndex` metadati-only per la retention. I 5 test black-box sul backup a generazioni sono passati **senza modifiche**: prova end-to-end che il comportamento osservabile è identico. Prima ancora, D19 (23 Agosto) — `GenerationManifest::save` riscriveva l'intera cronologia (fino a centinaia di MB sul profilo reale da 1,34M file) per registrare una sola nuova generazione; il formato su disco è ora NDJSON (una riga per generazione), con `GenerationManifest::append_generation` (nuovo, O(1)) per il caso comune e `save` (invariato di firma, ora scrive NDJSON anch'esso) riservato al pruning `--keep-generations`, l'unico caso che richiede davvero una riscrittura totale. Compatibilità all'indietro con manifest pre-D19 e recupero da riga finale troncata inclusi — vedi `ANALYSIS.md` D19 per il dettaglio. CodeRabbit sulla stessa PR ha trovato e fatto correggere prima del merge un bug reale: `append_generation` avrebbe corrotto un manifest legacy pre-D19 invece di migrarlo. Deriva da una proposta di 3 punti fatta all'utente il 22 Agosto (analisi dei log operativi reali in `_ops_reports/`); l'utente ha approvato **solo i punti 1+2** (default log level/rotazione = D18, formato manifest = D19) — il punto 3 (tre voci di debito tecnico già note, non nuove scoperte) è stato **deliberatamente escluso** da questa sessione, non dimenticato: resta negli spunti sotto se l'utente vorrà promuoverlo in futuro. Prima ancora, D18 (22 Agosto, PR #22) — default di `--log-level` da `debug` a `info` e `--log-max-bytes` reso una rotazione live durante il run in corso; CodeRabbit sulla stessa PR ha trovato e fatto correggere un altro bug reale, `bytes_written` azzerato anche dopo una rotazione fallita. Suite di test: **326** (`cargo test`), **341** con `cargo test --features notify-server` (più test `#[ignore]` — round-trip reali dei servizi Windows/Task Scheduler che richiedono elevazione, più due probe di misurazione a scala reale). CI reale su GitHub Actions verde su `windows-latest` e `ubuntu-latest`, entrambe le configurazioni di feature.

Milestone 5.2.0/5.3.0/6.0.0/6.1.0 tutte chiuse. Difetti storici: **D1-D21**, **nessuno aperto** — D10 (strumentazione grafo Graphify) è stato riclassificato il 23 Agosto 2026 come limite noto dello strumento di estrazione, non come lavoro da pianificare: la sua parte azionabile era già stata fatta e ciò che resta (dispatch indiretto non tracciato) non ha un fix. Non riaprirlo; se serve, rimisurare la reachability. Feature F1-F61 tutte classificate (chiuse, o rimandate al backlog con motivazione — vedi `ROADMAP.md`).

**Questa sessione ha chiuso l'intero `PIANO_MIGLIORAMENTI.md`** ad eccezione di due voci a bassa priorità mai promosse (P3/P4, vedi sotto):

- **Pilastro A** (lacune README) — chiuso 17 Agosto 2026.
- **Pilastro B / B5+B5b** (`CLAUDE.md` 50.108 → 34.203 caratteri, convenzione anti-ricrescita) — chiuso 20 Agosto 2026, PR #16.
- **Pilastro C / B3-B4** (bug/debito tecnico: semantiche di merge TOML, `unsafe` senza `// SAFETY:`) — chiuso 17 Agosto 2026.
- **Pilastro D** (launcher PowerShell interattivo `rustcopy-launcher.ps1` + refactor script→wrapper) — chiuso 18-19 Agosto 2026.
- **Pilastro E / P1-P2** (placeholder `{timestamp}` in `--report-path`, `previous_run_comparison` nel report JSON) — chiuso 19 Agosto 2026.
- **Pilastro E / P3-P4** — **ancora aperti**, priorità bassa (🟢), mai richiesti esplicitamente: P3 (cache dell'inventario di scan — verificare prima la sovrapposizione con `cache.rs`/`generations.rs` esistenti, rischio concreto di terza struttura duplicata) e P4 (retention dei report JSON, tipo `--report-retention-days N`).

**Coerenza documentale verificata e corretta nella sessione del 20 Agosto** (l'utente ha chiesto esplicitamente un controllo di coerenza ROADMAP/OKF dopo la chiusura di B5): conteggi test allineati a 302/317 in `README.md`/`ARCHITECTURE.md`/`AGENTS.md`/`ROADMAP.md`/`RUNBOOK.md` (erano fermi a 286/301 o 284/299) — **da allora aggiornati di nuovo a 307/322 dopo D17 (21 Agosto), vedi sopra**; `ROADMAP.md` riga sulla dimensione di `CLAUDE.md` aggiornata per riflettere la chiusura di B5 (misure e stato, non più "approvato, in attesa"); `docs/archive/AGENT_HARNESS_PLAN.md` (file orfano trovato in root, creato il 10 Agosto 2026, mai eseguito, senza frontmatter OKF) spostato in `docs/archive/` con frontmatter e nota, aggiunto al loop `okf parse` in CI — **13 file** coperti ora (11 root + 2 archiviati), non più 12. Nessuna azione da riaprire su questo fronte salvo nuove derive rilevate in futuro.

---

## 🎯 Obiettivo per la prossima sessione

Nessuna direttiva pregressa vincolante: il piano operativo (`PIANO_MIGLIORAMENTI.md`) è chiuso salvo P3/P4 (bassa priorità, non richiesti). La prossima sessione dovrebbe partire da una richiesta esplicita dell'utente, oppure — in assenza di una — da un nuovo giro di audit del codice Rust esistente (bug reali, robustezza, performance), sullo stesso modello disciplinato delle sessioni precedenti: **verificare empiricamente prima di proporre un fix**, mai fix speculativi su ipotesi non confermate.

### Spunti concreti non esplorati (nessuno confermato — verificare prima di agire)

1. **Il working set del prescan** — `ScanSummary` materializza l'intero inventario della sorgente, e resta un costo **consapevole**: ogni consumatore lo legge davvero, e `--no-prescan` è già la valvola per non pagarlo. D21 ha eliminato i *duplicati* (4 copie → 1), non il working set. Non trattarlo come un difetto aperto: se un giorno servisse ridurlo davvero, la strada è lo streaming verso i consumatori, un cambio architetturale da discutere con `AskUserQuestion`, non un fix.
2. **`engine::naive::copy_files` non traccia progresso parziale su fallimento** (la ragione per cui D15 non ha potuto arricchire il report della pipeline a generazioni con conteggi accurati sui fallimenti parziali).
3. **P3/P4** del Pilastro E (vedi sopra), se l'utente li vuole promuovere.
4. Una nuova lettura dei log operativi reali in `_ops_reports/` (19 file, incluso un profilo da 1.34M file) per nuove evidenze — più affidabile di ipotesi da sola lettura del codice.

### Come procedere

1. Scegli 1-2 aree (dai punti sopra, da una richiesta esplicita dell'utente, o da una nuova lettura del codice/log reali) e **verifica empiricamente** prima di proporre un fix.
2. Per ogni bug reale confermato: fix + unit test + test black-box sul binario reale (vedi convenzioni sotto) + documentazione (`ANALYSIS.md` nuovo D-number se è un difetto, `CLAUDE.md` nota tecnica condensata secondo la convenzione B5b — vedi sotto).
3. Per le opportunità di performance: misura prima di ottimizzare — `scripts/benchmark-threads.ps1`/`scripts/analyze-runs.ps1` o i report in `_ops_reports/`.
4. `AskUserQuestion` prima di qualunque deviazione architetturale o scelta di scope ambigua.
5. Commit/push solo su richiesta esplicita dell'utente in quel turno — un "procedi" su un piano non autorizza automaticamente anche il commit, salvo lo abbia già fatto esplicitamente in quel turno.
6. Chiudi ogni giro con un `grep` dei conteggi test/difetti su tutti i file `.md` toccati — è già successo più volte che restassero disallineati (vedi il controllo di coerenza fatto in questa sessione).

---

## Convenzioni stabilite nelle sessioni precedenti (da rispettare)

- **Test**: per ogni fix, unit test + almeno un test black-box che esegua il **binario compilato reale** (`tests/cli_smoke.rs` per `robocopy_ingest`, `tests/notify_server_e2e.rs` per `notify-server`), mai solo la funzione interna in isolamento. Verifica manuale contro file veri solo dentro `tempfile::tempdir()` isolate, mai contro cartelle reali.
- **Eccezione dichiarata**: quando un test toccherebbe stato di sistema reale fuori dal sandbox tempdir (VSS/F30, servizi Windows F37/F41: richiedono elevazione reale), non automatizzarlo nella suite normale — unit test sulla logica pura isolabile, test `#[ignore]` per il round-trip reale eseguibile a mano da un prompt elevato. F36 (Task Scheduler) è l'eccezione all'eccezione: un'attività per utente corrente non richiede elevazione, quindi è coperta da un vero test black-box **non** `#[ignore]`.
- **Deviazioni architetturali**: fermati e proponi con `AskUserQuestion` prima di implementare, non deciderle silenziosamente.
- **Commit/push**: mai senza richiesta esplicita dell'utente in quel turno.
- **Documentazione da aggiornare ad ogni fix chiuso, nello stesso giro**: `ANALYSIS.md`/`ROADMAP.md` (riga tabella), `CLAUDE.md` (nota tecnica **condensata secondo B5b** — solo prescrizione operativa + una riga di motivo + puntatore, mai narrazione completa), `AGENTS.md` (regole architetturali, conteggio test), `ARCHITECTURE.md` (tabella moduli, conteggio test), `README.md` (tabella flag CLI, conteggio test), `RUNBOOK.md` (esempio d'uso pratico se rilevante, conteggio test), e questo file.
  - **Lezione confermata più volte, e di nuovo in questa sessione**: aggiornare i conteggi test/difetti in **tutti** i file alla fine di ogni giro, non solo in alcuni — `grep` dei vecchi conteggi su tutti i file `.md` prima di chiudere.
  - **B5b (nuovo, 20 Agosto 2026)**: `CLAUDE.md` accetta **solo** la prescrizione operativa per feature nuove — la narrazione completa (alternative valutate, limiti di test, cronologia) va in `ROADMAP.md`/`ANALYSIS.md`. Non violare questa disciplina: è la ragione per cui `CLAUDE.md` era arrivato a 50K caratteri.
- **Ricompilare dopo modifiche**: non automatico — `cargo build --release [--features notify-server]` e/o `ISCC.exe installer\rustcopy.iss` solo su richiesta esplicita.
- **Config TOML**: quasi tutti i flag CLI recenti sono anche in `JobConfig`/`IngestConfig`. Eccezioni consapevoli: `--decrypt`, `--restore-from`, `--vss-snapshot`, `--resume-from`, `--force-purge`, `--exclude-junctions`, `--fast-verify`, `--html-report-path`, `--install-schedule`, `--install-service` (flag di sicurezza o CLI-only).
- **rtk**: attivo. `rtk gain` per verificare token risparmiati. Nessuna azione richiesta a inizio sessione.
- **CodeRabbit**: questo repo ha <10 stelle, quindi non riceve review automatiche — va attivata manualmente per ogni PR (checkbox "🔍 Trigger review" nel commento di CodeRabbit, via `gh api` PATCH sul commento). Ogni finding va verificato contro il codice reale prima di applicarlo, mai applicato ciecamente.

## Cosa NON toccare senza motivo

- `engine::robocopy::build_args` non deve mai passare `/Z` (restartable mode) — costo prestazionale deliberatamente evitato sui file piccoli.
- `src/oem_codec.rs` non va sostituito con `encoding_rs::Encoding::for_label(b"ibm850")`.
- Ogni operazione bloccante su filesystem/processo in `main.rs` deve restare dentro `spawn_blocking_with_span` (D13) — **mai** `tokio::task::spawn_blocking` diretto, e mai chiamate sincrone dentro le `async fn` di orchestrazione.
- `main.rs::run_jobs` (F33) ricostruisce `Args` per ogni job da un clone dell'invocazione CLI originale, mai da `try_parse_from` né dall'`Args` già mergiato del job precedente — stessa disciplina in `restore::build_restore_args`, `checkpoint::build_resume_args`, `schedule::strip_schedule_flags`.
- `execute_generation_backup` (F34) non deve rientrare in `transfer()`/robocopy per incrementale/differenziale — `engine::naive::copy_selected` resta l'unica strada corretta.
- `GenerationManifest::generations_to_prune` (F35) ragiona per **ciclo**, mai per singola generazione.
- `src/service.rs` (F37/F41): `robocopy_ingest` e `notify-server` hanno **due identità di servizio separate** — non farne una sola.
- `scan::scan`/`scan::inventory` (D11) devono continuare a pruning via `WalkDir::filter_entry()`.
- `robocopy_ingest::atomic_write` (D14) resta il solo modo corretto per una riscrittura totale (cache fast-verify, e `GenerationManifest::save` usato solo dal pruning `--keep-generations`) — ma il caso comune di registrare una generazione va per `GenerationManifest::append_generation` (D19, NDJSON append-only), non più per `push`+`save`.
- `GenerationManifest::load_or_default` (D20) va usata **solo** dove serve davvero l'intera cronologia — oggi il solo secondo load di `prune_old_generations`, che riscrive il file. Chi vuole la generazione di riferimento usa `load_latest_generation`/`load_latest_full_generation`, chi vuole decidere cosa potare usa `GenerationIndex::load`. E la definizione di *ciclo* resta una sola (`cycle_ranges`): non reimplementarla per il percorso metadati-only.
- `.github/workflows/ci.yml` gira su `windows-latest` **e** `ubuntu-latest` — non rimuovere il job Linux (ha trovato D16).
- I file `.md` root + `docs/archive/` con frontmatter OKF (13 file totali dopo questa sessione: 11 root + `PIANO_NOTIFY_SERVER.md` + `AGENT_HARNESS_PLAN.md`) — un nuovo file `.md` permanente in root va aggiunto sia col frontmatter sia alla lista nel job `docs` di CI.
- Dettaglio completo di ogni punto sopra: `CLAUDE.md` (condensato secondo B5b) rimanda a `ROADMAP.md`/`ANALYSIS.md`/`AGENTS.md` per la narrazione estesa — consultarli lì, non aspettarsi di trovarla in `CLAUDE.md`.

## Skill disponibile per operare rustcopy

`.agents/skills/rustcopy-flow/` (+ copia globale `~/.claude/skills/rustcopy-flow/`) — compound skill per costruire/eseguire comandi rustcopy reali con dry-run e checkpoint umani obbligatori. Zero dipendenza MCP, per design (vedi `ROADMAP.md` F61 sul perché un server MCP è stato rimandato).
