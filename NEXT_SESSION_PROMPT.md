# Prompt per la prossima sessione — robocopy-ingest-cli (rustcopy)

Riprendi il lavoro su robocopy-ingest-cli (rustcopy). Stato: `Cargo.toml` = **6.0.0**, ultimo
commit pushato su `main` (pulito, nessuna modifica in sospeso). Milestone 5.2.0 (Correttezza) e
5.3.0 (Operabilità) sono chiuse. **Milestone 6.0.0 (Backup Enterprise) è chiusa** (5 Agosto 2026):
F30 (VSS), F31 (checkpoint/resume), F33 (profili job multipli `[[jobs]]`), F34
(Full+Incrementale+Differenziale), F35 (ritenzione/rotazione per cicli, `--keep-generations`), F36
(scheduler via Task Scheduler, `--install-schedule`/`--uninstall-schedule`), F37 (servizio Windows
reale via SCM, infrastruttura minima e volutamente inattiva) e F39 (comandi pre/post job,
`--pre-command`/`--post-command`).

**Milestone 6.1.0 (Notifiche avanzate)**: F41 (notify-server come servizio Windows persistente
separato, `"RustcopyNotifyServer"`) è chiuso (5 Agosto 2026). Restano aperti: F42 (coda persistente
+ retry di consegna), F43 (`TelegramSink`), F44 (`EmailSink`/SMTP), F45 (priorità/tag nel payload).

Suite di test: **269** (`cargo test`), **284** con `cargo test --features notify-server` (più 2 test
`#[ignore]` — round-trip reale dei servizi Windows, richiedono elevazione, si eseguono a mano con
`cargo test -- --ignored` o con `scripts/verify-services.ps1`, non contano nel totale "passed").

## Decisioni prese il 5 Agosto 2026 (non riproporle come aperte)

- **F32/F38/F40 rimandati al backlog** (vedi `ROADMAP.md`, sezione
  `## 🗄️ Backlog non vincolato a una milestone`) — non erano bloccanti e mancava un caso d'uso
  concreto (F32: nessun processo continuativo da monitorare finché F41 non ha dato lavoro reale al
  servizio; F38: complessità reale per beneficio non ovvio; F40: troppo generico senza un
  provider/protocollo specifico).
- **Milestone 7.0.0 (Motore controllabile) rimandata al backlog** (F46-F51) — cambia la natura del
  prodotto da CLI a strumento interattivo, senza un bisogno concreto oggi. Non riproporla come
  scelta aperta finché l'utente non segnala un bisogno reale di interattività.
- **F41 completato**: `notify-server.exe` ha una propria identità di servizio Windows
  (`"RustcopyNotifyServer"`), separata da quella idle di `robocopy_ingest` (F37,
  `"RustcopyIngestService"`) — decisione presa esplicitamente per non far dipendere il binario
  `robocopy_ingest` di default da axum (vedi `AGENTS.md` regola 8/14).
- **D11 corretto (5 Agosto 2026)**: `scan::scan`/`scan::inventory` (prescan interno) ignoravano
  `exclude_dirs`/`exclude_files` — arrivavano solo a `engine/robocopy.rs` per `/XD`/`/XF` del vero
  trasferimento, mai al prescan. Scoperto dall'utente durante un test di backup dell'intero profilo
  (`C:\Users\auresystem`, ~995GB) con `exclude_dirs = ["AppData", ".ollama", "OneDrive"]`: il
  dry-run continuava a leggere le cartelle escluse. Fix via `WalkDir::filter_entry()` in tutti i
  call site (`main.rs::inventory_source`/`check_mirror_safety`/riconciliazione post-`CopyFailed`,
  `engine/naive.rs`). Dettagli in `ANALYSIS.md` D11 e `CLAUDE.md`. **Non ancora fatto**: commit/push
  di questo fix (richiede conferma esplicita in un turno futuro) e verifica del risultato del
  dry-run reale in background sul profilo completo — se non ancora riportato all'utente, farlo alla
  prossima occasione. **Gap parallelo noto e non corretto**: `--min-age-days`/`--max-age-days` hanno
  la stessa lacuna strutturale in `scan.rs`, lasciata come follow-up.

**Prossimo passo da proporre all'utente**: nessuna decisione predefinita. Le opzioni naturali sono
continuare la milestone 6.1.0 (F42/F43/F44/F45, tutti isolati tra loro) oppure altro su richiesta
dell'utente — chiedi conferma prima di iniziare, come fatto per le decisioni precedenti.

## Convenzioni stabilite nelle sessioni precedenti (da rispettare)

- **Test**: per ogni fix, unit test + almeno un test black-box che esegua il **binario compilato
  reale** (`tests/cli_smoke.rs` per `robocopy_ingest`, `tests/notify_server_e2e.rs` per
  `notify-server`), mai solo la funzione interna in isolamento. Qualsiasi verifica manuale contro
  file veri va fatta solo dentro `tempfile::tempdir()` isolate, mai contro cartelle reali.
