---
type: Reference
title: Come procedere con la GUI Tauri — piano operativo e vincoli
description: Percorso per la GUI Tauri (milestone 7.0.0): impatto misurato sulle prestazioni del motore di copia, tre decisioni con raccomandazione motivata, e il registro della rinumerazione 7.0.0/8.0.0 che ha rimosso uno stallo nella roadmap.
status: draft
generated:
  by: process:claude-code
  at: 2026-08-27T00:00:00Z
verified:
  by: process:claude-code
  at: 2026-09-02T00:00:00Z
---

# Come procedere con la GUI Tauri

> **Stato al 2 Settembre 2026**: §3, §5 e §6 tutte applicate. Lo stack (Svelte + Tailwind) e
> l'ambito v1 sono stati realizzati, e **§5.3 è stata accolta**: F60 impacchetta la console come
> componente opzionale di `installer/rustcopy.iss`, non come secondo bundle. La ROADMAP è stata
> aggiornata di conseguenza — i due documenti non divergono più sul veicolo di distribuzione.
>
> **L'ambito "sola lettura" della §5.2 è invece decaduto, di proposito**: F54 ha introdotto **un**
> percorso di scrittura (proposte di configurazione in file nuovi). Vedi il riquadro in §5.2 e la
> riga F54 della ROADMAP per le regole che ne limitano la portata.
>
> **Per un LLM che raccoglie questo lavoro**: la rinumerazione descritta in §6 **è stata applicata**
> a `ROADMAP.md` il 27 Agosto 2026 — la GUI è la milestone **7.0.0**, il motore controllabile la
> **8.0.0 condizionale**. I due documenti concordano; questo spiega il perché, la ROADMAP registra
> l'esito. Le §4 e §8 sono vincolanti, non consigli. La §5
> contiene raccomandazioni motivate su decisioni **di prodotto**: se l'operatore non le ha
> confermate, non darle per approvate.

---

## 1. In una pagina

La milestone della GUI esiste in [`ROADMAP.md`](ROADMAP.md) — **7.0.0** dopo la rinumerazione di §6 — con otto voci (F52-F57, F59, F60). **Questo
documento non la sostituisce**: risponde alla domanda che la precede — *una GUI compromette le
prestazioni e la robustezza, che restano la priorità assoluta?* — e spiega come muoversi se la
risposta è no.

| | Fatto | Dove |
|---|---|---|
| 1 | **Una GUI installata non tocca le prestazioni della CLI.** Sono binari separati; il percorso schedulato e il servizio Windows non la vedono mai | §2 |
| 2 | Il rischio reale non è la GUI, è **come riporta il progresso** — e il pattern sicuro è già nel codice | §2.2 |
| 3 | Il cancello dichiarato "non negoziabile" era **già superato**; il riquadro obsoleto è stato corretto | §3 |
| 4 | **F52** (workspace Cargo) è un prerequisito duro e non è fatto | §4 |
| 5 | Il "motore controllabile" **non era un prerequisito**: una sola voce su nove ne dipendeva. Milestone rinumerate; F58 spostata insieme a F47/F48 | §6 |

---

## 2. La GUI rallenta il motore di copia?

La domanda è legittima e ha una risposta precisa, non rassicurante-e-basta. Va divisa in due, perché
le due metà hanno risposte diverse.

### 2.1 Installare la GUI non rallenta la CLI. Per costruzione.

F52 separa il repo in `rustcopy-core` / `rustcopy-cli` / `rustcopy-gui`. Sono **binari distinti**:

- `robocopy_ingest.exe` non linka Tauri, non avvia una WebView, non ha dipendenze JS.
- Il **percorso non presidiato** — Task Scheduler (F36), servizio Windows (F37) — invoca la CLI.
  Non c'è modo che passi dalla GUI.
- Un backup lanciato da riga di comando su una macchina dove la GUI è installata esegue
  esattamente lo stesso codice di una macchina dove non lo è.

Questa non deve restare una promessa: la §4.2 la trasforma in un **gate di CI che fallisce** se un
domani la CLI acquisisse una dipendenza dalla GUI. È lo stesso meccanismo con cui la regola 8 tiene
axum fuori dal binario di default.

> **Conseguenza diretta sulla tua domanda su installazione separata**: l'architettura la impone già.
> Vedi §5.3 — installare **solo la CLI** resta sempre possibile, ed è ciò che sceglie il percorso
> silenzioso usato dai server (`/TYPE=cli`). Nell'installer interattivo il tipo predefinito è
> invece CLI + console: le due cose non sono in conflitto, sono due percorsi con utenti diversi.

