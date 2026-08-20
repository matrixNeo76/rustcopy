---
type: Reference
title: Runbook Operativo — robocopy-ingest-cli
description: Guide operative pratiche, esempi reali, benchmark SMB/NAS.
status: stable
generated:
  by: process:claude-code
  at: 2026-08-06T00:00:00Z
---

# 📖 OPERATIONAL RUNBOOK: robocopy-ingest-cli (rustcopy)

> **Manuale Operativo di Ingestion Massiva, Backup Incrementali Multi-Sorgente e Casi d'Uso REALI Verificati**  
> *Data: 5 Agosto 2026 | Versione: 6.0.0-Runbook | Stato: Documentazione Verificata su Share SMB Remota*

---

## 📌 1. Copia Multi-Sorgente verso la Stessa Destinazione

### ❓ È possibile copiare da più sorgenti diverse verso la stessa destinazione senza perdere l'allineamento incrementale?

**SÌ, assolutamente!**  
Esistono due modalità operative per gestire sorgenti multiple verso una destinazione comune (`\\FILESERV01\dati01\provarust`):

#### 🔹 Modalità A: Sorgenti Distinte in Sotto-cartelle (RACCOMANDATO)
Se vuoi consolidare più cartelle sorgente (es. `C:\repos`, `D:\projects`, `E:\docs`) dentro lo stesso repository di backup remoto, la buona norma è specificare una sotto-cartella dedicata in destinazione per ciascuna sorgente:
```powershell
# 1. Backup Sorgente A
.\target\release\robocopy_ingest.exe --source "C:\repos" --dest "\\FILESERV01\dati01\provarust\repos" --verify-integrity --hash-algo blake3

# 2. Backup Sorgente B
.\target\release\robocopy_ingest.exe --source "D:\projects" --dest "\\FILESERV01\dati01\provarust\projects" --verify-integrity --hash-algo blake3
```
- **Vantaggi**: Ogni sorgente mantiene la propria alberatura ed il proprio stato incrementale isolato. Non c'è alcun rischio di sovrascrittura o conflitto tra file con lo stesso nome provenienti da sorgenti diverse.

#### 🔹 Modalità B: Ingestion Multi-Sorgente nello Stesso Root (Merge Incremetale)
Se vuoi unire più sorgenti direttamente nel root della destinazione senza sotto-cartelle:
- **Copia Incrementale**: Robocopy confronterà ciascun file sorgente con il file corrispondente in destinazione. Se il file in destinazione non esiste o ha una data diversa, verrà aggiornato. Se è già identico, verrà saltato.
- **⚠️ REGOLA FONDAMENTALE (NON Usare `--mirror`)**: Quando si uniscono sorgenti multiple nello stesso root di destinazione, **NON si deve usare il flag `--mirror`**. Usando `--mirror` per la sorgente B, Robocopy cancellerebbe dalla destinazione i file precedentemente copiati dalla sorgente A!  
  *(Nota: Se si tenta di usare `--mirror`, la **Release 5.1.0** attiva la protezione `Mirror Safety Threshold` bloccando l'eliminazione accidentale).*

#### 🔹 Modalità C: Un File di Configurazione con Più Job (`[[jobs]]`, dalla Release 6.0.0)
Le Modalità A/B sopra richiedono di lanciare l'eseguibile una volta per sorgente, a mano o da uno
script esterno. Dalla **Release 6.0.0** un singolo file TOML può descrivere più job indipendenti,
eseguiti in sequenza da un solo comando:

```toml
# jobs.toml
# Campi di primo livello = default condivisi, ereditati da ogni job che non li sovrascrive.
verify_integrity = true
hash_algo = "blake3"

[[jobs]]
name = "repos"
source = "C:/repos"
dest = "\\\\FILESERV01\\dati01\\provarust\\repos"

[[jobs]]
name = "projects"
source = "D:/projects"
dest = "\\\\FILESERV01\\dati01\\provarust\\projects"
threads = 32   # sovrascrive il default solo per questo job
```

```powershell
.\target\release\robocopy_ingest.exe --config jobs.toml
```

- Ogni job produce il proprio report JSON: se un job non imposta `report_path`, ne riceve uno
  auto-generato con il nome del job inserito prima dell'estensione (es. `report.json` →
  `report.repos.json`, `report.projects.json`) — i report non si sovrascrivono mai a vicenda.
