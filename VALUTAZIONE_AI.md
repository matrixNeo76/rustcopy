---
type: Reference
title: Valutazione — Funzionalità AI e Orchestrazione Multi-Agente in rustcopy
description: Analisi di fattibilità per pianificazione autonoma, monitoraggio con azioni correttive, diagnosi in linguaggio naturale, orchestrazione multi-agente e memoria a context graph.
status: stable
generated:
  by: process:claude-code
  at: 2026-08-25T00:00:00Z
verified:
  by: process:claude-code
  at: 2026-08-25T00:00:00Z
---

# Valutazione — Funzionalità AI e Orchestrazione Multi-Agente in rustcopy

> **Stato**: valutazione **eseguita**. Le Fasi 0, 1 e 2 di §8 sono implementate (25 Agosto 2026);
> le Fasi 3 e 4 restano ferme alle loro condizioni di attivazione, non soddisfatte. Il registro di
> cosa è stato costruito, e dei due punti in cui l'implementazione ha smentito questa analisi, è
> in §12.
>
> Le affermazioni sullo stato *pre-implementazione* sono verificate contro il codice al
> 25 Agosto 2026 (commit `b73f032`); i riferimenti puntuali sono citati inline.

---

## 1. Verdetto in breve

La domanda era: *possiamo introdurre, o già possediamo, i requisiti per queste cinque cose?*

| # | Richiesta | Verdetto | Sintesi |
|---|---|---|---|
| 1 | Pianifica e ottimizza job in autonomia | 🟡 **Requisiti sì, dato non indicizzato** | I dati per dedurre schedulazione e retention esistono già in ogni report. Manca un indice che li renda interrogabili. **Non serve un modello**: è statistica. |
| 2 | Monitora run e propone azioni correttive | 🟢 **In parte già presente** | La molecola 7 lo fa già per la run appena conclusa. Manca il monitoraggio *durante* la run e la correlazione storica. |
| 3 | Diagnosi e Q&A in linguaggio naturale | 🟢 **Già disponibile oggi, in modo interattivo** | È esattamente ciò che fa `rustcopy-flow` dentro una CLI agentica. Il pezzo mancante è quello *non presidiato*, ed è quello che sconsiglio. |
| 4 | Orchestratore multi-agente ("harness") | 🟢 **Già costruito e attivo** | `.agents/skills/rustcopy-flow/` è un composto con 8 molecole specializzate. Non è dove forse te lo aspettavi: vive nel layer agentico, non nel binario. |
| 5 | Memoria a context graph fra agenti e servizi | 🔴 **Requisito tecnico sì, bisogno reale no** | È l'unico punto su cui dissento nel merito. Le relazioni in gioco sono poche e ad albero: un grafo qui è sovradimensionato. Dettaglio in §6. |

**La conclusione che conta**: il valore che cerchi è per la maggior parte raggiungibile **senza
introdurre un modello nel binario**. Il collo di bottiglia reale non è l'intelligenza — è che
ogni run scrive il suo report e poi lo dimentica. Risolto quello, tre delle cinque richieste
diventano codice deterministico e testabile; la quarta esiste già.

---

## 2. Cosa il progetto già possiede

Questa sezione esiste perché la risposta a "possediamo i requisiti?" è, in misura sorprendente, sì.

### 2.1 Un substrato dati insolitamente ricco

La maggior parte dei progetti che vogliono "funzioni AI" fallisce sul dato: log non strutturati,
nessuno schema, nessuno storico. rustcopy è nella condizione opposta.

| Artefatto | Dove | Cosa contiene |
|---|---|---|
| `IngestReport` | `src/report.rs` | 22 campi versionati (`schema_version: 2`), con `host_metadata`, `phase_timing`, `configuration` |
| `TransferReport` | `src/report.rs` | `elapsed_seconds`, `throughput_mbps`, `bytes_copied`, `files_copied`, `exit_code` + significato, `retry_attempts_used` |
| `IntegrityCheck` | `src/integrity.rs` | `files_checked`, `bytes_hashed`, `mismatches`, `missing_in_dest`, `unreadable`, `skipped_unchanged` |
| `PhaseTiming` | `src/report.rs` | tempo separato per inventario / trasferimento / verifica / baseline |
| `RunComparison` | `src/report.rs` (P2) | **delta già calcolato** rispetto alla run precedente: file, durata, throughput, variazione % |
| `GenerationManifest` | `src/generations.rs` (D19) | log append-only NDJSON di ogni generazione prodotta |
| `IngestCache` | `src/cache.rs` (F28) | dimensione + mtime per file, dall'ultima verifica pulita |
| `WebhookPayload` | `src/notify.rs` | superficie di evento già tipizzata, con `status` e `integrity_status` |
| Log strutturati | `src/logging.rs` | `tracing` con livelli, rotazione, filtro |
| Exit code 0-5 | `src/main.rs` | **semantica distinta**, non un booleano: `1` ≠ `4` ≠ `5` |

