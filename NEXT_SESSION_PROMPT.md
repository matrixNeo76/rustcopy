# Prompt per la prossima sessione — robocopy-ingest-cli (rustcopy)

## Stato del progetto (6 Agosto 2026, dopo il giro di audit D13-D15)

`Cargo.toml` = **6.0.0**. Ultimo commit pushato su `main`: `625426f` (fix D12). **Modifiche non
ancora committate in questa sessione** (fix D13, D14, D15, vedi sotto) — chiedere conferma esplicita
all'utente prima di commit/push, come da convenzione. Milestone 5.2.0/5.3.0/6.0.0 chiuse. Milestone
6.1.0: solo F41 chiuso, F42-F45 in backlog (nessuna decisione da riproporre — vedi `ROADMAP.md`).
Difetti storici documentati: **D1-D15**, di cui solo D10 (strumentazione del grafo Graphify, bassa
priorità) resta aperto — vedi `ANALYSIS.md` Parte 3.

Suite di test: **284** (`cargo test`), **299** con `cargo test --features notify-server` (più 3
test `#[ignore]` — 2 round-trip reali dei servizi Windows che richiedono elevazione, più 1 probe di
misurazione a scala reale, `generations::tests::probe_manifest_size_at_real_world_scale`, ~2 minuti,
non da eseguire di default). Binari release e installer **non ancora ricompilati** dopo i fix di
questo giro — farlo solo su richiesta esplicita dell'utente (vedi convenzioni sotto).

**Cronologia audit di bug hunting** (per contesto, non da rileggere per intero — dettagli in
`ANALYSIS.md`): un giro precedente aveva verificato empiricamente 3 ipotesi — 2 falsi allarmi
(unwrap/expect su input esterno; nomi file riservati Windows tipo `NUL`, già gestiti correttamente
a valle) e 1 bug reale confermato e risolto (**D12**: cache/manifest non namespacizzati per job).
Il giro successivo ha chiuso altre 2 ipotesi ereditate (checkpoint namespacing → falso allarme;
correlazione log per job → **D13**, bug reale, span `tracing` + `spawn_blocking_with_span`).

**Questo giro** ha verificato empiricamente **tutte le 7 ipotesi rimaste** (elencate in una
versione precedente di questo file), usando anche i log operativi reali in `_ops_reports/` (19 file,
incluso il profilo reale da 1.340.613 file in `full-profile-test.json`) invece di sole ipotesi da
lettura del codice. Esito:

- **Confermate come bug reali e risolte**:
  - **D14** — `GenerationManifest::save`/`IngestCache::save_to` scrivevano con un `fs::write` non
    atomico; un manifest a scala reale (1.34M file) serializza a ~174 MB per generazione, ~872 MB
    con 5 generazioni trattenute — un crash a metà scrittura corrompe il file, e per il manifest
    questo è **fatale** (rompe ogni backup futuro contro quella destinazione). Fix: nuova
    `robocopy_ingest::atomic_write` (temp file + rename, stesso pattern di `crypto.rs`), usata da
    entrambi. Vedi `ANALYSIS.md` D14 e la nota tecnica in `CLAUDE.md`.
  - **D15** — un fallimento di copia in `execute_generation_backup` (`--backup-type`) mappava a
    `EXIT_UNRECOVERABLE` (2, lo stesso di un errore di configurazione) invece di
    `EXIT_INGESTION_PROBLEM` (1, come la pipeline plain-sync per lo stesso genere di fallimento), e
    non scriveva alcun report. Fix (scope deciso con `AskUserQuestion`, **senza** toccare il motore
    naive per tracciare conteggi parziali): errore catturato, nuovo campo
    `IngestReport::copy_error`, report sempre scritto, exit code corretto. Vedi `ANALYSIS.md` D15 e
    la nota tecnica in `CLAUDE.md`.
