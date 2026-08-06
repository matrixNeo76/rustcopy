# Prompt per la prossima sessione — robocopy-ingest-cli (rustcopy)

## Stato del progetto (6 Agosto 2026)

`Cargo.toml` = **6.0.0** (non ancora bumpato per D12, valutare se necessario). Milestone
5.2.0/5.3.0/6.0.0 chiuse. Milestone 6.1.0: solo F41 chiuso, F42-F45 in backlog (nessuna decisione
da riproporre — vedi `ROADMAP.md`). Difetti storici documentati: D1-D12, di cui solo D10
(strumentazione del grafo Graphify, bassa priorità) resta aperto — vedi `ANALYSIS.md` Parte 3.

Suite di test: **276** (`cargo test`), **291** con `cargo test --features notify-server` (più 2
test `#[ignore]` — round-trip reale dei servizi Windows, richiedono elevazione).

### Ultima modifica (6 Agosto 2026): D12 risolto

Audit di bug hunting mirato su questa sessione. Verificate empiricamente 3 delle 10 aree
ipotizzate nel prompt precedente:
- **Area 1 (unwrap/expect su input esterno)**: falso allarme, nessun panic raggiungibile da input
  non fidato nel codice di produzione.
- **Area 9 (nomi file riservati Windows)**: già gestito correttamente (retry esaurito → scansione
  reale della destinazione per il conteggio, nessun crash).
- **Area 6 (job multipli + file condivisi)**: bug reale confermato e risolto — vedi `ANALYSIS.md`
  D12. `cache::default_cache_path`/`GenerationManifest::path_for` erano derivati solo da `dest`,
  senza identità di job: due job dello stesso batch `[[jobs]]` con la stessa `dest` e
  `--backup-type` mescolavano le proprie cronologie di generazioni, con rischio concreto che
  `--keep-generations` cancellasse il `Full` di un job scambiandolo per "vecchio" rispetto ai
  cicli dell'altro. Fix: `job_name: Option<&str>` opzionale su entrambe le funzioni, namespacizzato
  via la nuova `robocopy_ingest::namespaced_path` (spostata da `main.rs` a `lib.rs`), valorizzato
  incondizionatamente da `run_jobs` per ogni job. Vedi `CLAUDE.md` per la nota tecnica completa.

Le aree 2, 3, 4, 5, 7, 8, 10 del prompt precedente **non sono ancora state verificate** — restano
valide ipotesi di lavoro per la prossima sessione (numerazione invariata, vedi sotto: 1/6/9 marcate
✅ chiuse).

---

## 🎯 Obiettivo di questa sessione: bug hunting, criticità, performance e robustezza

**Non proporre nuove feature dalla roadmap (F42-F45, milestone 7.0.0) come primo passo.** La
priorità di questa sessione è un **audit del codice Rust esistente**: trovare bug reali,
criticità di robustezza e opportunità di ottimizzazione delle prestazioni nell'applicativo che
wrappa `robocopy.exe` — non costruire altra tooling per agenti, non altra documentazione fine a
sé stessa. Ogni fix va verificato con test reali (unit + black-box sul binario compilato, come da
convenzione sotto), non solo letto/dedotto.

### Aree da investigare (punto di partenza, non esaustivo)

Queste sono ipotesi di lavoro basate sulla lettura del codice fatta finora, **da verificare
empiricamente prima di agire** — nessuna è confermata come bug reale:

1. ✅ **CHIUSA (6 Agosto 2026, falso allarme)** — ~~Panic/unwrap in percorsi raggiungibili da input
   esterno~~: verificato ogni `.unwrap()`/`.expect()` nel codice di produzione (fuori dai moduli
   test) in tutto `src/`. Restano solo invarianti già garantite da clap (`cli.rs:429,436`) e panic
   legittimi di avvio (init runtime tokio in `main.rs`, signal handler in `notify_server.rs`).
   Nessun panic raggiungibile da path/config/output di robocopy non fidato.
2. **Memoria su alberi da milioni di file**: `ScannedFile`/`ScanSummary` (`scan.rs`) tengono
   l'intero inventario in RAM come `Vec`; `GenerationManifest.files` (`generations.rs`) fa lo
   stesso per OGNI generazione salvata nel manifest JSON. Sul profilo utente reale testato in
   questa sessione (1.34M file) il JSON del manifest potrebbe diventare grande — verificare la
   dimensione reale su disco dopo alcune generazioni e se serve un formato più compatto o
   streaming invece di caricare tutto in memoria ad ogni run.
3. **Dimensionamento buffer/soglie hardcoded**: `engine/naive.rs::BUFFER_BYTES = 64 * 1024`,
   `crypto.rs` chunk da 1 MiB, `logging.rs` canale bounded a `10_000`, `integrity.rs`
   `MAX_REPORTED_ERRORS = 10_000` — sono scelte ragionevoli ma mai bench-marcate sistematicamente;
   valutare se vale la pena renderle configurabili o se ci sono evidenze (dai report reali in
   `_ops_reports/`) che vadano ritoccate.
