---
name: rustcopy-flow
version: 1.0.0
category: compound
tags: [rustcopy, robocopy, backup, restore, generations, retention, scheduling, windows-service, cli-wrapper, disaster-recovery]
triggers:
  - "rustcopy"
  - "robocopy ingest"
  - "backup con rustcopy"
  - "esegui un backup"
  - "copia con robocopy_ingest"
  - "backup a generazioni"
  - "restore rustcopy"
  - "installa uno schedule di backup"
  - "installa il servizio rustcopy"
description: "Orchestratore a 2 livelli (compound + molecole) per invocare la CLI rustcopy (robocopy_ingest.exe) da qualunque agente/CLI di coding — Claude Code, OpenCode, ecc. Copre backup rapido/mirror, backup a generazioni con retention, restore da report, e automazione (Task Scheduler / servizio Windows). Nessuna dipendenza da tool MCP proprietari: solo Bash/PowerShell e il binario stesso, quindi portabile ovunque un agente possa eseguire shell."
status: active
last_improved: 2026-08-05
improvement_log: "v1.0.0: Creata adattando la struttura compound+molecole di structured-memory-flow (craft-skills-flow) a rustcopy. Rimossa ogni dipendenza da MCP craft-memory: sub-agenti sono opzionali (usati solo se l'ambiente li offre), l'unico canale garantito e' l'esecuzione shell del binario."
---

# Rustcopy Flow — Composto Orchestratore per la CLI rustcopy

> **⚠️ PORTABILITA'**: Questa skill non usa NESSUN tool MCP specifico di un ambiente (niente
> `remember()`, niente `spawn_session()` craft-memory-style). L'unico requisito e' un tool shell
> (Bash su Linux/macOS/git-bash, PowerShell su Windows) capace di eseguire `robocopy_ingest.exe`.
> Funziona quindi identica su Claude Code, OpenCode, o qualunque altro agente CLI — cambia solo
> la sintassi di shell effettivamente usata (vedi Molecola 0).
>
> **⚠️ SICUREZZA**: rustcopy include operazioni potenzialmente distruttive (`--mirror` cancella
> file in destinazione, `--keep-generations` cancella intere generazioni vecchie). Questa skill
> **non bypassa mai** le conferme di rustcopy: non passa mai `--force-purge` senza che l'utente
> l'abbia esplicitamente richiesto in questo turno, e propone sempre un `--dry-run` prima di
> qualunque esecuzione reale.
>
> **⚠️ WINDOWS-ONLY PER I TRASFERIMENTI REALI**: `robocopy.exe` esiste solo su Windows. Il
> binario si compila anche su Linux/macOS (motore di confronto, report, verifica — usati dai
> test), ma un trasferimento reale richiede Windows. Se l'agente gira su un altro OS, la skill si
> ferma alla pianificazione/dry-run e lo segnala.

## Architettura

```
rustcopy-flow (COMPOSTO)
  ├── Seleziona scenario (1-4)
  ├── Molecola 0: 🔎 Discover — trova binario, versione, config esistenti
  ├── Molecola 1: 📝 Plan — raccoglie intento utente, costruisce comando/TOML
  ├── Molecola 2: 🧪 Dry-run — SEMPRE prima di un'esecuzione reale (tranne Sc. 4)
  │     └── CHECKPOINT UMANO (piano confermato)
  ├── Molecola 3/4/5/6: esecuzione specifica dello scenario
  │     └── CHECKPOINT UMANO PRIMA di ogni operazione distruttiva
  └── Molecola 7: 📊 Verify & Report — interpreta report JSON/exit code, riepiloga
```

## Scenari

| # | Scenario | Molecole | Quando |
|---|---|---|---|
| **1** | Backup rapido (copia semplice o `--mirror`) | 0 → 1 → 2 → 3 → 7 | Copia una tantum o ricorrente senza storicizzazione delle versioni |
| **2** | Backup a generazioni + retention | 0 → 1 → 2 → 4 → 7 | `--backup-type full\|incremental\|differential` (+ `--keep-generations`) |
| **3** | Restore / disaster recovery | 0 → 5 → 7 | `--restore-from <report.json>`, opzionale `--decrypt` |
| **4** | Automazione (schedule / servizio) | 0 → 6 | `--install-schedule`/`--uninstall-schedule`, `--install-service`/`--uninstall-service` — non esegue un backup ora, registra/rimuove l'automazione |

