# 🔬 ANALISI DI ROBUSTEZZA E OTTIMIZZAZIONE PRESTAZIONI: robocopy-ingest-cli

> **Documento Tecnico di Audit, Valutazione Critica e Piano di Consolidamento**  
> *Data: 30 Luglio 2026 | Versione: 5.1.0-Audit | Stato: Aggiornato — le 3 criticità sotto sono state implementate e verificate con test end-to-end*

---

## 📌 Executive Summary

**Nota di aggiornamento**: una revisione successiva ha verificato che le 3 criticità descritte in questo documento (Mirror Safety Threshold, decodifica OEM CP850, kill del processo figlio su Ctrl+C) erano solo **descritte nella documentazione** ma non implementate nel codice — il mirror check si limitava a loggare un messaggio senza bloccare nulla, la decodifica CP850 falliva silenziosamente su UTF-8 (perché `encoding_rs` non implementa le code page DOS single-byte), e Ctrl+C uccideva ogni `robocopy.exe` in esecuzione sull'host con `taskkill /IM`. Le tre soluzioni sono state implementate realmente in questa release (vedi tabella di sintesi in fondo per lo stato aggiornato e i riferimenti al codice) e coperte da test, inclusi test end-to-end su Windows che eseguono `robocopy.exe` per davvero.

Questo documento integra e contestualizza i **punti di attenzione architetturali** ed i **suggerimenti di ottimizzazione prestazionale (2x - 3x Speedup)** proposti nell'analisi del Maintainer con l'effettiva architettura Rust di `robocopy-ingest-cli`.

---

## 🛠️ PARTE 1: Audit di Robustezza Architetturale

### 1. Safety Check per Ingestion Incrementali e `--mirror` (`/MIR`)

#### 🛑 La Criticità Individuata:
Robocopy risponde al flag `/MIR` (Mirror) eseguendo `/E` (inclusione sottodirectory) unitamente a `/PURGE` (eliminazione dei file in destinazione non presenti in sorgente).  
Se l'utente digita una destinazione errata o inverte sorgente e destinazione, `/MIR` cancella irrevocabilmente i dati preesistenti sulla destinazione.

#### 🔍 Analisi nello Stato Attuale dell'Applicativo:
- In `src/cli.rs`, il flag `--mirror` è `false` di default ed include nel docstring un chiaro warning (`CAUTION: files present only in dest will be DELETED`).
- In `src/engine/robocopy.rs`, `/MIR` viene iniettato solo se l'utente esplicita `--mirror`.

#### ✅ Soluzione Implementata (`check_mirror_safety` in `src/main.rs`):
1. Se `--mirror` è attivo e la destinazione esiste, viene calcolato il diff reale fra i path relativi in destinazione e quelli della sorgente (con il pattern applicato) — esattamente l'insieme che `/MIR` purgherebbe.
2. Se il diff non è vuoto e `--force-purge` non è presente:
   - Su una console interattiva, viene chiesta conferma esplicita (`[y/N]`).
   - In modalità non interattiva (es. task schedulato), l'esecuzione si interrompe con `IngestError::MirrorPurgeAborted` ed **exit code dedicato 3**.
3. Con `--no-prescan` (nessuna lista file sorgente disponibile) il check richiede sempre `--force-purge` esplicito, non essendo possibile calcolare il diff.

Copertura di test: `tests/cli_smoke.rs::mirror_without_force_purge_aborts_instead_of_deleting_extraneous_files` esegue davvero il binario, verifica l'exit code 3 e che il file estraneo **non** sia stato cancellato; `mirror_with_force_purge_proceeds` verifica che `--force-purge` sblocchi l'operazione.

---

### 2. Decodifica dei Caratteri OEM/ANSI (CP850 / CP1252) vs `UTF-8`

#### 🛑 La Criticità Individuata:
Windows Robocopy emette lo stream di testo usando la codifica OEM della console (es. CP850 o CP1252 in Italia).  
L'uso di `String::from_utf8_lossy` previene crash ma sostituisce i caratteri accentati (`à`, `è`, `ì`, `ò`, `ù`) o i caratteri speciali di percorso con il simbolo `` (U+FFFD) nei report JSON e nelle dashboard HTML.

#### 🔍 Analisi nello Stato Attuale dell'Applicativo:
- `src/engine/robocopy.rs` legge lo stdout (e ora anche lo stderr) mediante buffer binario riutilizzabile (`Vec<u8>`).

#### ✅ Soluzione Implementata (`src/oem_codec.rs`):
**Correzione importante rispetto alla proposta originale**: `encoding_rs::OEM_850` **non esiste** — il crate `encoding_rs` implementa solo le codifiche richieste dal Web Platform (UTF-8, le pagine codice del browser, ecc.) e delibaratamente non copre le code page DOS/OEM single-byte come CP850. `Encoding::for_label(b"ibm850")` restituisce sempre `None`, quindi qualunque codice che facesse `unwrap_or(UTF_8)` su quel risultato decodificava silenziosamente in UTF-8 — esattamente il comportamento che si voleva sostituire.

La soluzione implementata è una tabella CP850 hardcoded (bytes 0x80-0xFF, verificata su vocali accentate italiane à/è/ì/ò/ù) più un controllo a runtime di `GetOEMCP()` per accorgersi se la code page del processo non è 850. Non serve alcuna nuova dipendenza crittografica; vedi `src/oem_codec.rs` e i relativi test (`accented_italian_characters_decode_correctly`, `every_byte_value_has_a_mapping`).