### 2.2 Eseguire un backup *dalla* GUI: qui il rischio è reale, ed è misurabile

Se è la GUI a ospitare la copia, il costo aggiuntivo non è "Tauri". È **la frequenza con cui il
motore comunica il progresso all'interfaccia**. E su questo il progetto ha già sbagliato una volta,
in modo istruttivo.

**Il precedente (D18)**: il livello di log `debug` emetteva una riga per file copiato. Su una run
reale da 1.34M file il log risultante era **356 MB, 76× più grande** dello stesso run a `info`. Il
lavoro per-file nel percorso caldo è il difetto di prestazioni caratteristico di questo dominio.

**La scala in gioco** (`_ops_reports/full-profile-test.json`, run reale):

| Metrica | Valore |
|---|---|
| File | 1.340.613 |
| Byte | 252 GB |
| Durata | 354 s |
| Throughput | 711,8 MB/s |
| **File al secondo** | **≈ 3.787** |

Una GUI che emettesse un evento IPC per file dovrebbe consegnare ~3.800 messaggi al secondo a una
WebView. Sarebbe D18 di nuovo, peggio.

**Ma il pattern corretto è già implementato in `src/progress.rs`**, e non va inventato:

- Percorso caldo: `fetch_add(..., Ordering::Relaxed)` su contatori atomici — lock-free, costo in
  nanosecondi, nessuna allocazione, nessun I/O.
- Rendering: `enable_steady_tick(Duration::from_millis(200))` — **disaccoppiato dal numero di file**.
- Lettura: getter pollabili (`current_bytes()`, `files()`, `average_mbps()`).
- Esiste già `ThroughputProgress::hidden()` e un trait `ProgressSink` con implementazione no-op: il
  punto di innesto per un consumatore che non è il terminale **c'è già**.

A 200 ms di campionamento sono **5 aggiornamenti al secondo**, che il trasferimento copi 10 file o
1,34 milioni: circa **757× meno eventi** dell'approccio per-file.

### 2.3 Le tre regole che rendono la cosa sicura

Vincolanti per chiunque implementi F53/F58:

1. **La GUI campiona, il motore non notifica.** Il frontend legge i contatori atomici su un suo
   timer. Nessun evento IPC per file, mai — nemmeno "solo per i file grandi".
2. **L'inventario non attraversa il confine IPC.** `ScanSummary::files` è un `Arc<[ScannedFile]>`
   proprio per non essere copiato (D21); serializzarlo in JSON verso la WebView annullerebbe quel
   lavoro. Un manifest per l'albero da 1.34M file misura ~174 MB. Alla UI vanno **aggregati** e,
   al più, una finestra paginata su richiesta.
3. **Il default resta la CLI.** Per i backup pianificati e non presidiati la GUI non è coinvolta e
   non deve diventarlo.

### 2.4 E la robustezza?

La preoccupazione — *"copie che non devono sbagliare, e devono restare recuperabili dopo
un'interruzione"* — riguarda garanzie che vivono **nel core, non nella UI**: `atomic_write`
(D14), il checkpoint su `Ctrl+C` (F31), la scrittura NDJSON append-only (D19), il `VssGuard` con
`Drop` sincrono (F30), gli exit code semantici (regola 12).

Una GUI che rispetta la §4 non può indebolirle, perché non le reimplementa: le chiama. E la §5.2
raccomanda una v1 **senza percorsi di scrittura**, che è la garanzia più forte disponibile — una
versione che *per costruzione* non può danneggiare un backup.

Un punto di attenzione onesto: un processo GUI che viene chiuso mentre ospita una copia è un caso
nuovo rispetto al `Ctrl+C` della CLI. Va gestito con lo stesso meccanismo (checkpoint prima di
uscire), e **va testato**, non assunto.

---

## 3. Il cancello era già aperto — corretto

Il riquadro in testa alla milestone recitava:

> restano 4 difetti P1 aperti in 5.2.0 (F26a-d: flag muti, blocco del runtime su mirror safety,
> versionamento schema, junction). Prerequisito non negoziabile: **5.2.0 chiusa per intero**.

**È obsoleto.** F26a-d risultano `[x] Completato` dal 3 Agosto 2026, e la 5.2.0 è marcata
"Milestone chiusa" nello stesso file, poche decine di righe più in alto.

Il prerequisito era **soddisfatto**. Finché quel testo restava, chi leggeva la ROADMAP per decidere
se iniziare concludeva — a torto — che non si poteva.

✅ **Corretto il 27 Agosto 2026**: il riquadro ora dichiara il prerequisito soddisfatto e indica
F52 come unico prerequisito reale.

