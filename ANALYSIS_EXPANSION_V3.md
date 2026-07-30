# Analisi di Espansione & Architettura v3.0 Ultra-Enterprise — robocopy-ingest-cli

> **Data Audit Evolutivo**: 2026-07-30  
> **Obiettivo**: Definire la visione e la roadmap per estendere `robocopy-ingest-cli` verso uno **strumento v3.0 di classe Ultra-Enterprise con orchestrazione Daemon/Service, Dashboard Web/UI HTML interattiva e crittografia Client-Side (AES-256-GCM)**.

---

## 1. Stato Attuale dell'Applicativo (v2.0.0 Enterprise)

La versione v2.0.0 Enterprise ha completato la gestione sicura del backup su Windows con:
- Motore Robocopy altamente performante con parsing a zero-allocazioni;
- Preservazione metadati NTFS & ACL (`/COPYALL`, `/DCOPY:DAT`);
- Supporto Windows Long Paths (`\\?\`);
- Hashing parallelizzato multi-core con Rayon (BLAKE3 / SHA-256);
- Notifiche asincrone HTTP Webhook (Slack / Teams / Discord);
- Disaster Recovery & Restore Mode guidato da report JSON (`--restore-from`).

---

## 2. Nuove Opportunità di Espansione (Visione v3.0)

### 2.1 📊 Generatore di Dashboard / Report HTML Interattivo (`--html-report`)
- **Problema**: I report JSON sono eccellenti per il consumo da parte di macchine o script, ma difficili da analizzare visivamente da parte degli amministratori di sistema per job con centinaia di migliaia di file.
- **Soluzione v3.0**: Introdurre la generazione automatica di un **report HTML standalone con grafici interattivi** (tramite Chart.js embeddato senza dipendenze esterne online) che illustra visivamente:
  - Throughput nel tempo (MB/s);
  - Distribuzione per dimensione e tipo dei file;
  - Mappa visiva degli errori / file mancanti o corrotti.

### 2.2 🔒 Crittografia Client-Side / Zero-Trust Backup (`--encrypt-aes256`)
- **Problema**: Nelle ingestion su storage di destinazione non fidati o cloud montati, i file sensibili (CSV aziendali, PII) rimangono in chiaro.
- **Soluzione v3.0**: Integrare un passaggio di crittografia in streaming **AES-256-GCM** post-copia o pre-invio con chiave passata via variabile d'ambiente (`INGEST_ENCRYPTION_KEY`) per backup Zero-Trust.

### 2.3 ⏱️ Scheduler Interno & Modalità Daemon / Windows Service (`--daemon`)
- **Problema**: L'esecuzione periodica dei backup richiede la configurazione di Windows Task Scheduler o cron esterni.
- **Soluzione v3.0**: Aggiungere una modalità daemon nativa (`--daemon --cron "0 2 * * *"`) che trasforma il binario in un processo di background persistente o Servizio Windows integrato che esegue i backup a intervalli programmati senza dipendenze esterne.

### 2.4 🧩 Modulo di Deduplica Preventiva su Checksum / Size (`--dedup`)
- **Problema**: Nelle ingestion incrementali frequenti, riricopiare o ricalcolare checksum su file immutati spreca I/O di disco.
- **Soluzione v3.0**: Mantenere un database SQLite locale o file di stato `.ingest_cache` per saltare la ricopia e la ri-verifica di file le cui dimensioni e timestamp di modifica non sono cambiati dall'ultima run valida.

---

## 3. Priorità e Roadmap Pianificata v3.0

| Feature | Categoria | Priorità | Impatto Operativo |
|---|---|---|---|
| **Report HTML Interattivo** | Visualizzazione | 🔴 High | Audit e presentazione visiva immediata per i manager IT. |
| **Deduplica & Cache di Stato (`.ingest_cache`)** | Performance | 🔴 High | Riduzione fino al 90% del tempo su backup incrementali. |
| **Crittografia AES-256-GCM** | Sicurezza | 🟠 Medium | Conformità Zero-Trust per backup su storage remoti. |
| **Modalità Daemon / Cron integrato** | Orchestrazione | 🟡 Low | Automatizzazione indipendente da scheduler di SO. |

---

## 4. Struttura Architetturale Prevista v3.0

```text
src/
├── main.rs              orchestrazione v3: prescan -> copy -> verify -> html report -> webhook
├── html_report.rs       [NUOVO] generatore di dashboard HTML interattiva standalone
├── cache.rs             [NUOVO] motore di deduplica e stato per backup incrementali veloci
├── notify.rs            client Webhook HTTP POST
└── restore.rs           disaster recovery guidato
```