4. **Default `--threads` = CPU logiche (spesso 48) su SMB/NAS**: il test reale di questa sessione
   ha mostrato throughput basso su NAS QNAP; capire se è un limite di rete/SMB del NAS o se
   `/MT:48` è contro-producente su quel tipo di destinazione, usando
   `scripts/benchmark-threads.ps1` già esistente per dati reali invece di ipotesi.
5. **Interazione fra `--fast-verify` e corruzione lato destinazione**: limite già documentato nel
   help text (`cache.rs` si fida di size+mtime della sorgente) — verificare se esiste un modo
   economico di rilevare almeno i casi più comuni di corruzione (es. campionamento periodico anche
   sui file "trusted" dalla cache) senza reintrodurre il costo pieno che `--fast-verify` voleva
   evitare.
6. ✅ **CHIUSA (D12, 6 Agosto 2026)** — ~~Concorrenza fra job multipli (`run_jobs`, F33) e file
   condivisi~~: confermato bug reale. `.ingest_cache`/`.rustcopy_generations.json` erano derivati
   solo da `dest`, senza identità di job — a differenza di `report_path`, mai namespacizzati per
   job. Fix: `job_name: Option<&str>` su `cache::default_cache_path`/
   `GenerationManifest::path_for`/`load_or_default`/`save`, valorizzato da `run_jobs` per ogni job.
   Vedi `ANALYSIS.md` D12.
7. **Gestione errori di rete SMB transitori**: `errors.rs::is_transient()` classifica cosa viene
   ritentato dal retry loop esterno — verificare che copra i codici di errore SMB/di rete più
   comuni osservati nei log reali di questa sessione (timeout, share temporaneamente non
   raggiudibile), non solo gli errori robocopy standard.
8. **`--resume-from` e file parzialmente scritti**: dato che non c'è mid-file resume (niente
   `/Z`), un file troncato a metà per un crash mid-write potrebbe essere visto da robocopy come
   "già presente" (size+timestamp) se il crash avviene dopo che l'OS ha già aggiornato i metadati
   ma prima di un flush completo — verificare se è un rischio reale o teorico.
9. ✅ **CHIUSA (6 Agosto 2026, comportamento già corretto)** — ~~Robustezza dei path Windows
   lunghi/con caratteri riservati~~: il caso reale `NUL`/`nul` è già gestito bene, non è un bug.
   robocopy fallisce quel file (atteso, nessun tool può copiare un device name riservato), il
   retry si esaurisce, e `main.rs` fa una scansione reale della destinazione (`scan::inventory`)
   per un conteggio byte/file accurato invece di fidarsi ciecamente dell'ultimo tentativo di
   retry. Non richiede ulteriore lavoro; non verificati altri nomi riservati (`CON`, `PRN`, `AUX`,
   `COM1-9`, `LPT1-9`) né edge case di path (trailing dot/space) in dettaglio — a basso rischio
   dato che il fallimento è comunque gestito correttamente a valle.
10. **Coerenza degli exit code fra le due pipeline** (plain-sync in `execute()` vs
    `execute_generation_backup`): verificare che ogni condizione di errore mappi allo stesso exit
    code indipendentemente dalla pipeline usata, non solo nei casi già testati.

### Come procedere

1. Scegli 2-3 aree dalla lista sopra (o trovane di nuove leggendo il codice) e **verifica
   empiricamente** se sono bug reali o falsi allarmi — non proporre fix per un problema non
   confermato.
2. Per ogni bug reale confermato: fix + unit test + test black-box sul binario reale (vedi
   convenzioni sotto) + documentazione (`ANALYSIS.md` nuovo `D12`, `CLAUDE.md` nota tecnica).
3. Per le opportunità di performance: **misura prima di ottimizzare** — usa
   `scripts/benchmark-threads.ps1`/`scripts/analyze-runs.ps1` già esistenti o i report in
   `_ops_reports/` per avere numeri reali, non stime.
4. Chiedi conferma con `AskUserQuestion` prima di qualunque deviazione architetturale, come da
   convenzione consolidata in questo progetto (vedi sotto).

---

## Convenzioni stabilite nelle sessioni precedenti (da rispettare)

- **Test**: per ogni fix, unit test + almeno un test black-box che esegua il **binario compilato
  reale** (`tests/cli_smoke.rs` per `robocopy_ingest`, `tests/notify_server_e2e.rs` per
  `notify-server`), mai solo la funzione interna in isolamento. Qualsiasi verifica manuale contro
  file veri va fatta solo dentro `tempfile::tempdir()` isolate, mai contro cartelle reali.
