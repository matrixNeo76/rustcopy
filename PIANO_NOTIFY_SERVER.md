# 📬 Piano di Implementazione — Notify Server (axum)

> **Documento di pianificazione operativa.** Scritto per essere eseguito in una sessione separata,
> senza il contesto della conversazione in cui è stato prodotto. Contiene le decisioni di design già
> prese (con le motivazioni), le fasi ordinate, i criteri di completamento e le insidie note.
>
> *Creato: 31 Luglio 2026 | Target: Release 5.4.0 | Stato: da eseguire*

---

## 1. Obiettivo

Introdurre un **server di notifica in Rust basato su axum** che riceva gli eventi di fine backup
prodotti da `robocopy-ingest-cli` e li inoltri su uno o più canali (log, ntfy, Telegram, webhook
generico Slack/Teams), centralizzando la logica multi-canale in un solo punto invece di replicarla in
ogni script di backup.

---

## 2. Decisioni di design già prese

Queste scelte sono state valutate in fase di analisi: **non vanno ridiscusse in fase di esecuzione**,
salvo emergano ostacoli tecnici concreti.

### 2.1 L'applicativo NON va modificato per inviare

`robocopy-ingest-cli` ha **già** il flag `--webhook-url`, implementato in `src/notify.rs` su
`reqwest`+`rustls`, con timeout di 10s, controllo dello status code ed errore propagato nel report
JSON (campo `webhook_error`). Il notify-server è semplicemente **il destinatario** di ciò che l'app
già spedisce.

**Conseguenza pratica**: negli script non va aggiunta alcuna chiamata `Invoke-RestMethod`. Basta
passare al binario:

```
--webhook-url "http://127.0.0.1:3000/notify"
```

Fare la POST da PowerShell sarebbe un duplicato peggiore dell'originale: l'app conosce byte, file,
esito dell'integrità ed exit code in forma strutturata, mentre lo script dovrebbe riparsare l'output
testuale.

### 2.2 Contratto condiviso come un solo tipo Rust

Mittente e destinatario devono usare **lo stesso `struct`** esportato da `robocopy_ingest::notify`,
non due definizioni parallele. Una duplicazione porterebbe a divergenze silenziose.

> ⚠️ **Difetto reale da cui nasce questa regola**: una bozza iniziale del server definiva un
> `BackupEvent` con `match` su `"success"`/`"error"` minuscoli, mentre l'app invia `"SUCCESS"`/
> `"FAILED"` maiuscoli. Ogni evento sarebbe finito nel ramo `_ => "evento generico"` senza che
> nessuno se ne accorgesse.

### 2.3 Il payload deve essere versionato

Aggiungere `schema_version` al payload di notifica fin dalla versione 1.

> ⚠️ **Motivazione storica**: in questo progetto lo schema di `Mismatch` è stato modificato in modo
> breaking senza incrementare `SCHEMA_VERSION` (difetto **D6** in `ANALYSIS.md`), rendendo i report
> storici non deserializzabili. Non ripetere l'errore su un secondo contratto.

### 2.4 Binario separato e feature-gated

Lo stack server (axum/tower/hyper-server) **non deve entrare nelle dipendenze normali**: il binario di
backup non ne usa nulla e ne verrebbe appesantito inutilmente.

```toml
[dependencies]
axum = { version = "0.8", optional = true }   # verificare la versione corrente prima di fissarla

[features]
notify-server = ["dep:axum"]

[[bin]]
name = "notify-server"
path = "src/bin/notify_server.rs"
required-features = ["notify-server"]
```

`tokio`, `serde`, `serde_json`, `tracing`, `tracing-subscriber`, `toml`, `anyhow`, `thiserror` e
`reqwest` sono **già presenti** nel `Cargo.toml`: non vanno riaggiunti.

### 2.5 `src/server.rs` va sostituito, non affiancato

`src/server.rs` è un mock: un `std::net::TcpListener` che serve una pagina statica "Status: ACTIVE",
non legge nemmeno la richiesta, non ha shutdown. È marcato `[PARZIALE]` nella documentazione. Con
axum in casa, quel modulo va **rimosso** insieme al flag `--serve-dashboard`, non tenuto in parallelo.

