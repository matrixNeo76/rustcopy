---
type: Reference
title: Installazione e distribuzione — robocopy-ingest-cli
description: Requisiti, installer Windows Inno Setup, deploy silenzioso e notify-server.
status: stable
generated:
  by: process:claude-code
  at: 2026-08-24T00:00:00Z
---

# 📦 Installazione e distribuzione

Requisiti di sistema, installer Windows e deploy del **notify-server**. Per i primi comandi vedi il
[README](../README.md); per il riferimento completo dei flag il
[Riferimento CLI](cli-reference.md).

---

## 📦 Installazione su altre macchine Windows

`rustcopy` è un eseguibile **portable**: nessuna installazione formale è tecnicamente necessaria, si
copiano gli `.exe` e si lanciano da qualunque cartella. Due avvertenze concrete verificate sul
binario compilato:

- **Richiede il Visual C++ Redistributable x64** (Microsoft, gratuito). Il binario Rust
  `windows-msvc` importa dinamicamente `VCRUNTIME140.dll`, che **non** è incluso in
  un'installazione Windows pulita (a differenza della Universal CRT, presente di default su
  Windows 10 1607+/11). Senza, l'eseguibile non parte.
- **Si appoggia a `robocopy.exe` di sistema**, presente su ogni Windows da Vista in poi: non serve
  installarlo, ma il tool non lo include.

### Installer Windows (Inno Setup)

Per una distribuzione più comoda di un semplice copia-incolla, il repo include uno script Inno
Setup (`installer/rustcopy.iss`) che genera un vero `setup.exe` con disinstaller, opzione di
aggiunta al PATH di sistema e verifica automatica del Visual C++ Redistributable.

Da F60 l'installer è **uno solo** e la console grafica è un **componente opzionale**:

| Tipo di installazione | Cosa installa |
|---|---|
| **CLI e console grafica** | `robocopy_ingest.exe`, `notify-server.exe`, `rustcopy-gui.exe` |
| **Solo CLI** | `robocopy_ingest.exe`, `notify-server.exe` |
| **Scelta manuale** | La CLI è obbligatoria, la console si spunta |

La console è opzionale di proposito: un server che esegue solo backup pianificati non ha alcun
uso per una finestra desktop, e la CLI è il componente che deve continuare a funzionare non
presidiato.

```powershell
# 1. Frontend della console (solo se la impacchetti)
npm --prefix crates/rustcopy-gui/ui ci
npm --prefix crates/rustcopy-gui/ui run build

# 2. Build dei binari (dalla root del repo)
cargo build --release --workspace --features rustcopy-cli/notify-server

# 3. Compilazione dell'installer (richiede Inno Setup 6: winget install JRSoftware.InnoSetup)
& "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe" installer\rustcopy.iss
# Output: installer-output\rustcopy-<versione>-setup.exe
```

Testato realmente (non solo compilato): installazione silenziosa, avvio dei due `.exe`, verifica
del PATH di sistema, disinstallazione con ripristino del PATH — ciclo completo verde.

```powershell
# Installazione silenziosa (utile per deploy automatizzati)
rustcopy-6.0.0-setup.exe /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /TASKS="addtopath"

# Solo CLI, senza console grafica
rustcopy-6.0.0-setup.exe /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /TYPE=cli /TASKS="addtopath"
```

#### WebView2

La console rende l'interfaccia attraverso il runtime **WebView2** di sistema invece di
impacchettare un motore browser — è il motivo per cui pesa 8,9 MB invece di ~150. Quel runtime
è presente su Windows 11 e arriva alla maggior parte delle installazioni Windows 10 aggiornate,
ma può mancare su immagini LTSC o offline. L'installer lo rileva e **avvisa** — solo se hai
scelto la console — senza bloccare il setup e senza impacchettare un secondo installer, come già
fa per il Visual C++ Redistributable. Senza WebView2 la CLI funziona comunque: è solo la finestra
della console che non si aprirebbe.

Il bundler di Tauri resta **disattivato** (`bundle.active: false`): produrrebbe un secondo
MSI/NSIS per la sola console, cioè esattamente la separazione che questo installer evita.

---

---

## 📬 Notify Server: notifiche di backup

`notify-server` è un secondo binario opzionale (non compilato di default: richiede
`--features notify-server`) che riceve le notifiche inviate da `--webhook-url` e le inoltra su più
canali configurabili da un solo file TOML, invece di replicare la logica in ogni script di backup.

```powershell
# Build (il binario di backup normale NON include axum a meno di questa feature)
cargo build --release -p rustcopy-cli --features notify-server

# Avvio: senza token, resta sul solo loopback (nessuna esposizione di rete)
.\target\release\notify-server.exe

# Avvio con canali configurati e autenticazione
$env:ROBOCOPY_NOTIFY_TOKEN = "un-token-lungo-e-casuale"
.\target\release\notify-server.exe --config notify-server.toml --bind 127.0.0.1:3000
```

Esempio di `notify-server.toml`:
```toml
bind = "127.0.0.1:3000"

[ntfy]
enabled = true
topic_url = "https://ntfy.sh/i-miei-backup"

[generic_webhook]
enabled = false
url = "https://hooks.slack.com/services/..."
```

Collegare un backup al server (avviato **senza** token, sul solo loopback):
```powershell
robocopy_ingest.exe --source D:\dati --dest \\SERVER\share `
  --verify-integrity --hash-algo blake3 `
  --webhook-url "http://127.0.0.1:3000/notify"
```

> [!IMPORTANT]
> **`--webhook-url` non può autenticarsi.** Il client invia solo `POST` + corpo JSON, senza header
> (`src/notify.rs`): non esiste un flag CLI per il token. Se avvii il server con
> `ROBOCOPY_NOTIFY_TOKEN` impostato, le notifiche di `robocopy_ingest` riceveranno **401**. Il token
> serve per client che sanno inviare l'header (`curl`, script, altri tool); per l'uso con
> `--webhook-url` lascia il server sul loopback senza token, come nell'esempio sopra.

**Sicurezza**: `/notify` richiede `Authorization: Bearer <token>` quando `ROBOCOPY_NOTIFY_TOKEN` è
impostato. Il server **si rifiuta di avviarsi** se il bind non è un indirizzo loopback (127.0.0.1 /
::1) e nessun token è configurato — esporre un endpoint non autenticato sulla rete permetterebbe a
chiunque di iniettare notifiche di backup false.

**Comportamento se il server è spento o irraggiungibile**: il backup **non fallisce**. L'errore di
consegna viene registrato nel report JSON (campo `webhook_error`) — una notifica mancata è visibile,
non silenziosa.

**Endpoint disponibili**: `GET /health` (stato + versione schema), `POST /notify` (riceve il payload
di `--webhook-url`; risponde `200` se consegnato su tutti i canali, `401` senza/con token errato,
`422` per payload malformato, `502` se un canale fallisce la consegna).

---