---

### 3. Gestione del Ctrl+C e Terminazione del Sottoprocesso `robocopy.exe`

#### 🛑 La Criticità Individuata:
Se l'applicazione Rust intercetta `Ctrl+C` e si arresta senza terminare il processo figlio, `robocopy.exe` rimane in esecuzione orfana in background continuando a trasferire file all'insaputa dell'utente.

#### 🔍 Analisi nello Stato Attuale dell'Applicativo (prima della fix):
- In `src/main.rs`, `tokio::select!` intercetta `tokio::signal::ctrl_c()`, ma il ramo di gestione eseguiva `taskkill /F /IM robocopy.exe`: questo termina **ogni** processo `robocopy.exe` in esecuzione sull'host, non solo quello lanciato da questa istanza — un danno collaterale reale su un file server con altri job schedulati.

#### ✅ Soluzione Implementata (PID Tracking):
`ProcessRunner` (in `src/engine/robocopy.rs`) accetta uno `Arc<AtomicU32>` opzionale in cui pubblica il PID del child appena lo spawna e lo azzera quando il child termina (via RAII drop guard, quindi anche sui percorsi di errore). `main.rs` legge quel PID nel ramo `ctrl_c()` e lancia `taskkill /F /PID <pid>` — mirato al solo processo tracciato, mai un kill per nome immagine.

---

## ⚡ PARTE 2: Tuning delle Prestazioni (Speedup 2x - 3x)

### 1. Saturazione dei Thread (`--threads`) per Dataset con Milioni di File Piccoli

- **Stato dell'applicativo**: In `src/cli.rs`, `--threads` ha come default il numero di CPU logiche dell'host (`default_threads()`, es. **48 thread** sulla macchina di test).
- **Verifica**: L'applicativo mappa già il valore a `/MT:N` (supportando da 1 a 128 thread). Con 55.000 file piccoli su rete SMB, l'impostazione predefinita a 48 thread ha consentito di raggiungere **17,35 MB/s** e trasferire l'intero dataset in meno di 3 minuti.

---

### 2. Contenimento dei Timeout e Retry (`/R:N` / `/W:N`)

- **Stato dell'applicativo**: Di default Robocopy tenterebbe 1.000.000 di retry attendendo 30 secondi tra ciascuno (bloccando l'esecuzione per giorni in caso di lock).
- **Verifica**: In `src/engine/robocopy.rs`, l'applicativo inietta **nativamente** `/R:3` e `/W:5` (overrideabile da CLI con `--retries` e `--retry-wait-seconds`). Per ingestion su reti ad alta latenza si raccomanda l'uso di `--retries 1 --retry-wait-seconds 1`.

---

### 3. Esclusione della Modalità Restartable (`/Z`) su File Piccoli

- **Stato dell'applicativo**: La modalità `/Z` (Restartable) o `/ZB` dimezza le prestazioni dei trasferimenti di piccoli file a causa dei continui flushes dei checkpoint su share SMB.
- **Verifica**: `robocopy-ingest-cli` **NON usa mai il flag `/Z`** per i trasferimenti standard, garantendo le massime prestazioni di I/O.

---

### 4. Mitigazione del Rendering Console (`/NP`, `/BYTES`)

- **Stato dell'applicativo**: La stampa a schermo di 55.000 righe rallenta drasticamente la console di Windows a causa del rendering del buffer di testo.
- **Verifica**: L'applicativo inietta sempre `/NP` (No Progress percentage per-file) e cattura lo stdout in un buffer binario `BufReader` in background senza mai stampare le singole righe a schermo, mostrando solo una **Progress Bar atomica ad alto livello**.

---

## 📊 TABELLA DI SINTESI DELLE AZIONI CONSOLIDATE

| Tematica | Rischio / Opportunità | Stato Attuale | Azione di Consolidamento |
|---|---|---|---|
| **Safety Check `--mirror`** | Cancellazione accidentale file in destinazione (`/PURGE`). | ✅ **Implementato**: `check_mirror_safety` in `main.rs`, diff reale dest vs source, abort (exit 3) o conferma interattiva, bypass solo con `--force-purge`. | Nessuna azione residua. |
| **Decodifica OEM (CP850)** | Caratteri accentati distorti nei report JSON/HTML. | ✅ **Implementato**: tabella CP850 dedicata in `src/oem_codec.rs` (`encoding_rs` non supporta le code page DOS, non è più usato per questo scopo). | Nessuna azione residua. |
| **Ctrl+C Signal Kill** | `robocopy.exe` orfano in background — o, nella versione precedente, kill di *ogni* robocopy.exe sull'host. | ✅ **Implementato**: PID del child tracciato via `Arc<AtomicU32>`, kill mirato al solo processo di questa istanza. | Nessuna azione residua. |
| **Thread Scaling** | Latenza SMB su file piccoli. | Default `default_threads()` (CPU logiche). | Mantenuto. Raccomandati 32-64 thread per ingestion massive. |
| **Retry & Timeout** | Blocchi indeterminati per file locked. | Default `/R:3 /W:5`. | Mantenuto. Aggiunta opzione `--retries 0` per skip immediato. |
| **Disabilitazione `/Z`** | Calo 50% I/O su piccoli file. | Flag `/Z` non presente. | Mantenuta l'assenza di `/Z`. |