> Se l'utente non specifica lo scenario, chiediglielo esplicitamente prima di procedere (come da
> "Selezione Scenario" sotto) — non indovinare tra "copia una volta" e "backup a generazioni":
> l'intento cambia layout della destinazione e semantica di retention.

## Selezione Scenario

**PRIMO PASSO: se l'utente non ha già specificato lo scenario, chiedilo.** Indizi utili nella
richiesta:
- "backup rapido", "copia da X a Y", "sincronizza" → Scenario 1
- "backup incrementale/differenziale", "storicizza le versioni", "tieni N backup" → Scenario 2
- "ripristina", "restore", "ho perso dei file", "recovery" → Scenario 3
- "ogni notte", "pianifica", "servizio Windows", "automatizza" → Scenario 4

---

## Molecola 0: 🔎 Discover (tutti gli scenari)

**File:** `molecules/molecule-0-discover.md`
**Scopo:** localizzare il binario `robocopy_ingest.exe`/`robocopy_ingest` (path assoluto o
relativo al repo, variabile d'ambiente, o via ricerca file), verificarne la versione, individuare
config TOML esistenti riutilizzabili in `examples/*.local.toml`.
**Checkpoint:** nessuno (fase di solo discovery) — ma se il binario non viene trovato, la skill
si ferma e chiede all'utente come procedere (compilare? path manuale?).

## Molecola 1: 📝 Plan (Scenari 1, 2)

**File:** `molecules/molecule-1-plan.md`
**Scopo:** raccogliere source/dest/pattern/esclusioni/opzioni dall'intento utente, applicare i
pitfall noti di rustcopy (vedi la molecola), produrre un comando o un TOML di config.
**Checkpoint:** ✅ OBBLIGATORIO — mostra il comando/TOML costruito e chiedi conferma prima del
dry-run.

## Molecola 2: 🧪 Dry-run (Scenari 1, 2)

**File:** `molecules/molecule-2-dryrun.md`
**Scopo:** eseguire SEMPRE `--dry-run` prima di un'esecuzione reale, leggere il report JSON
(conteggio file/byte, throughput stimato), mostrarlo all'utente.
**Checkpoint:** ✅ OBBLIGATORIO — "Il piano è confermato dal dry-run. Procedo con l'esecuzione
reale?" Se l'utente nega o vuole modificare qualcosa, torna alla Molecola 1.

## Molecola 3: ▶️ Quick Execute (Scenario 1)

**File:** `molecules/molecule-3-quickcopy.md`
**Scopo:** eseguire la copia reale (con o senza `--mirror`). Se `--mirror`, verifica che
l'utente abbia consapevolmente accettato il rischio di purge prima di aggiungere
`--force-purge` (altrimenti lascia che rustcopy chieda conferma interattiva o abortisca, non
bypassarlo mai silenziosamente).

## Molecola 4: 🗂️ Generations & Retention (Scenario 2)

**File:** `molecules/molecule-4-generations.md`
**Scopo:** eseguire `--backup-type` (full/incremental/differential) ed eventualmente
`--keep-generations`. Checkpoint separato e più esplicito prima di una rotazione con purge.

## Molecola 5: ♻️ Restore (Scenario 3)

**File:** `molecules/molecule-5-restore.md`
**Scopo:** localizzare il report JSON del backup da ripristinare, eseguire `--restore-from`
(opzionale `--decrypt`), verificare l'esito.
**Checkpoint:** ✅ OBBLIGATORIO prima dell'esecuzione reale — un restore scrive sulla directory
sorgente originale (source/dest invertiti), quindi può sovrascrivere dati esistenti.

## Molecola 6: ⏰ Automation — Schedule & Service (Scenario 4)

**File:** `molecules/molecule-6-automation.md`
**Scopo:** installare/rimuovere uno schedule Task Scheduler o un servizio Windows che rilancia
questo stesso binario. Non esegue un backup adesso.
**Checkpoint:** ✅ OBBLIGATORIO — mostra il comando esatto che verrà eseguito ad ogni trigger
(schedule) o all'avvio del servizio prima di installarlo.

## Molecola 7: 📊 Verify & Report (tutti gli scenari, dopo l'esecuzione reale)

**File:** `molecules/molecule-7-verify-report.md`
**Scopo:** interpretare l'exit code e il report JSON/HTML prodotto, riepilogare in italiano
all'utente cosa è successo (file copiati, eventuali mismatch, generazioni ruotate, ecc.).

---

## Execution Model — Sub-agenti Opzionali

A differenza di `structured-memory-flow` (che spawna sempre un sub-agente per fase via tool MCP
di un ecosistema specifico), qui i sub-agenti sono **facoltativi** perché non tutti gli ambienti
li offrono con la stessa interfaccia:

1. **Se l'ambiente espone un tool generico di sub-task/agente** (es. `Agent`/`Task` in Claude
   Code) **e** la fase è pesante (dry-run o esecuzione reale su alberi di milioni di file, il cui
   log può pesare centinaia di MB — vedi D9/D11 in `ANALYSIS.md` del progetto rustcopy): usalo
   per tenere l'output verboso fuori dal contesto principale, e fatti restituire solo il riepilogo
   (conteggio file/byte, exit code, eventuali errori).