Il punto sugli exit code merita enfasi. `4` significa "la copia è riuscita ma la verifica ha
trovato un mismatch" e `5` "purge di retention annullato": è già una tassonomia di esiti
disambiguata, cioè precisamente ciò su cui una diagnosi automatica dovrebbe ragionare. Molti
strumenti restituiscono 0/1 e a valle non resta niente su cui ragionare.

### 2.2 Un orchestratore multi-agente già in produzione

`.agents/skills/rustcopy-flow/SKILL.md` è un **composto a due livelli** con otto molecole:

```
rustcopy-flow (COMPOSTO)
  ├── Molecola 0: Discover        → trova binario, versione, config
  ├── Molecola 1: Plan            → raccoglie intento, costruisce comando/TOML
  ├── Molecola 2: Dry-run         → SEMPRE prima di un'esecuzione reale
  ├── Molecola 3: Quick copy      ┐
  ├── Molecola 4: Generations     ├─ esecuzione per scenario
  ├── Molecola 5: Restore         │
  ├── Molecola 6: Automation      ┘
  └── Molecola 7: Verify & Report → interpreta exit code + JSON, riepiloga
```

Confrontato con gli agenti specializzati che hai elencato — *scan, verify, notify, restore,
backup*:

| Agente richiesto | Copertura attuale |
|---|---|
| scan | ✅ Molecola 0 (discover) + Molecola 2 (dry-run) |
| backup | ✅ Molecole 3 e 4 |
| verify | ✅ Molecola 7 |
| restore | ✅ Molecola 5 |
| notify | ⚠️ **Nessuna molecola dedicata** — la notifica è `--webhook-url` e il binario `notify-server`, non un passo orchestrato |

Quattro su cinque esistono. E la disciplina di sicurezza che chiederesti a un orchestratore di
backup è già scritta dentro la skill: *non passa mai `--force-purge` senza richiesta esplicita
dell'utente in quel turno, e propone sempre un `--dry-run` prima di qualunque esecuzione reale*.

### 2.3 Diagnosi e azioni correttive: parzialmente già fatte

La molecola 7 non si limita a riassumere. Sull'exit code `4` prescrive:

> mostra i mismatch dal report, valuta se sono transitori (log/tmp: `--ignore-transient-missing`
> alla prossima run)

Questa **è** una diagnosi che propone un'azione correttiva concreta e specifica del dominio. Il
punto 2 della tua richiesta non è a zero: è a "fatto per la run appena conclusa, assente per la
run in corso e per lo storico".

---

## 3. La distinzione che decide tutto

La richiesta accorpa due cose che hanno costi, rischi e tempi di realizzazione completamente
diversi. Separarle è la decisione architetturale più importante di questa valutazione.

### (a) Automazione deterministica — ciò che la gente chiama "AI"

Suggerire una finestra di schedulazione, una retention, un numero di thread, o segnalare che una
run è anomala **non richiede un modello linguistico**. Richiede statistica descrittiva su uno
storico:

- *"i tuoi backup durano in media 47 min, il 95° percentile è 68 min, quindi `daily@02:00`
  finisce prima delle 04:00 anche nel caso peggiore"* → media e percentile su `phase_timing`
- *"il 3% dei file cambia tra una run e l'altra; con `--keep-generations 7` conservi ~5 settimane
  di storia in 1.4× lo spazio del full"* → tasso di variazione da `GenerationManifest`
- *"questa run ha copiato 8× i file della mediana: qualcosa ha toccato l'albero sorgente"* →
  z-score su `files_copied`
- *"il throughput è calato del 60% su tre run consecutive"* → `RunComparison` già lo calcola