- Un job con `source`/`dest` mancanti o non validi viene segnalato ed **escluso**, senza abortire
  gli altri job del file.
- `Ctrl+C` interrompe il job in corso (con checkpoint, come sempre — vedi `--resume-from` più
  sotto) e **interrompe anche i job successivi** del batch, non li salta soltanto.
- Tutti i job condividono un solo file di log (`log_path` del file, o quello di default): il
  logger è una risorsa di processo, non per-invocazione.
- Path Windows nel TOML: usare `/` (accettato senza problemi da Windows) oppure raddoppiare i
  backslash (`\\\\`) — un singolo `\` in una stringa TOML è un carattere di escape.

## 📌 1bis. Backup a Generazioni (Full/Incrementale/Differenziale, dalla Release 6.0.0)

A differenza delle Modalità A/B/C sopra — che sincronizzano sempre lo **stesso** stato in
destinazione — `--backup-type` mantiene una **cronologia** di backup distinti, ciascuno nella
propria sotto-cartella:

```powershell
# 1. Primo backup: sempre full — non c'è ancora nulla da cui calcolare un delta.
.\target\release\robocopy_ingest.exe --source "C:\dati" --dest "E:\backup\dati" --backup-type full

# 2a. Incrementale: copia solo i file nuovi/cambiati dall'ULTIMA generazione (di qualsiasi tipo) —
#     ogni run incrementale si incatena al precedente, quindi ciascuno resta piccolo ma per
#     ricostruire lo stato servono il full più TUTTI gli incrementali intermedi in ordine.
.\target\release\robocopy_ingest.exe --source "C:\dati" --dest "E:\backup\dati" --backup-type incremental

# 2b. Differenziale (alternativa a 2a, non cumulabile nello stesso schema senza pianificarlo):
#     copia i file nuovi/cambiati dall'ULTIMO FULL, non dall'ultimo differenziale — ogni
#     differenziale cresce nel tempo, ma per ricostruire lo stato basta il full più l'ULTIMO
#     differenziale (nessuna catena da riapplicare in ordine).
.\target\release\robocopy_ingest.exe --source "C:\dati" --dest "E:\backup\dati" --backup-type differential
```

- Ogni run crea `E:\backup\dati\<timestamp>_<tipo>\` con **solo** i file effettivamente copiati in
  quel run (per `full`, tutti; per `incremental`/`differential`, solo il delta rispetto al
  rispettivo riferimento).
- `E:\backup\dati\.rustcopy_generations.json` registra ogni generazione con l'inventario
  **completo** della sorgente al momento del run (non solo il delta), cosicché il prossimo run
  confronti sempre contro uno stato pieno, non contro un delta parziale.
- `--backup-type incremental` senza **nessuna** generazione precedente in destinazione fallisce con
  errore chiaro (serve prima un `--backup-type full`). `--backup-type differential` senza un
  **full** precedente fallisce allo stesso modo — un incrementale intermedio non basta come
  riferimento per il differenziale.
- Incompatibile con `--mirror` (rifiutato da `--backup-type` insieme a `--mirror`): la
  destinazione qui è un archivio di generazioni, non un mirror 1:1 della sorgente.
- **Limiti dichiarati**: non ancora collegati a `--backup-type`: `--compare-baseline`,
  `--verify-integrity`, `--encrypt-aes256`/`--decrypt`, `--vss-snapshot` (lato destinazione).

### Ritenzione e rotazione delle generazioni (`--keep-generations`, F35)

Senza `--keep-generations`, ogni generazione resta indefinitamente sul disco. Per limitare lo
spazio occupato, `--keep-generations <N>` mantiene solo gli ultimi N **cicli** (un `full` più
tutti gli `incremental`/`differential` successivi fino al prossimo `full`) ed elimina interamente
le cartelle dei cicli più vecchi:

```powershell
# Dopo ogni run, mantiene solo gli ultimi 3 cicli completi (full + relativi
# incrementali/differenziali), eliminando le cartelle e le voci di manifest dei cicli più vecchi.
# --force-purge evita la richiesta di conferma interattiva (necessaria per script/scheduler).
.\target\release\robocopy_ingest.exe --source "C:\dati" --dest "E:\backup\dati" `
  --backup-type incremental --keep-generations 3 --force-purge
```

