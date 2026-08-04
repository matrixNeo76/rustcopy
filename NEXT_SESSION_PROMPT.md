# Prompt per la prossima sessione — robocopy-ingest-cli (rustcopy)

Riprendi il lavoro su robocopy-ingest-cli (rustcopy). Stato: `Cargo.toml` = 5.4.2, ultimo commit
pushato `d9e3a54` su `main` (pulito, nessuna modifica in sospeso). Milestone 5.2.0 (Correttezza),
5.3.0 (Operabilità) sono chiuse. Nella milestone 6.0.0: F30 (VSS), F31 (checkpoint/resume), F33
(profili job multipli `[[jobs]]`) e F34 in versione Full+Incrementale sono chiusi e verificati —
vedi `ANALYSIS.md` Parte 3 e `ROADMAP.md` per il dettaglio completo di ciascuno (deviazioni,
limiti di test dichiarati, ecc.).

Procedi con i task rimanenti della **milestone 6.0.0 — Backup Enterprise** (`ROADMAP.md`):
- **F34 (Differenziale)** — completamento naturale a basso rischio: stessa infrastruttura di
  `src/generations.rs` (manifest, cartelle di generazione) del Full/Incrementale già chiuso,
  cambia solo il riferimento del diff (`GenerationManifest::latest_full()` invece di `latest()`).
  Candidato ovvio per iniziare la prossima sessione.
- **F35 (ritenzione/rotazione)** — ora sbloccato: le generazioni esistono davvero come cartelle
  distinte in `<dest>/.rustcopy_generations.json`, quindi F35 ha finalmente qualcosa da ruotare/
  eliminare. Dipende concettualmente da F34 (meglio se F34-Differenziale è chiuso prima, anche se
  non strettamente bloccante: la ritenzione può ragionare per generazione a prescindere dal tipo).
- **F32** (metriche Prometheus), **F36** (scheduler integrato), **F37** (servizio Windows reale),
  **F38** (compressione zip/7z), **F39** (comandi pre/post job), **F40** (cloud/FTP/SFTP reale) —
  restano tutti aperti e sono più isolati tra loro (nessuna catena di dipendenza stretta).

Prima di iniziare, verifica sempre lo stato attuale del codice (non fidarti solo del testo della
roadmap: leggi `src/generations.rs`, `src/cli.rs::validate()`, `main.rs::execute_generation_backup`
per capire esattamente cosa esiste già) e dimmi come vuoi procedere (quale task per primo, quanti
in un solo giro) — attendi conferma prima di partire, come fatto per i passi precedenti.

## Convenzioni stabilite nelle sessioni precedenti (da rispettare)

- **Test**: per ogni fix, unit test + almeno un test black-box che esegua il **binario compilato
  reale** (`tests/cli_smoke.rs`), mai solo la funzione interna in isolamento — è la lezione che ha
  già scoperto due bug reali nel progetto (F24, F25b). Qualsiasi verifica manuale contro file veri
  va fatta solo dentro `tempfile::tempdir()` isolate, mai contro cartelle reali.
- **Eccezione dichiarata**: quando un test toccherebbe stato di sistema reale al di fuori del
  sandbox tempdir (es. F30/VSS: creare/cancellare una vera shadow copy richiede elevazione reale),
  non automatizzarlo — copri solo la logica pura isolabile e dichiara esplicitamente il limite nei
  commenti/doc invece di fingere copertura completa.
- **Deviazioni dal testo della roadmap**: quando l'implementazione letterale della roadmap è più
  rischiosa di un'alternativa equivalente (es. F28: cache size+mtime invece di tracciare "i file
  che robocopy dichiara copiati"; F30: shell a `vssadmin` invece dell'API COM VSS; F34: motore
  naive per-file invece di robocopy per gli incrementali, perché robocopy non sa selezionare un
  elenco arbitrario di percorsi relativi), fermati e proponi la deviazione con la motivazione
  prima di implementare — non deciderla silenziosamente. Per F34 questo è già avvenuto via
  `AskUserQuestion` (scelta del modello a cartelle di generazione vs. manifest+destinazione
  singola) — usa lo stesso approccio per scelte architetturali equivalenti in F35+.
- **Commit/push**: mai senza richiesta esplicita dell'utente in quel turno. Un "ok procedi" su un
  piano non autorizza automaticamente anche il commit.
- **Documentazione da aggiornare ad ogni fix chiuso, nello stesso giro**: `ANALYSIS.md`/
  `ROADMAP.md` (riga della tabella del task, sezione "Analisi di Parità Funzionale" se pertinente),
  `CLAUDE.md` (nota tecnica per i futuri agenti), `README.md` (tabella flag CLI se rilevante),
  `RUNBOOK.md` (esempio d'uso pratico se il task introduce un flusso operativo nuovo, come fatto
  per F33/F34). Sono in italiano (README/ARCHITECTURE/ANALYSIS/ROADMAP/RUNBOOK); codice/commenti/
  commit in inglese.