---

## 4. L'architettura, e come renderla verificabile

### 4.1 I quattro divieti

F53 lo dice in una riga, e va reso esplicito perché è il vincolo che protegge tutto il lavoro fatto:

> La GUI è un **consumatore della lib** come lo è `notify-server`: i comandi `#[tauri::command]`
> chiamano `CopyEngine`/`ScanSummary`/`IngestReport`, non reimplementano logica.

1. **Il frontend non decide mai.** Non calcola exit code, non stabilisce se una purge è sicura, non
   valuta se un mismatch è transitorio. Chiede alla lib e mostra la risposta.
2. **Nessuna logica duplicata in TypeScript.** Se serve un giudizio, il giudizio si aggiunge alla
   lib. Vale in particolare per `check_mirror_safety`, `GenerationManifest::cycles` e il calcolo
   dell'exit code.
3. **La GUI non allenta un presidio della CLI.** `--force-purge` non diventa una checkbox
   pre-spuntata. I divieti conservati in F61 e applicati a `--advise` valgono identici.
4. **La toolchain JS non entra nel crate della CLI.** È il motivo di esistere di F52.

### 4.2 Il gate che trasforma la promessa in garanzia

La regola 8 tiene axum fuori dal binario di default con un controllo eseguibile, non con una
convenzione. La stessa cosa va fatta qui, e va aggiunta a `ci.yml` **insieme a F52**, non dopo:

```bash
# La CLI non deve mai acquisire una dipendenza dalla GUI
if cargo tree --locked -p rustcopy-cli | grep -qi tauri; then
  echo "::error::tauri ha raggiunto il binario della CLI"
  exit 1
fi
```

> ⚠️ **La forma `... | grep -qi tauri && exit 1` è sbagliata** e va evitata: quando `grep` non
> trova nulla esce con stato 1, che diventa lo stato dell'intera lista `&&` — il gate fallirebbe
> **proprio quando l'albero è conforme**. Usare `if`, come già fa il gate axum in `ci.yml`.
> (Segnalato da CodeRabbit sulla PR di questo documento; la prima stesura conteneva l'errore.)

È questo che rende la §2.1 una proprietà verificata a ogni commit invece di un'intenzione. È anche
la risposta più solida alla domanda "e se un domani qualcuno collega le due cose?": la CI rifiuta.

### 4.3 Perché F52 viene prima di tutto

Tauri porta npm/vite, `tauri.conf.json`, icone e bundler. `notify-server` può restare un binario
feature-gated perché è **puro Rust**; la GUI no.

```text
crates/rustcopy-core   ← la lib attuale
crates/rustcopy-cli    ← il binario di oggi
crates/rustcopy-gui    ← nuovo, con la toolchain JS confinata qui
```

La ristrutturazione tocca ogni percorso in `ci.yml`, `scripts/okf-docs.sh`, `installer/rustcopy.iss`
e i gate `cargo tree --locked`. Va fatta come lavoro **a sé**, con la CI verde alla fine e nessuna
funzionalità nuova nello stesso commit.

---

## 5. Le tre decisioni — raccomandazione motivata

Sono decisioni di prodotto. Qui c'è una raccomandazione per ciascuna, con il criterio dichiarato:
**prestazioni e robustezza prima di tutto, sicurezza quando entra una GUI.**

### 5.1 Stack frontend → **Svelte + Tailwind**

| Opzione | Dipendenze npm | Runtime | Valutazione |
|---|---|---|---|
| **Svelte + Tailwind** | Poche; Tailwind è solo build-time | Compilato via, nessun virtual DOM | ✅ **Raccomandato** |
| React + shadcn/ui | Decine (React, ReactDOM, primitive Radix, utility) | Virtual DOM, diffing a ogni update | Più componenti pronti, superficie molto maggiore |
| HTML + Tailwind puro | Minime | Nessuno | Stato a mano; F58 diventa faticoso |

Tre ragioni, nell'ordine dei tuoi criteri:

1. **Sicurezza — è il criterio decisivo.** La catena di fornitura npm è di gran lunga la maggiore
   superficie d'attacco *nuova* che il progetto assumerebbe. Pesa più del solito qui: questa
   applicazione richiede **Amministratore** per VSS (F30) e per l'installazione del servizio (F37),
   e F56 gestirà credenziali SMB/NAS/SMTP e chiavi di cifratura. Meno pacchetti transitivi = meno
   superficie. Va comunque aggiunto un audit npm accanto a `cargo-audit`, qualunque sia la scelta.
2. **Prestazioni.** Svelte compila in operazioni DOM dirette; niente diffing a ogni aggiornamento.
   Con il campionamento a 200 ms della §2.2 la differenza è modesta in assoluto — ma va nella
   direzione giusta e non ne costa nulla.
3. **Coerenza col progetto.** Questo repo ha una disciplina spiccata sulle dipendenze: regola 8,
   gate `cargo tree --locked`, D19/D20 che hanno evitato SQLite avendone la scusa pronta. Portare
   centinaia di pacchetti npm sarebbe in tensione con quella disciplina.

`ui-ux-pro-max` copre sia `svelte.csv` sia `html-tailwind.csv`, quindi la scelta non perde supporto
di design (§10).

> ✅ **Confermato e misurato il 31 Agosto 2026.** Il frontend realizzato porta **52 pacchetti npm,
> zero vulnerabilità**, e produce 41 KB di JS più 8 KB di CSS. Era il criterio dichiarato — la
> superficie della catena di fornitura — ed è l'unico su cui la scelta si giocava davvero.

### 5.2 Ambito v1 → **console in sola lettura** (superato in parte il 2 Set 2026)

Nessun percorso di scrittura: mostra job configurati, avanzamento in sola lettura, cronologia
(`.rustcopy_history.jsonl`, già esistente) e report. Creazione/modifica dei job, credenziali e
operazioni distruttive arrivano dopo.


> **Aggiornamento 2 Set 2026.** Vale ancora per tutto tranne un punto: F54 ha aggiunto un
> percorso di scrittura, e uno solo — `job_editor::propose_config`, che scrive una **proposta** in
> un file nuovo e non tocca mai la configurazione in uso. La garanzia "non può danneggiare un
> backup" è quindi decaduta e sostituita da regole esplicite: l'editor può restringere il rischio,
> mai allargarlo (riga F54 della ROADMAP). Nessun comando esegue, copia, cancella, pianifica o
> installa: quello resta fuori.

Quattro ragioni:

1. **È la garanzia di robustezza più forte disponibile.** Una v1 senza percorso di scrittura **non
   può** corrompere un backup, né sbagliare una purge. Non "è improbabile che": non può.
2. **Non dipende dalla 7.0.0** (§6), quindi è realizzabile subito dopo F52.
3. **Verifica l'architettura al costo minimo.** Se la logica sta filtrando nel frontend contro la
   §4.1, si scopre qui, dove non c'è nulla da perdere.
4. **Rimanda le parti pericolose a quando c'è esperienza.** F56 (credenziali) e F55 (script
   configurabili) portano i rischi di §8: meglio affrontarli su una base già collaudata.

