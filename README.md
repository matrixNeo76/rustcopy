# 🚀 robocopy-ingest-cli (rustcopy)

> **CLI High-Performance in Rust per Ingestion Massiva, Backup Enterprise, Disaster Recovery e Real-Time Web Monitoring su Windows (e Cross-Platform).**

`robocopy-ingest-cli` è uno strumento da riga di comando per il **trasferimento, sincronizzazione, backup e verifica di integrità di grandi volumi di dati (da 50 GB a oltre 10 TB con milioni di file)**.  
Combina la potenza nativa di `robocopy.exe` su Windows con le garanzie di sicurezza della memoria, concorrenza multi-threaded ed asincronia di Rust.

---

## 💡 Perché usare robocopy-ingest-cli?

Quando si gestiscono backup o ingestion di volumetrie massive su Windows, l'uso diretto di script PowerShell (`Copy-Item`) o invocazioni grezze di `robocopy.exe` presenta problemi critici:
- **Deadlock su pipe stdout/stderr** quando robocopy emette milioni di righe di diagnostica.
- **Saturazione della RAM (OOM)** durante il logging o la generazione dei report JSON per milioni di file.
- **Mancanza di verifica di integrità automatica e veloce** (checksum checksum single-thread troppo lenti).
- **Mancanza di visibilità in tempo reale** o notifiche per le pipeline CI/CD aziendali.

`robocopy-ingest-cli` risolve alla radice tutti questi problemi!

---

## 🛠️ Come Funziona la Pipeline (Fase per Fase)

Il tool orchestra ogni operazione attraverso 6 fasi distinte:

```mermaid
graph LR
    A[1. Inventario & Pre-scan] --> B[2. Trasferimento Robocopy Stream]
    B --> C[3. Retry Esterni & Backoff]
    C --> D[4. Verifica Integrità Rayon]
    D --> E[5. Cifratura & Cloud Sync]
    E --> F[6. Live Monitoring & Report]
```

