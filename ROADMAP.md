# 🗺️ Roadmap di Progetto — robocopy-ingest-cli

> **Stato Attuale**: 🟢 **Release 5.0.0 Cloud-Native Completata** | 🟡 **Pianificazione Release 5.1.0 Robustness & Encoding Enhancement**.

---

## 📅 Diagramma Gantt delle Release (v1.0 - v5.1)

```mermaid
gantt
    title Roadmap robocopy-ingest-cli
    dateFormat YYYY-MM-DD
    axisFormat %b %d

    section Milestone Completate
    Release 1.0 - 4.0 Core, Enterprise, Web & Crypto :done, m1, 2026-07-20, 10d
    Release 5.0.0 Cloud-Native (S3/Azure & Service)  :done, m2, 2026-07-30, 1d

    section Release 5.1 Robustness & Encoding (In Corso)
    F21: Mirror Safety Threshold & Confirmation       :active, f21, 2026-07-30, 2d
    F22: OEM CP850 / CP1252 via encoding_rs           :active, f22, 2026-08-01, 2d
    F23: Reliable Child Process Signal Kill (Ctrl+C)  :active, f23, 2026-08-03, 1d
```

---

## 📋 Tabella dei Task e Milestones

| Milestone | Caratteristica / Task | Stato | Descrizione Tecnico-Operativa |
|---|---|---|---|
| **v5.0.0** | **Direct Cloud Sync** | `[x] Completato` | Sincronizzazione ed il backup diretto di dataset verso bucket S3 / Azure Blob Storage (`src/cloud.rs`). |
| **v5.0.0** | **Windows Service Integration** | `[x] Completato` | Registrazione ed avvio nativo come servizio daemon di background (`src/service.rs`). |
| **v5.1.0** | **F21: Mirror Safety Threshold** | `[ ] In Programma` | Controllo di sicurezza preventivo su `--mirror` (richiede conferma interattiva o `--force-purge` se i file in dest eccedono la soglia del 20%). |
| **v5.1.0** | **F22: OEM CP850 Decoder** | `[ ] In Programma` | Sostituzione di `utf8_lossy` con `encoding_rs` per preservare i caratteri accentati nei report JSON/HTML. |
| **v5.1.0** | **F23: Child Process Kill Guard** | `[ ] In Programma` | Intercettazione `Ctrl+C` per inviare `child.kill()` ed arrestare subito `robocopy.exe`. |

---

## 📄 Storico delle Release

- **v1.0.0**: Core CopyEngine, Zero-Alloc Stdout Stream, Rayon Hashing, Bounded Logging.
- **v2.0.0**: Enterprise NTFS ACLs (`/COPYALL`), Long Paths (`\\?\`), Disaster Recovery Restore.
- **v3.0.0**: Standalone HTML5 Dashboard Generator, State Cache & Deduplicazione (`.ingest_cache`).
- **v4.0.0**: Live Web Server HTTP Dashboard, Zero-Trust AES-256 Streaming Encryption.
- **v5.0.0**: Direct Cloud Sync (S3/Azure), Windows Service Daemon Manager.