- **Ricompilare dopo modifiche**: se l'utente chiede di usare un binario aggiornato,
  `cargo build --release` non è automatico — va lanciato esplicitamente (il binario in
  `target/release/` non si aggiorna da solo).
- **Config TOML**: quasi tutti i flag CLI recenti (incluso `--backup-type`, F34) sono ormai
  presenti anche in `JobConfig`/`IngestConfig` (`src/config.rs`) — mantenere questa parità quando
  si aggiungono nuovi flag rilevanti per un job pianificato. Eccezioni consapevoli e già accettate:
  `--decrypt`, `--restore-from`, `--vss-snapshot`, `--resume-from` (flag di sicurezza o d'uso non
  ricorrente, volutamente assenti dal TOML).

## Cosa NON toccare senza motivo

- `engine::robocopy::build_args` non deve mai passare `/Z` (restartable mode) — costo prestazionale
  deliberatamente evitato sui file piccoli, vedi `ANALYSIS.md` Parte 2 §3. F31 (checkpoint/resume)
  si appoggia allo skip-automatico di robocopy proprio per questo motivo, non è un resume a metà
  file.
- `src/oem_codec.rs` non va sostituito con `encoding_rs::Encoding::for_label(b"ibm850")` —
  `encoding_rs` non implementa le code page DOS/OEM single-byte, quel path decodifica
  silenziosamente in UTF-8.
- `check_mirror_safety`/`VssGuard` e ogni operazione bloccante su filesystem in `main.rs` devono
  restare dentro `tokio::task::spawn_blocking` — mai chiamate sincrone dentro le `async fn` di
  orchestrazione. Lo stesso vale per `execute_generation_backup` (F34): il caricamento/salvataggio
  del manifest e la copia naive sono già dentro `spawn_blocking`, non toglierli.
- `main.rs::run_jobs` (F33) ricostruisce `Args` per ogni job da un **clone dell'invocazione CLI
  originale**, mai da `try_parse_from` né dall'`Args` già mergiato del job precedente — è la stessa
  disciplina di `restore::build_restore_args`/`checkpoint::build_resume_args`, e la lezione è
  esattamente quella di F25b (i flag della vera invocazione vengono scartati altrimenti).
- `execute_generation_backup` (F34) non deve essere fatto rientrare in `transfer()`/robocopy per
  il caso incrementale: robocopy seleziona i file per pattern/nome ad ogni livello di cartella
  durante la scansione, non per un elenco arbitrario di percorsi relativi — non c'è modo di
  dirgli "copia esattamente questi N file". Il motore naive (`engine::naive::copy_selected`) resta
  l'unica strada corretta per la copia selettiva finché questo non cambia.
- `GenerationManifest`/`Generation.files` deve sempre contenere l'inventario **completo** della
  sorgente al momento del run, non solo i file effettivamente copiati — altrimenti l'incrementale
  successivo confronterebbe contro un delta parziale invece che contro lo stato pieno precedente,
  accumulando drift nel tempo.

## Raccomandazioni strategiche (da review architetturale agosto 2026)

Le seguenti indicazioni derivano da una review strutturale completa della base di codice v5.4.2:

- **Chiudere 6.0.0 prima di iniziare 7.0.0**: le feature mancanti (F34-Differenziale, F35,
  F32, F36–F40) sono tutte interne alla milestone Enterprise. Portarle a termine prima di
  iniziare il «Motore Controllabile» (7.0.0) riduce il rischio di refactor incompatibili tra
  pipeline nuova e vecchia — e soprattutto dà una release stabile e completa da cui tagliare
  un branch di manutenzione.
- **F35 (ritenzione) — attenzione alla semantica di «purge»**: la rotazione delle generazioni
  deve rimuovere **intere cartelle di generazione** in `<dest>/`, non singoli file. L'eliminazione
  deve avvenire solo dopo conferma (interattiva o `--force-purge`), con lo stesso pattern
  difensivo di `check_mirror_safety`.
- **Disciplina documentazione costante**: ogni feature chiusa deve aggiornare nello stesso
  commit/push almeno: `ANALYSIS.md` (riga nella tabella F-xxx), `ROADMAP.md` (checkmark ✅),
  `CLAUDE.md` (nota tecnica), `README.md` (se introduce un flag CLI visibile all'utente),
  `RUNBOOK.md` (se introduce un flusso operativo nuovo).
- **Consolidamento test counts**: le documentazioni ora riportano **223 test / 236 con
  notify-server** (allineate il 4 agosto 2026). Quando i conteggi cambiano, aggiornare tutte le
  referenze: `AGENTS.md` §3, `README.md` §Test, `RUNBOOK.md` indice, e il presente file.