- La rotazione è **per ciclo, non per singola generazione**: eliminare un `full` ancora
  referenziato da un `incremental`/`differential` rimasto orfanerebbe quella catena, quindi un
  intero ciclo viene tenuto o eliminato insieme.
- Richiede `--backup-type` (senza generazioni non c'è nulla da ruotare) — `--keep-generations`
  da solo viene rifiutato.
- Senza `--force-purge` e senza conferma interattiva a console, la rotazione si interrompe con
  **exit code 5** (distinto dal `3` di `--mirror`) — ma il backup appena eseguito in quello stesso
  run **resta comunque salvato**: solo l'eliminazione dei cicli vecchi viene annullata, va
  ripetuta con `--force-purge` (o confermata a console) in un run successivo.

### Comandi pre/post job (`--pre-command`/`--post-command`, F35→F39)

Gli "eventi" di Cobian: un comando eseguito prima e uno dopo il job, tipicamente per fermare un
servizio/database perché i suoi file siano coerenti durante il backup, e riavviarlo dopo:

```powershell
.\target\release\robocopy_ingest.exe --source "C:\dati-db" --dest "E:\backup\db" `
  --pre-command "net stop MioServizioDB" `
  --post-command "net start MioServizioDB"
```

- `--pre-command` gira **prima di tutto**, incluso lo snapshot VSS. Se esce con codice diverso da
  zero (o non può essere lanciato), il job si interrompe **senza copiare nulla** — nessuna
  cartella di destinazione viene nemmeno creata.
- `--post-command` gira dopo che il backup è già riuscito. A differenza di `--pre-command`, un suo
  fallimento **non** fa fallire il job: viene solo loggato e registrato nel campo
  `post_command_error` del report JSON — riavviare il servizio è importante ma non deve
  retroattivamente far apparire fallito un backup che in realtà è riuscito.
- Il comando gira via `cmd /C` su Windows (`sh -c` altrove) — è una stringa singola, stessa
  fiducia di `--webhook-url`: interamente fornita dall'operatore, nessun escaping applicato.
- Disponibili anche nel TOML (`pre_command`/`post_command` per job).

### Pianificazione via Task Scheduler (`--install-schedule`/`--uninstall-schedule`, F36)

Nessuno scheduler interno: `--install-schedule` installa l'invocazione corrente come voce di
Task Scheduler via `schtasks.exe` — è Windows stesso a rilanciare `rustcopy` al momento giusto:

```powershell
# Pianifica un backup incrementale giornaliero alle 02:00. Gli stessi flag dell'invocazione
# (--source, --dest, --backup-type, ecc.) sono quelli che gireranno ogni volta.
.\target\release\robocopy_ingest.exe --source "C:\dati" --dest "E:\backup\dati" `
  --backup-type incremental --install-schedule daily@02:00 --schedule-name rustcopy-dati

# Verifica con lo strumento nativo (nessun --list-schedules in questo primo taglio):
schtasks /Query /TN rustcopy-dati

# Rimuove la voce pianificata. Non richiede --source/--dest.
.\target\release\robocopy_ingest.exe --uninstall-schedule rustcopy-dati
```

- `SPEC` accetta: `daily@HH:MM`, `hourly@N` (ogni N ore), `weekly@LUN,MER,VEN@HH:MM` (codici
  giorno inglesi a 3 lettere: `MON`, `TUE`, `WED`, `THU`, `FRI`, `SAT`, `SUN`).
- La voce pianificata rilancia il binario con gli **stessi argomenti** dell'invocazione che ha
  installato lo schedule (tolti solo i tre flag di scheduling) — se si è usato `--config
  job.toml`, il file viene riletto ad ogni esecuzione pianificata, non congelato all'installazione.
- Ri-eseguire `--install-schedule` con lo stesso `--schedule-name` **aggiorna** la voce esistente
  invece di fallire.
- `--install-schedule` valida prima l'invocazione (stessi controlli di un run normale: source/dest
  esistenti, ecc.) — installare uno schedule che fallirebbe sempre non ha senso.

### Servizio Windows (`--install-service`/`--uninstall-service`, F37)

```powershell
# Da un prompt con privilegi di Amministratore:
.\target\release\robocopy_ingest.exe --install-service

# Verifica/avvio con gli strumenti nativi di Windows:
sc query RustcopyIngestService
sc start RustcopyIngestService

# Rimozione:
.\target\release\robocopy_ingest.exe --uninstall-service
```

- **Perimetro di questo primo taglio (F37)**: il servizio di `robocopy_ingest`, una volta avviato,
  resta **inattivo** — risponde solo a Stop/Interrogate, nessuna logica di backup gira al suo
  interno. È pura infrastruttura SCM. Non è un sostituto di `--install-schedule` (F36): i backup
  pianificati passano comunque da Task Scheduler, non da questo servizio.
- Parte con avvio `OnDemand` (non automatico) — cambiabile in `Automatic` via `services.msc` o
  `sc config RustcopyIngestService start= auto`.
- **Richiede Amministratore** per entrambi i comandi; senza elevazione fallisce con un errore
  chiaro (mai un fallback silenzioso).

### Notify-server come servizio persistente (`--install-service`/`--uninstall-service`, F41)

A differenza del servizio di `robocopy_ingest` sopra (che resta inattivo), il binario
`notify-server` ha una **propria** identità di servizio Windows che esegue davvero il server:

```powershell
# Da un prompt con privilegi di Amministratore. --bind/--config dati qui sopravvivono
# nell'esecuzione del servizio (stesso principio di --install-schedule, F36).
.\target\release\notify-server.exe --install-service --bind 127.0.0.1:3000

# Verifica/avvio con gli strumenti nativi di Windows:
sc query RustcopyNotifyServer
sc start RustcopyNotifyServer

# Rimozione:
.\target\release\notify-server.exe --uninstall-service
```

- Servizio **separato** da quello di `robocopy_ingest` (`RustcopyIngestService`) — due identità
  SCM indipendenti, ciascuna installabile/rimovibile/avviabile a sé.
- Lo stop da `services.msc`/`sc stop` innesca lo stesso shutdown graceful già usato per
  Ctrl+C/SIGTERM in primo piano: le richieste in corso vengono drenate prima della chiusura.
- Parte con avvio `OnDemand` — cambiabile in `Automatic` via `services.msc` o
  `sc config RustcopyNotifyServer start= auto` una volta verificato che funzioni come atteso.
- **Richiede Amministratore** per entrambi i comandi; senza elevazione fallisce con un errore
  chiaro (mai un fallback silenzioso).

### Verifica manuale elevata dei servizi (`scripts/verify-services.ps1`)

`CreateService`/`StartService`/`StopService`/`DeleteService` richiedono Amministratore, quindi il
round-trip reale non può far parte della suite `cargo test` normale (limite dichiarato, stesso
pattern di `--vss-snapshot`/F30). Per verificarlo comunque in modo ripetibile invece di ridigitare
i comandi a mano:

```powershell
# Da un prompt PowerShell con privilegi di Amministratore, dopo:
#   cargo build --release --features notify-server
.\scripts\verify-services.ps1
```

Installa, avvia, ferma e disinstalla **entrambi** i servizi (`RustcopyIngestService` e
`RustcopyNotifyServer`), verificando lo stato reale via `sc query` ad ogni passaggio, con cleanup
automatico anche in caso di fallimento a metà. Esiste anche una versione automatizzata ma
**disattivata di default** dello stesso controllo (`#[ignore]`) in `tests/cli_smoke.rs` e
`tests/notify_server_e2e.rs`, eseguibile esplicitamente da un prompt elevato con:

```powershell
cargo test --test cli_smoke -- --ignored install_and_uninstall_service_round_trip
cargo test --features notify-server --test notify_server_e2e -- --ignored install_and_uninstall_service_round_trip
```

### Benchmark e analisi trend su grandi volumi (`scripts/benchmark-threads.ps1`, `scripts/analyze-runs.ps1`)

Per dataset grandi (centinaia di GB, milioni di file, spesso su SMB/NAS) il tuning di `--threads`
e la comprensione di dove va il tempo di ogni run vanno misurati sull'infrastruttura reale, non
assunti. Due script riusano la reportistica JSON già esistente (`--report-path`,
`PhaseTiming`/`TransferReport`) invece di introdurre un nuovo formato di log:

```powershell
# Confronta /MT diversi sulla STESSA sorgente/destinazione reale (fa un run di warm-up non
# misurato, poi un run misurato per ciascun valore — il caso che interessa è lo stato
# "steady-state" di una destinazione già quasi allineata, non la prima copia a freddo).
.\scripts\benchmark-threads.ps1 -ConfigPath .\examples\smb-nas-mirror.toml -Threads 8,16,32,48

# Analizza lo storico dei report accumulati (es. dalla cartella del benchmark sopra, o da una
# cartella dove uno scheduler archivia una copia del report dopo ogni run pianificato).
.\scripts\analyze-runs.ps1 -ReportsDir _ops_reports\benchmark -Recurse
```

Su un dataset con pochi file cambiati per run (il caso tipico di un mirror incrementale ogni N
ore), aspettati che `--threads` incida poco sul tempo — lo scan metadata di robocopy stesso
domina, e nessuno dei due script può cambiare quel costo, solo misurarlo. Vedi
`examples/*.toml` per configurazioni di partenza commentate (mirror NAS, job multipli, prima
copia completa) — usano percorsi placeholder apposta: copiali in `esempio.local.toml` (pattern
già in `.gitignore`, come `scripts/*.local.ps1`) prima di inserire i tuoi percorsi/credenziali
reali, così restano fuori da git.

---

## 💻 2. Comandi Reali Eseguiti e Verificati con Successo

Di seguito sono riportati i comandi **realmente testati ed eseguiti sul campo con successo** (inclusi i benchmark di performance ed esito di integrità):

### 1. Ingestion Massiva Iniziale (55.314 File, 3.18 GB su SMB)
Esecuzione del trasferimento completo con verifica di integrità multi-core **BLAKE3**:
```powershell
.\target\release\robocopy_ingest.exe `
  --source "C:\Users\auresystem\repos" `
  --dest "\\FILESERV01\dati01\provarust" `
  --verify-integrity `
  --hash-algo blake3
```
- **Esito**: 55.314 file trasferiti su rete SMB a 17.35 MB/s costante in 3 minuti e 4s.

---

### 2. Aggiornamento Incrementale ad Alta Velocità (Filtro Log Attivo)
Esecuzione dell'aggiornamento incrementale con esclusione dei log in scrittura attiva:
```powershell
.\target\release\robocopy_ingest.exe `
  --source "C:\Users\auresystem\repos" `
  --dest "\\FILESERV01\dati01\provarust" `
  --exclude-files "robocopy_ingest.log" `
  --verify-integrity `
  --hash-algo blake3
```
- **Esito**: 55.269 file inalterati saltati a banda ZERO all'istante; 905 file modificati/nuovi trasferiti in soli **38 secondi**.

---

### 3. Simulazione Preventiva Dry-Run (Senza Modifiche ai Dati)
Test di verifica senza scrivere o alterare i dati di destinazione:
```powershell
.\target\release\robocopy_ingest.exe `
  --source "C:\Users\auresystem\repos" `
  --dest "\\FILESERV01\dati01\provarust" `
  --dry-run `
  --verify-integrity
```
- **Esito**: Generazione dell'inventario e simulazione completata in 1.75 secondi.

---

### 4. Backup Enterprise con Notifica e Dashboard HTML
`--serve-dashboard` è stato **rimosso** (era una pagina statica mock, mai un dashboard live — vedi
`ROADMAP.md`). Per il progresso in tempo reale usare la progress bar in console; per la notifica di
completamento e un report visivo a fine job:
```powershell
.\target\release\robocopy_ingest.exe `
  --source "C:\Users\auresystem\repos" `
  --dest "\\FILESERV01\dati01\provarust" `
  --html-report-path "\\FILESERV01\dati01\provarust\dashboard.html" `
  --webhook-url "http://127.0.0.1:3000/notify" `
  --verify-integrity `
  --hash-algo blake3
```
Il `--webhook-url` può puntare al **notify-server** incluso nel repo (`cargo build --release
--features notify-server`, vedi `README.md`), che inoltra la notifica su più canali configurati in
un file TOML.

---

### 5. Disaster Recovery / Ripristino da Report JSON (Reverse Restore)
Ripristino guidato in caso di guasto del server principale partendo dal report JSON di backup:
```powershell
.\target\release\robocopy_ingest.exe `
  --restore-from "\\FILESERV01\dati01\provarust\robocopy_ingest_report.json"
```
**Corretto e verificato** (F24, `ANALYSIS.md` D1): `--source`/`--dest` sono ora `Option<PathBuf>`
e non richiesti in questa modalità — il difetto precedente (`--source`/`--dest` sempre obbligatori
a causa di un `default_value = ""` che clap trattava come "nessun default") è stato risolto. Test
black-box dedicato in `tests/cli_smoke.rs` (`restore_from_runs_end_to_end_without_source_or_dest`):
simula una perdita di file in una sandbox temporanea isolata e ne verifica il recupero completo
tramite questo stesso comando, senza `--source`/`--dest`.

---

## 📑 3. Indice Documentazione di Progetto

| Documento | Descrizione e Contenuto |
|---|---|
| 📘 **[README.md](file:///c:/Users/auresystem/repos/robocopy-ingest-cli/README.md)** | Guida generale, tabella flag CLI e panoramica di alto livello. |
| 📖 **[RUNBOOK.md](file:///c:/Users/auresystem/repos/robocopy-ingest-cli/RUNBOOK.md)** | **[QUESTO DOCUMENTO]** Guida operativa, backup multi-sorgente e comandi reali testati. |
| 📄 **[ARCHITECTURE.md](file:///c:/Users/auresystem/repos/robocopy-ingest-cli/ARCHITECTURE.md)** | Architettura interna v6.0.0, diagrammi di flusso e mappa dei moduli Rust. |
| 📊 **[ANALYSIS.md](file:///c:/Users/auresystem/repos/robocopy-ingest-cli/ANALYSIS.md)** | Diagnosi di robustezza, tuning 3x performance e 302 test di validazione (317 con `notify-server`). |
| 🗺️ **[ROADMAP.md](file:///c:/Users/auresystem/repos/robocopy-ingest-cli/ROADMAP.md)** | Diagramma Gantt delle release (v1.0 → v8.0) e pianificazione futura. |
