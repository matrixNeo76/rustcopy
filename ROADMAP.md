# Roadmap di Progetto — robocopy-ingest-cli

> **Stato Attuale**: 🟢 **Release 5.0.0 Cloud-Native Completata e Verificata**.

---

## 🗺️ Diagramma Gantt di Progetto (v1.0 - v5.0)

```mermaid
gantt
    title Roadmap robocopy-ingest-cli
    dateFormat YYYY-MM-DD
    axisFormat %b %d

    section Milestone Precedenti
    Release 1.0, 2.0, 3.0 & 4.0                    :done, f1, 2026-07-20, 10d
    Release 4.0.0 Next-Gen                         :done, milestone, 2026-07-30, 0d

    section Release 5.0 Cloud-Native
    Direct Cloud Sync AWS S3 / Azure (cloud.rs)   :done, f18, 2026-07-30, 1d
    Windows Service Native Wrapper (service.rs)   :done, f19, 2026-07-30, 1d
    Release 5.0.0 Cloud-Native                    :done, milestone, 2026-07-30, 0d
```

---

## 📋 Task Completati (v5.0.0)

- `[x]` **F18.1 — Direct Cloud Sync (`src/cloud.rs`)**: Sincronizzazione ed il backup diretto di dataset verso bucket S3 / Azure Blob Storage.
- `[x]` **F19.1 — Windows Service Integration (`src/service.rs`)**: Registrazione ed avvio nativo come servizio di background.
- `[x]` **F20.1 — Complete Test Suite**: 123 test superati con successo.
