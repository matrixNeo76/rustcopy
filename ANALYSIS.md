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

*Documento salvato in `ANALYSIS.md` ed allineato con le direttive di sviluppo del repository.*