Proprietà di questa classe: **offline, deterministica, testabile con unit test, a costo zero per
run, funziona alle 3 di notte dentro un servizio Windows senza rete.** Per uno strumento di
backup queste non sono proprietà accessorie: sono il requisito.

### (b) Inferenza LLM — linguaggio naturale vero

Rispondere a *"perché il backup del NAS è lento da martedì?"* con una narrazione, o correlare in
prosa un log da 356 MB, richiede un modello. Comporta: dipendenza di rete, gestione di una chiave
API, non determinismo, costo per invocazione, e — il punto più serio — **invio all'esterno di
percorsi di file, hostname e nomi di condivisione**, cioè la mappa del file server aziendale.

**Raccomandazione**: (a) dentro il binario, (b) fuori. E (b) in larga parte **esiste già**: è la
CLI agentica che stai usando in questo momento, che legge i report con la molecola 7. Non serve
reimplementarla dentro `robocopy_ingest.exe` per averla.

---

## 4. Valutazione per singola richiesta

### 4.1 Pianificazione e ottimizzazione autonoma — 🟡 fattibile, prerequisito mancante

**Requisiti presenti**: sì, tutti. Ogni report contiene durata per fase, throughput, conteggi,
configurazione usata e metadati host (inclusi `logical_cpus`, che serve per suggerire `--threads`).

**Requisito mancante**: un indice. Oggi ci sono 18 report in `_ops_reports/`, ciascuno un file
JSON isolato. Per rispondere a *"quanto durano di solito i backup del NAS?"* bisogna aprirli tutti
e riconciliarli a mano. Con `{timestamp}` in `--report-path` (P1) il numero di file cresce a ogni
run, quindi il problema peggiora nel tempo.

Questo non è un problema nuovo né una mia invenzione: **F50 in backlog dice già esattamente
questo** — *"I report JSON esistono già: serve un indice consultabile, non nuovi dati."*

**Cosa servirebbe**: un indice SQLite (`rusqlite`, una dipendenza) alimentato a fine run, e un
sottocomando `rustcopy advise` che produca suggerimenti motivati con i numeri che li sostengono.

**Vincolo di progettazione**: deve **suggerire**, mai applicare. Un pianificatore che riscrive da
sé la retention di un backup è un pianificatore che può cancellare dati.

### 4.2 Monitoraggio della run e azioni correttive — 🟢 metà fatta

**Già presente**: interpretazione post-run completa (molecola 7), exit code semantici,
`RunComparison`, `webhook_error`/`post_command_error`/`copy_error` nel report.

**Mancante**:

- **Durante** la run: oggi il progresso va su `indicatif` e sui log, ma nessuno lo confronta con
  l'atteso. Un rilevamento tipo *"siamo al 20% dopo il tempo in cui di solito eravamo all'80%"*
  richiede la baseline storica del §4.1 — di nuovo l'indice.
- **Correlazione storica**: *"questo stesso mismatch si ripresenta ogni lunedì"* non è
  esprimibile senza storico interrogabile.

**Il vincolo di sicurezza è qui il più stringente di tutta la valutazione.** "Azione correttiva"
su uno strumento di backup significa, nel caso peggiore, cancellare. Il design conservato di
**F61 lo mette già nero su bianco** per la superficie MCP: *mai* esporre `--force-purge`,
`--mirror` non presidiato, purge di retention, install/uninstall di servizi e schedule. Quella
prescrizione va estesa a qualunque layer AI, senza eccezioni. **Un componente AI può proporre;
eseguire operazioni distruttive resta dell'operatore.**

### 4.3 Diagnosi e Q&A in linguaggio naturale — 🟢 già ottenibile

**Uso interattivo**: già funziona. Apri una CLI agentica, la skill `rustcopy-flow` si attiva sui
trigger dichiarati, la molecola 7 legge i report e risponde in italiano. Costo aggiuntivo: zero.

**Uso non presidiato** (il servizio che si auto-diagnostica alle 3 di notte): tecnicamente
possibile — `reqwest` con `rustls` è **già** una dipendenza, quindi il trasporto HTTPS verso una
API di modello non ne aggiungerebbe una nuova. Ma sconsiglio di metterlo sul percorso non
presidiato, per tre ragioni concrete:

