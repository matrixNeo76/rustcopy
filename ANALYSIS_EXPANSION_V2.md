# Analisi di Espansione & Architettura v2.0 — robocopy-ingest-cli

> **Data Audit Evolutivo**: 2026-07-30  
> **Obiettivo**: Definire la visione e la roadmap tecnica per portare `robocopy-ingest-cli` dalla v1.0 a uno **strumento Enterprise v2.0 di backup, sincronizzazione e ripristino ad altissime prestazioni per Windows (e Cross-Platform)**.

---

## 1. Valutazione dell'Architettura Attuale (v1.0.0)

La versione 1.0.0 ha consolidato una solida base per il trasferimento dati:
- **Engine**: Wrapper efficiente e resiliente per `robocopy.exe` con parsing a zero-allocazioni ed elusione del deadlock su pipe.
- **Integrità**: Verifica checksum parallelizzata con Rayon e algoritmi avanzati (BLAKE3 / SHA-256).
- **Controllo**: Gestione segnali Ctrl+C, profili TOML, retry con backoff e protezione OOM.

Tuttavia, per gestire scenari Enterprise reali su scala **1 TB - 10+ TB** con requisiti stringenti di Disaster Recovery e backup continuativi, emergono nuove opportunità di ampliamento.

---

## 2. Opportunità di Espansione Identificate (Vettori di Crescita v2.0)

### 2.1 ⚡ Preservazione Completa dei Metadati NTFS & ACL (`/DCOPY`, `/COPYALL`)
- **Problema**: Attualmente Robocopy copia il contenuto dei file e la struttura delle cartelle, ma non preserva i timestamp delle directory (`/DCOPY:DAT`) né i permessi di sicurezza NTFS/ACL (`/COPYALL` o `/COPY:DATSOU`).
- **Soluzione v2.0**: Aggiungere opzioni CLI e TOML per `--preserve-timestamps` (`/DCOPY:T`), `--preserve-acl` (`/COPYALL` / `/SEC`), consentendo backup compliant a livello enterprise.

### 2.2 🔌 Webhook & Notifiche Event-Driven (Slack / Discord / Webhook HTTP)
- **Problema**: L'esito dell'ingestion è attualmente consultabile solo tramite il report JSON locale o gli exit code.
- **Soluzione v2.0**: Integrare un sistema di notifiche HTTP/Webhook asincrono che invia un payload JSON summary a fine job (o in caso di errore fatale) verso endpoint come Slack, Microsoft Teams, Discord o API di monitoraggio centralizzate.

### 2.3 🛡️ Ripristino Automatico & Modalità Restore (`--restore-mode`)
- **Problema**: L'applicativo è specializzato nell'ingestion/backup da `Source` a `Destination`.
- **Soluzione v2.0**: Introdurre la modalità inversa `--restore`, che legge il report JSON di un backup precedente, inverte sorgente e destinazione, ed esegue il ripristino selettivo o totale verificando nuovamente l'integrità dei file.

### 2.4 📂 Supporto per Long Path Windows Nativi (`\\?\` Prefix)
- **Problema**: Su strutture profonde (> 260 caratteri), l'API classica Win32 può generare errori `PathTooLong`.
- **Soluzione v2.0**: Canonicalizzare automaticamente tutti i percorsi sorgente e destinazione aggiungendo il prefisso `\\?\` (o `\\?\UNC\` per le share di rete SMB), sbloccando la gestione di percorsi annidati fino a 32.767 caratteri.

### 2.5 🌐 Supporto Cloud / Remote Sync (Multi-Engine Abstraction)
- **Problema**: Il tool è limitato a storage locale o SMB montato su Windows tramite Robocopy.
- **Soluzione v2.0**: Estendere il trait `CopyEngine` per supportare motori remoti come `RcloneEngine` o `S3Engine`, consentendo la sincronizzazione diretta verso storage S3/Azure Blob/Google Cloud Storage.

---

## 3. Matrice dei Nuovi Requisiti v2.0

| Feature | Categoria | Priorità | Beneficio Operativo |
|---|---|---|---|
| **Preservazione NTFS ACL & Timestamp Dir** | Metadati | 🔴 High | Ripristino conforme ai permessi di dominio Active Directory. |
| **Notifiche Webhook (HTTP POST)** | Integrazione | 🔴 High | Monitoraggio centralizzato in tempo reale delle pipeline CI/CD o notturne. |
| **Long Path Canonicalization (`\\?\`)** | Robustezza | 🔴 High | Gestione di strutture cartelle annidate senza limiti `MAX_PATH`. |
| **Modalità Ripristino (`--restore`)** | Disaster Recovery | 🟠 Medium | Inversione guidata e validata del flusso di backup. |
| **Rotazione ed Archiviazione Report/Log** | Manutenibilità | 🟡 Low | Pulizia automatica dei vecchi file di log e report storici. |

---

## 4. Impatto Architetturale Previsto

```
src/
├── main.rs              orchestrazione: prescan -> copy -> verify -> webhook notification
├── cli.rs               nuovi flag: --preserve-acl, --webhook-url, --long-paths
├── notify.rs            [NUOVO] client HTTP asincrono per invio Webhook (Slack/Teams/Generic)
├── restore.rs           [NUOVO] logica per il ripristino guidato da report JSON
└── engine/
    └── robocopy.rs      aggiunta flag /DCOPY:DAT, /COPYALL, prefisso long path \\?\
```