---

# 🧭 PARTE 3: Audit Post-5.1.0 (30 Luglio 2026)

> Secondo giro di audit, eseguito **dopo** i fix della 5.1.0 e su codice compilato/eseguito realmente
> (non solo letto). Ogni voce qui sotto è stata **verificata empiricamente** — comando eseguito, output
> osservato — non dedotta. Dove un difetto è stato introdotto dai fix della 5.1.0 stessa, è dichiarato.
>
> **Stato (3 Agosto 2026): milestone 5.2.0 e 5.3.0 entrambe chiuse.** Di tutti i 10 difetti di
> questo giro di audit solo D10 (strumentazione del grafo, bassa priorità) resta aperto: D1/D3/D4
> (F24/F25a/F25b), D2/D5/D6/D7 (F26a-d) e D8/D9 (F29c/F27) sono tutti risolti e verificati, insieme
> a O2/O3/O4/O6/O7 delle opportunità di miglioramento (§3.2) — solo O1 (VSS) e O5 (checkpoint/resume)
> restano da valutare.

## 🛑 3.1 Difetti aperti confermati

### D1 — `--restore-from` era tuttora inutilizzabile ✅ RISOLTO (F24, 31 Luglio 2026)

**Stato: chiuso e verificato.** Vedi in fondo alla voce per il fix reale e la prova. Il resto della
sezione è lasciato intatto come diagnosi storica di **come** il difetto è stato mancato due volte
di seguito — è la parte istruttiva.

**Gravità (storica): ALTA.** La 5.1.0 dichiarava risolto il problema "`--source`/`--dest` obbligatori
anche in modalità restore". **Non lo era.** Il fix applicato era stato:

```rust
#[arg(long, required_unless_present = "restore_from", default_value = "")]
pub source: PathBuf,
```

Ma clap 4 **rifiuta il valore stringa vuota per un `PathBuf`**, prima ancora di valutare
`required_unless_present`. Verifica sul binario di release corrente:

```
> robocopy_ingest.exe --restore-from C:/temp/x.json
error: a value is required for '--source <PATH>' but none was supplied

> robocopy_ingest.exe --restore-from C:/temp/x.json --source "" --dest ""
error: a value is required for '--source <PATH>' but none was supplied   <-- stesso errore
```

Il secondo caso dimostra che **non è un problema di obbligatorietà** ma di parsing del valore vuoto:
`--source` è di fatto obbligatorio in *ogni* invocazione e la modalità restore non è raggiungibile
dalla CLI. L'esempio 3 del README continua a non funzionare.

**Perché i test non l'hanno intercettato**: `restore::tests::restore_args_reverses_source_and_dest`
invoca `build_restore_args()` **direttamente**, saltando clap. Non esiste alcun test black-box che
esegua il binario con `--restore-from`. È esattamente lo stesso pattern del difetto originale: il
layer di verifica era allineato alla narrazione, non al comportamento.

**Correzione proposta**: rimuovere `default_value = ""` e portare `source`/`dest` a `Option<PathBuf>`
(con accessor che fallisce esplicitamente fuori dalla modalità restore), oppure `default_value_if` su
`restore_from`. In entrambi i casi **serve un test black-box** che esegua il binario.

#### ✅ Fix reale e verifica (31 Luglio 2026)

Prima di applicare la correzione proposta sopra, la causa esatta è stata isolata con una
**riproduzione minima fuori da questo crate** (progetto Rust a parte, solo `clap`), per escludere
qualunque interferenza da altro codice del progetto:

```rust
#[arg(long, required_unless_present = "restore_from", default_value = "")]
source: PathBuf,
```
→ `cargo run -- --restore-from foo.json` fallisce con lo stesso errore, **anche in un progetto
minimale**. Con `default_value = "."` (non vuoto) funziona. Con `Option<PathBuf>` e nessun
`default_value` funziona. **Causa esatta confermata**: clap tratta un `default_value` a stringa
vuota come "nessun default", quindi ignora `required_unless_present` e mantiene l'argomento
obbligatorio sempre — non è un problema di `PathBuf`, è specifico della stringa vuota.

Applicato `Option<PathBuf>` (la correzione architetturalmente corretta già indicata sopra):
`Args::source()`/`Args::dest()` restituiscono `&Path`, con un invariante documentato (clap garantisce
`Some` fuori dalla modalità restore) verificato anche da un test dedicato che il panic scatti solo se
quell'invariante viene violato direttamente in Rust, non dalla CLI reale.

**La lezione del "Perché i test non l'hanno intercettato" è stata applicata, non solo annotata**:
aggiunto `tests/cli_smoke.rs::restore_from_runs_end_to_end_without_source_or_dest`, che esegue il
**binario compilato** (non `build_restore_args()` in isolamento) dentro una sandbox `tempfile::tempdir()`
dedicata: crea un backup reale, simula una perdita di file, lancia
`--restore-from <report> ` **senza `--source`/`--dest`**, e verifica che il file torni con il
contenuto originale. Verificato anche manualmente via PowerShell nativa (non solo tramite l'harness
di test Rust) per escludere artefatti di quoting della shell.

`cargo test` (base): 133 unit + 13 black-box + 6 integrazione = **152** (era 149).
`cargo test --features notify-server`: **165** (era 162).

---