1. **Affidabilità.** Un backup la cui riuscita dipende dalla raggiungibilità di una API è un
   backup peggiore. Se il modello va in timeout, cosa fa la run? Se la risposta è "prosegue e
   ignora", allora la funzione non stava presidiando nulla.
2. **Riservatezza.** Il log contiene percorsi, hostname, nomi di share. Su una rete aziendale è
   una mappa dell'infrastruttura.
3. **Volume.** Un log misurato a 356 MB su una run reale da 1.34M file (`_ops_reports/full-profile-test.log`,
   citato in `CLAUDE.md` per D18) non entra in nessuna finestra di contesto. Andrebbe comunque
   pre-aggregato — e una volta pre-aggregato, il §4.1 risponde già alla maggior parte delle domande
   senza modello.

**Se lo si volesse comunque**: `--ai-explain` dietro feature flag, opt-in, che invii un **riassunto
redatto** (metriche e conteggi, non percorsi), mai sul percorso schedulato, e che **non possa in
nessun caso alterare l'exit code** — quello è un contratto con lo scheduler.

### 4.4 Orchestratore multi-agente visibile come "harness" — 🟢 esiste, con un'ambiguità da sciogliere

**Attenzione a un equivoco di terminologia in questo repo**: `docs/archive/AGENT_HARNESS_PLAN.md`
parla di *harness di test YAML* (scenari, fixture, assertion), **non** di agenti AI. È archiviato e
mai eseguito. Se dici "harness" a qualcuno che legge questo repo, penserà a quello.

L'orchestratore multi-agente vero è `rustcopy-flow`, ed è attivo. Ciò che manca rispetto alla tua
descrizione:

| Gap | Costo | Nota |
|---|---|---|
| Molecola `notify` assente | Basso | Le altre 7 esistono; questa seguirebbe `_template-molecule.md` |
| Nessuna esecuzione parallela | Medio | Le molecole sono sequenziali per costruzione — ed è corretto: dry-run *deve* precedere l'esecuzione |
| Nessuna "visibilità" come dashboard | Medio-alto | Oggi la traccia è la conversazione con l'agente |

Sulla parallelizzazione vale la pena essere espliciti: l'ordine sequenziale non è una limitazione
da rimuovere, è il presidio di sicurezza. `Plan → Dry-run → Checkpoint umano → Esecuzione` è
esattamente ciò che impedisce a un agente di lanciare un `--mirror` su una destinazione sbagliata.

### 4.5 Memoria a context graph — 🔴 vedi §6

---

## 5. Vincoli architetturali non negoziabili

Qualunque cosa si decida di costruire deve rispettare regole già in vigore. Non sono preferenze
di stile: ciascuna nasce da un difetto reale già occorso.

| Vincolo | Origine | Implicazione per il lavoro AI |
|---|---|---|
| Feature-gating stretto | `AGENTS.md` regola 8 | Ogni dipendenza AI dietro una feature. Il gate `cargo tree --locked \| grep -i axum` va replicato per la nuova (es. `rmcp`, `rusqlite`) |
| Dispatch servizio prima del runtime tokio | Regola 13 | Nessuna inizializzazione AI prima di `service::is_service_launch()` |
| Exit code = contratto | Regola 12 | L'AI **non** può cambiare l'exit code. `4` deve restare `4` |
| Mirror safety | Regola 6 | `check_mirror_safety` non è bypassabile da un agente |
| Il binario di default resta magro | Regola 8 + F41 | Un modello non entra nel percorso di default, come axum non ci è entrato |
| Scrittura atomica | D14 | Un eventuale indice va scritto via `atomic_write`, non `fs::write` |
| Memoria O(chunk), non O(file) | Regola 4, D20, D21 | Un indice non deve caricare in RAM l'intero storico: `GenerationIndex` è il precedente da imitare |

Nota su `rusqlite`: porta con sé una compilazione C (SQLite amalgamation). Su un crate che oggi
compila e testa pulito su Windows **e** Linux in CI, va verificato prima di impegnarsi — non è un
dettaglio, è il tipo di cosa che D16 ha insegnato a controllare invece che assumere.

---

## 6. Il context graph: dove dissento

Chiedi una memoria a context graph per "interpolare fra agenti e servizi". Rispondo nel merito,
perché credo che qui la soluzione proposta non sia commisurata al problema.