### 5.3 Distribuzione → **Tauri costruisce, Inno Setup distribuisce, GUI opzionale**

> ✅ **Accolta e implementata il 2 Settembre 2026 (F60).** La riga F60 della ROADMAP è stata
> riscritta di conseguenza: non più MSI/NSIS via bundler Tauri. La decisione poggia su una misura
> presa prima di prenderla — la console pesa 8,9 MB contro un'installazione CLI da 13,7, perché
> Tauri rende attraverso la WebView2 di sistema invece di impacchettare un motore browser.

Non un secondo installer. `installer/rustcopy.iss` esiste, è **completato e testato realmente**
(ciclo installazione silenziosa → PATH → disinstallazione → PATH ripristinato). Inno Setup
impacchetta la console come **componente opzionale**.

> **Correzione rispetto alla formulazione originaria di questa sezione**: il bundler Tauri **non**
> viene usato nemmeno per costruire. `cargo build --release -p rustcopy-gui` produce l'eseguibile
> direttamente, e `bundle.active` resta `false` in `tauri.conf.json` — accenderlo genererebbe un
> MSI/NSIS separato per la sola console, cioè esattamente la separazione che questa sezione evita.

```text
[X] rustcopy CLI          (obbligatorio, non deselezionabile)
[X] Interfaccia grafica   (opzionale; preselezionata nell'installazione interattiva,
                           assente con /TYPE=cli)
```

Quattro ragioni:

1. **È esattamente ciò che hai chiesto**: l'utente sceglie solo CLI oppure CLI + GUI.
2. **Un solo installer da mantenere**, già collaudato, con una sola voce di disinstallazione e un
   solo percorso di aggiornamento.
3. **Una sola firma del codice.** Su Windows un eseguibile non firmato che chiede privilegi genera
   avvisi SmartScreen: un solo certificato e un solo passo di firma è meno da sbagliare.