2. **Altrimenti** (o per fasi leggere come Discover/Plan): esegui tutto inline nella stessa
   sessione — non è un requisito architetturale, solo un'ottimizzazione di contesto.
3. **Non forzare mai un modello specifico** per un sub-agente: invocare un binario esterno via
   shell non richiede capacità di reasoning particolari, il modello ereditato è sempre
   sufficiente.

## Cosa NON Fare

- ❌ **Non eseguire mai un'esecuzione reale senza dry-run prima** (Scenari 1-2) — il dry-run è
  l'unico modo per validare source/dest/esclusioni prima di toccare dati reali.
- ❌ **Non aggiungere `--force-purge` di tua iniziativa** — solo se l'utente lo chiede
  esplicitamente in questo turno per QUESTA operazione. Un "ok procedi" generico non lo autorizza.
- ❌ **Non dimenticare `--exclude-junctions` quando ci sono `--exclude-dirs`** — senza questo
  flag, junction/symlink con nomi diversi possono ricopiare una cartella esclusa sotto un altro
  nome (vedi `CLAUDE.md` del progetto, nota su F26d).
- ❌ **Non combinare `--no-prescan` con `--mirror`** — senza prescan, `check_mirror_safety` non
  ha un inventario di riferimento: o abortisce sempre (senza `--force-purge`) o purga alla cieca
  (con `--force-purge`), senza reale controllo intermedio.
- ❌ **Non usare `--backup-type` insieme a `--mirror`** — sono mutuamente esclusivi (rustcopy lo
  rifiuta comunque con un errore, ma non proporlo nel piano).
- ❌ **Non passare `--keep-generations` senza `--backup-type`** — non c'è nulla da ruotare senza
  una storia di generazioni; rustcopy lo rifiuta con `KeepGenerationsWithoutBackupType`.
- ❌ **Non ricostruire un comando da zero quando esiste già un TOML riutilizzabile** — controlla
  prima `examples/*.local.toml` (Molecola 0); il TOML copre già source/dest/esclusioni/soglie
  ricorrenti, evitando di ridigitare path di rete lunghi e a rischio di errore.
- ❌ **Non fidarti ciecamente della cache di `--fast-verify`** — trust model basato su
  size+mtime della SORGENTE, non un ricontrollo dei byte in destinazione (vedi help del flag).
  Non proporlo come sostituto di `--verify-integrity` puro se l'utente ha bisogno di rilevare
  corruzione lato destinazione.
- ❌ **Non usare `xxh3` come algoritmo di hash se il backup deve proteggere da manomissione**
  — non è crittografico, solo per rilevare corruzione accidentale (vedi `--hash-algo`).
- ❌ **Non proporre `--install-service`/`--install-schedule` senza avvisare dei privilegi
  richiesti** — il servizio Windows richiede Amministratore; lo schedule Task Scheduler per
  l'utente corrente no, ma un trigger di sistema sì.

## Riferimenti

- Repo del progetto: `robocopy-ingest-cli` (rustcopy) — `CLAUDE.md`/`ANALYSIS.md`/`ROADMAP.md`
  per i dettagli implementativi dietro ogni flag citato in questa skill.
- Skill di riferimento per il pattern compound+molecole: `structured-memory-flow`
  (`craft-skills-flow`), da cui questa skill eredita struttura e checkpoint umani ma non le
  dipendenze MCP.
- Esempi di config pronti: `examples/smb-nas-mirror.toml`, `examples/scheduled-incremental.toml`,
  `examples/first-time-full-copy.toml` nel repo rustcopy.