- **Eccezione dichiarata**: quando un test toccherebbe stato di sistema reale al di fuori del
  sandbox tempdir (es. F30/VSS: creare/cancellare una vera shadow copy; F37/F41 servizi Windows:
  `CreateService`/`StartService`/`DeleteService` richiedono elevazione reale), non automatizzarlo
  nella suite normale — copri la logica pura isolabile con unit test, e per un round-trip reale
  aggiungi un test `#[ignore]` eseguibile a mano da un prompt elevato (`cargo test -- --ignored`),
  come fatto per F37/F41 (`install_and_uninstall_service_round_trip` in entrambi i file di test) e
  documentato in `scripts/verify-services.ps1`/`RUNBOOK.md`. F36 (Task Scheduler) resta l'eccezione
  all'eccezione: la creazione di un'attività Task Scheduler *per utente corrente* non richiede
  elevazione, quindi è coperta da un vero test black-box **non** `#[ignore]`, con round-trip
  install→uninstall contro `schtasks.exe` reale (cleanup `Drop`-based best-effort).
- **Deviazioni dal testo della roadmap**: quando l'implementazione letterale della roadmap è più
  rischiosa/complessa di un'alternativa equivalente, fermati e proponi la deviazione con
  `AskUserQuestion` prima di implementare — non deciderla silenziosamente. Esempi già avvenuti così
  in questo progetto: F34 (cartelle di generazione vs. manifest+destinazione singola), F35
  (rotazione per ciclo vs. per singola generazione — per non orfanare una catena
  incrementale/differenziale), F36 (scheduler leggero via `schtasks.exe` vs. scheduler interno al
  servizio — decisione che ha esplicitamente disaccoppiato F36 da F37), F37 (scope minimo:
  infrastruttura SCM idle, nessun `--service-name`), F41 (notify-server con identità di servizio
  propria invece di far ospitare axum al servizio idle di `robocopy_ingest`, per non violare la
  regola "notify-server resta feature-gated").
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
  - **Lezione dalla review del 5 Agosto 2026, confermata due volte**: aggiornare i conteggi test e
    le tabelle moduli in **tutti** i file alla fine di ogni feature chiusa, non solo in alcuni. È
    già successo due volte nella stessa giornata che questo file specifico (NEXT_SESSION_PROMPT.md)
    restasse stale dopo un giro di lavoro (una volta per i conteggi test, una volta per il
    contenuto narrativo — descriveva F41 come "da proporre" quando era già stato implementato).
    Prima di chiudere un giro: (1) fai un `grep` dei vecchi conteggi test su tutti i file `.md`,
    (2) rileggi la sezione "prossimo passo"/"decisione in sospeso" di questo file e verifica che
    non stia ancora presentando come aperta una decisione già presa in questo stesso giro.
- **Ricompilare dopo modifiche**: se l'utente chiede di usare un binario aggiornato,
  `cargo build --release` (aggiungere `--features notify-server` se serve anche quel binario) non
  è automatico — va lanciato esplicitamente.
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
- `src/service.rs` (F37/F41): è ormai **generico**, parametrizzato per nome/display-name
  (`install_named`/`uninstall_named`/`start_dispatcher`/`register_and_wait_for_stop`) — non
  reintrodurre costanti hardcoded al posto dei parametri. Per ciascuna identità di servizio, il
  nome passato a `install_named`/`uninstall_named` e quello usato nella registrazione del control
  handler (dentro il `service_main` bound via `define_windows_service!`) devono restare
  **esattamente** la stessa stringa, altrimenti SCM non riesce a instradare i controlli al
  servizio. `robocopy_ingest` e `notify-server` hanno **due identità separate**
  (`"RustcopyIngestService"` / `"RustcopyNotifyServer"`) — non farne una sola. Entrambi i `main()`
  devono continuare a controllare `service::is_service_launch()` **prima** di costruire il runtime
  tokio, non dopo (vedi `AGENTS.md` regola 13).
- `notify_server::serve_until_shutdown` (percorso foreground normale) non va toccata per aggiungere
  il segnale SCM — quello è `serve_until_shutdown_or` (F41), una funzione separata apposta per non
  rischiare la funzione già testata.

## Raccomandazioni strategiche (da review architetturale agosto 2026)

- **Consolidamento documentazione ad ogni giro**: vedi la lezione sopra, confermata due volte nella
  stessa giornata — non è un rischio teorico, è già successo.
- **Version bump fatto**: `Cargo.toml` è a `6.0.0` (bump deciso il 5 Agosto 2026, a chiusura
  effettiva della milestone 6.0.0). Il prossimo bump segue la stessa logica: incrementale durante
  il lavoro, salto di milestone alla sua chiusura — non è stato rifatto per F41 (milestone 6.1.0
  ancora aperta con F42-F45).
