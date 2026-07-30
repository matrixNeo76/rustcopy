# Roadmap Storica ed Evolutiva — robocopy-ingest-cli

> **Stato del Progetto**: 🟢 **Release 4.0.0 Next-Gen Completata e Verificata**.

---

## 🗺️ Diagramma Gantt Completo del Progetto

```mermaid
gantt
    title Storico & Sviluppo Futuro robocopy-ingest-cli
    dateFormat YYYY-MM-DD
    axisFormat %b %d

    section Release 1.0-3.0 (Completate)
    Core Pipeline, Rayon & TOML Config         :done, f1, 2026-07-20, 5d
    NTFS ACL, Long Paths, Webhook & Restore     :done, f2, 2026-07-26, 4d
    HTML Dashboard & State Cache Deduplication  :done, f3, 2026-07-30, 1d

    section Release 4.0 Next-Gen (Completata)
    Live Web Dashboard HTTP Server (server.rs)  :done, f14, 2026-07-30, 1d
    Zero-Trust Streaming Encryption AES-256     :done, f15, 2026-07-30, 1d
    Release 4.0.0 Next-Gen                      :done, milestone, 2026-07-30, 0d

    section Release 5.0 Cloud-Native (Pianificata)
    Direct Cloud Sync AWS S3 / Azure (cloud.rs) :f18, 2026-08-05, 4d
    Windows Service Native Wrapper (service.rs) :f19, 2026-08-09, 3d
    Real-Time SSE Web Dashboard Stream          :f20, 2026-08-12, 2d
    Release 5.0.0 Cloud-Native                  :milestone, 2026-08-15, 0d
```

---

## 📋 Pianificazione Dettagliata Prossime Fasi (v5.0)

- `[ ]` **Fase 18 — Modulo Cloud Sync (`src/cloud.rs`)**: Sincronizzazione ed il backup diretto di dataset verso bucket S3 / Azure Blob Storage.
- `[ ]` **Fase 19 — Windows Service Integration (`src/service.rs`)**: Avvio nativo come servizio di background persistente registrato nel SCM di Windows.
- `[ ]` **Fase 20 — Real-Time SSE Dashboard Stream**: Trasmissione tramite Server-Sent Events dei dati di throughput in tempo reale verso la dashboard web.
