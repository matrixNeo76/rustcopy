---
type: Reference
title: Stack Rust per un Agentic OS & Automation Harness
description: Tabella di riferimento personale sui crate Rust leader per ciascuna area funzionale di un sistema agentico (runtime real-time, orchestrazione LLM, workflow, sandboxing, rete, persistenza, configurazione/sicurezza). Non descrive rustcopy — è materiale di riferimento non tracciato, fuori dal bundle OKF di questo progetto.
status: draft
generated:
  by: user
  at: 2026-09-04T00:00:00Z
---

# Stack Rust per un Agentic OS & Automation Harness

> **Nota di ambito**: questo documento non è tracciato nel bundle OKF di `robocopy-ingest-cli`
> (`scripts/okf-docs.sh`) — il suo contenuto non descrive rustcopy, che non usa la stragrande
> maggioranza dei crate elencati qui sotto (fanno eccezione `tokio`, `serde`, `tracing`, già in uso
> nel progetto). È una nota di riferimento personale sull'ecosistema Rust per un tipo di sistema
> diverso (un "Agentic OS" con agenti LLM, workflow durevoli, sandboxing, RAG), salvata con lo
> stesso frontmatter OKF v0.2 per coerenza di formato, non per appartenenza al progetto. Vive sotto
> `docs/` (non alla radice del repo) proprio per questo: il gate `scripts/okf-docs.sh check`
> rifiuta qualunque `.md` non tracciato alla radice, ma non scansiona `docs/` allo stesso modo.