- **Verificate ma senza fix, nessuna evidenza reale a supporto** (dettagli completi in `ANALYSIS.md`,
  aggiornamento del 6 Agosto 2026 in cima alla Parte 3 — non da riaprire senza nuova evidenza):
  - Buffer/soglie hardcoded (`BUFFER_BYTES`, chunk crypto, canale logging, `MAX_REPORTED_ERRORS`):
    nessuna evidenza nei report reali che siano mal dimensionati.
  - `--threads` su NAS/SMB: i log reali non contengono un confronto A/B pulito (stesso file-set,
    stessa destinazione, thread diversi, transfer reale non dry-run) — il throughput basso osservato
    (2.7-8.3 MB/s sia su fileserv01 sia su QNAP) è più coerente con overhead per-file su tanti file
    piccoli che con un problema di thread-count.
  - `--fast-verify` + corruzione lato destinazione: trade-off già dichiarato deliberatamente in
    help text/CLAUDE.md; una mitigazione (campionamento periodico) sarebbe una nuova feature, non
    un bug fix — da valutare come proposta separata, non come correzione.
  - `errors.rs::is_transient()`/errori SMB transitori: nessun codice di errore di rete reale mai
    osservato nei 19 log operativi (solo `ERRORE 5`, già ricondotto al caso noto dei nomi
    riservati). Trovato un limite teorico (l'exit code 16 "fatal" di robocopy potrebbe derivare da
    un'irraggiungibilità transitoria della destinazione, non solo da un errore di configurazione
    permanente) ma non corretto: renderlo retryable rischierebbe di nascondere più a lungo un vero
    errore di configurazione, senza un solo caso reale osservato a giustificarlo.
  - `--resume-from` e file troncati: rischio teorico, non riproducibile con il normale
    comportamento di scrittura sequenziale (un file troncato ha size diversa dalla sorgente, quindi
    robocopy lo ricopia comunque al prossimo run).

**Non ci sono più ipotesi ereditate da riprendere all'inizio della prossima sessione** — il prossimo
giro di audit deve partire da una nuova lettura del codice/dei log reali, non da questa lista.

---

## 🎯 Obiettivo per la prossima sessione: bug hunting, criticità, performance e robustezza

**Non proporre nuove feature dalla roadmap (F42-F45, milestone 7.0.0) come primo passo.** La
priorità resta continuare l'**audit del codice Rust esistente**: trovare bug reali, criticità di
robustezza e opportunità di ottimizzazione delle prestazioni nell'applicativo che wrappa
`robocopy.exe` — non costruire altra tooling per agenti, non altra documentazione fine a sé stessa.
Ogni fix va verificato con test reali (unit + black-box sul binario compilato, come da convenzione
sotto), non solo letto/dedotto.

### Punti di partenza per la prossima sessione (non esaustivo)

Non ci sono ipotesi ereditate aperte. Spunti concreti emersi durante questo giro ma **non
esplorati**, utili come punto di partenza (nessuno è confermato — verificare empiricamente prima di
agire, stesso discorso di sempre):