1. **Scansione Iniziale (Prescan)**: Mappa l'albero della directory sorgente (`walkdir`), calcola le dimensioni ed esegue il matching dei filtri glob (`--pattern "*.csv"`). Se si gestiscono milioni di file, il flag `--no-prescan` avvia il trasferimento all'istante senza attese.
2. **Trasferimento Robocopy Streaming**: Invoca `robocopy.exe` su Windows iniettando i flag ottimali (`/MT:N` thread automatici sulle CPU dell'host, `/COPYALL` permessi ACL, `/DCOPY:DAT` timestamp directory, `/MIR` mirroring, `/IPG` throttling). Legge stdout e stderr tramite buffer binario riutilizzato (drenati su thread separati per evitare deadlock), decodificando i caratteri OEM (CP850) con una tabella dedicata (`src/oem_codec.rs`) invece di un fallback UTF-8 silenzioso.
3. **Retry Esterni & Resilience**: Se Robocopy restituisce exit code transitori di blocco file (codici 8, 9, 11), l'invocazione viene ripetuta automaticamente con backoff esponenziale. Se si preme `Ctrl+C`, l'applicazione intercetta il segnale e termina **solo** il processo `robocopy.exe` figlio effettivamente in corso (tramite il suo PID), senza toccare altri eventuali processi robocopy in esecuzione sull'host.
4. **Verifica Integrità Multi-Threaded**: Verifica la corrispondenza dei checksum tra sorgente e destinazione utilizzando **Rayon** su tutte le CPU dell'host. Supporta **SHA-256** ed l'algoritmo **BLAKE3** (3-5x più veloce).
5. **Cifratura**: Supporta la cifratura **AES-256-GCM** reale (`--encrypt-aes256`) dei file in destinazione a fine trasferimento, con nonce casuale per file. La sincronizzazione diretta verso S3/Azure (`--cloud-sync-target`) è **riservata ma non implementata** (vedi tabella flag).
6. **Reporting & Notifiche**: Scrive un report JSON completo con metadati sull'host, genera una **Dashboard HTML Standalone** (`--html-report-path`, con escaping di ogni valore interpolato) e invia un alert **HTTP/HTTPS Webhook** (`--webhook-url`, con timeout ed errori realmente riportati). `--serve-dashboard` avvia un server HTTP che espone **una pagina di stato statica**, non un dashboard con dati live.

---

## 📌 Guida ai Flag CLI e Mappatura Robocopy

| Flag CLI | Default | Flag Robocopy | Descrizione Operativa |
|---|---|---|---|
| `--source <PATH>` | *obbligatorio* | 1° arg | Percorso della directory sorgente. |
| `--dest <PATH>` | *obbligatorio* | 2° arg | Percorso della directory di destinazione. |
| `--pattern <GLOB>` | `*` | 3° arg | Pattern dei file da includere nell'ingestion (default `*` = tutti i file). |
| `--config <PATH>` | *nessuno* | — | Carica la configurazione da un file TOML riutilizzabile. |
| `--threads <N>` | *CPU logiche* | `/MT:N` | Thread paralleli di copia per Robocopy (1-128). |
| `--preserve-acl` | `false` | `/COPYALL` | Preserva i permessi di sicurezza NTFS e le ACL di dominio. |
| `--preserve-timestamps` | `false` | `/DCOPY:DAT` | Preserva le date di creazione e modifica delle directory. |
| `--long-paths` | `false` | — | Attiva il prefisso `\\?\` per percorsi lunghi oltre 240 caratteri. |
| `--mirror` | `false` | `/MIR` | Sincronizza ed elimina i file in destinazione non presenti in sorgente. Senza `--force-purge`, se ci sono file estranei in destinazione l'esecuzione si interrompe (exit code 3) o chiede conferma a console. |
| `--force-purge` | `false` | — | Disattiva la soglia di protezione per la modalità `--mirror` (F21). |
| `--exclude-files <GLOB>` | *nessuno* | `/XF` | Esclude file corrispondenti ai pattern indicati (ripetibile). |
| `--exclude-dirs <GLOB>` | *nessuno* | `/XD` | Esclude directory corrispondenti ai pattern indicati (ripetibile). |
| `--min-age-days <N>` | *nessuno* | `/MINAGE:N` | Esclude i file modificati negli ultimi N giorni. |
| `--max-age-days <N>` | *nessuno* | `/MAXAGE:N` | Esclude i file più vecchi di N giorni. |
| `--bandwidth-limit-mbps <N>`| *nessuno* | `/IPG` | Limita la banda di trasferimento a N MB/s. |
| `--no-prescan` | `false` | — | Salta la scansione preventiva ed avvia immediatamente la copia. |
| `--verify-integrity` | `false` | — | Esegue la verifica dei checksum sorgente vs destinazione a fine copia. |
| `--hash-algo <ALGO>` | `sha256` | — | Algoritmo per la verifica checksum: `sha256` o `blake3`. |
| `--html-report-path <PATH>`| *nessuno* | — | Genera un report visivo autonomo in formato HTML (valori interpolati sempre sottoposti ad escaping). |
| `--serve-dashboard <PORT>`| *nessuno* | — | **[PARZIALE]** Avvia un server HTTP che serve una pagina di stato statica; non trasmette dati live. |
| `--webhook-url <URL>` | *nessuno* | — | Trasmette una notifica HTTP/HTTPS POST JSON a fine job (timeout 10s, errori reali riportati, non più ignorati). |
| `--restore-from <PATH>` | *nessuno* | — | **[ROTTO — vedi D1/F24]** Modalità Disaster Recovery: inverte il backup Dest -> Source dal report JSON. Attualmente **non eseguibile**: clap richiede comunque `--source`/`--dest` e rifiuta il valore vuoto, quindi la modalità non è raggiungibile dalla CLI. |
| `--cloud-sync-target <URI>`| *nessuno* | — | **[NON IMPLEMENTATO]** Accettato per compatibilità futura; nessuna sincronizzazione viene eseguita. |
| `--encrypt-aes256 <KEY>` | *nessuno* | — | Cifra ogni file in destinazione con **AES-256-GCM** dopo il trasferimento (nonce casuale per file). `KEY` può essere `env:NOME`, `file:PERCORSO` o una passphrase letterale (sconsigliata: visibile nella process list). |
| `--install-service` | `false` | — | **[NON IMPLEMENTATO]** Accettato per compatibilità futura; nessun servizio viene registrato. |
| `--enable-dedup` | `false` | — | **[NON IMPLEMENTATO]** Accettato per compatibilità futura; nessuna cache di stato viene usata. |
| `--dry-run` | `false` | `/L` | Simula le operazioni senza modificare o copiare file. |

---

## 💻 Esempi d'Uso Pratici

### 1. Ingestion Base Veloci
```powershell
robocopy_ingest.exe --source D:\landing --dest E:\warehouse
```

### 2. Ingestion Enterprise con Dashboard Live, Webhook e Hashing BLAKE3
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

### 3. Ripristino da Disastro (Disaster Recovery Mode) — ⚠️ NON FUNZIONANTE
```powershell
robocopy_ingest.exe --restore-from E:\reports\robocopy_ingest_report.json
```
> **Attenzione**: questo esempio **non funziona** nella versione corrente. Il comando termina con
> `error: a value is required for '--source <PATH>'` perché `--source`/`--dest` restano obbligatori
> anche in modalità restore. Difetto tracciato come **D1** in `ANALYSIS.md` e pianificato come **F24**
> nella milestone 5.2.0. Nel frattempo il ripristino va eseguito come copia normale invertendo
> manualmente sorgente e destinazione.

---

## 🏗️ Architettura e Documentazione Estesa

Per dettagli tecnici approfonditi, diagrammi architetturali e roadmap di sviluppo consultare:
- 📖 **[RUNBOOK.md](file:///c:/Users/auresystem/repos/robocopy-ingest-cli/RUNBOOK.md)** — Manuale operativo, copie multi-sorgente e comandi reali verificati.
- 📄 **[ARCHITECTURE.md](file:///c:/Users/auresystem/repos/robocopy-ingest-cli/ARCHITECTURE.md)** — Diagrammi di sequenza, gestione memoria anti-OOM e struttura interna dei moduli.
- 📊 **[ANALYSIS.md](file:///c:/Users/auresystem/repos/robocopy-ingest-cli/ANALYSIS.md)** — Diagnosi delle criticità storiche e validazione dei 123 test.
- 🗺️ **[ROADMAP.md](file:///c:/Users/auresystem/repos/robocopy-ingest-cli/ROADMAP.md)** — Diagramma Gantt dello storico delle release e pianificazione futura.
- 🤖 **[AGENTS.md](file:///c:/Users/auresystem/repos/robocopy-ingest-cli/AGENTS.md)** — Linee guida per sviluppatori e contributori AI.

---

## 🧪 Esecuzione dei Test (140 Test Superati)

```bash
cargo test
```

Esito: `test result: ok.` su tutti i target (124 unit test di libreria, 10 test black-box del binario, 6 test di integrazione della pipeline).

---

## 📄 Licenza

Rilasciato sotto licenza **MIT**.