### 2.6 Consegna sincrona, non fire-and-forget

Se l'handler rispondesse `200 OK` prima di aver realmente consegnato sul canale, l'app registrerebbe
"consegnato" per una notifica persa. Per un notificatore di backup — dove la notifica mancata è
esattamente l'informazione che serve — la consegna va effettuata **prima** di rispondere, con timeout
breve, e l'esito reale va riflesso nello status code.

---

## 3. Fasi ed elenco attività

### ▸ Fase 0 — Preparazione e scaffolding

- [ ] Verificare la versione corrente di `axum` (docs.rs / context7) e fissarla in `Cargo.toml` come
      dipendenza **opzionale**.
- [ ] Aggiungere la feature `notify-server` e la sezione `[[bin]]` con `required-features`.
- [ ] Creare `src/bin/notify_server.rs` come scheletro compilabile (solo `/health`).
- [ ] Verificare che `cargo build --release` **senza** la feature continui a non compilare axum
      (`cargo tree | grep axum` deve essere vuoto).
- [ ] Verificare che `cargo build --features notify-server` produca entrambi i binari.

**Fatto quando**: entrambi i profili compilano e il binario di backup non è cresciuto in dipendenze.

---

### ▸ Fase 1 — Contratto di notifica condiviso

- [ ] In `src/notify.rs`, introdurre `pub const NOTIFY_SCHEMA_VERSION: u32 = 1;`.
- [ ] Convertire `status` da `String` a enum tipizzato, **preservando la forma JSON attuale**:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum BackupStatus { Success, Failed }   // serializza come "SUCCESS" / "FAILED"
```

- [ ] Estendere `WebhookPayload` con i campi già disponibili in `IngestReport` ma oggi non
      strutturati (sono solo interpolati dentro `text`):

| Campo nuovo | Origine in `IngestReport` | Perché serve |
|---|---|---|
| `schema_version: u32` | costante | Evita la deriva del contratto (D6). |
| `source: String` | `report.source` | Oggi leggibile solo dentro `text`. |
| `dest: String` | `report.dest` | Idem. |
| `host: String` | `report.host_metadata.hostname` | Indispensabile se più macchine notificano lo stesso server. |
| `tool_version: String` | `report.tool_version` | Diagnostica. |
| `exit_code: Option<i32>` | `report.robocopy_transfer.exit_code` | Distingue 1 / 2 / 3. |
| `integrity_status: Option<String>` | `report.integrity_check.map(...)` | Segnale più importante del solo esito robocopy. |

- [ ] Marcare i campi non essenziali con `#[serde(default)]` **lato ricevente**, così il server
      accetta anche POST da script generici che non popolano tutto.
- [ ] Aggiornare/estendere i test esistenti in `src/notify.rs`.

**Fatto quando**: `cargo test` verde e un payload serializzato contiene tutti i campi nuovi.

---

### ▸ Fase 2 — Server axum: sicurezza e robustezza

- [ ] **Configurazione bind**: default `127.0.0.1:3000`, sovrascrivibile da argomento CLI o env.
      Mantenere il loopback come default.
- [ ] **Autenticazione**: token condiviso via header (`Authorization: Bearer <token>`), letto da
      `ROBOCOPY_NOTIFY_TOKEN`.
  - [ ] Regola di sicurezza: se il bind **non** è loopback e il token **non** è configurato, il server
        deve **rifiutarsi di partire** con un errore esplicito (non un warning ignorabile).
- [ ] **Nessun `unwrap()`** su bind/serve: gestione errori con `anyhow`, exit code sensato e messaggio
      leggibile se la porta è occupata.
- [ ] **Limite dimensione body**: `DefaultBodyLimit` esplicito (es. 64 KiB); il payload reale è di
      pochi KB.
- [ ] **Graceful shutdown** su Ctrl+C (`axum::serve(...).with_graceful_shutdown(...)`).
- [ ] **Endpoint**:
  - [ ] `GET /health` → `200` con `{ "status": "ok", "version": ..., "schema_version": ... }`
  - [ ] `POST /notify` → vedi tabella status code sotto.

