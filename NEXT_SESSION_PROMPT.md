---
type: Concept
title: Prompt per la prossima sessione
description: Handoff di sessione — stato progetto, aree da investigare, convenzioni stabilite. Riscritto ad ogni sessione.
status: draft
generated:
  by: process:claude-code
  at: 2026-09-04T00:00:00Z
---

# Prompt per la prossima sessione — robocopy-ingest-cli (rustcopy)

## Stato del progetto (4 Settembre 2026)

`Cargo.toml` = **6.0.0**. Suite di test: **467** (`cargo test --workspace --exclude rustcopy-gui`), **482** con `--features rustcopy-cli/notify-server` (più test `#[ignore]` — round-trip reali dei servizi Windows che richiedono elevazione, più due probe di misurazione a scala reale). CI verde su `windows-latest` e `ubuntu-latest` per entrambe le configurazioni, più i job dedicati `gui`, `gui-npm-audit`, `versions` e `docs`.

**Ultimo lavoro: espansione ed rifacimento visivo della console, dopo la chiusura della milestone 7.0.0 (2 Set 2026).** Ordine cronologico delle PR #63-#83:

- **Onda 1 e 2** del piano di espansione (`PIANO_GUI.md`, ex `PIANO_GUI_ESPANSIONE.md`): progress bar con etichetta di batch, badge pianificazione, drag&drop su `PathBar`, export CSV, notifiche desktop a fine run, filtro Storico/Report, coda job durante un batch (F49), gestione credenziali dalla console (F56 metà GUI) — tutte chiuse.
- **D23** (3 Set): `--bandwidth-limit-mbps` falliva sempre contro `robocopy.exe` reale per il conflitto `/IPG`+`/MT` mai gestito in `build_args` — risolto omettendo `/MT` quando il limite di banda è impostato.
- **D24** (4 Set): `schedule.rs` faceva lampeggiare una console nera davanti alla GUI ad ogni `schtasks.exe` per un `CREATE_NO_WINDOW` mancante su due spawn — risolto.
- **Consolidamento documentale** (4 Set): `PIANO_GUI_TAURI.md` (il piano pre-implementazione) archiviato in `docs/archive/`, `PIANO_GUI_ESPANSIONE.md` rinominato `PIANO_GUI.md` — **da qui in avanti è l'unico piano attivo della console**, non duplicarlo né riaprire un terzo file.
- **Onda 3, metà GUI di F31** (4 Set): ripresa da checkpoint dalla console — `gui_api::list_checkpoints` scansiona la cartella del config per `*.checkpoint.json` (non calcola un percorso atteso), `resume_job` avvia `runner::resume_arguments`. Verificato contro un trasferimento reale interrotto a metà. **Trovato in verifica, non nel disegno**: la ripresa eredita solo pattern/thread/tentativi/verifica dell'interruzione, non il resto della configurazione originale — comportamento preesistente di `checkpoint::build_resume_args`, mai dichiarato prima d'ora → **D25, aperto, non bloccante** (l'asimmetria gioca a favore della sicurezza, mai verso il distruttivo).
- **Rifacimento visivo, tutti e tre i livelli** (4 Set, PIANO_GUI.md §10): Livello 1 (4/5 — contenimento layout, colonne tabella esplicite, badge di provenienza inline, traduzione di `integrity_status`, collegamento "Apri il report di questa run"; `exit_code_meaning` **non** tradotto di proposito, non è un enum chiuso), Livello 2 (`@lucide/svelte` per le icone — 0 vulnerabilità, tree-shaken — card di raggruppamento, scala tipografica, larghezza campi editor), Livello 3 (sidebar verticale al posto dei pulsanti in testa, finestra di apertura 1440×900, empty state con icona). **Bug trovato e corretto durante la verifica live**: una prima bozza derivava un'icona ✓/✗ per "Esito" in Report da `report.exit_code`, un campo che `ReportView` non espone affatto (solo `exit_code_meaning`, la stessa frase bitmask aperta già esclusa dalla traduzione al Livello 1) — icona sempre-falsa, rimossa.

**Il difetto più istruttivo resta D22** (2 Set, non di questa sessione, ma da rileggere prima di toccare la GUI): la console installata caricava il server di sviluppo invece del proprio frontend, e `cargo build`/`clippy`/tutti i test erano verdi, perché nessuno di loro apre una finestra. **Per una GUI non esiste sostituto all'aprire la finestra e cliccarci dentro** — ogni bug di questa sessione (D23, D24, il bug dell'icona morta) è stato trovato così, mai leggendo solo il diff.