- **Eccezione dichiarata**: quando un test toccherebbe stato di sistema reale al di fuori del
  sandbox tempdir (es. F30/VSS: creare/cancellare una vera shadow copy; F37/F41 servizi Windows:
  `CreateService`/`StartService`/`DeleteService` richiedono elevazione reale), non automatizzarlo
  nella suite normale — copri la logica pura isolabile con unit test, e per un round-trip reale
  aggiungi un test `#[ignore]` eseguibile a mano da un prompt elevato (`cargo test -- --ignored`).
  F36 (Task Scheduler) resta l'eccezione all'eccezione: creare un'attività *per utente corrente*
  non richiede elevazione, quindi è coperta da un vero test black-box **non** `#[ignore]`.
- **Deviazioni architetturali**: quando l'implementazione più ovvia è più rischiosa/complessa di
  un'alternativa equivalente, fermati e proponi la deviazione con `AskUserQuestion` prima di
  implementare — non deciderla silenziosamente.
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
  - **Lezione confermata più volte**: aggiornare i conteggi test/difetti in **tutti** i file alla
    fine di ogni giro, non solo in alcuni — è già successo che numeri storici (conteggio test,
    conteggio difetti D1-D10 vs D1-D11) restassero disallineati tra `ANALYSIS.md`, `ROADMAP.md` e
    `CLAUDE.md` dopo un fix. Prima di chiudere un giro: `grep` dei vecchi conteggi su tutti i file
    `.md`, e rilettura della sezione "prossimo passo" di questo file.
- **Ricompilare dopo modifiche**: se l'utente chiede di usare un binario aggiornato,
  `cargo build --release` (aggiungere `--features notify-server` se serve anche quel binario) non
  è automatico — va lanciato esplicitamente.
- **Config TOML**: quasi tutti i flag CLI recenti sono ormai presenti anche in `JobConfig`/
  `IngestConfig` (`src/config.rs`). Eccezioni consapevoli e già accettate: `--decrypt`,
  `--restore-from`, `--vss-snapshot`, `--resume-from`, `--force-purge`, `--exclude-junctions`,
  `--fast-verify`, `--html-report-path`, `--install-schedule`, `--install-service` (flag di
  sicurezza o CLI-only, volutamente assenti dal TOML).
- **rtk**: attivo e confermato funzionante dal 6 Agosto 2026 (`rtk gain` mostra comandi tracciati
  e token risparmiati). Hook globale in `~/.claude/settings.json`, inizializzato via `rtk init -g`.
  Nessuna azione richiesta a inizio sessione oltre una verifica rapida con `rtk gain`.

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
  `incremental`/`differential` successivi), mai per singola generazione.
- `src/hooks.rs` (F39): un `--pre-command` fallito deve sempre abortire il job prima di qualunque
  copia; un `--post-command` fallito non deve mai far fallire un backup già riuscito.
- `src/schedule.rs` (F36): il comando pianificato (`/TR`) va costruito dall'argv **reale**
  dell'invocazione (`strip_schedule_flags`), mai da una ricostruzione sintetica di `Args`.
- `src/service.rs` (F37/F41): generico, parametrizzato per nome/display-name — non reintrodurre
  costanti hardcoded al posto dei parametri. `robocopy_ingest` e `notify-server` hanno **due
  identità separate** (`"RustcopyIngestService"` / `"RustcopyNotifyServer"`) — non farne una sola.
- `notify_server::serve_until_shutdown` (percorso foreground normale) non va toccata per aggiungere
  il segnale SCM — quello è `serve_until_shutdown_or` (F41), una funzione separata.
- `scan::scan`/`scan::inventory` (D11) devono continuare a pruning via `WalkDir::filter_entry()` —
  non tornare a un filtro post-hoc dopo aver già camminato l'albero.
- `robocopy_ingest::namespaced_path` (D12) vive in `lib.rs`, non più duplicata in `main.rs` —
  riusata da `run_jobs` (report), `cache::default_cache_path` e
  `generations::GenerationManifest::path_for`. `Args::job_name` è interno (`#[arg(skip)]`, mai un
  flag CLI reale) e va valorizzato **incondizionatamente** da `run_jobs` per ogni job (a differenza
  di `report_path`, cache/manifest non hanno un campo di config utente da rispettare prima).

## Skill disponibile per operare rustcopy

`.agents/skills/rustcopy-flow/` (+ copia globale `~/.claude/skills/rustcopy-flow/`) — compound
skill per costruire/eseguire comandi rustcopy reali con dry-run e checkpoint umani obbligatori.
Utile per generare i dati reali richiesti dal punto "Come procedere" sopra (benchmark, riproduzione
di un bug su un caso reale), non per il lavoro di audit del codice in sé.