**Primo, un chiarimento su ciò che esiste già.** `graphify-out/graph.json` (1.4 MB, tracciato nel
repo) c'è, ma è un grafo **del codice sorgente** — serve a navigare i moduli Rust, non i backup. E
su di esso vige già una prescrizione esplicita, in `CLAUDE.md` sotto D10: *il grafo graphify è un
ausilio di navigazione, **mai** un gate anti-dead-code*. È stato riclassificato il 23 Agosto 2026
proprio perché se ne erano sopravvalutate le garanzie. Vale la pena non ripetere quell'errore su
un secondo grafo.

**Secondo, il merito.** Guardiamo le entità reali del dominio:

```
job ──< generazione ──< file
 │           │
 │           └──< run ──< esito (exit code, mismatch, timing)
 └──< schedule
```

Sono cinque entità e quattro relazioni, tutte **ad albero e con cardinalità nota**. Non ci sono
cicli, non c'è traversata a profondità variabile, non c'è ricerca di cammini. La query più
complessa che serve davvero è del tipo *"per il job X, la mediana della durata delle ultime 30 run
in cui l'exit code era 0"* — che è una `GROUP BY` con una `WHERE`.

Un grafo dà valore quando le relazioni sono molte, eterogenee e percorse a profondità arbitraria.
Qui non lo sono. Il costo — un motore in più, un formato in più, un'altra cosa che può corrompersi
— non è ripagato. E la corruzione non è ipotetica in questo progetto: D14 documenta che un
`GenerationManifest` corrotto è **fatale** e blocca ogni run futura finché un operatore non
interviene a mano.

**La mia proposta alternativa**: una tabella di indice SQLite. Ottieni le stesse risposte, con
transazioni, un formato che sopravvive agli aggiornamenti, e uno strumento (`sqlite3`) che
qualunque operatore sa già interrogare a mano quando qualcosa va storto alle 3 di notte.

Se emergesse un caso d'uso che il modello relazionale non copre, il grafo si può aggiungere sopra
un indice esistente. Il contrario — togliere un grafo da un'architettura che ci si è appoggiata —
è molto più costoso.

---

## 7. Rischi

| Rischio | Gravità | Mitigazione |
|---|---|---|
| Un agente esegue un'operazione distruttiva | 🔴 Alta | Estendere la lista di divieti di F61 a ogni superficie AI. Proporre, mai eseguire |
| Percorsi/hostname inviati a una API esterna | 🟠 Media-alta | Redazione prima dell'invio; opt-in esplicito; mai sul percorso schedulato |
| Il backup dipende dalla rete o da una API | 🟠 Media-alta | Nessuna funzione AI sul percorso critico. Fallimento AI ⇒ backup comunque completo |
| Suggerimento di retention sbagliato ⇒ perdita di storia | 🟠 Media | Mostrare sempre il calcolo e i dati; richiedere conferma; mai auto-applicare |
| Deriva fra due superfici agentiche | 🟡 Media | Già previsto in F61: *"valutare prima se un livello condiviso con `rustcopy-flow` evita di far divergere due superfici"* |
| Non determinismo in uno strumento di backup | 🟡 Media | Confinare l'LLM alla spiegazione; le decisioni restano deterministiche |
| Aumento della superficie di dipendenze | 🟡 Media | Feature-gate + gate `cargo tree --locked` per ciascuna |

---

## 8. Percorso consigliato

Ordinato per rapporto valore/rischio. **Ogni fase ha valore autonomo**: se ci si ferma dopo la
Fase 1, ciò che è stato costruito serve comunque.

### Fase 0 — Indice dei report (prerequisito, nessuna AI)

Chiude **F50**, già in backlog. Un indice SQLite alimentato a fine run: una riga per run, con job,
timestamp, durata per fase, throughput, conteggi, exit code, esito integrità.

*Perché prima di tutto*: le richieste 1, 2 e 3 hanno **tutte** bisogno di interrogare lo storico.
Senza indice, ognuna dovrebbe rileggere e riconciliare una directory di JSON a ogni domanda.

*Valore anche senza AI*: risponde già a "quando è fallito l'ultimo backup del NAS?" con una query.

### Fase 1 — `rustcopy advise` (deterministico, nessun modello)

Statistica sull'indice, che produce suggerimenti **motivati dai numeri che li sostengono**:
finestra di schedulazione dai percentili di durata, retention dal tasso di variazione osservato,
`--threads` da throughput e `logical_cpus`, anomalie da z-score.

