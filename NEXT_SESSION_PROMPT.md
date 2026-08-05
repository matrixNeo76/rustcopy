# Prompt per la prossima sessione — robocopy-ingest-cli (rustcopy)

Riprendi il lavoro su robocopy-ingest-cli (rustcopy). Stato: `Cargo.toml` = **6.0.0**, ultimo
commit pushato su `main` (pulito, nessuna modifica in sospeso). Milestone 5.2.0 (Correttezza) e
5.3.0 (Operabilità) sono chiuse. **Milestone 6.0.0 (Backup Enterprise) è chiusa** (5 Agosto 2026):
F30 (VSS), F31 (checkpoint/resume), F33 (profili job multipli `[[jobs]]`), F34
(Full+Incrementale+Differenziale), F35 (ritenzione/rotazione per cicli, `--keep-generations`), F36
(scheduler via Task Scheduler, `--install-schedule`/`--uninstall-schedule`), F37 (servizio Windows
reale via SCM, infrastruttura minima e volutamente inattiva) e F39 (comandi pre/post job,
`--pre-command`/`--post-command`).

Suite di test: **265** (`cargo test`), **278** con `cargo test --features notify-server`.

## Decisione presa il 5 Agosto 2026: F32/F38/F40 rimandati al backlog

L'utente ha espresso perplessità sulla loro utilità attuale ed è stato deciso di **rimandarli**
esplicitamente (vedi `ROADMAP.md`, sezione `## 🗄️ Backlog non vincolato a una milestone`), non
implementarli "a vuoto":
- F32 (metriche Prometheus) ha senso solo con qualcosa di continuativo da monitorare — con solo
  Task Scheduler (F36, nessun processo persistente) e un servizio Windows ancora inattivo (F37),
  manca un target concreto.
- F38 (compressione zip/7z) aggiunge complessità reale (interazione con `--verify-integrity` e
  cifratura) per un beneficio non ovvio senza un caso d'uso specifico.
- F40 (cloud/FTP/SFTP reale) è scritto in modo troppo generico per essere implementabile senza
  sapere quale provider/protocollo serve davvero.

**Prossimo passo da proporre all'utente**: procedere con **F41** (notify-server persistente,
milestone 6.1.0, per cui F37 ha già posto le basi infrastrutturali — un servizio Windows reale ma
ancora inattivo) oppure l'inizio della milestone **7.0.0** ("Motore Controllabile"). Non dare per
scontato quale dei due: chiedi conferma prima di iniziare, come fatto per le decisioni
architetturali precedenti.

## Convenzioni stabilite nelle sessioni precedenti (da rispettare)

- **Test**: per ogni fix, unit test + almeno un test black-box che esegua il **binario compilato
  reale** (`tests/cli_smoke.rs`), mai solo la funzione interna in isolamento. Qualsiasi verifica
  manuale contro file veri va fatta solo dentro `tempfile::tempdir()` isolate, mai contro cartelle
  reali.
- **Eccezione dichiarata**: quando un test toccherebbe stato di sistema reale al di fuori del
  sandbox tempdir (es. F30/VSS: creare/cancellare una vera shadow copy; F37/servizio Windows:
  `CreateService`/`StartService`/`DeleteService` richiedono elevazione reale), non automatizzarlo —
  copri solo la logica pura isolabile e dichiara esplicitamente il limite nei commenti/doc invece
  di fingere copertura completa. F36 (Task Scheduler) è l'eccezione all'eccezione: la creazione di
  un'attività Task Scheduler *per utente corrente* non richiede elevazione, quindi è coperta da un
  vero test black-box con round-trip install→uninstall contro `schtasks.exe` reale (con cleanup
  `Drop`-based best-effort).
- **Deviazioni dal testo della roadmap**: quando l'implementazione letterale della roadmap è più
  rischiosa/complessa di un'alternativa equivalente, fermati e proponi la deviazione con
  `AskUserQuestion` prima di implementare — non deciderla silenziosamente. Esempi già avvenuti così
  in questo progetto: F34 (cartelle di generazione vs. manifest+destinazione singola), F35
  (rotazione per ciclo vs. per singola generazione — per non orfanare una catena
  incrementale/differenziale), F36 (scheduler leggero via `schtasks.exe` vs. scheduler interno al
  servizio — decisione che ha esplicitamente disaccoppiato F36 da F37), F37 (scope minimo:
  infrastruttura SCM idle, nessun `--service-name`, comportamento reale rimandato a F41).
- **Commit/push**: mai senza richiesta esplicita dell'utente in quel turno. Un "ok procedi" su un
  piano non autorizza automaticamente anche il commit.
