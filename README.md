---
type: Reference
title: robocopy-ingest-cli (rustcopy) — README
description: Panoramica del progetto — cosa fa, come si installa, primi comandi e indice della documentazione.
status: stable
generated:
  by: process:claude-code
  at: 2026-08-06T00:00:00Z
verified:
  by: process:claude-code
  at: 2026-09-14T00:00:00Z
---

<p align="center">
  <img src="images/rustcopy.jpg" alt="Logo di rustcopy: un ingranaggio con due frecce intrecciate e la scritta RUSTCOPY" width="220">
</p>

# 🚀 robocopy-ingest-cli (rustcopy)

[![CI](https://github.com/matrixNeo76/rustcopy/actions/workflows/ci.yml/badge.svg)](https://github.com/matrixNeo76/rustcopy/actions/workflows/ci.yml)
[![Audit di sicurezza](https://github.com/matrixNeo76/rustcopy/actions/workflows/security-audit.yml/badge.svg)](https://github.com/matrixNeo76/rustcopy/actions/workflows/security-audit.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Crate version](https://img.shields.io/badge/version-7.8.1-informational.svg)](Cargo.toml)

<p>
  <a href="https://slint.dev" title="La console grafica di rustcopy è fatta con Slint">
    <img src="https://raw.githubusercontent.com/slint-ui/slint/master/logo/MadeWithSlint-logo-whitebg.png" alt="Made with Slint" width="160">
  </a>
</p>

**Backup e ingestion di grandi volumi di dati su Windows, con verifica di integrità.** `rustcopy`
avvolge `robocopy.exe` in un binario Rust che ne risolve i limiti pratici sui dataset enormi —
deadlock sulle pipe, saturazione della RAM, nessuna verifica dei checksum — e aggiunge backup a
generazioni, cifratura, scheduling e notifiche. Progettato per volumi da 50 GB a oltre 10 TB con
milioni di file.

```console
$ rustcopy --source C:\dati --dest E:\backup\dati --verify-integrity --hash-algo blake3 `
    --report-path E:\backup\report.json --log-path E:\backup\ingest.log

Inventory: 8 file(s) matching *, 2.50 MB

Verifying integrity with Blake3...

Source          : C:\dati
Destination     : E:\backup\dati
Inventory       : 8 file(s), 2.50 MB
Robocopy        : 2.50 MB in 0.06s (44.13 MB/s), exit code 1, 0 retry attempt(s)
Integrity       : PASSED (8 file(s) checked, 0 mismatch(es), 0 missing)

JSON report: E:\backup\report.json
Log file   : E:\backup\ingest.log
```

> [!NOTE]
> I backup li esegue **solo la CLI**, e non c'è monitoraggio live: il progresso si segue dalla
> progress bar a terminale; a run concluso restano il report JSON e la dashboard HTML statica
> (`--html-report-path`).
>
> Esiste anche una **console grafica** (componente opzionale dell'installer, `rustcopy-gui.exe`): dalle
> versioni successive alla 7.8.1 è la console **Slint** (`crates/rustcopy-ui`), un solo processo senza runtime web. Avvia la stessa
> CLI come processo separato e permette di: copiare cartelle e file al volo (anche trascinandoli), controllare
> prima cosa si copierà, verificare, **mettere in pausa**, fermare e **riprendere**, **spostare** (copia,
> verifica e solo dopo conferma cancella gli originali), salvare una copia come attività, aprire configurazioni e
> report qualsiasi, leggere lo storico con osservazioni ed esportarlo, vedere le impostazioni risolte con la
> loro provenienza, preparare **proposte** di configurazione in file nuovi, creare e togliere **pianificazioni**,
> gestire le credenziali e ricevere una notifica di sistema a fine copia. Cosa può preparare dipende da un
> **livello di sicurezza** (Prudente di base) che sta nelle impostazioni dell'utente e mai in un file di job;
> un mirror o una pulizia **non presidiati** non partono mai dalla console. La console Tauri precedente
> (`crates/rustcopy-gui`) resta nel repository finché la Slint non supera la
> [checklist di accettazione](CHECKLIST_ACCETTAZIONE_GUI.md). Dettagli in [ROADMAP.md](ROADMAP.md) e
> [`PIANO_GUI_SLINT.md`](PIANO_GUI_SLINT.md).
>
> Dal 10 Settembre 2026 esiste anche `crates/rustcopy-shell` (F85): un'estensione Shell che
> propone "Copia con RustCopy" sul menu di conferma del drag & drop di Explorer (solo cartelle,
> solo tasto destro o fra unità diverse) e avvia la console con la copia già in corso. Dall'11
> Settembre 2026 è anche un componente dell'installer (`gui\shell`, richiede la console), vedi la
> riga F85 di [ROADMAP.md](ROADMAP.md).

---

## 📚 Documentazione

| Documento | Contenuto |
|---|---|
| 📋 **[Riferimento CLI](docs/cli-reference.md)** | Tutti i flag, i codici di uscita e il comportamento dettagliato di ogni funzionalità. |
| 📦 **[Installazione](docs/installation.md)** | Requisiti, installer Windows, deploy silenzioso, notify-server. |
| 📖 **[RUNBOOK](RUNBOOK.md)** | Flussi operativi, copie multi-sorgente, comandi reali verificati sul campo. |
| 📄 **[ARCHITECTURE](ARCHITECTURE.md)** | Diagrammi, pipeline interna, gestione memoria anti-OOM, struttura dei moduli. |
| 🗺️ **[ROADMAP](ROADMAP.md)** | Storico delle release e pianificazione futura. |
| 📊 **[ANALYSIS](ANALYSIS.md)** | Audit di robustezza e difetti storici documentati. |
| 📝 **[CHANGELOG](CHANGELOG.md)** | Cronologia delle versioni. |
| 🤖 **[AGENTS](AGENTS.md)** | Linee guida per sviluppatori e contributori AI. |
| 🖥️ **[Console Slint: piano](PIANO_GUI_SLINT.md)** | Piano a fasi, registro di avanzamento e limiti noti. |
| ✅ **[Checklist di accettazione](CHECKLIST_ACCETTAZIONE_GUI.md)** | Lacune verso la console Tauri, confronto con TeraCopy e Cobian, prove da fare. |
| 🧪 **[Controlli manuali](CONTROLLI_MANUALI_GUI.md)** | Le prove che richiedono una persona o un'altra macchina. |

---

## ⚡ Installazione

`rustcopy` è un eseguibile **portable**: si copiano gli `.exe` e si lanciano da qualunque cartella.

```powershell
# Build dai sorgenti
cargo build --release -p rustcopy-cli --features notify-server
```

Requisiti verificati sul binario compilato:

- **Windows 10 / Windows Server 2016 o successivo.** Dalla 7.7.0 il runtime C è collegato in modo
  statico: il Visual C++ Redistributable **non** serve più.
- **`robocopy.exe` di sistema**, presente su ogni Windows da Vista in poi: non serve installarlo,
  ma il tool non lo include.

Il repo include anche uno script **Inno Setup** (`installer/rustcopy.iss`) — un installer unico
in cui la console grafica è un componente opzionale (F60) — che genera un vero
`setup.exe` con disinstaller, aggiunta al PATH e deploy silenzioso — istruzioni complete in
**[Installazione e distribuzione](docs/installation.md)**, che copre anche il **notify-server**
per le notifiche multi-canale.

---

## 🏃 Primi comandi

```powershell
# Copia con verifica di integrità
rustcopy --source "C:\dati" --dest "E:\backup" --verify-integrity --hash-algo blake3

# Simulazione: mostra cosa farebbe, senza scrivere nulla
rustcopy --source "C:\dati" --dest "E:\backup" --dry-run

# Backup a generazioni: il primo run dev'essere full, poi si incrementa
rustcopy --source "C:\dati" --dest "E:\backup" --backup-type full
rustcopy --source "C:\dati" --dest "E:\backup" --backup-type incremental --keep-generations 3 --force-purge
```

I flag essenziali per l'uso quotidiano:

| Flag | Cosa fa |
|---|---|
| `--source` / `--dest` | Cartella di origine e destinazione. |
| `--verify-integrity` | Confronta i checksum sorgente/destinazione a copia finita. |
| `--hash-algo <sha256\|blake3\|xxh3>` | Algoritmo di verifica; `blake3` è 3-5x più veloce di `sha256`. |
| `--dry-run` | Simula senza modificare nulla. |
| `--config <PATH>` | Carica i parametri da un file TOML riutilizzabile. |
| `--backup-type <full\|incremental\|differential>` | Attiva il backup a generazioni. |

**[→ Riferimento completo: tutti i flag e i codici di uscita](docs/cli-reference.md)**

---

## ✨ Funzionalità

- **Verifica di integrità multi-core** — checksum SHA-256, BLAKE3 o xxHash3 parallelizzati con
  Rayon su tutte le CPU. `--fast-verify` salta i file già verificati e immutati.
- **Backup a generazioni** — full, incrementale e differenziale in sottocartelle separate, con
  ritenzione per cicli (`--keep-generations`) che non orfana mai una catena.
- **Resistente ai dataset enormi** — streaming dell'output di robocopy senza deadlock, logging su
  canale limitato, cifratura a blocchi: la memoria di picco non cresce con la dimensione dei dati.
- **Disaster recovery** — `--restore-from` inverte un backup a partire dal suo report JSON;
  `--resume-from` riprende un run interrotto da `Ctrl+C`.
- **Cifratura AES-256-GCM** — a blocchi da 1 MiB, con la controparte `--decrypt`.
- **Volume Shadow Copy** — `--vss-snapshot` per leggere file bloccati da altri processi.
- **Automazione** — scheduling via Task Scheduler, servizi Windows, comandi pre/post job,
  configurazioni multi-job in TOML.
- **Notifiche** — report JSON, dashboard HTML standalone, webhook HTTP e un **notify-server**
  opzionale che li inoltra su più canali.

**[→ Dettaglio del comportamento di ogni funzionalità](docs/cli-reference.md#-comportamento-dettagliato-per-funzionalità)**

---

## 🧪 Sviluppo

```bash
cargo test --locked --workspace --exclude rustcopy-gui --exclude rustcopy-shell --all-targets                                   # 521 test
cargo test --locked --workspace --exclude rustcopy-gui --exclude rustcopy-shell --all-targets --features rustcopy-cli/notify-server  # 536 test
```

CI su Windows e Linux, `clippy -D warnings` e `cargo fmt --check` su entrambe le configurazioni di
feature, più un audit RustSec settimanale delle dipendenze. Convenzioni e regole architetturali in
[AGENTS.md](AGENTS.md).

---

## 📄 Licenza

Rilasciato sotto licenza **MIT**.