*Proprietà*: offline, deterministico, unit-testabile, costo zero per run. Copre onestamente la
maggior parte delle richieste 1 e 2.

### Fase 2 — Molecole `diagnose` e `notify` in `rustcopy-flow`

Zero codice Rust, zero dipendenze. Una molecola che interroga l'indice della Fase 0 e risponde a
domande sullo storico; una che colma il gap `notify` del §2.2.

*Copre la richiesta 3 per l'uso interattivo, che è l'uso in cui serve davvero.*

### Fase 3 — `--ai-explain` opzionale (solo se serve davvero)

Feature-gated, opt-in, invia un riassunto **redatto**, non può alterare l'exit code, mai sul
percorso schedulato.

*Da fare solo se dopo la Fase 2 resta un bisogno concreto e nominabile.* Ho il sospetto che non
resterà.

### Fase 4 — Server MCP (F61), solo su necessità concreta

Il design è già conservato in ROADMAP F61, con la lista dei tool ammessi e vietati. La condizione
di attivazione è già scritta: *"quando emerge un host agentico non-CLI concreto da supportare"*.
Non anticiparla.

---

## 9. Cosa non farei

Per simmetria, e perché un'analisi che raccomanda soltanto è meno utile:

| Cosa | Perché no |
|---|---|
| Un database a grafo | §6. Relazioni ad albero, cardinalità nota: SQLite basta |
| Agenti dentro il processo di backup | Il processo deve restare prevedibile, misurabile, uccidibile con Ctrl+C |
| Esecuzione autonoma di operazioni distruttive | Il rischio non è simmetrico: un backup non fatto si rifà, dati cancellati no |
| Un modello sul percorso schedulato | Introduce una dipendenza di rete nel punto in cui l'affidabilità conta di più |
| Riscrivere `rustcopy-flow` dentro il binario | Duplicherebbe una superficie che già funziona ovunque ci sia una shell |
| Un "AI agent" che modifica il TOML da solo | La configurazione è ciò che l'operatore controlla. Suggerire il diff, non applicarlo |

---

## 10. Stime

Indicative, per dare un ordine di grandezza — non impegni.