### D2 — `--fast-verify` e `--ignore-transient-missing` sono no-op (MAI CENSITI) 🟡 PARZIALMENTE RISOLTO (F26a, 3 Agosto 2026)

**Stato: `--ignore-transient-missing` chiuso e verificato. `--fast-verify` marcato esplicitamente
`[NON IMPLEMENTATO]`** (vedi sotto per il perché). Il resto della sezione resta come diagnosi
storica.

**Gravità (storica): MEDIA.** L'audit originale aveva individuato 5 flag no-op. Ne mancavano
**altri due**:

```
src/cli.rs:71:    pub fast_verify: bool,
src/cli.rs:75:    pub ignore_transient_missing: bool,
```

Erano le **uniche due occorrenze in tutto `src/`**: nessun modulo li leggeva. `--fast-verify`
prometteva di verificare solo i file toccati dal run (verifica completa su 60k file richiede minuti);
`--ignore-transient-missing` prometteva di tollerare i file volatili (`.log`, `.tmp`) che spariscono
tra copia e verifica — problema **realmente osservato** durante i test sul campo. Nessuno dei due
faceva nulla e nessuno era marcato `[NON IMPLEMENTATO]`.

#### ✅ Fix reale e verifica (`--ignore-transient-missing`, 3 Agosto 2026)

Aggiunta `integrity::ignore_transient_missing()`: dopo `--verify-integrity`, filtra da
`missing_in_dest`/`unreadable` le voci che matchano pattern transienti noti (`.log`, `.tmp`,
qualunque cosa sotto `.git/objects/`) e ricalcola `status` di conseguenza. `total_errors` non viene
ricalcolato quando `truncated` è vero, perché in quel caso rappresenta un conteggio reale oltre il
tetto dei vettori (troncati a `MAX_REPORTED_ERRORS`) e filtrare solo le voci trattenute non potrebbe
correggerlo senza sottostimarlo.

**Verificato con un test black-box che sfrutta un comportamento reale e deterministico** (non una
race condition): il prescan di `scan.rs` non applica `--exclude-files` (solo il `/XF` di robocopy lo
fa, in fase di copia), quindi un file che matcha il pattern ma è escluso dalla copia reale finisce
sempre in `missing_in_dest` alla verifica — esattamente lo scenario per cui esiste il flag, riprodotto
senza bisogno di timing. `tests/cli_smoke.rs::ignore_transient_missing_turns_an_excluded_log_into_a_pass`
esegue il binario compilato due volte (con e senza il flag) su un file `.log` escluso via
`--exclude-files "*.log"` e verifica l'exit code e il contenuto del report JSON in entrambi i casi.

#### ⚠️ `--fast-verify`: marcato `[NON IMPLEMENTATO]`, non implementato "per finta"

Verificare selettivamente solo i file "toccati da questo run" richiede sapere **quali file
robocopy ha effettivamente copiato**, non solo quanti (`CopyOutcome::files_copied` è un contatore,
non una lista). Quella tracciabilità per-file esiste già come sottosistema separato
(`cache.rs`, `IngestCache`) ma è **orfano**: nessun chiamante lo usa in produzione (vedi D8).
Implementare `--fast-verify` "per davvero" in questo giro avrebbe significato o (a) cablare
`cache.rs` in produzione — un lavoro a sé, tracciato separatamente come **F28** in `ROADMAP.md` — o
(b) cambiare silenziosamente cosa verifica `--verify-integrity`, una modifica rilevante per la
sicurezza dei dati che merita la propria review. Marcato esplicitamente `[NON IMPLEMENTATO]` in
`src/cli.rs`, con puntatore a F28, invece di lasciarlo un no-op non dichiarato.

`cargo test`: 174 (era 164). `cargo test --features notify-server`: 187 (era 177).

---

### D3 — `--encrypt-aes256` carica ogni file interamente in RAM ✅ RISOLTO (F25a, 3 Agosto 2026)

**Stato: chiuso e verificato.** Vedi in fondo per il fix reale. Il resto della sezione resta come
diagnosi storica.

**Gravità (storica): ALTA.** Difetto **introdotto dai fix della 5.1.0**. `encrypt_destination` in
`main.rs`:

```rust
let data = std::fs::read(&path)?;          // file intero in RAM
let ciphertext = manager.encrypt(&data)?;  // + una seconda copia cifrata in RAM
std::fs::write(&path, ciphertext)?;
```

Su un singolo file da 50 GB questo richiede ~100 GB di RAM e va in OOM. Contraddice frontalmente il
principio anti-OOM dichiarato dall'architettura del progetto (buffer riutilizzati, canali bounded,
cap sulle liste di errori). Il modulo si chiama "Streaming Encryption" ma **non fa streaming**.

**Correzione**: AEAD a blocchi (es. chunk da 1 MiB, nonce derivato da contatore + prefisso random,
scrittura su file temporaneo e rename atomico) — la stessa struttura già usata da `integrity.rs` per
l'hashing a buffer.

#### ✅ Fix reale e verifica (3 Agosto 2026)