1. **`GenerationManifest`/`ScanSummary` tengono l'intero inventario in RAM come `Vec` e lo
   serializzano/deserializzano per intero ad ogni run** — D14 ha chiuso il rischio di corruzione
   sulla scrittura, ma non la domanda architetturale più ampia: a 174-872 MB per file, vale la pena
   un formato streaming/incrementale (es. NDJSON append-only, o una history compattata che non
   ripeta l'inventario completo ad ogni generazione) invece di un unico JSON riscritto per intero
   ad ogni run? Richiede una decisione di design (probabilmente `AskUserQuestion`), non un fix
   meccanico.
2. **`engine::naive::copy_files` non traccia progresso parziale su fallimento** (la ragione per cui
   D15 non ha potuto arricchire il report della pipeline a generazioni con conteggi accurati) — se
   diventa prioritario avere un report accurato anche sui fallimenti parziali di `--backup-type`,
   questo è il punto da cui ripartire.
3. Le 5 aree "verificate senza fix" sopra restano scenari plausibili se emergono nuove evidenze
   reali (nuovi log in `_ops_reports/`, un incidente riportato dall'utente) — non riaprirle senza
   quello.

### Come procedere

1. Scegli 2-3 aree (dai punti sopra o da una nuova lettura del codice/dei log reali) e **verifica
   empiricamente** se sono bug reali o falsi allarmi — non proporre fix per un problema non
   confermato. I log operativi reali in `_ops_reports/` (19 file, incluso un profilo da 1.34M file)
   sono spesso più utili di ipotesi da lettura del codice — usali prima di ipotizzare.
2. Per ogni bug reale confermato: fix + unit test + test black-box sul binario reale (vedi
   convenzioni sotto) + documentazione (`ANALYSIS.md` nuovo `D16`, `CLAUDE.md` nota tecnica).
3. Per le opportunità di performance: **misura prima di ottimizzare** — usa
   `scripts/benchmark-threads.ps1`/`scripts/analyze-runs.ps1` già esistenti o i report in
   `_ops_reports/` per avere numeri reali, non stime.
4. Chiedi conferma con `AskUserQuestion` prima di qualunque deviazione architetturale o scelta di
   scope ambigua, come da convenzione consolidata in questo progetto (vedi sotto).
5. Se l'utente chiede un binario/installer aggiornato: `cargo build --release --features
   notify-server` poi `ISCC.exe installer\rustcopy.iss` — non automatico, va fatto solo su
   richiesta esplicita (vedi convenzioni sotto).

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
    conteggio difetti) restassero disallineati tra `ANALYSIS.md`, `ROADMAP.md` e `CLAUDE.md` dopo
    un fix. Prima di chiudere un giro: `grep` dei vecchi conteggi su tutti i file `.md`, e
    rilettura della sezione "prossimo passo" di questo file.
- **Ricompilare dopo modifiche**: se l'utente chiede di usare un binario/installer aggiornato,
  `cargo build --release` (aggiungere `--features notify-server` se serve anche quel binario) e/o
  `ISCC.exe installer\rustcopy.iss` non sono automatici — vanno lanciati esplicitamente solo su
  richiesta.
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
- `main.rs::spawn_blocking_with_span` (D13) è il **solo** modo corretto di chiamare
  `tokio::task::spawn_blocking` in `main.rs` — propaga lo span `tracing` attivo (l'identità del
  job, in un batch `[[jobs]]`) sul thread bloccante. Un nuovo punto che chiama
  `tokio::task::spawn_blocking` direttamente invece di `spawn_blocking_with_span` reintroduce il
  gap di D13 (righe di log non attribuibili al job).
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
- `robocopy_ingest::atomic_write` (D14) è il **solo** modo corretto di scrivere il manifest
  generazioni o la cache fast-verify su disco — non reintrodurre un `std::fs::write`/`fs::write`
  diretto per questi due file, che possono arrivare a centinaia di MB (174 MB/generazione a scala
  reale) e la cui corruzione a metà scrittura è fatale per il manifest.
- `main.rs::execute_generation_backup` (D15) cattura l'errore di `copy_selected` invece di
  propagarlo con `?` — non tornare a farlo propagare fatalmente, altrimenti si reintroduce la
  mappatura a `EXIT_UNRECOVERABLE` (2) invece di `EXIT_INGESTION_PROBLEM` (1) e la perdita del
  report su fallimento. `IngestReport::copy_error` è popolato **solo** da questa pipeline — non
  aggiungerlo alla pipeline plain-sync, che non ne ha bisogno.

## Skill disponibile per operare rustcopy

`.agents/skills/rustcopy-flow/` (+ copia globale `~/.claude/skills/rustcopy-flow/`) — compound
skill per costruire/eseguire comandi rustcopy reali con dry-run e checkpoint umani obbligatori.
Utile per generare i dati reali richiesti dal punto "Come procedere" sopra (benchmark, riproduzione
di un bug su un caso reale), non per il lavoro di audit del codice in sé.