| Macro-Area | Funzionalità Richiesta | Crate / Tecnologia Leader | Descrizione e Ruolo Chiave nel Sistema |
|---|---|---|---|
| 1. Real-Time & Core Engine | Allocatore ad alte prestazioni | `mimalloc` / `jemalloc` | Sostituzione dell'allocatore di default per ridurre frammentazione e latenza in scenari multi-thread intensivi. |
| | Real-Time / Low Latency | `tokio` (feature `rt-multi-thread`) + `tokio-uring` | Runtime asincrono configurabile; `tokio-uring` per I/O basato su `io_uring` (file system e socket ad altissima velocità). |
| | Concorrenza & Parallelismo | `rayon` / `crossbeam` / `flume` | Canali MPMC e work-stealing per parallelismo dati e task. `crossbeam` per primitive lock-free. |
| | Gestione Stato Globale | `dashmap` / `arc-swap` / `evmap` | Mappe concorrenti lock-free; `evmap` per snapshot coerenti multi-lettore senza blocchi. |
| 2. OS Agentico & AI Core | Orchestrazione Agenti | `rig-core` | Framework per LLM, tool-use, multi-agente, con supporto a provider multipli (OpenAI, Anthropic, locali). |
| | Inferenza Locale (GPU/CPU) | `candle-core` (Hugging Face) + `tokenizers` | Esecuzione di LLM e embedding locali; `tokenizers` per tokenizzazione efficiente. |
| | Memoria Vettoriale (RAG) | `qdrant-client` / `faiss` / `pgvector` | Ricerca semantica; `pgvector` per integrare vettori in PostgreSQL quando serve un unico DB relazionale+vector. |
| | Gestione Conversazioni & Prompt | `llm-chain` / `langchain-rust` | Astrazioni per catene di prompt, template, memoria conversazionale e agenti reattivi. |
| 3. Workflow & Automazione | Workflow Deterministici | `temporal-sdk` | Workflow durevoli: riprendono dopo crash, con cron, segnali e query. |
| | Motore a Stati Flussi | `stateless` / `automata` | FSM per transizioni sicure e verificabili. |
| | Scripting Dinamico | `rhai` / `mlua` / `wasmtime` | Esecuzione di script esterni in sandbox; `wasmtime` per moduli WASM sicuri e performanti. |
| | Comunicazione Publish/Subscribe | `zenoh` | Protocollo leggero per pub/sub, query e storage distribuito, ideale per coordinare agenti su nodi diversi. |
| 4. Sandboxing & Controllo OS | Isolamento Processi | `bollard` / `podman-api` | Gestione container Docker/Podman per eseguire codice non fidato. |
| | Sandboxing a livello syscall | `seccomp` / `landlock` | Filtri syscall e restrizioni filesystem per processi figli senza overhead di container. |
| | Gestione Processi Figli | `shared_child` / `tokio::process` | Monitoraggio e kill di processi; `shared_child` per condividere il child tra task. |
| | File System ad Alta Velocità | `walkdir` / `notify` / `glob` | Scansione ricorsiva, monitoraggio eventi, pattern matching su percorsi. |
| 5. Rete, API & Protocolli | Server Web & WebSocket | `axum` / `actix-web` | API REST e streaming via WebSocket. |
| | Comunicazione Inter-Nodo | `tonic` (gRPC) + `quinn` (QUIC) | gRPC per RPC binario; `quinn` per connessioni QUIC/HTTP3 a bassa latenza e mobilità. |
| | Serializzazione Zero-Copy | `serde` / `bincode` / `rkyv` | `rkyv` per accesso diretto a dati serializzati senza deserializzazione. |
| | WebRTC (streaming audio/video) | `webrtc` | Per comunicazione real-time tra agenti e utenti (voce, video, dati). |
| 6. Frontend & Osservabilità | Dashboard UI (WASM) | `leptos` / `dioxus` | Frontend reattivo in Rust compilato in WASM. |
| | Tracciamento & Log | `tracing` / `opentelemetry` + `tracing-appender` | Tracciamento asincrono distribuito; appender per log su file con rotazione. |
| | Metriche di Sistema | `metrics` / `prometheus` + `metrics-exporter-prometheus` | Raccolta telemetria ed esposizione su endpoint Prometheus. |
| 7. Persistenza & Caching | Database Relazionale | `sqlx` / `diesel` / `tokio-postgres` | `sqlx` per query verificate a compile-time; `tokio-postgres` per accesso asincrono diretto. |
| | Chiave-Valore Embedded | `rocksdb` / `sled` / `redb` | DB embedded ad alte prestazioni; `redb` puro Rust con API moderne e zero dipendenze native. |
| | Database multi-modello (grafi, documenti, time-series) | `surrealdb` | Unico DB per dati relazionali, documenti e grafi, con binding Rust nativo. |
| | Messaggistica / Code | `lapin` (AMQP) / `rdkafka` | Integrazione con RabbitMQ o Kafka per code e streaming eventi. |
| | Caching distribuito | `moka` / `cached` / `redis` | `moka` per cache in-process ad alta concorrenza; `redis` per cache distribuita tra nodi. |
| 8. Configurazione & Sicurezza | Gestione Configurazione | `figment` / `config` | Caricamento e fusione di configurazioni da file, env, etc. con validazione. |
| | Gestione Segreti | `secrecy` / `vault-client` | `secrecy` per evitare che segreti finiscano nei log; `vault-client` per integrare HashiCorp Vault. |
| | TLS & Crittografia | `rustls` / `ring` | `rustls` per TLS moderno senza OpenSSL; `ring` per primitive crittografiche. |
| | Autenticazione & Autorizzazione | `jsonwebtoken` / `oauth2` / `casbin` | JWT, OAuth2, e controllo accessi basato su policy (RBAC/ABAC). |

## Criticità risolte e note

- Aggiunta un'area dedicata a configurazione e sicurezza, spesso trascurata ma essenziale per un
  sistema enterprise.
- Introdotto `mimalloc`/`jemalloc` per prestazioni allocate.
- Sostituito `rt-format` con `tokio-uring` per I/O ad alte prestazioni.
- Aggiunto `webrtc` e `quinn` per comunicazioni moderne.
- Incluso `surrealdb` come database multi-modello, utile per dati relazionali, documentali e a
  grafo in un unico motore.
- Menzionato `tokio-postgres` e `redb` come alternative valide.
- Aggiunto `llm-chain` per astrazione di prompt e catene conversazionali, complementare a `rig-core`.
- Evidenziato `seccomp`/`landlock` per sandboxing più granulare dei container, riducendo overhead.

Questa tabella copre anche gli aspetti di sicurezza, configurazione, e comunicazione real-time,
rendendo lo stack completo per un Agentic OS & Automation Harness di livello enterprise.