4. **La CLI è sempre installata e non è deselezionabile** (`Flags: fixed`), perché è il componente
   che deve funzionare non presidiato.

   **Divergenza dichiarata rispetto alla formulazione originaria** («componente opzionale,
   deselezionata di default»): nell'installer realizzato il tipo predefinito è **CLI + console**.
   Il ragionamento della §2.3 punto 3 — un server non deve ritrovarsi una GUI — resta valido ma si
   applica al percorso che i server usano davvero, cioè l'installazione silenziosa, dove chi
   distribuisce sceglie esplicitamente (`/TYPE=cli`). Un'installazione interattiva è invece una
   persona davanti allo schermo, per cui il default sensato è l'insieme completo. La console non
   incide comunque sulle prestazioni di un backup: è un binario separato, e il gate di `ci.yml`
   dimostra che la CLI non ne acquisisce dipendenze.

F60 è stata riscritta per dire questo, quindi il conflitto potenziale non esiste più: nessuno dei
due documenti prescrive più il bundler Tauri.

---

## 6. Il vincolo di sequenza non esisteva — e nascondeva uno stallo

La ROADMAP presentava il "motore controllabile" come prerequisito della GUI. **Verificato riga per
riga, non lo era**, e quella formulazione produceva uno stallo che nessuno aveva deciso.

✅ **Rinumerazione applicata il 27 Agosto 2026.** Questa sezione resta come registro del perché.

### 6.1 Una voce su nove

Dipendenze dichiarate dalle nove voci della 8.0.0:

| Voce | Dipende da | Stato della dipendenza |
|---|---|---|
| F52, F53, F56, F57, F60 | — | nessuna |
| F54 | F33, F34 | ✅ entrambe chiuse |
| F55 | F39 | ✅ chiusa |
| F59 | F50 | 🟡 indice chiuso; la navigazione **è** F59 stessa |
| **F58** | **F47** | ❌ **l'unica dipendenza reale dalla 7.0.0** |

Otto voci su nove sono realizzabili **oggi**. Il prerequisito è quindi F47 per F58, non 7.0.0 per
8.0.0.

### 6.2 Lo stallo

La 7.0.0 porta in testa questo:

> 🗄️ **Rimandata al backlog il 5 Agosto 2026** — non c'è un bisogno concreto di interattività oggi.

e poche righe dopo:

> Va fatta **prima** della GUI, altrimenti si ottiene una finestra con pulsanti collegati a nulla.

Le due affermazioni insieme significano che **la GUI è bloccata dietro un lavoro che è stato messo
in backlog perché nessuno l'ha chiesto.** Non è una decisione: è un effetto collaterale di due
scelte prese in momenti diversi, ciascuna sensata da sola.

### 6.3 L'argomento dei "pulsanti collegati a nulla" è un vincolo di design, non di sequenza

È corretto: non si spediscono pulsanti disattivati. Ma la conseguenza è **non disegnarli**, non
"rimandare tutta la GUI". La v1 in sola lettura raccomandata in §5.2 non ha pausa né skip da
nessuna parte, quindi l'argomento non la tocca.

### 6.4 La rinumerazione applicata

Non uno scambio in blocco: le due milestone non sono blocchi omogenei. La divisione che segue le
dipendenze reali è questa.

**7.0.0 — Interfaccia grafica** (era 8.0.0, meno F58): F52, F53, F54, F55, F56, F57, F59, F60.
Nessuna dipendenza aperta.

**8.0.0 — Motore controllabile, condizionale** (era 7.0.0): F47 + F48 + F58, cioè il gruppo che
condivide la stessa dipendenza architetturale (sostituire `robocopy.exe` con un motore pilotabile).

**Voci uscite da entrambe**, passate al backlog indipendente perché non hanno mai avuto a che
fare con l'interattività:

| Voce | Perché è indipendente |
|---|---|
| F46 — modalità "sposta" | Copia → verifica → elimina: i primi due passi esistono già, manca solo il terzo. Puro lavoro di core |
| F49 — coda di job | Dipende da F33, chiusa |
| F50 — cronologia | Indice già chiuso; la metà restante è F59, dentro la GUI |
| F51 — shell extension | Deliverable separato (DLL COM), lo era già |

### 6.5 Perché questo ordine è anche il più prudente

Non è solo possibile: è la scelta più conservativa rispetto alle priorità dichiarate.

1. **F47 è il singolo lavoro a maggior rischio prestazionale dell'intera roadmap.** Il baseline
   misurato è **711,8 MB/s** su 1,34M file, ottenuto da `robocopy.exe` con `/MT`. Sostituirlo con un
   motore proprio mette a rischio esattamente il numero che hai indicato come prioritario.
   Metterlo *prima* di lavoro che non ne ha bisogno è l'ordine peggiore possibile.
