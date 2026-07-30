# Analisi Architetturale & Stato del Progetto — robocopy-ingest-cli

> **Data Ultimo Audit**: 2026-07-30  
> **Versione Progetto**: **v4.0.0 Next-Gen**  
> **Esito Audit**: 🟢 **PRONTO PER IL PRODUZIONE ENTERPRISE (120/120 test superati)**.

---

## 1. Panoramica del Sistema

`robocopy-ingest-cli` è una soluzione avanzata sviluppata in Rust progettata per l'**ingestion, il backup Enterprise, il Disaster Recovery ed il monitoraggio in tempo reale di dataset trasversali e volumetrie massive (da 50 GB a oltre 10 TB)**.

Il sistema combina l'efficienza nativa del binario Windows `robocopy.exe` con la sicurezza della memoria, il multi-threading concorrente (Rayon) e le capacità asincrone (Tokio) di Rust.

---

## 2. Diagnosi Criticità Storiche & Contromisure Adottate

Nel corso dell'evoluzione dell'applicativo sono state identificate e risolte diverse criticità architetturali tipiche dei backup su larga scala:

### 2.1 🔴 Deadlock su StdPipe di Robocopy
- **Diagnosi**: Invocando `robocopy.exe` con sia `stdout` che `stderr` reindirizzati in pipe senza un thread lettore dedicato per `stderr`, il buffer del sistema operativo (4–64 KB) si saturava provocando il congelamento in deadlock dell'intero processo.
- **Soluzione Implementata (`src/engine/robocopy.rs`)**: `stderr` viene reindirizzato su `Stdio::null()`. L'output `stdout` viene letto tramite uno streaming binario `read_until` a buffer riutilizzato e decodificato con `from_utf8_lossy` per gestire in modo sicuro i set di caratteri OEM/ANSI Windows (CP850/CP437).

### 2.2 🔴 Windows Argument Quoting & Trailing Backslashes
- **Diagnosi**: Percorsi sorgente/destinazione che terminano con un backslash (es. `"C:\Data\"`) causavano l'escaping della virgoletta di chiusura (`\"`) durante l'invocazione della shell, fondendo il percorso con i flag CLI successivi.
- **Soluzione Implementata (`src/engine/robocopy.rs`)**: La funzione `normalize_path_arg` rimuove automaticamente i separatori finali e applica il prefisso `\\?\` per percorsi lunghi (`--long-paths`).

### 2.3 🔴 Prevenzione OOM su Logging Asincrono & Report JSON
- **Diagnosi**: Durante il trasferimento di milioni di file, l'emissione di log per-file non limitata poteva saturare la memoria RAM se il disco di scrittura fosse stato lento.
- **Soluzione Implementata (`src/logging.rs` & `src/integrity.rs`)**:
  - Il canale di logging asincrono è configurato come `bounded_channel(10_000)` con inoltro non-bloccante (`try_send`).
  - L'elenco dei file disallineati nel report JSON è troncato a **10.000 elementi** (`MAX_REPORTED_ERRORS`) con indicatore `truncated: true`.

---

## 3. Matrice Architetturale dei Moduli (v4.0.0)

| Modulo | Responsabilità Principali | File Sorgente |
|---|---|---|
| **CLI & Parser** | Definition delle opzioni, parsing delle direttive e merge da profili TOML. | [src/cli.rs](file:///c:/Users/auresystem/repos/robocopy-ingest-cli/src/cli.rs), [src/config.rs](file:///c:/Users/auresystem/repos/robocopy-ingest-cli/src/config.rs) |
| **Robocopy Engine** | Costruzione argomenti (`/MT`, `/COPYALL`, `/DCOPY:DAT`, `/MIR`, `/IPG`, `\\?\`), execution streaming e parsing stdout zero-alloc. | [src/engine/robocopy.rs](file:///c:/Users/auresystem/repos/robocopy-ingest-cli/src/engine/robocopy.rs) |
| **Integrità & Security** | Verifica checksum parallelizzata con Rayon, algoritmi BLAKE3/SHA-256 e cifratura streaming AES-256. | [src/integrity.rs](file:///c:/Users/auresystem/repos/robocopy-ingest-cli/src/integrity.rs), [src/crypto.rs](file:///c:/Users/auresystem/repos/robocopy-ingest-cli/src/crypto.rs) |
| **Monitoring & Server** | Server HTTP multithread integrato per live dashboard web, dispatcher notifiche Webhook HTTP POST e generatore HTML standalone. | [src/server.rs](file:///c:/Users/auresystem/repos/robocopy-ingest-cli/src/server.rs), [src/notify.rs](file:///c:/Users/auresystem/repos/robocopy-ingest-cli/src/notify.rs), [src/html_report.rs](file:///c:/Users/auresystem/repos/robocopy-ingest-cli/src/html_report.rs) |
| **Disaster Recovery** | Ripristino guidato e reverse restore mode partendo dal report JSON di backup. | [src/restore.rs](file:///c:/Users/auresystem/repos/robocopy-ingest-cli/src/restore.rs) |
| **Deduplica & Cache** | Mappa di stato `.ingest_cache` basata su timestamp e dimensione file. | [src/cache.rs](file:///c:/Users/auresystem/repos/robocopy-ingest-cli/src/cache.rs) |

---

## 4. Validazione della Suite di Test

L'intero progetto è validato da **120 test automatizzati**:
- **107 Unit Test**: Copertura di bitmask, regole di retry, parsing stdout, decodifica OEM/ANSI, Rayon, BLAKE3, AES-256, HTTP Live Server ed iniezione argomenti.
- **7 Smoke Test CLI**: Rifiuto argomenti non validi, dry-run, report di versione e help.
- **6 Test di Integrazione Pipeline**: Esecuzione end-to-end con motore Robocopy mockato (`ScriptedRunner`).

**Esito Esecuzione Test**: 🟢 `120 passed; 0 failed`.