| Fase | Dimensione | Dipendenze nuove | Rischio |
|---|---|---|---|
| 0 — Indice | Media | `rusqlite` (⚠️ verificare la build C su CI Linux+Windows) | Basso |
| 1 — `advise` | Media | Nessuna | Basso |
| 2 — Molecole | Piccola | Nessuna | Molto basso |
| 3 — `--ai-explain` | Media | Nessuna nuova per il trasporto (`reqwest` c'è già) | **Alto** — riservatezza, non determinismo |
| 4 — MCP (F61) | Media | `rmcp` | Medio |

---

## 11. Risposta sintetica

**Possiedi già i requisiti per la maggior parte di ciò che hai chiesto**, e per il punto 4
possiedi direttamente la cosa: `rustcopy-flow` è un orchestratore multi-agente funzionante, con
otto molecole specializzate e i presidi di sicurezza giusti già scritti dentro.

Il collo di bottiglia non è l'intelligenza — è la **memoria**. Ogni run produce un report ricco e
poi lo dimentica. Un indice interrogabile (F50, già in backlog) sblocca da solo pianificazione,
rilevamento anomalie e diagnosi storica, **senza alcun modello**, in modo deterministico e
testabile.

L'unico punto su cui dissento nel merito è il context graph: il requisito tecnico c'è, il bisogno
no, e il costo di manutenzione supera il ritorno per il dominio in questione.

---

## Riferimenti

- `.agents/skills/rustcopy-flow/SKILL.md` — orchestratore composto e le sue 8 molecole
- [`ROADMAP.md`](ROADMAP.md) — F50 (cronologia navigabile), F61 (server MCP, design conservato)
- [`ANALYSIS.md`](ANALYSIS.md) — D10 (limiti del grafo graphify), D14 (manifest corrotto = fatale), D20/D21 (disciplina di memoria)
- [`AGENTS.md`](AGENTS.md) — regole 4, 6, 8, 12, 13
- [`PIANO_MIGLIORAMENTI.md`](PIANO_MIGLIORAMENTI.md) — P1/P2 (già chiusi), P3/P4 (aperti)
- `docs/archive/AGENT_HARNESS_PLAN.md` — harness *di test*, da non confondere con l'orchestrazione
  agentica. Citato deliberatamente **senza link**: è un concetto `deprecated`, e questa valutazione
  lo nomina per segnalare l'omonimia, non perché ne dipenda.

---

## 12. Registro di implementazione (25 Agosto 2026)

Cosa è stato effettivamente costruito, e — più utile — **dove questa analisi si è rivelata
sbagliata una volta messa alla prova**.

### Fatto

| Fase | Consegnato |
|---|---|
| 0 — Indice | `src/history.rs`: `RunRecord`/`RunHistory`, NDJSON append-only, finestra limitata in streaming. Scritto da entrambe le pipeline (standard e generazioni). |
| 1 — Advisor | `src/advise.rs` + flag `--advise`: schedulazione, retention, thread, anomalie, integrità ricorrente. Deterministico, nessun modello, nessuna rete. |
| 2 — Molecole | `rustcopy-flow` v1.1.0: Molecola 8 (Diagnose) e Molecola 9 (Notify), Scenari 5 e 6. |

Test: +36 (362 con feature di default, 377 con `notify-server`), di cui 4 black-box sul binario
compilato.

### Non fatto, e perché

**Fase 3 (`--ai-explain`)** e **Fase 4 (server MCP, F61)** hanno condizioni di attivazione scritte
in §8: rispettivamente "solo se dopo la Fase 2 resta un bisogno concreto e nominabile" e "quando
emerge un host agentico non-CLI concreto". Nessuna delle due si è verificata. Seguire il percorso
proposto significa rispettarne i cancelli, non ignorarli.

### Due punti in cui questa analisi era sbagliata

**1. SQLite era la scelta peggiore.** §8 proponeva `rusqlite`, segnalandone la build C come rischio
da verificare in CI. Guardata contro il precedente già nel codice, la dipendenza non andava presa
affatto: D19/D20 avevano già stabilito NDJSON append-only con lettori in streaming, recupero di
righe troncate e rilevamento di formato legacy. A questa scala — poche centinaia di byte per run,
milioni di run per arrivare a qualcosa di grande — indici e query planner non comprano nulla, e un
operatore può fare `grep` sul file alle 3 di notte. **Zero dipendenze nuove**, contro una con
toolchain C sulla matrice Windows+Linux.

**2. L'indice non poteva stare in `--dest`.** Era la posizione ovvia — `.ingest_cache` e
`.rustcopy_generations.json` stanno lì. È sbagliata, e non per ragionamento ma per misura: con
l'indice in `--dest`, una sincronizzazione ripetuta su un albero immutato passava da **2 a 3**
elementi copiati, perché scrivere nella destinazione ne cambia l'mtime e robocopy se ne accorge
alla run successiva. Un file di statistiche non deve perturbare il trasferimento che misura. Gli
altri due se lo permettono solo perché sono opt-in; questo lo scrive ogni run. Spostato accanto ai
report, che è già il posto di rustcopy per le registrazioni *sulle* run.

Trovato dai test black-box preesistenti, non da quelli che ho scritto io.

### Un difetto latente emerso di lato

Durante il lavoro è emerso che rustcopy inventariava i **propri** file di servizio come se fossero
contenuto da copiare. Conta perché `--restore-from` inverte sorgente e destinazione: la
destinazione di ieri diventa la sorgente di oggi, e con `--decrypt` il ripristino falliva su un
file che non era mai stato cifrato (`missing RCE1 header`). Chiuso con
`robocopy_ingest::is_rustcopy_metadata`, applicato in entrambi i punti di scansione. Il difetto
riguardava già `.ingest_cache` e `.rustcopy_generations.json`: semplicemente nessun test lo
raggiungeva.

### Cosa resta vero

La tesi centrale di §11 regge: il collo di bottiglia era la **memoria**, non l'intelligenza. Le
richieste 1, 2 e 3 sono ora soddisfatte in misura sostanziale **senza un solo modello linguistico**
— e la parte più difficile non è stata la statistica, ma impedirle di mentire: rifiutare di
rispondere sotto le 3 run, escludere i dry-run e le run troppo piccole dai campioni, e pretendere
che un'anomalia sia grande *oltre* che statisticamente estrema.