2. **Fare per primo il pezzo rischioso, per abilitare un pezzo meno rischioso che ha valore
   autonomo, è un errore di sequenza.** Otto noni della GUI non aspettano nulla.
3. **La GUI informa F47 invece di indovinarlo.** Una console di consultazione mostra cosa gli
   operatori chiedono davvero. Può emergere che il bisogno reale sia "interrompi e riprendi" — già
   coperto da `Ctrl+C` + checkpoint (F31) + `--resume-from` — e non "pausa e salta file".
4. **F47/F48 nascono come "parità TeraCopy"**, cioè allineamento a un concorrente, non un bisogno
   espresso. È la stessa classe di motivazione per cui F42-F45 e F61 sono stati rimandati.

### 6.6 Le condizioni di attivazione della nuova 8.0.0

Coerenti con la disciplina già usata per F61 e per le Fasi 3/4 di `VALUTAZIONE_AI.md`: condizioni
scritte, non "quando ci sarà tempo".

1. **Un bisogno concreto e nominabile** di pausa/skip per-file, espresso da un uso reale — non
   parità di funzionalità.
2. **Una misura, prima di adottarlo**: un motore nativo va confrontato con `robocopy /MT` sul
   profilo reale da 1,34M file. Se non regge i 711,8 MB/s, non si adotta. Questo gate protegge la
   priorità numero uno indipendentemente da ogni altra considerazione.

Finché non valgono entrambe, la GUI resta senza controlli interattivi e il motore resta
`robocopy.exe`. Che è lo stato odierno, e funziona.

---

## 7. Sequenza proposta

Ogni passo ha valore autonomo: fermarsi dopo il 3 lascia comunque qualcosa di usabile.

| # | Passo | Costo | Valore se ci si ferma qui |
|---|---|---|---|
| 0 | Correggere il riquadro obsoleto (§3) | Minimo | La ROADMAP smette di contraddirsi |
| 1 | **F52** workspace Cargo + il gate CI di §4.2 | Alto, isolato | `rustcopy-core` separato da `rustcopy-cli`; nessuna funzionalità nuova |
| 2 | Sistema visivo (§10): palette con token, tipografia, spaziatura, densità, stati | Basso | Riutilizzabile anche per il report HTML esistente |
| 3a | **`gui_api`** — superficie di sola lettura nel core (fatto 31 Ago 2026) | Basso | Testabile e utile già senza GUI; indipendente dallo stack |
| 3b | ~~**F53** scheletro Tauri sopra `gui_api`~~ ✅ **fatto 31 Ago 2026** | Medio | Console di consultazione utile da sola |
| 4 | **F54 + F59** job e cronologia navigabile (chiude la metà aperta di F50) | Medio | — |
| 5 | **F56** credenziali (🔴 P0) — **prima** di F55 | Medio | — |
| 6 | **F55 + F57** settings e ruoli | Medio | — |
| 7 | **F60** bundle e firma, come componente opzionale (§5.3) | Medio | — |

F56 precede F55 deliberatamente: una pagina di settings senza una sede sicura per i segreti invita
a scriverli in chiaro nel file di configurazione.

**F58 non compare più in questa sequenza**: con la rinumerazione di §6.4 passa alla nuova 8.0.0
condizionale, insieme a F47 e F48. La GUI si completa senza di esso.

---

## 7bis. Cosa è già pronto per il Passo 3

`crates/rustcopy-core/src/gui_api.rs` (31 Agosto 2026) è la superficie di sola lettura su cui i
comandi Tauri saranno **involucri sottili**: un `#[tauri::command]` chiama una funzione lì e
restituisce ciò che ottiene. È la §4.1 resa meccanica invece che dichiarativa — se il giudizio sta
in Rust ed è testato, al frontend non resta nulla da decidere.

**Non dipende dallo stack**, quindi è stata costruita prima che la §5.1 fosse confermata: nulla lì
conosce Tauri, e la scelta fra Svelte e React non la cambia.