Milestone 5.2.0/5.3.0/6.0.0/6.1.0/7.0.0 chiuse (7.0.0 a sette voci su otto: resta la metà in **scrittura** di F55 — script pre/post — e F57, i ruoli, fermo con raccomandazione esplicita di non farlo). Difetti storici: **D1-D25**, **un solo aperto (D25)**, non bloccante. Feature F1-F66 tutte classificate — **F62-F66 sono nuove**, aggiunte al backlog il 5 Set 2026 da un'analisi richiesta dall'utente su una metodologia a workspace e su funzionalità CLI non ancora valutate (`--list-schedules`, anteprima mirror/purge, anteprima restore, controllo spazio libero, preferiti nominati in GUI) — spec tecnica completa in `ROADMAP.md`, nessuna ancora implementata.

---

## 🎯 Obiettivo per la prossima sessione

Nessuna richiesta esplicita in sospeso all'apertura di questa sessione. Le aree con lavoro reale ancora da fare, in ordine di come il piano le prioritizza (`PIANO_GUI.md` §8/§11):

1. **Flusso di ripristino guidato** (`--restore-from`, Onda 3) — la lacuna funzionale più sentita della console. Richiede un disegno di conferma esplicita (elenco report → anteprima → conferma → avvio) prima di qualunque riga, stessa disciplina che ha retto il mirror. Proporre con `AskUserQuestion` prima di implementare.
2. **Due decisioni bloccate, entrambe spettano all'utente**:
   - Interruttore VSS in Modifica — serve prima `vss_snapshot: Option<bool>` su `JobConfig` lato core, non è lavoro di frontend.
   - Scrittura di webhook/script pre-post in Modifica (F55, metà scrittura) — morde il vincolo permanente 2 (§2.3 di `PIANO_GUI.md`): script configurabili + servizio privilegiato = escalation locale. Non procedere senza una decisione esplicita.
3. **D25** (`checkpoint::build_resume_args` scarta la maggior parte della configurazione originale) — aperto ma non bloccante. Il fix corretto è un tipo dedicato per il checkpoint, non allargare `ConfigurationReport` (condiviso con i report di run completate). Non affrontarlo con una patch rapida.
4. **F62-F66** (backlog, 5 Set 2026, spec completa in `ROADMAP.md`) — nessuno richiesto esplicitamente ancora, ma pronti se l'utente ne sceglie uno: `--list-schedules` (F62) e l'anteprima mirror/purge (F63) sono i più economici perché riusano quasi per intero logica già scritta e testata; l'anteprima di ripristino (F64) è il primo mattone del punto 1 sopra, ma richiede prima una verifica empirica se `--restore-from --dry-run` già funziona; il controllo spazio libero (F65) e i preferiti nominati in GUI (F66) sono indipendenti dal resto.

In assenza di una richiesta, il modello resta quello delle sessioni precedenti: **verificare empiricamente prima di proporre un fix**, mai fix speculativi su ipotesi non confermate — e, per qualunque cosa tocchi la GUI, **aprire la finestra** contro il binario release compilato, non fidarsi di `cargo build`/test/clippy da soli.

### Come procedere

