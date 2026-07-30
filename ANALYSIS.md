# Analisi Architetturale & Strategia Evolutiva v5.0 Cloud-Native — robocopy-ingest-cli

> **Data Ultimo Audit**: 2026-07-30  
> **Versione Corrente**: **v4.0.0 Next-Gen** (120/120 test passati)  
> **Target Evolutivo**: **v5.0 Cloud-Native & Autonomous Service**

---

## 1. Panoramica ed Esito dello Stato Attuale (v4.0.0)

`robocopy-ingest-cli` ha raggiunto la completa maturità per ambienti Windows Enterprise locali e di rete:
- **Zero-Alloc Streaming Parser**: Lettura dello stdout di Robocopy priva di allocazioni heap per riga con decodifica OEM/ANSI (CP850).
- **Integrità Multi-Core**: Rayon parallel hashing con BLAKE3 / SHA-256 e pre-check dimensioni.
- **Enterprise NTFS Security**: Preservazione permessi ACL (`/COPYALL`), timestamp directory (`/DCOPY:DAT`) e Windows Long Paths (`\\?\`).
- **Live Monitoring & Alerting**: Web Server HTTP integrato (`--serve-dashboard 8080`), notifiche Webhook asincrone (Slack/Teams) e Dashboard HTML standalone.
- **Disaster Recovery & Deduplica**: Restore Mode guidata da report JSON e cache di stato `.ingest_cache`.

---

## 2. Analisi Approfondita per la Versione 5.0 Cloud-Native

Per evolvere l'applicativo verso un vero **Hub di Ingestion e Sync Multi-Cloud Autonomo**, sono stati identificati 4 nuovi moduli ad alto impatto:

### 2.1 ☁️ Modulo Cloud Native Sync: Connettore S3 & Azure Blob Storage (`src/cloud.rs`)
- **Opportunità**: Attualmente il backup trasferisce file solo verso file system locali o SMB montati.
- **Proposta v5.0**: Estendere l'astrazione `CopyEngine` creando il modulo `src/cloud.rs` con supporto nativo (o tramite wrapper per `rclone` / S3 API) per sincronizzare dataset direttamente verso bucket AWS S3 o container Azure Blob Storage.

### 2.2 🛠️ Modalità Servizio Windows Nativo (`src/service.rs`)
- **Opportunità**: Per operare in ambiente server senza richiedere un'interfaccia utente o Task Scheduler esterni, il binario deve potersi installare ed eseguire come vero **Windows Service**.
- **Proposta v5.0**: Integrare la gestione dei segnali di controllo del Service Control Manager (SCM) di Windows via `--install-service` e `--run-service`.

### 2.3 📊 Web Dashboard Reattiva con Grafici Real-Time (SSE / WebSocket)
- **Opportunità**: Il Live Server HTTP attuale restituisce uno stato base.
- **Proposta v5.0**: Aggiungere uno stream Server-Sent Events (SSE) `/events` al modulo `src/server.rs` per aggiornare grafici reattivi di throughput (MB/s) in tempo reale su browser senza ricaricare la pagina.

### 2.4 🔒 Gestione Credenziali Sicure tramite Windows Credential Manager (`src/credentials.rs`)
- **Opportunità**: Evitare l'inserimento in chiaro di chiavi AES-256 o Webhook URL nel file di configurazione TOML.
- **Proposta v5.0**: Integrare la memorizzazione cifrata delle credenziali nell'archivio protetto di Windows.

---

## 3. Matrice Architetturale dei Moduli (v4.0 Corrente & v5.0 Pianificato)

| Modulo Sorgente | Stato | Ruolo Architetturale |
|---|---|---|
| [src/cli.rs](file:///c:/Users/auresystem/repos/robocopy-ingest-cli/src/cli.rs) | ✅ v4.0 | Parsing Clap, TOML merging, flag ACL, long paths, AES-256, dashboard. |
| [src/engine/robocopy.rs](file:///c:/Users/auresystem/repos/robocopy-ingest-cli/src/engine/robocopy.rs) | ✅ v4.0 | Builder argomenti Robocopy, streaming zero-alloc, decodifica CP850. |
| [src/integrity.rs](file:///c:/Users/auresystem/repos/robocopy-ingest-cli/src/integrity.rs) | ✅ v4.0 | Verification pass con Rayon, BLAKE3 / SHA-256 e cap OOM 10k items. |
| [src/server.rs](file:///c:/Users/auresystem/repos/robocopy-ingest-cli/src/server.rs) | ✅ v4.0 | Live Web Dashboard HTTP server. *(Pianificato v5.0: Stream SSE real-time)* |
| [src/crypto.rs](file:///c:/Users/auresystem/repos/robocopy-ingest-cli/src/crypto.rs) | ✅ v4.0 | Streaming Encryption simmetrica AES-256. |
| `src/cloud.rs` | 🎯 Pianificato v5.0 | Connettore Direct Cloud Sync per AWS S3 / Azure Blob Storage. |
| `src/service.rs` | 🎯 Pianificato v5.0 | Wrapper per installazione ed esecuzione come Windows Service nativo. |

---

## 4. Validazione e Copertura Test

Stato del Build e della Test Suite:
- **107 Unit Test**: Copertura di parsing, bitmask exit code, Rayon, BLAKE3, AES-256, server HTTP e TOML.
- **7 Smoke Test CLI**: Rifiuto argomenti errati, dry-run e report di versione.
- **6 Test Integrati Pipeline**: Esecuzione end-to-end con mock process (`ScriptedRunner`).

**Esito Globale**: 🟢 `120 passed; 0 failed`.