Contiene `JobSummary`/`list_jobs` (elenco dei job configurati, con `--mirror` reso visibile per
job perché è l'impostazione più distruttiva che un job può portare), `ReportView`/`read_report`,
`HistoryView`/`read_history` e `read_advice` (F59) e `JobSettings`/`read_settings` (F55).

Quest'ultima mostra ciò che il TOML non dice: da quale strato viene il valore che vince per quel
job (`SettingOrigin`) e quali impostazioni portano una conseguenza (`caution`). Entrambi sono
giudizi di semantica del backup, quindi stanno qui e non nel frontend. `webhook_url` è troncato
a schema e host prima di attraversare il confine — l'URL di un webhook *è* la credenziale, e una
pagina impostazioni finisce negli screenshot; `pre_command`/`post_command` no, perché vedere cosa
un job esegue è l'intera ragione per guardarli.

### La regola di confine, verificata su una misura

`ScanSummary` è già al sicuro **per costruzione**: contiene `Arc<[ScannedFile]>` e **non** deriva
`Serialize`, quindi un inventario da 1,34M file non *può* essere serializzato verso una WebView
(D21). `IntegrityCheck` invece sì, e le sue tre liste per-file sono limitate solo da
`MAX_REPORTED_ERRORS` = 10.000 ciascuna: **fino a 30.000 stringhe in un solo messaggio IPC**.

`ReportView` restituisce quindi una **pagina** più il totale reale, mai la lista intera — ed è un
test, non un commento. `truncated_at_source` distingue "10.000 errori" da "almeno 10.000", perché
una UI che scrive il primo quando vale il secondo mente arrotondando.

### Cosa è deliberatamente fuori

Il progresso live. `ThroughputProgress` espone già contatori pollabili dietro atomici lock-free, e
la §2.3 richiede che la UI **campioni** su un proprio timer invece di essere notificata per file.
Avvolgerlo qui inviterebbe un'API a eventi, che è esattamente la forma che D18 ha dimostrato
rovinosa a questa scala.

---

## 8. Vincoli di sicurezza (da leggere prima di progettare)

I primi tre sono già in ROADMAP; sono qui perché è facile progettare una UI che li viola senza
accorgersene.

1. **I ruoli in un'app desktop non sono un confine di sicurezza.** Chi ha una sessione locale può
   eseguire `rustcopy.exe` direttamente, leggere il TOML o modificare i job, scavalcando la UI.
   "Operatore" impedisce **errori**, non azioni deliberate. Se serve controllo accessi reale va
   lato server. **Dichiaralo nella UI** invece di lasciar credere il contrario.
2. **Script configurabili + servizio privilegiato = escalation locale.** Se il servizio F37 gira
   come SYSTEM ed esegue gli script pre/post configurati dalla UI (F39/F55), un utente non
   amministratore che può scrivere quello script ottiene esecuzione come SYSTEM. Mitigazioni:
   account dedicato a privilegi minimi, **e/o** rifiuto degli script scrivibili da non
   amministratori (verifica ACL prima dell'esecuzione).
3. **La UI non deve diventare una nuova sede dei segreti.** Esiste già una convenzione funzionante
   (`env:NAME`, `file:PATH`, `*.local.ps1` fuori da git): F56 la **estende** al Credential Manager
   via `keyring`, non ne introduce una parallela.
4. **Le azioni distruttive non si allentano passando dalla UI.** `--mirror` con purge e
   `--keep-generations` hanno presidi espliciti ed exit code dedicati (`3` e `5`, distinti apposta).
   Nella UI: conferma che nomina **cosa** verrà cancellato e **quanto**, mai una scorciatoia
   ricordata fra le sessioni. Il rischio non è simmetrico — un backup non fatto si rifà, dati
   cancellati no.
5. **Gli exit code sono un contratto con gli scheduler.** La UI li mostra e li interpreta; non li
   ridefinisce. `4` (copia riuscita, verifica fallita) resta distinguibile da `1` (copia fallita)
   anche nel linguaggio dell'interfaccia.
6. **La superficie npm va sottoposta ad audit come quella Rust.** `security-audit.yml` copre oggi
   solo RustSec. Con una GUI serve l'equivalente per npm, con la stessa disciplina di job separati
   per trigger già adottata lì.

---

## 9. Stato delle azioni

### Fatte (27 Agosto 2026)

| # | Azione |
|---|---|
| 1 | Corretto il riquadro-prerequisito obsoleto in `ROADMAP.md` (§3) |
| 2 | **Rinumerazione applicata** (§6.4): GUI → 7.0.0; F47/F48/F58 → 8.0.0 condizionale con due condizioni di attivazione scritte; F46/F49/F50/F51 → backlog indipendente. Aggiornati anche Gantt, introduzione, `README.md`, `docs/installation.md`, `PIANO_MIGLIORAMENTI.md` |
| 3 | Riparata la skill `ui-ux-pro-max` (§10) |

### Aperte — in attesa di decisione

| # | Azione | Tipo |
|---|---|---|
| 4 | Confermare le tre raccomandazioni di §5: **Svelte + Tailwind**, **v1 in sola lettura**, **installer unico con componente opzionale** | Decisione |
| 5 | Eseguire **F52** come lavoro a sé, **con** il gate CI di §4.2 nello stesso PR | Lavoro, alto |
| 6 | Aggiungere l'audit npm a `security-audit.yml` quando entra la toolchain JS (§8.6) | Lavoro, basso |

**Nessuna riga di codice della GUI è stata scritta.**

---

## 10. Il ruolo di `ui-ux-pro-max`, misurato

Installata e funzionante in `~/.claude/skills/ui-ux-pro-max` (v2.13.0, dal repo ufficiale). La copia
precedente era **rotta**: `data/` e `scripts/` erano file di 31 byte contenenti percorsi relativi
(`../../../src/ui-ux-pro-max/data`) validi solo dentro il repo di origine. Il primo comando che il
suo stesso SKILL.md prescrive sarebbe fallito. La copia rotta è in `~/.claude/_backups/`, fuori da
`skills/` per non essere registrata come una seconda skill.

### Dove aiuta, e dove no — con la prova

Interrogata su `"backup operator console desktop file transfer"` in modalità automatica, ha
restituito il pattern *"Product Demo + Features"*: Hero → video di prodotto → CTA. Una **landing
page di marketing**, non una console operativa. I colori invece erano ottimi: token completi con
coppie a contrasto verificato, inclusi `destructive`/`on-destructive`.

**Ma dipende dalla query**, ed è il dato più utile emerso. La stessa skill, interrogata con
`"data dense dashboard operator monitoring"`, restituisce lo stile `data-dense-dashboard`:

> *Best For: ... operational dashboards* — griglia a 12 colonne, padding 8-12px, tabelle ordinabili
> con header sticky, tipografia 12-14px, massima densità informativa.

Esattamente ciò che serve a una console di backup, con valori CSS concreti.

**Lezione operativa**: `--design-system` in automatico assume un prodotto web di marketing, perché è
per quello che il catalogo è tarato. Interrogata su ciò che la GUI *è davvero* — un cruscotto
operativo denso — risponde bene. Non prendere il primo output della modalità automatica come
risposta.

| Usala per | Non usarla per |
|---|---|
| Palette e token di colore | I presidi sulle azioni distruttive (§8.4) |
| Coppie di font e scala tipografica | Decidere quali operazioni esporre a un operatore |
| Spaziatura, densità, stati di interazione | Convenzioni desktop native (finestra, menu, tray) |
| Comportamento chiaro/scuro, contrasto | Qualunque decisione con conseguenze sui dati |

Copre 22 stack, inclusi desktop nativi (`winui`, `wpf`, `uwp`, `avalonia`, `uno`, `javafx`). Tauri
non è fra questi, ma il suo frontend è web e `svelte`/`html-tailwind` — gli stack raccomandati in
§5.1 — ci sono entrambi. Mancano davvero le convenzioni desktop: chrome della finestra, menu nativi,
tray, multi-finestra, dialoghi di file, workflow da tastiera. Per quelle sono più centrate
`ux-heuristics`, `design-everyday-things`, `accessibility-compliance` e `refactoring-ui`.

> Nota per un LLM: le copie in `~/.agents/skills/`, `~/.openclaw/skills/` e `~/.openfang/skills/`
> sono versioni diverse e in parte rotte. Quella autorevole è `~/.claude/skills/ui-ux-pro-max`.

---

## Riferimenti

- [`ROADMAP.md`](ROADMAP.md) — milestone **7.0.0** interfaccia grafica (F52-F57, F59, F60) e i
  suoi avvisi di sicurezza; **8.0.0** motore controllabile condizionale (F47, F48, F58); backlog
  indipendente (F46, F49, F50, F51)
- [`AGENTS.md`](AGENTS.md) — regole 8 (feature-gate, il modello del gate di §4.2), 12 (exit code), 16 (metadati fuori da `--dest`)
- [`ANALYSIS.md`](ANALYSIS.md) — D14 (scrittura atomica), D18 (il costo del lavoro per-file), D20/D21 (disciplina di memoria)
- [`ARCHITECTURE.md`](ARCHITECTURE.md) — struttura attuale del package singolo che F52 ristruttura
- [`VALUTAZIONE_AI.md`](VALUTAZIONE_AI.md) — i divieti su operazioni distruttive, validi per qualunque superficie automatica, GUI inclusa
- [`docs/cli-reference.md`](docs/cli-reference.md) — flag ed exit code che la GUI deve rispettare
