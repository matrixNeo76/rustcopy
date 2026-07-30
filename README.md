# robocopy-ingest-cli (v4.0.0 Next-Gen)

CLI in Rust per l'**ingestion avanzata, backup Enterprise, Disaster Recovery, monitoraggio web in tempo reale e cifratura Zero-Trust di grandi volumi di file e dataset trasversali** (scala 50 GB - 10 TB+) tramite `robocopy.exe`, con misurazione del throughput, retry esterni con backoff esponenziale, verifica di integrità parallelizzata (SHA-256 e BLAKE3), notifiche Webhook asincrone, **Live Web Dashboard HTTP Server** e **Streaming Encryption AES-256**.

Nome del crate: `robocopy_ingest` — binario: `robocopy_ingest`.

---

## 📚 Indice della Documentazione

1. [Panoramica Funzionale](#1-panoramica-funzionale)
2. [Requisiti e Supporto Piattaforma](#2-requisiti-e-supporto-piattaforma)
3. [Installazione e Build](#3-installazione-e-build)
4. [Guida Rapida ed Esempi d'Uso](#4-guida-rapida-ed-esempi-duso)
5. [Guida Completa ai Flag CLI (v4.0 Next-Gen)](#5-guida-completa-ai-flag-cli-v40-next-gen)
6. [Feature Avanzate Enterprise & Disaster Recovery](#6-feature-avanzate-enterprise--disaster-recovery)
7. [Architettura di Sistema & Moduli](#7-architettura-di-sistema--moduli)
8. [Esecuzione Suite di Test (120 Test)](#8-esecuzione-suite-di-test-120-test)
9. [Pianificazione Evolutiva v5.0 Cloud-Native](#9-pianificazione-evolutiva-v50-cloud-native)
10. [Licenza](#10-licenza)

---

## 1. Panoramica Funzionale

`robocopy-ingest-cli` unisce le altissime prestazioni e l'affidabilità di `robocopy.exe` su sistemi Windows con la sicurezza della memoria e la concorrenza multi-core di Rust.

### ✨ Punti di Forza:
- **Zero Stderr Deadlock & Zero Allocations**: Parsing in streaming dello stdout di Robocopy privo di allocazioni per riga con decodifica trasparente dei set di caratteri OEM/ANSI (CP850 Windows).
- **Integrità Multi-Threaded**: Hashing parallelizzato su tutte le CPU dell'host tramite **Rayon**, con supporto per **BLAKE3** (3-5x più veloce di SHA-256).
- **Protezioni Anti-OOM**: Logging asincrono con buffer limitato (`bounded_channel(10_000)`) e cap di sicurezza a 10.000 elementi sui report per gestire esecuzioni su dataset da oltre 10 Terabyte.
- **Enterprise Metadati & Long Paths**: Supporto per la preservazione dei permessi ACL NTFS (`/COPYALL`), dei timestamp delle cartelle (`/DCOPY:DAT`) e canonicalizzazione dei percorsi profondi (`\\?\`).
- **Disaster Recovery**: Modalità `--restore-from` per il ripristino guidato a partire dai report di backup.
- **Live Monitoring & Notifiche**: Web Server HTTP integrato (`--serve-dashboard 8080`), notifiche Webhook asincrone (Slack/Teams) e Dashboard HTML standalone.

---

## 2. Requisiti e Supporto Piattaforma

| Funzionalità / Piattaforma | Windows 10 / 11 / Server | Linux / macOS |
|---|---|---|
| Compilazione Binario & Crate | ✅ | ✅ |
| Suite Test Completa (120 Test) | ✅ | ✅ |
| Copia Naive Baseline & Cross-platform | ✅ | ✅ |
| Trasferimenti Reali Robocopy & ACL | ✅ | ❌ (Simulato via Mock Testkit) |

---

## 3. Installazione e Build

Requisiti: toolchain Rust stabile (edition 2021, `rust-version = 1.74`).

```bash
# Clone del repository
git clone <repo> robocopy-ingest-cli
cd robocopy-ingest-cli

# Compilazione ottimizzata Release
cargo build --release
```

Il binario compilato sarà disponibile in `target\release\robocopy_ingest.exe`.

---

## 4. Guida Rapida ed Esempi d'Uso

### 4.1 Ingestion Base con Autodetect CPU Core
```powershell
robocopy_ingest.exe --source D:\landing --dest E:\warehouse
```

### 4.2 Ingestion Enterprise Completa con Web Dashboard, Webhook e BLAKE3
```powershell
robocopy_ingest.exe `
  --source D:\landing\2026-07 `
  --dest E:\warehouse\2026-07 `
  --pattern "*.csv" `
  --preserve-acl `
  --preserve-timestamps `
  --long-paths `
  --verify-integrity `
  --hash-algo blake3 `
  --serve-dashboard 8080 `
  --html-report-path E:\reports\dashboard.html `
  --webhook-url "http://api.company.local/webhook/backup"
```

### 4.3 Ripristino Guidato (Disaster Recovery Mode)
```powershell
robocopy_ingest.exe --restore-from E:\reports\robocopy_ingest_report.json
```

---

## 5. Guida Completa ai Flag CLI (v4.0 Next-Gen)

| Flag CLI | Default | Flag Robocopy | Descrizione Operativa |
|---|---|---|---|
| `--config <PATH>` | *nessuno* | — | Carica i parametri da un file di configurazione TOML. |
| `--source <PATH>` | *obbligatorio* | 1° arg | Directory di origine per il trasferimento/backup. |
| `--dest <PATH>` | *obbligatorio* | 2° arg | Directory di destinazione. |
| `--pattern <GLOB>` | `*.csv` | 3° arg | Pattern dei file da includere nell'ingestion. |
| `--threads <N>` | *logical CPUs* | `/MT:N` | Numero di thread paralleli per Robocopy (1-128). |
| `--hash-algo <ALGO>` | `sha256` | — | Algoritmo per la verifica di integrità (`sha256` o `blake3`). |
| `--serve-dashboard <PORT>`| *nessuno* | — | Avvia il server Web Dashboard HTTP live sulla porta specificata (es. 8080). |
| `--encrypt-aes256 <KEY>` | *nessuno* | — | Cifra i dati inviati con algoritmo AES-256 usando la chiave fornita. |
| `--html-report-path <PATH>`| *nessuno* | — | Percorso per la generazione della Dashboard HTML Standalone. |
| `--enable-dedup` | `false` | — | Attiva la cache di stato `.ingest_cache` per saltare file immutati. |
| `--long-paths` | `false` | — | Abilita il prefisso `\\?\` per percorsi lunghi > 240 caratteri. |
| `--preserve-timestamps` | `false` | `/DCOPY:DAT` | Preserva i timestamp originali delle directory. |
| `--preserve-acl` | `false` | `/COPYALL` | Preserva la lista di controllo accessi NTFS/ACL. |
| `--webhook-url <URL>` | *nessuno* | — | Invia notifica HTTP POST JSON al Webhook specificato. |
| `--restore-from <PATH>` | *nessuno* | — | Modalità Disaster Recovery: ripristina invertendo Dest -> Source. |
| `--mirror` | `false` | `/MIR` | Attiva il mirroring (cancella file extra in destinazione). |
| `--verify-integrity` | `false` | — | Esegue la verifica checksum post-trasferimento. |

---

## 6. Feature Avanzate Enterprise & Disaster Recovery

- **Dashboard HTML Standalone**: Generazione automatica di report visivi autonomi (HTML5/SVG) consultabili offline senza dipendenze CDN.
- **Deduplica & Cache di Stato**: La modalità `--enable-dedup` mantiene la mappa `.ingest_cache` riducendo fino al 90% i tempi di I/O sulle ingestion ricorrenti.
- **Disaster Recovery**: La modalità `--restore-from` inverte i percorsi dal report JSON ripristinando la struttura originaria con ricontrollo di integrità.

---

## 7. Architettura di Sistema & Moduli

Per una descrizione esaustiva dell'architettura interna, dei pattern di progettazione, della gestione della memoria anti-OOM e dei diagrammi di sequenza, consultare il documento dedicato:

👉 **[ARCHITECTURE.md](file:///c:/Users/auresystem/repos/robocopy-ingest-cli/ARCHITECTURE.md)**

---

## 8. Esecuzione Suite di Test (120 Test)

L'intero crate è coperto da **120 test automatizzati** che verificano bitmask, decodifica OEM/ANSI, retry logic, Rayon, BLAKE3, Webhook, Restore mode e Live HTTP Server:

```bash
cargo test
```

Esito: `test result: ok. 120 passed; 0 failed`.

---

## 9. Pianificazione Evolutiva v5.0 Cloud-Native

La futura iterazione **v5.0 Cloud-Native** introdurrà:
- **Direct Cloud Sync AWS S3 & Azure Blob Storage (`src/cloud.rs`)**: Trasferimento diretto verso object storage remoti senza richiedere volumi SMB montati.
- **Esecuzione come Servizio Windows Nativo (`src/service.rs`)**: Integrazione con il Service Control Manager (SCM) di Windows via `--install-service`.
- **Stream Dashboard Reattiva SSE**: Server-Sent Events per aggiornare i grafici visivi su browser in tempo reale.

Per la roadmap completa di sviluppo, consultare **[ROADMAP.md](file:///c:/Users/auresystem/repos/robocopy-ingest-cli/ROADMAP.md)** ed **[ANALYSIS.md](file:///c:/Users/auresystem/repos/robocopy-ingest-cli/ANALYSIS.md)**.

---

## 10. Licenza

Rilasciato sotto licenza **MIT**.