- **Documentazione da aggiornare ad ogni fix chiuso, nello stesso giro**: `ANALYSIS.md`/
  `ROADMAP.md` (riga della tabella del task), `CLAUDE.md` (nota tecnica per i futuri agenti),
  `AGENTS.md` (regole architetturali, albero directory, conteggio test), `ARCHITECTURE.md`
  (tabella moduli, diagrammi Mermaid, conteggio test), `README.md` (tabella flag CLI, conteggio
  test), `RUNBOOK.md` (esempio d'uso pratico se il task introduce un flusso operativo nuovo,
  conteggio test), e **questo file**. Sono in italiano (README/ARCHITECTURE/ANALYSIS/ROADMAP/
  RUNBOOK/NEXT_SESSION_PROMPT), tranne CLAUDE.md e AGENTS.md che sono in inglese; codice/commenti/
  commit in inglese.
  - **Lezione dalla review del 5 Agosto 2026**: aggiornare i conteggi test e le tabelle moduli in
    **tutti** i file alla fine di ogni feature chiusa, non solo in alcuni — un giro di
    implementazione (F39+F36) aveva aggiornato AGENTS.md/ARCHITECTURE.md/README.md ma lasciato
    stale una riga in RUNBOOK.md (cross-reference ad ANALYSIS.md), e questo file
    (NEXT_SESSION_PROMPT.md) non era mai stato toccato nei 3 commit successivi al suo ultimo
    aggiornamento. Prima di chiudere un giro, fai un `grep` dei vecchi conteggi test su tutti i
    file `.md` per assicurarti di aver aggiornato ogni occorrenza.
- **Ricompilare dopo modifiche**: se l'utente chiede di usare un binario aggiornato,
  `cargo build --release` non è automatico — va lanciato esplicitamente.
- **Config TOML**: quasi tutti i flag CLI recenti sono ormai presenti anche in `JobConfig`/
  `IngestConfig` (`src/config.rs`) — mantenere questa parità quando si aggiungono nuovi flag
  rilevanti per un job pianificato. Eccezioni consapevoli e già accettate: `--decrypt`,
  `--restore-from`, `--vss-snapshot`, `--resume-from`, `--force-purge` (flag di sicurezza o d'uso
  non ricorrente, volutamente assenti dal TOML — `--force-purge` in particolare non deve mai
  diventare settabile silenziosamente da un file di config, dato che disattiva una conferma di
  eliminazione).

## Cosa NON toccare senza motivo

- `engine::robocopy::build_args` non deve mai passare `/Z` (restartable mode) — costo prestazionale
  deliberatamente evitato sui file piccoli.
- `src/oem_codec.rs` non va sostituito con `encoding_rs::Encoding::for_label(b"ibm850")`.
- `check_mirror_safety`/`VssGuard`/`prune_old_generations` e ogni operazione bloccante su
  filesystem/processo in `main.rs` devono restare dentro `tokio::task::spawn_blocking` — mai
  chiamate sincrone dentro le `async fn` di orchestrazione.
- `main.rs::run_jobs` (F33) ricostruisce `Args` per ogni job da un **clone dell'invocazione CLI
  originale**, mai da `try_parse_from` né dall'`Args` già mergiato del job precedente.
- `execute_generation_backup` (F34) non deve essere fatto rientrare in `transfer()`/robocopy per i
  casi incrementale/differenziale: robocopy seleziona i file per pattern/nome, non per un elenco
  arbitrario di percorsi relativi. Il motore naive (`engine::naive::copy_selected`) resta l'unica
  strada corretta.
- `GenerationManifest`/`Generation.files` deve sempre contenere l'inventario **completo** della
  sorgente al momento del run, non solo i file effettivamente copiati.
- `GenerationManifest::generations_to_prune` (F35) ragiona per **ciclo** (un `full` + i suoi
  `incremental`/`differential` successivi), mai per singola generazione — altrimenti si rischia di
  eliminare un `full` ancora referenziato da una generazione mantenuta, rompendo la catena di
  ripristino.
- `src/hooks.rs` (F39): un `--pre-command` fallito deve sempre abortire il job prima di qualunque
  copia; un `--post-command` fallito non deve mai far fallire un backup già riuscito.
- `src/schedule.rs` (F36): il comando pianificato (`/TR`) va costruito dall'argv **reale**
  dell'invocazione (`strip_schedule_flags`), mai da una ricostruzione sintetica di `Args` — stessa
  lezione di F25b applicata in un contesto diverso.
- `src/service.rs` (F37): l'identità di servizio (`SERVICE_NAME`) è fissa e condivisa tra
  `install()`, `uninstall()` e la registrazione del control handler in `run_service_dispatcher()` —
  deve restare **esattamente** la stessa stringa nei tre punti, altrimenti SCM non riesce a
  instradare i controlli al servizio. `main()` deve continuare a controllare
  `service::is_service_launch()` **prima** di costruire il runtime tokio, non dopo.

## Raccomandazioni strategiche (da review architetturale agosto 2026)

- **Consolidamento documentazione ad ogni giro**: la review del 5 Agosto 2026 ha trovato
  disallineamenti reali (conteggi test stale in un file su cinque, questo file mai aggiornato per
  3 commit) nonostante la disciplina generale sia buona — vedi la lezione sopra su come evitarlo.
- **Version bump fatto**: `Cargo.toml` è a `6.0.0` (bump deciso il 5 Agosto 2026, a chiusura
  effettiva della milestone). Il prossimo bump segue la stessa logica: incrementale durante il
  lavoro, salto di milestone alla sua chiusura.
