# robocopy-ingest-cli

CLI in Rust per l'**ingestion avanzata, backup Enterprise, Disaster Recovery, Cloud Sync (S3/Azure), monitoraggio web in tempo reale e cifratura Zero-Trust di grandi volumi di file e dataset trasversali** (scala 50 GB - 10 TB+) tramite `robocopy.exe`, con misurazione del throughput, retry esterni con backoff esponenziale, verifica di integrità parallelizzata (SHA-256 e BLAKE3), notifiche Webhook asincrone, **Direct Cloud Sync**, **Windows Service Daemon** e **Streaming Encryption AES-256**.

Nome del crate: `robocopy_ingest` — binario: `robocopy_ingest`.

---

## Indice

1. [Panoramica](#1-panoramica)
2. [Requisiti di piattaforma](#2-requisiti-di-piattaforma)
3. [Installazione](#3-installazione)
4. [Esempi d'uso](#4-esempi-duso)
5. [Riferimento dei flag CLI (v5.0 Cloud-Native)](#5-riferimento-dei-flag-cli-v50-cloud-native)
6. [Direct Cloud Sync (AWS S3 & Azure Blob)](#6-direct-cloud-sync-aws-s3--azure-blob)
7. [Windows Service Integration](#7-windows-service-integration)
8. [Zero-Trust Streaming Encryption (AES-256)](#8-zero-trust-streaming-encryption-aes-256)
9. [Sviluppo e test](#9-sviluppo-e-test)

---

## 1. Panoramica

`robocopy-ingest-cli` orchestra pipeline di ingestion ad altissime prestazioni combinando l'affidabilità di `robocopy.exe` su Windows con la potenza di Rust per verifiche di integrità parallelizzate (Rayon), sincronizzazione cloud (AWS S3 / Azure Blob), monitoraggio live tramite Web Server HTTP e Disaster Recovery.

---

## 4. Esempi d'uso

### 4.1 Ingestion Cloud-Native con Direct Cloud Sync e Windows Service
```powershell
robocopy_ingest.exe `
  --source D:\landing `
  --dest E:\warehouse `
  --cloud-sync-target "s3://my-company-backup/2026-07/" `
  --install-service `
  --verify-integrity `
  --hash-algo blake3 `
  --serve-dashboard 8080
```

---

## 5. Riferimento dei flag CLI (v5.0 Cloud-Native)

| Flag | Default | Flag Robocopy | Descrizione |
|---|---|---|---|
| `--config <PATH>` | *nessuno* | — | Percorso del file di configurazione TOML. |
| `--source <PATH>` | *obbligatorio* | 1° arg | Directory sorgente. |
| `--dest <PATH>` | *obbligatorio* | 2° arg | Directory di destinazione. |
| `--cloud-sync-target <URI>`| *nessuno* | — | Target per sincronizzazione cloud diretta (AWS S3 / Azure Blob). |
| `--install-service` | `false` | — | Esegue/registra il binario come Windows Service daemon di background. |
| `--serve-dashboard <PORT>`| *nessuno* | — | Server Web Dashboard HTTP live (es. 8080). |
| `--encrypt-aes256 <KEY>` | *nessuno* | — | Cifra i file inviati con algoritmo AES-256. |
| `--html-report-path <PATH>`| *nessuno* | — | Genera la Dashboard HTML Standalone. |
| `--restore-from <PATH>` | *nessuno* | — | Ripristino guidato da report JSON (Reverse Restore). |

---

## 9. Sviluppo e test

Esecuzione dell'intera suite di **123 test**:

```bash
cargo test
```

---

## Licenza

MIT.