1. Scegli 1-2 aree (dai punti sopra, da una richiesta esplicita dell'utente, o da una nuova lettura del codice/log reali) e **verifica empiricamente** prima di proporre un fix.
2. Per ogni bug reale confermato: fix + unit test + test black-box sul binario reale (vedi convenzioni sotto) + documentazione (`ANALYSIS.md` nuovo D-number se è un difetto, `CLAUDE.md` nota tecnica condensata secondo la convenzione B5b — vedi sotto).
3. Per le opportunità di performance: misura prima di ottimizzare — `scripts/benchmark-threads.ps1`/`scripts/analyze-runs.ps1` o i report in `_ops_reports/`.
4. `AskUserQuestion` prima di qualunque deviazione architetturale o scelta di scope ambigua.
5. Commit/push solo su richiesta esplicita dell'utente in quel turno — un "procedi" su un piano non autorizza automaticamente anche il commit, salvo lo abbia già fatto esplicitamente in quel turno.
6. Chiudi ogni giro con un `grep` dei conteggi test/difetti su tutti i file `.md` toccati — è già successo più volte che restassero disallineati (di nuovo in questa sessione: 422/437 erano rimasti fermi al 2 Set su 5 file diversi, e ROADMAP.md dichiarava ancora "nessun difetto aperto" con D25 già aperto da ore).

---

## Convenzioni stabilite nelle sessioni precedenti (da rispettare)

- **Test**: per ogni fix, unit test + almeno un test black-box che esegua il **binario compilato reale** (`tests/cli_smoke.rs` per `robocopy_ingest`, `tests/notify_server_e2e.rs` per `notify-server`), mai solo la funzione interna in isolamento. Verifica manuale contro file veri solo dentro `tempfile::tempdir()` isolate, mai contro cartelle reali.
- **GUI**: verifica visiva reale contro `target/release/rustcopy-gui.exe` (via Windows-MCP o equivalente), non solo `cargo build -p rustcopy-gui`/`clippy`/`npm run build` — nessuno di questi apre una finestra. `cargo build --release -p rustcopy-gui` **non** ricompila `robocopy_ingest.exe`/`notify-server.exe` (crate diversi nel workspace): se il fix tocca anche la CLI, ricompilarla separatamente.
- **Eccezione dichiarata**: quando un test toccherebbe stato di sistema reale fuori dal sandbox tempdir (VSS/F30, servizi Windows F37/F41: richiedono elevazione reale), non automatizzarlo nella suite normale — unit test sulla logica pura isolabile, test `#[ignore]` per il round-trip reale eseguibile a mano da un prompt elevato. F36 (Task Scheduler) è l'eccezione all'eccezione: un'attività per utente corrente non richiede elevazione, quindi è coperta da un vero test black-box **non** `#[ignore]`.
- **Deviazioni architetturali**: fermati e proponi con `AskUserQuestion` prima di implementare, non deciderle silenziosamente.
- **Commit/push**: mai senza richiesta esplicita dell'utente in quel turno. Mai direttamente su `main` — sempre un branch dedicato, PR, CI verde (o l'eccezione nota dell'outage npmjs.org su `gui-npm-audit`, verificato nei log del job prima di ignorarlo), merge.
- **Documentazione da aggiornare ad ogni fix chiuso, nello stesso giro**: `ANALYSIS.md`/`ROADMAP.md` (riga tabella), `CLAUDE.md` (nota tecnica **condensata secondo B5b** — solo prescrizione operativa + una riga di motivo + puntatore, mai narrazione completa), `AGENTS.md` (regole architetturali, conteggio test), `ARCHITECTURE.md` (tabella moduli, conteggio test), `README.md` (tabella flag CLI, conteggio test), `RUNBOOK.md` (esempio d'uso pratico se rilevante, conteggio test), `PIANO_GUI.md` (se il lavoro tocca la console), e questo file.
  - **Lezione confermata più volte, di nuovo in questa sessione**: aggiornare i conteggi test/difetti in **tutti** i file alla fine di ogni giro, non solo in alcuni — `grep` dei vecchi conteggi su tutti i file `.md` prima di chiudere.
  - **B5b**: `CLAUDE.md` accetta **solo** la prescrizione operativa per feature nuove — la narrazione completa (alternative valutate, limiti di test, cronologia) va in `ROADMAP.md`/`ANALYSIS.md`. Non violare questa disciplina: è la ragione per cui `CLAUDE.md` era arrivato a 50K caratteri.
- **Ricompilare dopo modifiche**: non automatico — `cargo build --release [--features notify-server] [-p rustcopy-gui]` e/o `ISCC.exe installer\rustcopy.iss` solo su richiesta esplicita o quando serve per una verifica visiva della GUI.
- **Config TOML**: quasi tutti i flag CLI recenti sono anche in `JobConfig`/`IngestConfig`. Eccezioni consapevoli: `--decrypt`, `--restore-from`, `--vss-snapshot`, `--resume-from`, `--force-purge`, `--exclude-junctions`, `--fast-verify`, `--html-report-path`, `--install-schedule`, `--install-service` (flag di sicurezza o CLI-only).
- **rtk**: attivo. `rtk gain` per verificare token risparmiati. Nessuna azione richiesta a inizio sessione. **Attenzione ai comandi con pipe lunga attraverso il wrapper `rtk`** (es. `rtk grep ... | head -N`): possono andare in timeout e restare appesi in background per ore senza produrre output se non ripuliti — controllare e terminare i task in background dimenticati a fine sessione.
- **CodeRabbit**: questo repo ha <10 stelle, quindi non riceve review automatiche — va attivata manualmente per ogni PR (checkbox "🔍 Trigger review" nel commento di CodeRabbit, via `gh api` PATCH sul commento). Ogni finding va verificato contro il codice reale prima di applicarlo, mai applicato ciecamente.
- **Monitor in background su `gh pr checks`**: la notifica di completamento non è sempre affidabile in questa sessione (successo più volte che la CI fosse verde da minuti prima che la notifica arrivasse) — se l'utente segnala che la CI sembra ferma, controllare subito con `gh pr checks <N>` invece di aspettare oltre.

## Cosa NON toccare senza motivo

- `engine::robocopy::build_args` non deve mai passare `/Z` (restartable mode) — costo prestazionale deliberatamente evitato sui file piccoli. Da D23: **omettere `/MT` per intero** (non solo ridurlo) quando `--bandwidth-limit-mbps` è impostato — `/IPG`+`/MT` insieme è un errore fatale di robocopy, non un degrado.
- `src/oem_codec.rs` non va sostituito con `encoding_rs::Encoding::for_label(b"ibm850")`.
- Ogni operazione bloccante su filesystem/processo in `main.rs` deve restare dentro `spawn_blocking_with_span` (D13) — **mai** `tokio::task::spawn_blocking` diretto, e mai chiamate sincrone dentro le `async fn` di orchestrazione.
- `main.rs::run_jobs` (F33) ricostruisce `Args` per ogni job da un clone dell'invocazione CLI originale, mai da `try_parse_from` né dall'`Args` già mergiato del job precedente — stessa disciplina in `restore::build_restore_args`, `checkpoint::build_resume_args`, `schedule::strip_schedule_flags`.
- `execute_generation_backup` (F34) non deve rientrare in `transfer()`/robocopy per incrementale/differenziale — `engine::naive::copy_selected` resta l'unica strada corretta.
- `GenerationManifest::generations_to_prune` (F35) ragiona per **ciclo**, mai per singola generazione.
- `src/service.rs` (F37/F41): `robocopy_ingest` e `notify-server` hanno **due identità di servizio separate** — non farne una sola.
- `scan::scan`/`scan::inventory` (D11) devono continuare a pruning via `WalkDir::filter_entry()`.
- `robocopy_ingest::atomic_write` (D14) resta il solo modo corretto per una riscrittura totale (cache fast-verify, e `GenerationManifest::save` usato solo dal pruning `--keep-generations`) — ma il caso comune di registrare una generazione va per `GenerationManifest::append_generation` (D19, NDJSON append-only), non più per `push`+`save`.
- `GenerationManifest::load_or_default` (D20) va usata **solo** dove serve davvero l'intera cronologia — oggi il solo secondo load di `prune_old_generations`, che riscrive il file.
- Ogni nuovo spawn di uno strumento a linea di comando in un processo GUI-avviato (`schtasks.exe`, `robocopy_ingest.exe`, ecc.) deve portare `creation_flags(CREATE_NO_WINDOW)` — D24 l'ha trovato mancante su due spawn in `schedule.rs`, dimenticati perché precedono l'esistenza della GUI.
- `checkpoint::build_resume_args` non eredita quasi nulla della configurazione originale oltre pattern/thread/tentativi/verifica (D25, aperto) — non presumere che una ripresa si comporti come la run interrotta su limite di banda, esclusioni, hash, mirror.
- **Console (GUI)**: `runner.rs`'s `run_arguments`/`resume_arguments` restano l'unico modo per costruire gli argomenti passati alla CLI — forma fissa, mai un parametro che inoltri flag arbitrari (il vincolo F61 che tiene fuori `--force-purge`/`--mirror`/install-service/install-schedule). `gui_api.rs` resta un involucro sottile sul core: se un comando Tauri cresce un ramo di giudizio sulla semantica di backup, quel ramo va spostato nel core.
- I file `.md` con frontmatter OKF tracciati da `scripts/okf-docs.sh` (18 totali: 13 in root + `docs/cli-reference.md` + `docs/installation.md` + 3 archiviati in `docs/archive/`, questi ultimi tutti `status: deprecated`) — un nuovo file `.md` permanente va aggiunto sia col frontmatter sia alla lista `TRACKED_DOCS` in `scripts/okf-docs.sh`, poi `scripts/okf-docs.sh index` per rigenerare gli indici.
- `.github/workflows/ci.yml` gira su `windows-latest` **e** `ubuntu-latest` — non rimuovere il job Linux (ha trovato D16).
- Dettaglio completo di ogni punto sopra: `CLAUDE.md` (condensato secondo B5b) rimanda a `ROADMAP.md`/`ANALYSIS.md`/`PIANO_GUI.md` per la narrazione estesa — consultarli lì, non aspettarsi di trovarla in `CLAUDE.md`.

## Skill disponibile per operare rustcopy

`.agents/skills/rustcopy-flow/` (+ copia globale `~/.claude/skills/rustcopy-flow/`) — compound skill per costruire/eseguire comandi rustcopy reali con dry-run e checkpoint umani obbligatori. Zero dipendenza MCP, per design (vedi `ROADMAP.md` F61 sul perché un server MCP è stato rimandato).
