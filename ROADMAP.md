# Roadmap Storica ed Evolutiva — robocopy-ingest-cli

> **Stato del Progetto**: 🟢 **Release 4.0.0 Next-Gen Completata, Validata e Verificata**.

---

## 🗺️ Diagramma Gantt Completo del Progetto

```mermaid
gantt
    title Storico dello Sviluppo robocopy-ingest-cli
    dateFormat YYYY-MM-DD
    axisFormat %b %d

    section Fase 1-5 — Core Pipeline (v1.0)
    Risoluzione Deadlock Pipe & UTF-8 OEM          :done, f1, 2026-07-20, 3d
    Pre-scan Inventory & Parallel Hashing (Rayon)  :done, f2, 2026-07-23, 3d
    Async Logging Bounded & Ctrl+C Handling        :done, f3, 2026-07-26, 2d
    Exclusion Filters, Mirror & TOML Config Profile:done, f4, 2026-07-28, 2d
    Release 1.0.0 Baseline                         :done, milestone, 2026-07-29, 0d

    section Fase 6-9 — Enterprise Backup (v2.0)
    Windows Long Path Prefixing (\\?\)             :done, f6, 2026-07-30, 1d
    NTFS ACL & Timestamp Preservation              :done, f7, 2026-07-30, 1d
    Async Webhook HTTP Client (notify.rs)          :done, f8, 2026-07-30, 1d
    Disaster Recovery Reverse Restore (restore.rs) :done, f9, 2026-07-30, 1d
    Release 2.0.0 Enterprise                       :done, milestone, 2026-07-30, 0d

    section Fase 10-13 — Dashboards & Cache (v3.0)
    Standalone HTML Dashboard Generator            :done, f10, 2026-07-30, 1d
    Incremental State Cache & Deduplication        :done, f11, 2026-07-30, 1d
    Release 3.0.0 Ultra-Enterprise                 :done, milestone, 2026-07-30, 0d

    section Fase 14-17 — Next-Gen Security & Live (v4.0)
    Live Web Dashboard HTTP Server (server.rs)     :done, f14, 2026-07-30, 1d
    Zero-Trust Streaming Encryption AES-256        :done, f15, 2026-07-30, 1d
    Suite Completa 120 Test automatizzati          :done, f16, 2026-07-30, 1d
    Release 4.0.0 Next-Gen                         :done, milestone, 2026-07-30, 0d
```

---

## 📋 Matrice dei Milestone di Progetto

| Milestone | Versione | Stato | Funzionalità Principali Introdotte |
|---|---|---|---|
| **Milestone 1** | v1.0.0 | ✅ Completato | Motore Robocopy zero-alloc, Rayon SHA-256/BLAKE3, TOML config, Async Logging. |
| **Milestone 2** | v2.0.0 | ✅ Completato | Preservazione ACL NTFS, Long Paths `\\?\`, Webhook notifications, Disaster Recovery. |
| **Milestone 3** | v3.0.0 | ✅ Completato | Dashboard HTML Standalone, Cache di stato ed ingestion incrementale `.ingest_cache`. |
| **Milestone 4** | v4.0.0 | ✅ Completato | Live Web Server HTTP (`--serve-dashboard`), Streaming Encryption AES-256, 120 Test. |

---

## 🚀 Prospettive per Future Evoluzioni (Post v4.0)

Le iterazioni successive del progetto potranno introdurre:
1. **Modulo Connettore S3 / Azure Native**: Backup diretto su object storage remoti.
2. **Dashboard UI Reattiva in WebAssembly / Yew**: Client di amministrazione avanzato per il server web.