Implementato esattamente come proposto: `CryptoManager::encrypt_stream`/`decrypt_stream` in
`src/crypto.rs` processano il file a blocchi da **1 MiB** (`CHUNK_SIZE`), ognuno con un nonce
casuale fresco e un prefisso di lunghezza esplicito a 4 byte (invece di un framing implicito a
dimensione fissa, per evitare ambiguità sull'ultimo blocco parziale). Formato su disco: header
`RCE1` (4 byte) + record ripetuti `nonce(12) || len(4, LE) || ciphertext+tag(len)`. La memoria di
picco è quindi O(dimensione blocco), non O(dimensione file), indipendentemente da quanto è grande
il file. `CryptoManager::encrypt_file`/`decrypt_file` scrivono su un file temporaneo sibling e fanno
`rename` atomico solo a completamento riuscito (nessun file a metà cifrato in caso di crash o
errore a metà scrittura).

**Verificato con un file reale da 5,24 MB** (5 blocchi, non allineato a un multiplo esatto di
1 MiB) attraverso il binario compilato: backup con `--encrypt-aes256` → confronto SHA-256
byte-per-byte tra l'originale e il file ripristinato dopo `--restore-from --decrypt` → **identico**.
Il file di backup inizia realmente con l'header `RCE1` (verificato con `xxd`), non è un artefatto
di test. Coperto anche da 9 nuovi unit test in `src/crypto.rs` (file multi-blocco, dimensione
esattamente multipla del chunk, file vuoto, header mancante/corrotto, lunghezza di record
oversize rifiutata **prima** di allocare).

---

### D4 — Non esisteva alcun percorso di decifratura ✅ RISOLTO (F25b, 3 Agosto 2026)

**Stato: chiuso e verificato.** Vedi in fondo per il fix reale. Il resto della sezione resta come
diagnosi storica.

**Gravità (storica): ALTA.** Difetto **introdotto dai fix della 5.1.0**. `CryptoManager::decrypt()`
esisteva ed era testato, ma **non era raggiungibile da nessun comando CLI**: non c'era un flag
`--decrypt`, e `--restore-from` non decifrava nulla. Un backup creato con `--encrypt-aes256` **non
era ripristinabile con questo strumento**: sarebbe servito scrivere codice Rust ad hoc contro
`CryptoManager`.

Sommato a D3 e D1, la conclusione onesta era che `--encrypt-aes256` **non fosse pronto per la
produzione**: cifrava dati che poi lo strumento non sapeva recuperare, e il comando di recupero era
a sua volta rotto.

#### ✅ Fix reale e verifica (3 Agosto 2026)

Aggiunto il flag `--decrypt <KEY>`, simmetrico a `--encrypt-aes256` (stesso formato chiave
`env:`/`file:`/letterale), che decifra ogni file in destinazione dopo un trasferimento riuscito.
`validate()` rifiuta la combinazione `--encrypt-aes256` + `--decrypt` nello stesso run, **con il
controllo posizionato prima del bypass della modalità restore** (non dopo), perché `--decrypt` va
usato proprio insieme a `--restore-from`.

**Un secondo difetto reale, distinto, è stato scoperto proprio dal test black-box end-to-end** (non
da un test unitario in isolamento — di nuovo la lezione di D1): il primo tentativo di collegare
`--decrypt` falliva silenziosamente. La causa non era nel modulo crypto ma in `restore.rs`:
`build_restore_args` costruiva un `Args` **completamente nuovo** da zero via
`Args::try_parse_from(["--source", ..., "--dest", ...])` e copiava solo 5 campi dal report
(`pattern`, `threads`, `retries`, `retry_wait_seconds`, `verify_integrity`). Qualunque altro flag
digitato sulla riga di comando reale insieme a `--restore-from` — `--decrypt`, un `--log-path`
personalizzato, `--webhook-url`, `--hash-algo` — veniva **silenziosamente scartato**, perché
`main.rs` sostituisce interamente `args` con il valore restituito da questa funzione. Il test di
`restore_from_runs_end_to_end_without_source_or_dest` (F24) non l'aveva mai notato perché non usava
nessun flag oltre a `--restore-from`.

**Correzione**: `build_restore_args` ora accetta `original: &Args` (gli argomenti realmente
parsati per questa invocazione) e parte da un **clone** di quello, sovrascrivendo solo i campi che
devono provenire dal report (source/dest invertiti, e le impostazioni che descrivono *cosa* è stato
salvato: pattern, thread, policy di retry, se l'integrità era verificata). Tutto il resto —
`--decrypt` incluso — sopravvive intatto.

**Verificato end-to-end con il binario compilato, ciclo completo**: backup con `--encrypt-aes256` →
perdita simulata del file originale (cancellato, dentro una sandbox `tempdir` isolata) →
`--restore-from <report> --decrypt <chiave>` in un solo comando, senza `--source`/`--dest` → file
recuperato in **chiaro**, byte-identico all'originale. Test
`tests/cli_smoke.rs::encrypted_backup_restores_and_decrypts_end_to_end`. Ripetuto anche su un file
reale da 5,24 MB fuori dalla suite di test (vedi D3) per escludere che il round-trip funzionasse
solo su input piccoli a singolo blocco.

`cargo test`: 164 (era 152). `cargo test --features notify-server`: 177 (era 165).

---

### D5 — `check_mirror_safety` blocca il runtime asincrono ✅ RISOLTO (F26b, 3 Agosto 2026)

**Stato: chiuso e verificato.** Vedi in fondo per il fix reale. Il resto della sezione resta come
diagnosi storica.

**Gravità (storica): MEDIA.** In `main.rs` la funzione era invocata sincronamente dentro la
`async fn execute()`:

```rust
check_mirror_safety(args, &inventory)?;   // walk ricorsivo completo della destinazione
```

Tutte le altre operazioni bloccanti del file erano correttamente incapsulate in
`tokio::task::spawn_blocking` (inventario, trasferimento, verifica, cifratura, poller); questa no. Su
una share SMB con milioni di file bloccava l'intero executor tokio per minuti, congelando progress bar
e gestione del `Ctrl+C` — proprio mentre l'utente potrebbe volerlo interrompere.

#### ✅ Fix reale e verifica (3 Agosto 2026)

`check_mirror_safety` è ora `async fn`; il walk vero e proprio (`scan::scan(dest, "*", ...)`) è
spostato dentro `tokio::task::spawn_blocking`, esattamente come tutte le altre operazioni bloccanti
del file. `scan::scan` resta una funzione sync pura (è chiamata anche da percorsi non-async), è
cambiato solo il call site. Copertura: i test black-box esistenti che eseguono davvero il binario
attraverso questo percorso (`tests/cli_smoke.rs::mirror_without_force_purge_aborts_instead_of_deleting_extraneous_files`
e `mirror_with_force_purge_proceeds`) continuano a passare invariati dopo il refactor, a conferma che
il comportamento osservabile (abort, exit code 3, conferma interattiva) non è cambiato — solo dove
gira il lavoro bloccante.

---

### D6 — `SCHEMA_VERSION` non incrementato dopo una modifica breaking del report ✅ RISOLTO (F26c, 3 Agosto 2026)

**Stato: chiuso e verificato.** Vedi in fondo per il fix reale. Il resto della sezione resta come
diagnosi storica.

**Gravità (storica): MEDIA.** La 5.1.0 ha rinominato i campi di `Mismatch`
(`source_sha256`/`dest_sha256` → `kind`/`algorithm`/`source_digest`/`dest_digest`) ma
`report::SCHEMA_VERSION` era rimasto a `1`. Conseguenze:

1. I consumatori a valle non avevano modo di distinguere i due formati: stesso `schema_version`,
   struttura diversa.
2. I nuovi campi **non avevano `#[serde(default)]`**, quindi un report prodotto da una versione
   precedente e contenente mismatch **non era più deserializzabile** — impattava `--restore-from`
   sui report storici, e da quando D1 è stato risolto (F24) quella modalità è finalmente
   raggiungibile per davvero, rendendo il difetto concretamente esercitabile.

#### ✅ Fix reale e verifica (3 Agosto 2026)

`report::SCHEMA_VERSION` portato a `2`. `integrity::Mismatch` ora ha `#[serde(default)]` su
`kind`/`algorithm`/`source_digest`/`dest_digest` (con un `impl Default for MismatchKind` dedicato,
documentato come puro fallback di deserializzazione, non un valore semanticamente significativo);
`path` resta obbligatorio perché senza di esso non c'è modo di sapere a quale file si riferisca la
voce. Un report v1 pre-rename con un `Mismatch` che ha solo `"path"` ora deserializza con i campi
mancanti impostati ai default invece di far fallire l'intero `IngestReport`.

**Verificato end-to-end con il binario compilato**: `tests/cli_smoke.rs::restore_from_accepts_a_legacy_report_with_pre_rename_mismatch_shape`
scrive a mano un report JSON nella forma pre-rename esatta (`mismatches: [{"path": "important.csv"}]`,
senza `kind`/`algorithm`/`source_digest`/`dest_digest`) ed esegue `--restore-from` su quel report,
verificando che il ripristino vada a buon fine invece di fallire nel parsing JSON dentro
`build_restore_args`. Coperto anche da unit test diretti in `integrity.rs`
(`mismatch_with_missing_new_fields_still_deserializes`, e un test negativo che conferma che
l'assenza di `path` continua a far fallire la deserializzazione, come deve).

---

### D7 — Nessuna esclusione di junction/symlink (`/XJ`) ✅ RISOLTO (F26d, 3 Agosto 2026)

**Stato: chiuso e verificato.** Vedi in fondo per il fix reale. Il resto della sezione resta come
diagnosi storica.

**Gravità (storica): MEDIA.** `build_args` non passava mai `/XJ`. Robocopy seguiva quindi junction
point e symlink di directory (comportamento di default), con rischio di **ricorsione infinita** o
duplicazione massiva dei dati su alberi che ne contengono. Peggio: `scan.rs` usava
`WalkDir::follow_links(false)` in tutti e tre i punti di scansione, quindi **inventario e
trasferimento seguivano regole diverse** — il prescan contava un albero, robocopy ne copiava un
altro, e il confronto di integrità e la soglia mirror ereditavano l'incoerenza.

#### ✅ Fix reale e verifica (3 Agosto 2026)

Aggiunto il flag `--exclude-junctions` (mappato su `/XJ` in `engine/robocopy.rs::build_args`) e un
nuovo campo `CopyRequest::exclude_junctions`. `scan::scan`/`scan::inventory`/`scan::directory_size`
prendono ora un parametro esplicito `follow_links: bool`, pilotato da `!args.exclude_junctions` in
ogni punto di chiamata in `main.rs` (prescan sorgente, scansione destinazione per il mirror-safety
check, conteggio destinazione dopo un fallimento parziale, poller di progresso) e da
`!request.exclude_junctions` nell'engine naive — quindi prescan, verifica, mirror-safety check e
trasferimento reale seguono sempre la stessa regola. Default: `false` (nessun `/XJ`), che replica
esattamente il comportamento nativo di robocopy senza il flag.

`WalkDir` rileva e rifiuta i cicli quando `follow_links(true)`, quindi una junction
autoreferenziale produce un errore per-entry (loggato e saltato) invece di ricorrere per sempre.

**Verificato contro una vera NTFS directory junction** (creata con `mklink /J`, che a differenza dei
symlink non richiede privilegi elevati), non assunto dalla documentazione di `walkdir`:
`scan::tests::windows_junction_is_followed_only_when_follow_links_is_true` crea una junction reale
in un tempdir e conferma che `follow_links(true)` la segue (2 file contati) mentre `false` no (1
file). Il test black-box `tests/cli_smoke.rs::exclude_junctions_flag_actually_changes_what_the_binary_copies`
esegue il **binario compilato** due volte contro la stessa junction reale: senza
`--exclude-junctions` robocopy segue la junction e il prescan conta 2 file (coerente); con
`--exclude-junctions` la junction non viene nemmeno creata in destinazione e il prescan conta 1 file.

---

### D8 — Codice morto in produzione ✅ RISOLTO (F29c, 3 Agosto 2026)

**Stato: chiuso e verificato.** Il resto della sezione resta come diagnosi storica.

**Gravità (storica): BASSA.** Verificato per grep sull'intero `src/`, escludendo i moduli di test:

| Elemento | Stato |
|---|---|
| `CopyRequestBuilder` + `CopyRequest::builder()` | Mai invocati. Aggiunti in una release passata come "fluent builder pattern", nessun chiamante. |
| `IngestError::IntegrityFailed` | Mai costruito. Il fallimento di integrità viaggia via `acceptable=false`, non via questa variante. |
| `report::seconds()` | Nessun chiamante. |
| `IngestCache`, `sync_to_cloud`, `register_windows_service` | Solo i rispettivi test (moduli scaffolding già dichiarati non implementati). |

Essendo tutto `pub` dietro `lib.rs`, il `dead_code` lint di rustc non li segnala: la superficie
pubblica maschera il codice morto.

#### ✅ Fix reale e verifica (3 Agosto 2026)

Rimossi `CopyRequestBuilder`, `CopyRequest::builder()`, `IngestError::IntegrityFailed` (e il suo
match arm in `is_transient()`), `report::seconds()`. **`IngestCache` non è stato rimosso**: F28
(vedi sotto) l'ha reso il cuore di `--fast-verify`, quindi è passato da "scaffolding orfano" a
codice di produzione con chiamanti reali — resta nella tabella storica sopra solo come nota, non
è più vero che sia morto. `sync_to_cloud`/`register_windows_service` restano scaffolding dietro
flag ancora `[NON IMPLEMENTATO]` (`--cloud-sync-target`/`--install-service`), non toccati da
questo fix.

`cargo build` pulito (nessun warning `dead_code` residuo su questi simboli, che comunque il lint
non avrebbe mai segnalato da solo — vedi sopra).

---

### D9 — Volume dei log ingestibile su dataset reali ✅ RISOLTO (F27, 3 Agosto 2026)

**Stato: chiuso e verificato.** Il resto della sezione resta come diagnosi storica.

**Gravità (storica): MEDIA.** `DEFAULT_FILTER = "robocopy_ingest=debug,warn"` produce **una riga di
log per file** (`robocopy transferred file`, `checksum matches`). Misurato sui run reali di questa
sessione:

| Run | File | Righe di log | Dimensione |
|---|---|---|---|
| `repos` → `provarust` | 59.963 | 121.576 | ~19 MB |
| `claude-code` → `provarust2` | 32.027 | 32.122 | ~5 MB |

Estrapolando ai "milioni di file" dichiarati come caso d'uso: **log da diversi GB per singola
esecuzione**, senza rotazione, senza `--log-level`, senza `--quiet`. Il canale bounded protegge la RAM
ma non il disco.

#### ✅ Fix reale e verifica (3 Agosto 2026)

Aggiunti `--log-level <trace|debug|info|warn|error>` (default `debug`, invariato) e `--quiet`
(scorciatoia per `--log-level warn`, mutuamente esclusivo con `--log-level` via `conflicts_with` di
clap — non un "vince il primo" silenzioso). `RUST_LOG` continua a vincere su entrambi, come prima.
Rotazione (`logging::rotate_if_needed`): se il log esistente al path indicato supera
`--log-max-bytes` (default 20 MB, la soglia osservata sopra) all'avvio, viene spostato in
`<path>.1` (shiftando `.1`→`.2` ecc. fino a `--log-max-backups`, default 3, il più vecchio
scartato) prima di aprire un file fresco — non è una rotazione "live" durante l'esecuzione.

**Verificato con il binario compilato**: `tests/cli_smoke.rs::quiet_suppresses_per_file_debug_lines_in_the_real_log`
confronta il log reale di due run (con e senza `--quiet`) e conferma l'assenza delle righe `DEBUG`
per-file; `tests/cli_smoke.rs::oversized_log_is_rotated_by_a_real_run` pre-semina un log da 1000
byte, lancia il binario con `--log-max-bytes 500 --log-max-backups 2`, e verifica che il contenuto
vecchio sia preservato in `<path>.1` mentre il nuovo run scrive in un file fresco.

---

### D10 — Il grafo rigenerato non regge ancora la reachability

**Gravità: BASSA (strumentazione).** La rigenerazione ha portato il grafo da 168 nodi / 409 archi / 4
file a **580 nodi / 1174 archi / 24 file**, con 22 community coincidenti con i moduli reali e archi
inter-file effettivi: un miglioramento netto e verificabile. Ma la query di reachability da
`main`/`lib` proposta dall'audit originale **continua a non funzionare**:

```
roots=2 reachable=33/580 unreachable=547
```

Il motivo è che l'estrattore AST emette i metodi come nodi non qualificati (`.encrypt()`,
`.decrypt()`, `.add_bytes()`) senza il tipo proprietario, quindi le chiamate cross-modulo non si
risolvono e 547 nodi risultano falsamente irraggiungibili. **Il risultato non va usato come gate
anti-dead-code**: il codice morto reale (D8) è stato trovato per grep, non con il grafo.

---

## 💡 3.2 Opportunità di miglioramento (non difetti)

Proposte ordinate per rapporto valore/rischio, motivate da problemi osservati sul campo:

| # | Proposta | Motivazione operativa |
|---|---|---|
| **O1** | **Snapshot VSS (Volume Shadow Copy)** prima della copia | I file bloccati da altri processi falliscono in modo permanente e fanno esaurire il budget di retry. Osservato realmente in sessione. È *la* funzionalità che distingue un tool di backup da una copia. |
| **O2** | ✅ **Implementato (F28, 3 Agosto 2026)**: `--fast-verify` via `cache.rs` | Deviazione consapevole dal testo originale: non "file che robocopy dichiara copiati" (il parser dell'output di robocopy non espone nomi file, solo byte totali, e reimplementarlo per questo sarebbe stato più rischioso del guadagno) ma file il cui size+mtime **sorgente** coincidono con l'ultima verifica riuscita in `<dest>/.ingest_cache`. Un file che fallisce non viene mai messo in cache come fidato. Verificato su tre scenari black-box reali: skip su run invariato, ri-verifica del solo file cambiato, mai-fidarsi-di-un-file-fallito. |
| **O3** | ✅ **Implementato (F25a/F25b, 3 Agosto 2026)**: Cifratura a blocchi + comando di decifratura | Risolve D3+D4 insieme e rende `--encrypt-aes256` effettivamente utilizzabile. |
| **O4** | ✅ **Implementato (F27, 3 Agosto 2026)**: `--log-level` / `--quiet` + rotazione dei log | Risolve D9 senza perdere l'audit trail quando serve. |
| **O5** | **Checkpoint e ripresa dei trasferimenti interrotti** | Un `Ctrl+C` o un calo della share su un trasferimento da ore riparte oggi da zero. |
| **O6** | ✅ **Implementato (F29a, 3 Agosto 2026)**: xxHash3 come terzo algoritmo di integrità | Per rilevare corruzione (non manomissione) è ~5-10x più veloce di BLAKE3. Aggiunta la dipendenza `xxhash-rust`; documentato chiaramente come non-crittografico in help text e report. |
| **O7** | ✅ **Implementato (F29b, 3 Agosto 2026)**: Exit code dedicato per fallimento di integrità | `EXIT_INTEGRITY_FAILED = 4`, distinto da `1` (trasferimento fallito). `run()` ora restituisce l'exit code direttamente invece di un `bool`. |
| **O8** | **Endpoint metriche reale (Prometheus/OpenMetrics)** | `--serve-dashboard` è stato rimosso (era una pagina statica mock). Il notify-server axum introdotto in F-notify è il punto naturale su cui montare un endpoint `/metrics` scrapabile — stesso processo, stesso runtime. |
| **O9** | **`--exclude-junctions` (`/XJ`) e `--exclude-attributes` (`/XA`)** | Risolve D7 e copre i casi enterprise (file di sistema/nascosti). |
| **O10** | **Profilo multi-sorgente nel file TOML** | Il RUNBOOK descrive workflow multi-sorgente eseguiti a mano, un run per sorgente: il config potrebbe descriverli in un file solo. |

---

## 📋 3.3 Sintesi delle priorità

| Priorità | Voci | Razionale |
|---|---|---|
| **P0** | ~~D1~~ ✅, ~~D3~~ ✅, ~~D4~~ ✅ | Tutte e tre risolte e verificate: D1 il 31 Luglio 2026 (F24), D3/D4 il 3 Agosto 2026 (F25a/F25b). Nessun difetto P0 aperto al momento. |
| **P1** | D2, D5, D6, D7 | Correttezza e coerenza: flag muti, blocco del runtime, versionamento dello schema, semantica delle junction. |
| **P2** | D8, D9, D10 + O1-O10 | Debito tecnico, ergonomia operativa ed evoluzione funzionale. |

**Lezione metodologica ricorrente**: D1 e D2 erano *invisibili ai test* perché i test verificavano
l'unità sottostante (`build_restore_args`) o non verificavano affatto (flag mai letti). Il difetto non
è nel codice ma nel **livello a cui si testa**: finché il binario compilato non viene eseguito con gli
argomenti che l'utente digita davvero, la documentazione può continuare a divergere dal comportamento.

---

*Documento salvato in `ANALYSIS.md` ed allineato con le direttive di sviluppo del repository.*