| Situazione | Status code |
|---|---|
| Consegnato su tutti i canali | `200 OK` |
| Payload malformato | `422` (automatico dall'extractor `Json` di axum) |
| Token assente/errato | `401 Unauthorized` |
| Consegna fallita su almeno un canale | `502 Bad Gateway` (l'app lo registrerà in `webhook_error`) |

**Fatto quando**: il server rifiuta richieste non autenticate, non va in panic su porta occupata e si
chiude pulito con Ctrl+C.

---

### ▸ Fase 3 — Canali di notifica

- [ ] Definire un trait `NotificationSink`, **seguendo il pattern già usato nel progetto**
      (`CommandRunner`, `CopyEngine`, `ProgressSink`): permette il test con un doppio scriptato
      esattamente come `ScriptedRunner` in `src/testkit.rs`.

```rust
pub trait NotificationSink: Send + Sync {
    fn name(&self) -> &'static str;
    async fn deliver(&self, payload: &WebhookPayload) -> Result<(), NotifyError>;
}
```

- [ ] Implementare `LogSink` (sempre attivo, scrive via `tracing`).
- [ ] Implementare `NtfySink` (HTTP POST via `reqwest`, già in dipendenze).
- [ ] Implementare `GenericWebhookSink` (compatibile Slack/Teams).
- [ ] *(Opzionale)* `TelegramSink`.
- [ ] **Configurazione da file TOML** (`notify-server.toml`), seguendo il pattern di `src/config.rs`:
      quali canali attivare, URL/topic/token per ciascuno.
- [ ] I segreti dei canali vanno letti da env o file, **mai committati**.

**Fatto quando**: con un solo file TOML si abilita/disabilita un canale senza ricompilare.

---

### ▸ Fase 4 — Rimozione del mock `server.rs`

- [ ] Eliminare `src/server.rs` e il relativo test `dashboard_server_binds_to_available_port`.
- [ ] Rimuovere `pub mod server;` da `src/lib.rs`.
- [ ] Rimuovere il flag `--serve-dashboard` da `src/cli.rs` e la sua invocazione in `src/main.rs`.
- [ ] Aggiornare i test che citano il flag (`help_documents_every_flag` in `tests/cli_smoke.rs`).
- [ ] Aggiornare la documentazione che lo menziona:
  - [ ] `README.md` (tabella flag + esempio 2 "Ingestion Enterprise")
  - [ ] `ARCHITECTURE.md` (diagramma + tabella moduli)
  - [ ] `RUNBOOK.md` (**esempio 4 usa `--serve-dashboard 8080`**)
  - [ ] `ROADMAP.md` (debito tecnico: la voce sul dashboard statico va chiusa)
  - [ ] `CLAUDE.md` e `AGENTS.md` (regole architetturali)

> ⚠️ Rimuovere un flag CLI è una **modifica breaking**: annotarla esplicitamente nello storico release
> di `ROADMAP.md`.

**Fatto quando**: nessun riferimento a `--serve-dashboard` o `server.rs` sopravvive nel repo
(`grep -ri "serve-dashboard\|server.rs"` pulito).

---

### ▸ Fase 5 — Test

- [ ] **Unit test** (devono girare anche su Linux, vincolo del progetto):
  - [ ] Rifiuto con token mancante/errato → 401
  - [ ] Payload malformato → 422
  - [ ] Dispatch verso più sink con un doppio di test
  - [ ] Fallimento di un sink → 502
- [ ] **Test black-box end-to-end**:
  - [ ] Avviare `notify-server` su porta effimera
  - [ ] Eseguire `robocopy_ingest` con `--webhook-url` verso quel server
  - [ ] Verificare che il server abbia ricevuto un payload **con i campi corretti**
  - [ ] Verificare che, con server **spento**, il backup **non fallisca** e il report contenga
        `webhook_error` valorizzato

> ⚠️ **Il test end-to-end non è opzionale.** Il difetto **D1** (`--restore-from` inutilizzabile) è
> sopravvissuto perché il test invocava la funzione interna saltando clap: il binario reale non è mai
> stato eseguito. Non ripetere quel pattern qui.

**Fatto quando**: `cargo test` e `cargo test --features notify-server` entrambi verdi.

---

### ▸ Fase 6 — Documentazione e chiusura

- [ ] Aggiornare `README.md`: sezione sul notify-server, come avviarlo, come collegarlo.
- [ ] Aggiornare `ARCHITECTURE.md`: nuovo modulo nel diagramma e nella tabella.
- [ ] Aggiornare `ROADMAP.md`: milestone 5.4.0 completata; collegare **F32** (endpoint metriche
      Prometheus) come naturale evoluzione **sulla stessa istanza axum**.
- [ ] Aggiornare `CLAUDE.md` / `AGENTS.md` con le nuove regole architetturali.
- [ ] Documentare il **nodo operativo**: il server deve restare vivo, e `service.rs` è un mock (la
      registrazione come servizio Windows non esiste). Indicare Task Scheduler "all'avvio" o NSSM.
      Nota positiva da riportare: un server spento **non è silenzioso**, perché l'app scrive
      `webhook_error` nel report.
- [ ] **Rigenerare il grafo graphify** dopo l'aggiunta dei file (procedura in `AGENTS.md`).
- [ ] `cargo build --release` finale + commit.

---

## 4. Insidie note (lezioni di questo repository)

Errori realmente incontrati in questo progetto, da non ripetere:

1. **Testare il binario, non solo l'unità.** Difetti D1 e D2 sono sopravvissuti a una suite di 140
   test perché nessuno eseguiva il binario compilato con gli argomenti che l'utente digita davvero.
2. **Versionare i contratti.** Vedi D6: schema cambiato, versione non incrementata, report storici
   illeggibili.
3. **Script PowerShell in ASCII puro.** Un em-dash (`—`) in una stringa ha rotto il parser di
   PowerShell 5.1 (che senza BOM interpreta il file con la codepage ANSI). Verificare con:
   `python3 -c "print([c for c in open(f,encoding='utf-8').read() if ord(c)>127])"`
4. **PowerShell mescola stdout nativo e valore di ritorno.** `$x = MiaFunzione` cattura *anche*
   l'output dell'exe lanciato dentro la funzione. Usare una variabile `$script:` invece di `return`.
5. **Non fidarsi della reachability del grafo graphify.** I nodi metodo non sono qualificati con il
   tipo proprietario, quindi la query da `main`/`lib` restituisce falsi irraggiungibili (33/580).
   Vedi **D10**. Per il codice morto usare `grep`.
6. **`encoding_rs` non implementa le code page DOS/OEM.** `for_label(b"ibm850")` ritorna sempre
   `None`. Se serve CP850, usare `src/oem_codec.rs`.

---

## 5. Comandi utili

```bash
# Build del solo binario di backup (default, senza axum)
cargo build --release

# Build con il notify-server
cargo build --release --features notify-server

# Test
cargo test
cargo test --features notify-server

# Verificare che axum NON entri nel build di default
cargo tree | grep -i axum        # deve essere vuoto

# Avviare il server
$env:ROBOCOPY_NOTIFY_TOKEN = "..."
./target/release/notify-server

# Collegare un backup al server
./target/release/robocopy_ingest.exe `
  --source "C:\dati" --dest "\\SERVER\share" `
  --verify-integrity --hash-algo blake3 `
  --webhook-url "http://127.0.0.1:3000/notify"
```

---

## 6. Criterio di completamento complessivo

Il lavoro è concluso quando:

1. `cargo test` e `cargo test --features notify-server` sono **entrambi verdi**;
2. `cargo tree` **senza** feature non contiene axum;
3. un backup reale con `--webhook-url` produce una notifica **realmente consegnata** su almeno un
   canale non-log;
4. lo stesso backup con il server **spento** completa comunque, con `webhook_error` valorizzato nel
   report JSON;
5. `grep -ri "serve-dashboard"` sul repo non restituisce nulla;
6. la documentazione (`README`, `ARCHITECTURE`, `ROADMAP`, `RUNBOOK`, `CLAUDE`, `AGENTS`) descrive
   **ciò che il codice fa davvero** — il problema storico di questo progetto.
