---
type: Reference
title: Piano della console rustcopy
description: Documento unico e vivo per la console (F52-F60) — consolida il piano pre-implementazione (stack, ambito, distribuzione, vincoli permanenti) con l'inventario di ciò che espone oggi rispetto alla CLI, le lacune funzionali con un piano in tre onde, un audit visivo/di usabilità con un piano di rifacimento a tre livelli (chiuso), una valutazione di una metodologia a workspace più cinque funzionalità CLI non ancora costruite (implementate), un audit visivo/funzionale reale post-implementazione, un confronto con TeraCopy/Cobian Reflector sulle capacità della GUI, l'analisi di rischio del motore pilotabile (sospesa), un piano per la selezione di percorsi e i campi di configurazione ancora irraggiungibili dalla GUI, una sincronizzazione rapida senza un file di configurazione esistente, e un'analisi di usabilità della scheda Modifica (validazione, verifica percorsi, suggerimenti, esempio funzionante) da un uso estensivo reale. Sostituisce PIANO_GUI_TAURI.md (archiviato) e PIANO_GUI_ESPANSIONE.md (questo stesso file, rinominato).
status: draft
generated:
  by: process:claude-code
  at: 2026-09-03T14:12:16Z
verified:
  by: process:claude-code
  at: 2026-09-04T10:00:00Z
---

# Piano della console rustcopy

**Nota di consolidamento (4 Set 2026)**: questo file nasce dalla fusione di due documenti —
`PIANO_GUI_ESPANSIONE.md` (di cui è la continuazione diretta, stesso file rinominato) e
`docs/archive/PIANO_GUI_TAURI.md` (il piano pre-implementazione,
ora archiviato perché eseguito per intero). Da qui in avanti **questo è l'unico piano attivo** della
console: cosa è stato deciso e perché (§2), cosa fa oggi (§3-§4), cosa manca e con che priorità
(§5-§8), e — dal 4 Set 2026 — come renderla meno spartana (§9-§10). §11 riassume tutto in una
tabella sola: fatto, da fare, proposto.

La milestone 7.0.0 (la console) ha chiuso sette voci su otto — F52/F53/F54/F55-lettura/F56/F59/F60 —
e ha lasciato scritte due decisioni deliberatamente aperte: la metà in scrittura di F55 (script
pre/post) e F57 (ruoli), quest'ultima non raccomandata. Questo documento non riparte da zero: legge
cosa la console fa oggi, la confronta campo per campo con quello che la CLI sa fare, e organizza la
distanza fra le due in un piano. Non riapre nessuna decisione già presa — le richiama e basta, dove
sono rilevanti — e non propone nulla che allarghi il vincolo strutturale di `runner.rs`: **la console
può riferire un'operazione distruttiva, mai autorizzarla** (§7 sotto per il dettaglio).

**Aggiornamento 4 Set 2026**: dopo che l'Onda 1 e l'Onda 2 sono state spedite (§8), l'utente ha
segnalato che la console resta "poco intuitiva" e "con una UI eccessivamente spartana" — un giudizio
sulla *forma*, non sulla copertura funzionale che §3-§8 misurano. §9-§10 sono un secondo audit,
ortogonale al primo: ogni scheda riaperta nella console compilata (non solo il codice) per catalogare
cosa rende difficile usarla, e un piano di rifacimento visivo a tre livelli che non tocca nessuna
delle scelte di sicurezza sopra.

## 1. Perimetro e metodo

Ispezionati per questo piano: i 12 comandi `#[tauri::command]` in
[`crates/rustcopy-gui/src/main.rs`](crates/rustcopy-gui/src/main.rs), le 7 schede Svelte in
[`crates/rustcopy-gui/ui/src`](crates/rustcopy-gui/ui/src), l'API di sola lettura in
[`gui_api.rs`](crates/rustcopy-core/src/gui_api.rs), l'unico percorso di scrittura in
[`job_editor.rs`](crates/rustcopy-core/src/job_editor.rs), il vincolo di avvio in
[`runner.rs`](crates/rustcopy-core/src/runner.rs), e per confronto i 54 campi di `Args`
(`cli.rs`) e i 29 campi di `JobConfig` (`config.rs`). Non è un audit di codice: è una mappa di cosa
esiste, cosa manca, e cosa manca *di proposito*.

## 2. Decisioni fondative e vincoli permanenti

Prima di scrivere una riga di GUI, tre domande di prodotto e una domanda tecnica hanno avuto una
risposta esplicita, motivata e — le prime tre — verificata dopo il fatto. Il ragionamento completo,
coi dati misurati, è in `docs/archive/PIANO_GUI_TAURI.md`; qui
solo l'esito, perché resta la base su cui ogni voce successiva di questo piano si appoggia.

### 2.1 La GUI rallenta il motore di copia? No, per costruzione

`rustcopy-core`/`rustcopy-cli`/`rustcopy-gui` sono binari separati (F52): `robocopy_ingest.exe` non
linka Tauri, non ha dipendenze JS, e il percorso non presidiato (Task Scheduler, servizio Windows)
invoca solo la CLI. Un gate CI dimostra la proprietà a ogni commit invece di lasciarla una promessa
(`ci.yml`, "the CLI never depends on the GUI toolchain"). Il rischio reale non era "installare Tauri"
ma **la frequenza con cui il motore comunica il progresso alla UI** — e il pattern sicuro (contatori
atomici lock-free, campionamento a 200ms disaccoppiato dal numero di file, mai un evento IPC per
file) era già in `src/progress.rs` prima che la GUI esistesse. `Run.svelte` campiona `run_status` sul
proprio timer; non riceve mai una notifica per file.

### 2.2 Tre decisioni di prodotto, tutte confermate dopo l'implementazione

| Decisione | Scelta | Verificato |
|---|---|---|
| Stack frontend | **Svelte + Tailwind** (non React+shadcn) — criterio decisivo: superficie della catena di fornitura npm, non prestazioni | 52 pacchetti npm, 0 vulnerabilità, 41 KB di JS (31 Ago 2026) |
| Ambito v1 | **Sola lettura** — nessun percorso di scrittura può corrompere un backup o sbagliare una purge | Valido fino al 2 Set 2026, quando F54 ha aggiunto **un** percorso di scrittura (proposte in file nuovi, mai la configurazione in uso) |
| Distribuzione | **Un solo installer** (`installer/rustcopy.iss`), console come componente opzionale — non un bundle Tauri separato | F60, 2 Set 2026: setup da 6,6 MB, ciclo installazione→disinstallazione verificato |

### 2.3 Vincoli di sicurezza permanenti

Non decisioni prese una volta: regole che ogni voce di questo piano — comprese le proposte non ancora
implementate in §8 e §10 — deve rispettare, perché è facile progettare una UI che le viola senza
accorgersene.

1. **I ruoli in un'app desktop non sono un confine di sicurezza.** Chi ha una sessione locale può
   eseguire `rustcopy.exe` direttamente o leggere/modificare il TOML, scavalcando la UI. "Operatore"
   impedisce errori, non azioni deliberate — è per questo che F57 non è raccomandata (§6).
2. **Script configurabili + servizio privilegiato = escalation locale.** Se un servizio Windows gira
   come SYSTEM ed esegue script pre/post configurati dalla UI, un utente non amministratore che può
   scrivere quello script ottiene esecuzione come SYSTEM. È la ragione per cui la scrittura di
   `pre_command`/`post_command` in Modifica resta in Onda 3 (§8, voce 12) e non prima.
3. **La UI non deve diventare una nuova sede dei segreti.** F56 estende la convenzione esistente
   (`env:`/`file:`/`keyring:`) al Credential Manager — non ne introduce una parallela.
4. **Le azioni distruttive non si allentano passando dalla UI.** `--mirror` e `--keep-generations`
   hanno presidi ed exit code dedicati (`3`, `5`); la console conferma **cosa** verrebbe cancellato,
   mai una scorciatoia ricordata fra sessioni.
5. **Gli exit code sono un contratto con gli scheduler.** La UI li mostra e li interpreta, non li
   ridefinisce — `4` (copiato, verifica fallita) resta distinguibile da `1` (copia fallita).
6. **La superficie npm va sottoposta ad audit come quella Rust.** `crates/rustcopy-gui/ui`'s
   `gui-npm-audit` in `ci.yml` copre questo dalla milestone 7.0.0.

## 3. Cosa la console fa oggi

| Scheda | Comandi Tauri | Cosa fa | Scrive? |
|---|---|---|---|
| **Job** | `list_jobs` | Elenca i job di un TOML: sorgente, destinazione, tipo di backup, se verifica/mirror sono attivi. | No |
| **Impostazioni** | `read_settings`, `set_credential`, `delete_credential` | Ogni job risolto (vista merged), raggruppato, con provenienza per campo e avviso sulle impostazioni che hanno una conseguenza (mirror cancella, ecc.). `webhook_url` troncato a schema+host; `pre_command`/`post_command` verbatim. **Onda 2**: sezione credenziali (F56) — salva/elimina un segreto nel Windows Credential Manager, il segreto viaggia solo su IPC. | Sì (solo credenziali, eccezione dichiarata) |
| **Modifica** | `read_job_drafts`, `suggest_proposal_path`, `write_proposal` | Crea o modifica job (26 dei 29 campi di `JobConfig`) e scrive **sempre un file nuovo**, mai quello in uso. Rifiuta di allargare il rischio (mirror spento→acceso, `keep_generations` abbassato, ecc. — regola in `job_editor.rs`). | Sì, ma solo su un file di proposta |
| **Esegui** | `list_jobs`, `run_status`, `start_job`, `stop_job`, `schedules_referencing` | Avvia la stessa CLI come processo separato con `runner::run_arguments` (forma fissa: solo `--config`/`--cancel-file`/`--progress-file`), mostra fase/percentuale/output catturato, ferma scrivendo il file che la run sorveglia. **Onda 2**: coda job in sola lettura durante un batch (F49) e badge se una pianificazione punta già a questo file. Notifica di sistema a fine run. Una run alla volta per finestra. | Esegue (via CLI esterna), non scrive configurazione |
| **Report** | `read_report_page` | Report JSON paginato: riepilogo, mismatch, errori. | No |
| **Storico** | `read_history`, `read_advice` | Run passate con il significato dell'exit code, più l'analisi deterministica di `--advise`. | No |
| **Aiuto** | — | Contenuto statico. | No |

## 4. Copertura CLI → GUI, per categoria

Legenda: **L**=leggibile in Impostazioni/Report/Storico, **S**=scrivibile in Modifica, **E**=eseguibile da Esegui.

| Categoria | Flag CLI | L | S | E | Nota |
|---|---|:-:|:-:|:-:|---|
| Copia base | `--source/--dest/--pattern/--threads/--retries/...` | ✅ | ✅ | ✅ | Copertura completa |
| Verifica | `--verify-integrity/--fast-verify/--hash-algo/--ignore-transient-missing` | ✅ | ✅ | ✅ | Completa |
| Filtri | `--exclude-files/-dirs/--min-max-age-days` | ✅ | ✅ | ✅ | Completa |
| Generazioni | `--backup-type/--keep-generations` | ✅ | ✅ (solo restrizione) | ✅ | Coerente col vincolo F54 |
| Rete/percorsi | `--bandwidth-limit-mbps/--long-paths/--exclude-junctions` | ✅ | ✅ | ✅ | Completa |
| Notifiche | `--webhook-url` | Troncato | ❌ | — | F55 scrittura non decisa |
| Hook | `--pre-command/--post-command` | Verbatim | ❌ | — | F55 scrittura non decisa (avviso 2) |
| Cifratura | `--encrypt-aes256/--decrypt` | ❌ | ❌ | ❌ | Nessuna superficie GUI |
| Credenziali | `--set-credential/--delete-credential` | ❌ | ✅ (Impostazioni, Onda 2) | — | Chiavi di cifratura via `keyring:`; token notify/SMB/SMTP restano solo CLI |
| VSS | `--vss-snapshot` | ❌ | ❌ | ❌ | Non è nemmeno un campo di `JobConfig` |
| Ripristino | `--restore-from` | ❌ | ❌ | ❌ | **Nessun flusso in GUI** |
| Ripresa | `--resume-from` | ❌ | ❌ | ❌ | Nessun flusso in GUI |
| Automazione | `--install/uninstall-schedule/-service` | ❌ | ❌ | ❌ | Vietato di proposito (vincolo `runner.rs`) |
| Mirror non presidiato | `--force-purge` | ❌ | ❌ | ❌ | Vietato di proposito |
| Job multipli | `[[jobs]]` in un TOML | ✅ (elenco) | ✅ | ✅ (posizione, Onda 2) | Vedi §5d — la posizione nel batch si vede (F49), l'esito per-job resta nel Report/Storico di quella run |

## 5. Lacune, per categoria

### a) Editor — script e notifiche (F55, scrittura)

`webhook_url`, `pre_command`, `post_command` sono già letti e round-trippati intatti da un edit (un
campo che il form non disegna torna com'era — regola di `job_editor.rs`), ma non sono **scrivibili**.
Questa non è una svista: ROADMAP la lascia esplicitamente non decisa perché morde l'avviso di
sicurezza 2 — *script configurabili più servizio privilegiato uguale escalation locale* (rilevante
da quando F37 rende `rustcopy_ingest` installabile come servizio Windows). Un campo di testo libero
che poi esegue con i privilegi del servizio è una superficie diversa da un mirror che si ferma da
solo con l'exit 3. **Non la riapro come già decisa** — la porto in §8 come Onda 3, con la stessa
cautela con cui ROADMAP la lascia aperta.

### b) Ripristino e ripresa — la lacuna più grande

`--restore-from` e `--resume-from` non hanno **nessun** punto di ingresso in console: nessun modo di
sfogliare un report passato e dire "ripristina da qui", nessun elenco di checkpoint disponibili dopo
un'interruzione. È l'azione con la ricorrenza operativa più alta dopo "avvia un backup" — è anche
quella con la conseguenza più grave se sbagliata, perché `--restore-from` **inverte** sorgente e
destinazione. Merita un disegno di conferma esplicita, non un pulsante in più nella scheda Esegui.

### c) Credenziali (F56 fatto solo a metà)

`--set-credential`/`--delete-credential` esistono e sono verificati contro Credential Manager reale,
ma solo da riga di comando. Restano fuori anche i token notify e le eventuali credenziali SMB/SMTP,
che oggi non passano da `resolve_key` — ROADMAP nota che "andranno ricondotti alla stessa forma
quando F55 li esporrà", quindi questa lacuna e quella del punto (a) si chiudono insieme.

### d) Esecuzione — nessuna vista per-job, nessuna coda, nessuna notifica di fine

- Un file con più `[[jobs]]` viene eseguito per intero (la console avvia la stessa CLI che eseguirebbe
  `run_jobs`), ma la barra di progresso non dice **quale** job sta girando: `ProgressSample` (in
  [`progress_file.rs`](crates/rustcopy-core/src/progress_file.rs)) non porta un nome o un indice di
  job, solo fase/byte/file. Su un batch di cinque job l'operatore vede un'unica barra continua.
- Nessuna vista "coda": F49 ("coda di job gestibile") è già in backlog, dipende solo da F33 (chiuso),
  nessuna dipendenza dal motore pilotabile. È il candidato naturale per dare un volto a questa lacuna.
- Nessuna notifica di sistema a fine run: oggi bisogna avere la finestra in primo piano (o tornarci)
  per sapere che una run è finita. `tauri-plugin-notification` non è ancora fra le dipendenze di
  [`crates/rustcopy-gui/Cargo.toml`](crates/rustcopy-gui/Cargo.toml).
- Una run alla volta per finestra è **per progetto**, non una lacuna: due run dello stesso job si
  pesterebbero su cache fast-verify e manifest delle generazioni. Non lo tocco.

### e) Report e Storico — nessun filtro, nessuna ricerca, nessuna esportazione

`Report.svelte` pagina il JSON ma non filtra per tipo di errore; `History.svelte` non filtra per job
o intervallo di date. Nessuna delle due schede esporta (CSV, per esempio) — oggi l'unico modo di
portare i dati fuori dalla console è aprire il JSON a mano.

### f) VSS non è nemmeno un campo di configurazione

`--vss-snapshot` non è nella struct `JobConfig`: non è solo "non editabile in console", è "non
esprimibile in un file TOML" a prescindere dalla GUI. Prima di qualunque interruttore in Modifica
serve un passo lato core (aggiungere il campo a `JobConfig`, farlo fluire come già fa per gli altri
26 campi) — non è lavoro di frontend.

### g) Usabilità minore

- Trascina-e-rilascia un file TOML sulla finestra: oggi bisogna passare da Sfoglia o Recenti.
- Nessun badge "questo job ha una pianificazione/un servizio installati?": sarebbe una lettura
  (`schtasks /Query`, `sc query`) esposta come comando di sola lettura — mai un'azione — utile a
  capire se un job già gira da solo prima di duplicarlo con `--install-schedule`.

## 6. Cosa resta fuori da questo piano, e perché

- **F57 (ruoli admin/operatore)** — ROADMAP la marca P2 e non raccomandata: "utile come prevenzione
  degli errori, non come confine di sicurezza". Non la ripropongo.
- **Milestone 8.0.0 (motore pilotabile: pausa/ripresa/skip-per-file)** — condizionale, senza un
  trigger concreto. "Finché la 8.0.0 non viene ripresa, la GUI semplicemente non disegna pulsanti di
  pausa e skip" (ROADMAP). Questo piano non li disegna nemmeno in bozza.
- **F51 (shell extension Explorer)** — il costo più alto della roadmap, "solo dopo che esiste una GUI
  da lanciare, e solo se richiesta". La precondizione ora è vera, ma resta un deliverable separato
  (DLL COM, installer proprio) che non appartiene a un piano di ampliamento della console stessa.
- **F46 (modalità "sposta")** — è una feature del motore di copia, non della console: comparirebbe in
  GUI solo come un interruttore in più *dopo* che esiste a livello CLI. Non è lavoro di questo piano.

## 7. Il vincolo che ogni onda deve rispettare

Ogni proposta sotto passa lo stesso test che `runner.rs` già impone e verifica con un test
(`the_argument_list_cannot_carry_a_destructive_flag`): la console può **eseguire** solo attraverso la
stessa CLI, con `run_arguments` a forma fissa, e può **riferire** un'operazione distruttiva ma mai
**autorizzarla** in modo non presidiato. Nessuna proposta qui sotto tocca `run_arguments`, aggiunge un
parametro che inoltra flag, o esegue codice di backup dentro il processo della console invece che
dentro la CLI.

## 8. Piano prioritizzato

### Onda 1 — Rischio basso, nessuna nuova superficie di scrittura ✅ **completata**

Tutte lettura pura o piccole aggiunte al formato interno del progresso; nessuna tocca `job_editor.rs`
né `runner.rs`. **Tutti e sei gli item sono in `main` da prima dell'Onda 2** (PR #66-#71) — non erano
mai stati segnati come fatti in questo documento; corretto qui il 4 Set 2026, verificato contro il
sorgente attuale, non contro la memoria di quando furono scritti.

1. ✅ **Notifica di sistema a fine run** — `Run.svelte::notifyFinished`, collegata al punto in cui
   `RunStatus.running` passa a `false`.
2. ✅ **Etichetta "job N di M" durante un batch** — `ProgressSample.batch_index`/`batch_total`
   (`Option`, `None` per chi non fa batch), scritto da `run_jobs` in `main.rs`, mostrato da
   `Run.svelte::phase_label()`.
3. ✅ **Filtro e ricerca in Report e Storico** — `Report.svelte`'s `query`, `History.svelte`'s
   `outcomeFilter`, entrambi client-side sui dati già restituiti.
4. ✅ **Esportazione CSV** di report e storico — `csv.js` (`toCsv`/`downloadCsv`, con neutralizzazione
   delle formule contro CSV injection, rilievo CodeRabbit corretto in corsa).
5. ✅ **Trascina-e-rilascia un TOML sulla finestra** — `PathBar.svelte`'s `onDragDropEvent`.
6. ✅ **Badge di sola lettura** "pianificazione installata per questo job" — `schedule::referencing_config`
   (`gui_api::schedules_referencing`), mostrato in `Run.svelte`.

### Onda 2 — Valore medio, nuova superficie ma dentro i limiti esistenti 🟠 **2 su 3 fatte**

7. ✅ **Fatto 3 Set 2026 (PR #73).** **Vista "coda job" (F49)** — elenco dei job di un batch con stato
   individuale (in attesa / in corso / concluso), letto da `ProgressSample.batch_index`/`batch_total`
   (già scritti da F49-Onda-1) mentre `start_job` esegue il batch. Nessuna esecuzione diretta nel
   processo della console: resta un pannello di lettura sopra la stessa run. 3 rilievi CodeRabbit
   corretti (persistenza sulla stessa run riesaminata, ultimo job troppo veloce per un campione live,
   accessibilità del badge) — dettaglio in `CLAUDE.md` e `ROADMAP.md` (riga F49).
8. ✅ **Fatto 3 Set 2026 (PR #74).** **UI per `--set-credential`/`--delete-credential`** — un campo
   mascherato che invoca un comando dedicato il quale, come oggi la CLI, legge il segreto e lo passa a
   `keyring` **senza** farlo transitare per un argomento di processo. Chiude la lacuna (c) per la parte
   già coperta da F56. Verificato contro il Credential Manager reale (salva → `cmdkey /list` conferma →
   elimina → assenza confermata).
9. **Interruttore VSS in Modifica** — *dopo* aver aggiunto `vss_snapshot: Option<bool>` a
   `JobConfig` lato core (non lavoro di frontend, vedi §5f). Fino ad allora questo punto resta bloccato.

### Onda 3 — Valore alto, richiede una decisione esplicita prima di progettare 🟠 **1 su 3 fatta**

10. **Flusso di ripristino guidato (`--restore-from`)** — la lacuna più sentita, ma `--restore-from`
    inverte sorgente e destinazione e può sovrascrivere dati: prima di un solo pixel di disegno serve
    la stessa disciplina che ha retto il resto della console — un meccanismo di conferma esplicita
    pari a quello del mirror (l'esecuzione non presidiata continua ad autofermarsi da sola; la console
    può proporre il ripristino, non forzarlo). Proposta di massima: elenco dei report disponibili →
    anteprima di cosa verrebbe ripristinato (letta, non eseguita) → conferma esplicita → avvio via
    `start_job` con un `--config` derivato, mai un flag forzato in `run_arguments`.
11. ✅ **Fatto 4 Set 2026.** **Ripresa da checkpoint (`--resume-from`)** — nella scheda Esegui, sotto
    "Riprese disponibili": `gui_api::list_checkpoints` scansiona la cartella del config per
    `*.checkpoint.json` (non calcola un percorso atteso — un `--report-path` può essere
    namespacizzato per job o portare `{timestamp}`, quindi non c'è un unico percorso da ricostruire
    senza duplicare quella logica), `resume_job` avvia la stessa CLI con `runner::resume_arguments`
    (`--resume-from`, forma fissa come `run_arguments`, proprio test F61). Verificato contro un
    trasferimento reale interrotto a metà: la ripresa completa i file restanti e la verifica
    d'integrità passa. **Trovato in verifica, non nel disegno**: la ripresa porta con sé solo
    pattern/thread/tentativi/verifica dell'interruzione, non il resto della configurazione originale
    (limite di banda, esclusioni, algoritmo di hash, mirror inclusi) — comportamento preesistente di
    `checkpoint::build_resume_args`, non introdotto qui, ma mai dichiarato prima d'ora. Dichiarato
    nel testo della console e in `ANALYSIS.md` (D25, aperto, non bloccante: l'asimmetria gioca quasi
    sempre a favore della sicurezza, mai verso il distruttivo).
12. **Scrittura di `webhook_url`/`pre_command`/`post_command` in Modifica (F55, metà in scrittura)** —
    **non la marco pronta**: la porto qui esattamente perché ROADMAP la lascia esplicitamente non
    decisa, con la ragione già scritta (avviso 2, script + servizio privilegiato = escalation locale).
    Se e quando si decide di procedere, la forma più cauta è probabilmente: mostrare uno stato
    "invariato dal file" per il campo finché non viene toccato (coerente con "l'omissione non cancella
    mai"), e non permettere di *aggiungere* un hook a un job che non ne aveva uno senza un avviso
    esplicito a schermo — ma questa è una discussione di sicurezza da avere prima, non un dettaglio
    di implementazione.

### Come leggere le tre onde

Le onde sono un ordine di rischio, non di urgenza: l'Onda 1 si può fare senza toccare `job_editor.rs`
o `runner.rs`, quindi non ha bisogno di nessuna nuova revisione del vincolo di sicurezza. L'Onda 2
introduce superficie nuova ma resta dentro pattern già verificati (lettura di file che la CLI già
scrive, credenziali che non passano da un argomento). L'Onda 3 è dove sta il valore più alto percepito
dall'operatore — e dove ogni singola voce ha già, nella cronologia di questo progetto, una ragione
scritta per non essere stata fatta subito. Non salterei l'ordine.

## 9. Bilancio visivo e usabilità (audit reale, 4 Set 2026)

Non un'altra lettura del codice: ogni scheda è stata aperta nella console compilata
(`target/release/rustcopy-gui.exe`, verificata con Windows-MCP, non solo letta) con e senza un file
caricato, su una finestra sia piccola che massimizzata, per vedere cosa vede davvero un operatore. La
console è deliberatamente "un pannello operativo denso, non una landing page" (`app.css`) — quella
scelta resta valida in principio, ma nell'esecuzione lascia diversi problemi concreti, misurati, non
di solo gusto.

### a) Il layout non usa lo spazio della finestra

Ogni scheda ancora il proprio contenuto in alto a sinistra dentro un semplice `<section class="p-4">`,
senza alcun contenitore che risponda alla larghezza reale della finestra. Su un monitor comune
(1620×1039, non insolito) questo lascia circa il 70% della finestra come un canvas grigio vuoto sotto
e a destra del contenuto — misurato: nella scheda Job vuota, il contenuto utile occupa un riquadro di
~700×350px in un'area di 1620×980. Non è densità operativa, è un'applicazione che sembra non essersi
caricata del tutto. Il problema si aggrava nell'editor: gli input di Nome/Sorgente/Pattern in Modifica
hanno `class="w-full"` dentro una griglia a colonna `1fr` (`Editor.svelte`), quindi un valore di 4
caratteri come `job1` finisce in una casella di testo larga quanto la finestra.

### b) Nessuna gerarchia visiva, tutto testo

La navigazione fra le 7 schede sono pulsanti di solo testo (`App.svelte` righe 35-41), senza icone né
separazione dal contenuto sottostante — a un primo sguardo non si distingue "questa è la barra di
navigazione dell'applicazione" da "questo è un elenco di link". Non c'è una singola icona in tutta
l'interfaccia: non sulle schede, non sui pulsanti, non sugli stati (riuscito/fallito/in corso sono resi
solo con colore e una parola). La dimensione del testo è quasi ovunque 11-12px (`text-xs`), con badge
fino a 10px (`text-[10px]`) — leggibile ma ai limiti, senza una vera scala tipografica: titoli di
sezione, etichette e valori usano pesi diversi dello stesso corpo, mai una dimensione diversa.

### c) Le sezioni non si distinguono dallo sfondo

Nessuna scheda usa card, ombre o sfondi leggermente diversi per raggruppare contenuto imparentato —
l'unico bordo visibile nell'intera applicazione è quello tratteggiato degli `EmptyState`. La tabella
dei job, la griglia di statistiche in Report, l'elenco di Impostazioni: tutte fluttuano libere sullo
sfondo `bg-slate-50`, senza un margine visivo che dica dove finisce una sezione e comincia la prossima.

### d) Le tabelle non colonnano bene su una finestra larga

`<table class="w-full">` senza larghezze di colonna esplicite lascia al motore di rendering la
decisione di dove mettere lo spazio in eccesso — quasi sempre nella prima colonna. Misurato in
Storico: la colonna "Quando" si allarga fino quasi a metà finestra mentre "Durata"/"Throughput"
restano strette, senza alcuna relazione con il contenuto.

### e) I badge di provenienza in Impostazioni sono staccati dal valore

Ogni riga di Impostazioni mostra `job`/`ereditato`/`default` in una colonna a larghezza fissa allineata
a destra (`w-24 text-right` dentro una tabella `w-full`, `Settings.svelte`) — su una finestra
massimizzata il badge finisce a ~1300px di distanza dall'etichetta e dal valore a cui si riferisce,
costringendo l'occhio a un salto enorme per ogni riga.

### f) Incoerenza linguistica

Alcune stringhe che arrivano dal core sono in inglese e finiscono in un'interfaccia altrimenti tutta
italiana: durante l'audit la scheda Report ha mostrato `Verifica: Passed` ed `Esito: files copied` —
`report.exit_code_meaning`/`report.integrity_status` sono stringhe scritte dalla CLI, non tradotte lato
frontend. Storico se ne accorge già (la propria tabella `EXIT_MEANING` traduce l'exit code per la
propria tabella), ma Report no.

### g) Nessun collegamento fra schede

Un'esecuzione riuscita in Esegui non offre alcun modo di passare al proprio report in Report/Storico:
il percorso è tecnicamente noto (`draft.report_path` esiste già lato core) ma ogni scheda tiene il
proprio campo percorso indipendente (`session.configPath` contro `session.reportPath`), quindi
l'operatore deve ricordare o ricopiare a mano il percorso del report appena scritto.

### h) Contenuto non aggiornato

Il testo statico di Aiuto affermava ancora *"Non esegue backup, non copia e non cancella nulla"* —
falso dall'introduzione dell'esecuzione (F54) — e l'elenco "Cosa significano le schede" non menzionava
affatto Esegui. **Corretto in questa stessa sessione** (`Help.svelte`), ma sintomo di un problema più
ampio: il contenuto statico non ha un proprietario che lo tenga aggiornato quando una scheda cambia
comportamento.

### i) Trovato per strada: una console che lampeggiava

Non un difetto visivo ma scoperto nello stesso giro di verifica: ogni «Esamina» nella scheda Esegui
apriva e chiudeva una finestra di terminale nera (mancava `CREATE_NO_WINDOW` su due spawn di
`schtasks.exe` in `schedule.rs`). **Già corretto** (D24, `ANALYSIS.md`) — non è nel piano sotto, perché
non è una scelta di design ma un difetto, e non aspetta una decisione visiva per essere risolto.

## 10. Piano di rifacimento visivo, prioritizzato

Diviso per rischio/sforzo come le tre onde funzionali di §8, ma ortogonale ad esse: nessuna di queste
voci tocca `runner.rs`, `job_editor.rs` o il confine F61 — sono tutte CSS/markup/organizzazione dei
contenuti Svelte esistenti, non nuova superficie verso il core.

### Livello 1 — correzioni puntuali, nessun rischio, poche righe ciascuna ✅ **completato 4 Set 2026**

1. ✅ **Contenere e centrare il contenuto** in un `<div class="mx-auto max-w-6xl">` attorno a
   header e schede in `App.svelte`, invece di lasciarlo ancorato in alto a sinistra su qualunque
   larghezza di finestra. Risolve (a) alla radice, senza toccare nessuna scheda individualmente.
2. ✅ **Colonne di tabella esplicite** (`<colgroup>` con larghezze percentuali, `table-fixed`) in
   Job e Storico, invece di lasciare il motore di rendering distribuire lo spazio in eccesso sulla
   prima colonna. Risolve (d).
3. ✅ **Badge di provenienza spostato accanto al valore** in Impostazioni — stessa cella, non più
   una colonna a distanza fissa dal bordo destro. Risolve (e).
4. 🟠 **Tradurre `integrity_status` in Report** — fatto: è `format!("{:?}", IntegrityStatus)`, un
   enum chiuso a 2 valori (Passed/Failed), tradurlo è una scelta di rendering. **`exit_code_meaning`
   non tradotto, di proposito**: verificando il codice si è scoperto che non è lo stesso tipo di
   dato dell'`EXIT_MEANING` di Storico — è la descrizione bitmask nativa di robocopy
   (`exit_code.rs::RobocopyStatus::describe`, una frase composta da fino a 5 frammenti inglesi
   combinabili, non un codice fisso 0-5). Applicare la tabella sbagliata avrebbe dato
   un'informazione fuorviante, non solo un'etichetta in inglese. Tradurlo davvero richiede o
   esporre il codice numerico e duplicare la logica bitmask in JS (viola il vincolo permanente
   "nessuna logica duplicata in TypeScript", §2.3) o una variante italiana lato core di
   `describe()` — che tocca un campo del report JSON che *è* un contratto con gli scheduler (regola
   12) e merita una decisione a sé, non una riga di Livello 1. Resta in inglese; non risolve (f)
   per intero.
5. ✅ **"Apri il report di questa run"** — collegamento in Esegui che appare a run conclusa (job
   singolo, stato ancora quello del file caricato — mai per una ripresa da checkpoint, il cui
   `config_path` è quello del checkpoint) e naviga a Report con `session.reportPath` già impostato
   e il caricamento già avviato. Ha richiesto un piccolo campo aggiuntivo (`report_path` su
   `JobSummary`, `gui_api.rs`): namespacizzato per job esattamente come `run_jobs` lo namespacizza
   (`namespaced_path`, F33/D12, riusata non duplicata), `None` quando porta ancora `{timestamp}`
   (P1) invece di indovinare un percorso che nessun calcolo a posteriori può prevedere. `tab` è
   stato spostato da stato locale di `App.svelte` a `session.svelte.js` perché una scheda potesse
   cambiarne un'altra. Risolve (g).

### Livello 2 — un sistema di design minimo, tocca ogni scheda ma senza logica nuova ✅ **completato 4 Set 2026**

6. ✅ **Una vera scala tipografica** — i valori nella griglia di Report e nelle righe di
   Impostazioni sono passati da `text-xs` (11-12px) a `text-sm`, mentre etichette e didascalie
   restano alla dimensione precedente: due livelli distinti invece di un unico corpo indifferenziato.
7. ✅ **Card per ogni gruppo di contenuto imparentato** — una classe `.card` (`app.css`,
   `@layer components`) applicata a: tabella Job, griglia statistiche Report, ogni job in
   Impostazioni più il riquadro credenziali, tabella Storico, riquadro checkpoint/coda in Esegui.
   Risolve (c).
8. ✅ **Un set minimo di icone**, `@lucide/svelte` — valutata contro icone scritte a mano: 55
   pacchetti totali (52 preesistenti + 3 della libreria e le sue dipendenze dirette), **0
   vulnerabilità** (`npm audit`), tree-shaking verificato (il bundle è cresciuto di ~11 KB per 7
   icone importate dal barrel, non del peso dell'intera libreria). Applicate a: le 7 voci di
   navigazione, gli stati di run (`CircleCheck`/`CircleX`/`LoaderCircle` animata), il badge mirror
   (`ShieldAlert`) e modello (`FileQuestionMark`), l'icona di ogni empty state. Risolve (b) insieme
   al punto 6.

   **Un'icona non aggiunta di proposito**: la prima bozza metteva ✓/✗ anche su "Esito" in Report,
   derivandola da `report.exit_code === 0` — ma `ReportView` (`gui_api.rs`) non espone affatto un
   `exit_code` numerico, solo `exit_code_meaning: Option<String>` (la stessa frase bitmask aperta
   di robocopy che il punto 4 del Livello 1 aveva già escluso dalla traduzione, per lo stesso
   identico motivo). Il confronto era quindi sempre falso — un'icona morta, non solo superflua.
   Trovato solo caricando un report vero nella console compilata, non leggendo il codice: la stessa
   lezione di §9/§10 su dove si annidano i difetti in questo progetto. Rimossa; "Verifica" mantiene
   la sua icona perché `integrity_status` **è** un enum chiuso a 2 valori.
9. ✅ **Larghezza dei campi dell'editor proporzionata al contenuto atteso** — Nome (`w-64`) e
   Pattern (`w-48`) ristretti; Sorgente, Destinazione, Escludi file/cartelle e Report restano
   larghi quanto la griglia, dove un percorso ha davvero bisogno di spazio. Completa (a) sulla
   scheda Modifica.

### Livello 3 — struttura ✅ **completato 4 Set 2026**

10. ✅ **Navigazione a barra laterale verticale** (con le icone del punto 8) al posto della riga di
    pulsanti testuali in testa. La testata liberata sopra ogni scheda mostra ora il nome del file
    di configurazione caricato (`session.configPath`, già condiviso fra le schede) al posto della
    stessa descrizione statica ripetuta ovunque — quando nessun file è ancora caricato, mostra
    quella descrizione come prima. **Scope volutamente più stretto di quanto il punto suggeriva**:
    "tipo, ultimo esito" del file attivo non sono mostrati, perché quell'informazione non vive in
    uno stato condiviso fra le schede (`status` è locale a Esegui) — costruirla avrebbe voluto dire
    una nuova superficie di stato condiviso, non una riga di questo livello. Il solo nome file è
    già la risposta a "cosa sto guardando", la domanda che il punto poneva.
11. ✅ **Dimensione di apertura della finestra**: 1100×700 → 1440×900 in `tauri.conf.json`. Scelta
    la strada senza superficie nuova (nessun salvataggio locale della dimensione lato Tauri) —
    resta ridimensionabile, e chi lavora su un monitor più piccolo può comunque restringerla.
12. ✅ **Empty state con un'ancora visiva** — `EmptyState.svelte` accetta ora un `icon` opzionale
    (default `Inbox`), reso accanto al titolo; ogni chiamata specifica l'icona pertinente alla
    propria scheda (`ListChecks`, `Play`, `FileText`, `Clock`, `SlidersHorizontal`) tranne dove il
    generico va bene così com'è (Modifica).

### Uno strumento per i Livelli 2-3: la skill `ui-ux-pro-max`

Installata in `~/.claude/skills/ui-ux-pro-max` (v2.13.0), utile per palette a token, coppie di font,
spaziatura e stati — **ma dipende dalla query**, misurato prima di scriverlo qui: interrogata in
automatico su `"backup operator console desktop file transfer"` ha restituito il pattern *Product
Demo + Features*, una landing page di marketing, non una console operativa. Interrogata invece su
`"data dense dashboard operator monitoring"` ha restituito lo stile `data-dense-dashboard` —
griglia a 12 colonne, tipografia 12-14px, tabelle con header sticky, esattamente il registro che
serve qui. **Non prendere il primo output della modalità automatica.** Copre gli stack `svelte` e
`html-tailwind` (quelli di questo frontend), non le convenzioni desktop native (chrome della
finestra, menu, tray, multi-finestra) — per quelle sono più centrate `ux-heuristics` e
`accessibility-compliance`. Non usarla per decidere quali operazioni esporre o allentare un presidio
di §2.3: quello resta un giudizio del core, mai della skill.

### Cosa resta fuori da questo rifacimento

Nessuna di queste voci riapre le scelte già motivate altrove: la sola-lettura salvo le due eccezioni
dichiarate (Modifica, Credenziali), il vincolo di `runner.rs`, la densità come principio — l'obiettivo
è renderla leggibile, non trasformarla in un'app consumer. Nessuna introduce una dipendenza pesante:
qualunque libreria scelta per il punto 8 va verificata contro lo stesso criterio che ha scelto Svelte
su React+shadcn (`CLAUDE.md`) — pacchetti e vulnerabilità aggiunte, non solo funzionalità offerta.

## 11. Stato riassuntivo — fatto, da fare, proposto

Una tabella sola per la domanda "a che punto siamo", sulle due dimensioni di questo piano
(funzionale e visivo) insieme.

| Traccia | Voce | Stato | Nota |
|---|---|---|---|
| Fondativa | F52-F60 (workspace, scheletro, editor, impostazioni, credenziali CLI, storico, installer) | ✅ **7/8 fatte** | F55 (script in scrittura) e F57 (ruoli) restano deliberatamente aperte |
| Funzionale — Onda 1 | 6 item, rischio basso (notifica, etichetta batch, filtro, CSV, drag&drop, badge pianificazione) | ✅ **6/6 fatte** | Spedite prima dell'Onda 2, mai segnate qui fino ad oggi |
| Funzionale — Onda 2 | Coda job (F49), credenziali in Impostazioni (F56) | ✅ **2/2 fatte** | PR #73, #74 |
| Funzionale — Onda 2 | Interruttore VSS in Modifica | ⛔ **bloccata** | Serve prima `vss_snapshot: Option<bool>` su `JobConfig` lato core — non è lavoro di frontend |
| Funzionale — Onda 3 | Flusso di ripristino guidato (`--restore-from`) | 🔴 **proposta, non iniziata** | La lacuna più sentita; richiede un disegno di conferma esplicita prima di qualunque riga |
| Funzionale — Onda 3 | Ripresa da checkpoint (`--resume-from`) | ✅ **fatta 4 Set 2026** | Verificata contro un trasferimento reale interrotto; limite noto dichiarato (D25) — la ripresa non eredita tutta la configurazione originale |
| Funzionale — Onda 3 | Scrittura di webhook/script pre-post in Modifica | 🔴 **proposta, bloccata da una decisione di sicurezza** | Morde il vincolo permanente 2 (§2.3); serve una decisione esplicita prima del disegno |
| Visivo — Livello 1 | 5 correzioni puntuali (contenimento layout, colonne tabella, badge provenienza, traduzione stringhe, collegamento run→report) | ✅ **4/5 fatte, 4 Set 2026** | La traduzione di `exit_code_meaning` non era fattibile come previsto — trovato verificando il codice, non un limite di sforzo (vedi §10 punto 4) |
| Visivo — Livello 2 | Sistema di design minimo (scala tipografica, card, icone, larghezza campi editor) | ✅ **fatto, 4 Set 2026** | `@lucide/svelte`, 0 vulnerabilità; un'icona di troppo (Esito in Report) trovata e tolta in verifica — derivava da un campo che `ReportView` non espone |
| Visivo — Livello 3 | Sidebar di navigazione, dimensione finestra, empty state con ancora visiva | ✅ **fatto, 4 Set 2026** | Sidebar libera la testata per il nome del file caricato (non "tipo/ultimo esito": nessuno stato condiviso li porta oggi); finestra 1440×900; icona per empty state |
| Difetto trovato per strada | D24 — console che lampeggiava (`schtasks.exe` senza `CREATE_NO_WINDOW`) | ✅ **corretto** | Non era nel piano: scoperto durante l'audit visivo, non una scelta di design |
| Difetto trovato per strada | D25 — la ripresa non eredita quasi nessuna impostazione originale (solo mirror ne beneficia) | 🟡 **aperto, non bloccante** | Trovato verificando la ripresa contro un trasferimento reale; comportamento preesistente di `checkpoint.rs`, non introdotto dalla console |
| Difetto trovato per strada | D26 — l'anteprima di ripristino (F64) falliva con "fatal error" su un report a percorsi relativi | ✅ **corretto il 6 Set 2026, stesso giorno della scoperta** | Trovato nel primo uso reale di F64 contro il demo eseguibile (§13a); il primo tentativo di fix (cwd sulla cartella del report) si è rivelato sbagliato ed è stato colto dalla stessa verifica — il fix corretto usa la cartella del config, non quella del report |
| Difetto trovato per strada | D27 — `files_copied`/`bytes_copied` sovrastimati su host non in lingua inglese | ✅ **corretto il 6 Set 2026, stesso giorno della scoperta** | Trovato indagando l'anomalia di conteggio del §13b; `parse_summary_row` non riconosce le etichette italiane, il fallback in streaming contava "Avviato:"/"Terminato:" come file trasferiti per l'assenza di spazio prima dei due punti nella loro localizzazione |

**In una frase**: la parte fondativa e funzionale a rischio basso/medio è quasi tutta fatta — Onda 1
e 2 chiuse, e ora anche la ripresa da checkpoint (Onda 3); le due voci rimaste bloccate (VSS, script
in scrittura) lo sono per una decisione esplicita da prendere, non per lavoro mancante. Il rifacimento
visivo è chiuso su tutti e tre i livelli (Livello 1 4/5, Livelli 2-3 al completo) — la sidebar con
icone, le card e la scala tipografica sono la parte che effettivamente rispondeva a "sembra spartana",
più del Livello 1 da solo, che aveva risolto soprattutto lo spazio sprecato. Resta da fare solo il
ripristino guidato (Onda 3, la lacuna funzionale più sentita) e le due voci bloccate da una decisione
esplicita.

## 12. Metodologia a workspace e funzionalità CLI non ancora valutate (analisi del 5 Set 2026)

Richiesta dall'utente dopo la chiusura dei Livelli 1-3: (a) una console a workspace avrebbe senso,
o l'architettura attuale non lo permette? (b) quali funzionalità CLI non ancora valutate potremmo
costruire e rendere utilizzabili anche in GUI? Risposta basata sul codice reale, non su ipotesi —
ogni affermazione sotto è stata verificata leggendo l'implementazione citata, non assunta.

### 12.1 Una metodologia "a workspace" ha senso qui?

**Cosa esiste già che è, di fatto, un workspace**: `[[jobs]]` in un TOML è già l'unità che il core
tratta come un gruppo di job correlati — un file, più destinazioni, condiviso da CLI e GUI allo
stesso modo (F33). La console vi si appoggia interamente: ogni scheda legge lo stesso
`session.configPath` (`session.svelte.js`), quindi aprire un file in Job lo rende immediatamente
disponibile in Impostazioni, Modifica ed Esegui senza incollarlo tre volte — è già, in miniatura,
"un progetto aperto in più viste". A questo si aggiunge "Recenti": una lista MRU di 8 percorsi
(separata per config/report) in `localStorage`, senza etichette, letta e scritta da `recent()`/
`remember()` nello stesso file.

**Cosa esiste in parallelo e non parla con nessuno dei due**: `scripts/profiles.json` (più
`scripts/_profiles-common.ps1`, `scripts/rustcopy-launcher.ps1`) è un **secondo** sistema di
"profili" nominati (nome, source, dest, threads, mirror, hash-algo, credenziali SMB) che il layer
PowerShell legge per costruire invocazioni CLI dirette — non passa da `[[jobs]]` TOML, non è letto
né scritto dalla GUI, e non c'è alcuna riconciliazione fra i due. Non è un difetto introdotto ora
(precede la console), ma è la prova concreta di cosa succede quando un secondo formato di
configurazione nasce accanto al primo: due elenchi di "backup che faccio abitualmente" che possono
divergere silenziosamente, e un operatore che aggiorna un profilo PowerShell non sposta nulla nel
mondo `[[jobs]]`/GUI, o viceversa.

**Conclusione**: un vero file di workspace separato (un terzo formato, superset di configurazioni
TOML non correlate, con un proprio stato salvato) **non ha senso qui** — duplicherebbe esattamente
ciò che `[[jobs]]` già risolve, e ripeterebbe l'errore che i profili PowerShell hanno già commesso,
frammentando la configurazione invece di consolidarla. È lo stesso principio che ha già escluso una
cache di scan duplicata (P3) e SQLite (D19/D20): niente terza struttura quando la prima basta.

**Cosa invece ha senso, a rischio pressoché nullo**: elevare "Recenti" a un piccolo elenco di
**preferiti nominati** (etichetta breve → percorso), *superset* di "Recenti" e non sua sostituzione
— stesso meccanismo (client-side, `localStorage`, zero modifiche al core, stesso pattern già in
`session.svelte.js`). Risolve il problema reale — un MRU di 8 percorsi anonimi è scomodo appena si
gestiscono più di due o tre destinazioni ricorrenti, che è esattamente il caso che i profili
PowerShell testimoniano nel repository — senza introdurre un secondo formato di configurazione o
toccare in alcun modo prestazioni, robustezza o potenza del motore. ✅ **Completato 5 Set 2026** —
[`F66`](ROADMAP.md) in `ROADMAP.md`, con il dettaglio implementativo e la verifica manuale contro
il binario compilato.

**Effetto collaterale di questa analisi**: vale la pena che l'utente sappia che il repository ha
oggi due sistemi di "profili" paralleli e disconnessi (PowerShell e, potenzialmente domani, i
preferiti GUI proposti sopra andrebbero costruiti come *etichette su percorsi `[[jobs]]` esistenti*,
mai come un terzo formato equivalente a `profiles.json`) — non una richiesta di azione immediata,
ma una cosa da tenere a mente la prossima volta che si tocca l'uno o l'altro.

### 12.2 Funzionalità CLI non ancora valutate, utilizzabili anche in GUI

Cinque idee, ciascuna verificata contro il codice reale — non proposte a vuoto. Le prime tre
riusano quasi per intero logica **già scritta e testata**, solo mai esposta con questo scopo.

1. **`--list-schedules`** (ora [`F62`](ROADMAP.md), ✅ completato 5 Set 2026) — lacuna già dichiarata (`CLAUDE.md`, riga F36: "Known gap: no
   `--list-schedules`") ma mai colmata. `schedule::referencing_config`
   (`crates/rustcopy-core/src/schedule.rs`) interroga già `schtasks.exe /Query /FO CSV /V` e
   filtra le attività il cui comando cita un `config_path` specifico — usata oggi solo dalla GUI
   (`gui_api::schedules_referencing`, il badge "un'attività punta già qui" in Esegui). Una
   variante che filtra invece sul percorso del **binario** (ogni attività che invoca
   `robocopy_ingest.exe`, non solo quelle che citano un file preciso) chiuderebbe la lacuna CLI
   riusando lo stesso motore di parsing CSV già coperto da test contro output reale catturato. La
   GUI guadagnerebbe un vero elenco al posto del semplice badge booleano di oggi.
2. **Anteprima di un mirror/purge, di sola lettura** (ora [`F63`](ROADMAP.md), ✅ metà mirror completata 5 Set 2026, retention non ancora fatta) — `check_mirror_safety`
   (`crates/rustcopy-cli/src/main.rs`) **calcola già** l'elenco esatto (`extraneous: Vec<&Path>`)
   dei file che `--mirror` cancellerebbe, ma oggi lo tronca a 5 voci e lo stampa solo su
   `stderr` quando sta per abortire in modo interattivo. L'avviso mirror della console
   (`Run.svelte`) dice letteralmente "eseguila dalla CLI, dove la conferma mostra quali file
   verrebbero eliminati" — un'opzione che scrive la lista **intera**, strutturata (JSON), senza
   mai eseguire il purge, chiuderebbe quella frase con un pulsante invece che con un rimando alla
   riga di comando. Il vincolo F61 resta intatto: leggere un elenco non è autorizzare una
   cancellazione, stessa distinzione già usata per il badge di pianificazione.
3. **Anteprima di ripristino** (ora [`F64`](ROADMAP.md), ✅ completato 5 Set 2026) — `--restore-from` e `--dry-run` non risultano in conflitto in
   `cli.rs` (nessun `conflicts_with` fra i due), quindi la combinazione **probabilmente** già
   funziona oggi — non verificato con un'esecuzione reale in questa sessione, va confermato prima
   di costruirci sopra. Se confermato, è il primo mattone naturale per il flusso di ripristino
   guidato già in cima al backlog (§5b, §8 Onda 3): elenco report → anteprima (questo comando) →
   conferma esplicita → avvio.
4. **Controllo preventivo di spazio libero in destinazione** (ora [`F65`](ROADMAP.md), ✅ completato 5 Set 2026) — verificato: **non esiste in
   nessuna forma** nel codice attuale (nessun riferimento a spazio libero/disco in tutto
   `rustcopy-core`/`rustcopy-cli`). Confrontare i byte totali del prescan con lo spazio libero a
   `--dest` prima di avviare il trasferimento eviterebbe una run di ore che fallisce a metà per
   disco pieno — attivo di default, con un `--skip-space-check` per destinazioni dove lo spazio
   libero non è interrogabile (alcune condivisioni di rete non lo espongono in modo affidabile).
   Naturale anche come indicatore "pronto a partire" nella scheda Esegui, prima di Avvia — una
   lettura, non un giudizio, quindi coerente con quanto la console può già fare da sola.
5. **Considerata e scartata**: un comando di introspezione schema (`--print-schema`) che
   permetterebbe all'editor della GUI di generare il proprio form dinamicamente invece del form
   scritto a mano di oggi (26 dei 29 campi di `JobConfig`). Scartata perché sposterebbe il pattern
   consolidato di questo progetto — "involucro sottile, il giudizio resta nel core" — verso uno
   schema-driven generico: un cambio di paradigma sproporzionato rispetto al problema reale, che è
   un form manuale funzionante e senza segnalata difficoltà di manutenzione.

**Le quattro voci costruibili sono ora implementate** (F62, F64 e F65 chiuse per intero; F63 per la
sola metà mirror, come annotato riga per riga sopra), e con esse F66 per il punto sul workspace in
§12.1 — questo paragrafo descriveva lo stato al momento dell'analisi iniziale, prima della decisione
di procedere con `AskUserQuestion` e della successiva implementazione. Le quattro costruibili
(punti 1-4) hanno una spec tecnica completa in `ROADMAP.md` come F62-F65, più F66 per il punto sul
workspace in §12.1 — il punto 5 resta solo qui, essendo stato scartato e non una voce di
backlog.

## 13. Audit reale post-F62–F66 (6 Set 2026): criticità e miglioramenti da implementare

Richiesto dall'utente subito dopo la chiusura di F62-F66: "controlla lo stato attuale delle
funzionalità e aspetto della GUI". Stesso metodo del §9 — non una rilettura del codice, ma la
console compilata (`target/release/rustcopy-gui.exe`, ricompilata da `main` a `5ab39eb` per
includere F62-F66 per intero) riaperta con Windows-MCP: `demo-locale.toml` caricato, un job eseguito
per davvero, il report risultante aperto, l'anteprima di ripristino (F64) e i preferiti (F66)
esercitati dal vivo, non solo letti nel sorgente.

### a) ✅ Corretto lo stesso giorno — D26: l'anteprima di ripristino falliva sul caso d'uso documentato come standard

Il primo utilizzo reale di "Anteprima ripristino" (F64) contro il report del demo eseguibile aveva
restituito **"Esito robocopy: fatal error, no files copied"** invece di un'anteprima. Un primo
tentativo di correzione (`command.current_dir(report.parent())`, lo stesso pattern già usato da
`run_arguments`/`resume_arguments`) è stato **verificato sbagliato** dalla stessa disciplina che lo
aveva trovato: ricompilato e riprovato, ha riprodotto lo stesso identico fallimento — la cartella
del *file* del report è spesso un livello più in profondità di quella da cui i suoi `source`/`dest`
sono relativi. Il fix corretto passa invece la cartella del **config** eventualmente già caricato
nella console (`session.configPath`, un nuovo parametro `config_path` su `preview_restore`), la
stessa da cui la run originale ha davvero risolto quei percorsi. Verificato di nuovo contro il
binario ricompilato, stesso identico scenario (Esegui → "Apri il report di questa run" → "Anteprima
ripristino"): anteprima reale, sorgente/destinazione invertite correttamente. Dettaglio, cronologia
completa dei due tentativi e verifica: [`ANALYSIS.md`](ANALYSIS.md) D26, riga F64 di `ROADMAP.md`.

### b) ✅ Corretto lo stesso giorno — D27: conteggio "File copiati" sovrastimato su host non inglese

Il report della run reale eseguita durante questo audit mostrava **"File copiati: 6 / 4"** — il
numeratore superava il denominatore. La causa **non** era, come ipotizzato inizialmente in questo
stesso paragrafo, un'ambiguità fra due fonti "entrambe valide" (robocopy contro prescan): era un
bug di parsing reale, isolato con certezza tramite log di debug (`RUST_LOG=debug`), non per ipotesi.
`engine::robocopy::parse_summary_row` cerca le etichette inglesi "Files"/"Bytes"; sull'host italiano
di questa sessione robocopy le stampa "File:"/"Byte:" (singolare), quindi il riepilogo autorevole
non viene mai letto e il codice ricade su un conteggio riga-per-riga che, a sua volta, non
riconosceva come intestazioni le righe "Avviato:"/"Terminato:" (Started/Ended, senza lo spazio
prima dei due punti che la versione inglese ha sempre) — ciascuna contava come un "file" fantasma
da un numero di byte pari al giorno del mese nella data. Corretto lo stesso giorno: dettaglio
completo, causa esatta e verifica in [`ANALYSIS.md`](ANALYSIS.md) D27.

### c) ✅ Corretto lo stesso giorno — il pannello "Preferiti"/"Recenti" si sovrapponeva all'intestazione della tabella sottostante

Aperto "Preferiti" nella scheda Job con un file caricato, il testo delle colonne
"Destinazione"/"Tipo"/"Verifica" restava visibile a metà, tagliato dal bordo inferiore del pannello
a tendina. **Non era un bug di stacking CSS** — un pannello `absolute`/`z-10` occlude sempre un
elemento statico sottostante, per specifica, e verificato con lo strumento Snapshot (coordinate
reali degli elementi, non solo pixel): il pannello e la riga dell'intestazione si sovrapponevano
davvero geometricamente, senza alcun margine di separazione. Il pannello copre correttamente ciò
che sta sotto per la porzione che effettivamente sovrappone; il resto del testo, appena fuori da
quella porzione, resta visibile — comportamento corretto di composizione, ma visivamente confuso
perché tagliava una riga di testo a metà altezza invece di lasciarla del tutto sopra o del tutto
sotto. Corretto dando al componente `PathBar` un proprio margine inferiore (`mb-8`, non `mb-4`: i
margini fra fratelli di blocco adiacenti collassano al maggiore dei due, non si sommano — un primo
tentativo con `mb-4` non ha spostato nulla, verificato dal vivo prima di capire perché e correggere
il valore). Riverificato: la tabella ora appare chiaramente sotto il pannello, senza alcuna
sovrapposizione, con 1-3 preferiti (il caso comune).

### d) 🟡 Frizione ripetuta — ogni scheda richiede un nuovo clic anche quando il percorso è già noto

Passando da Job (caricato) a Impostazioni, poi a Modifica, poi a Esegui — tutte con lo stesso
`demo-locale.toml` — il campo percorso arriva già precompilato in ognuna (conferma che
`session.configPath` è davvero condiviso), ma il contenuto della scheda resta vuoto finché non si
preme di nuovo il proprio pulsante ("Apri impostazioni", "Apri per modifica", "Esamina"...). Il §9g
aveva già trovato e risolto questo esatto problema per il solo salto Esegui→Report (Livello 1, punto
5); questo audit lo trova **generalizzato a ogni coppia di schede**, non solo a quella. **Non
proposto come fix immediato**: un auto-caricamento su ogni cambio scheda avrebbe un costo (una
lettura I/O per ogni clic di navigazione anche quando l'operatore sta solo guardando) da soppesare,
non una correzione ovvia a costo zero come lo era il collegamento Esegui→Report.

### e) ✅ Corretto lo stesso giorno — contenuto di Aiuto non aggiornato per F64/F66

`Help.svelte` non menzionava né "preferiti" né "anteprima ripristino": zero occorrenze di entrambi i
termini, verificato con una ricerca diretta nel sorgente. Stessa causa già diagnosticata al §9h per
un'altra scheda ("il contenuto statico non ha un proprietario che lo tenga aggiornato quando una
scheda cambia comportamento") — non un'osservazione nuova nel meccanismo, solo nella ricorrenza:
due funzionalità aggiunte nella stessa sessione avevano lasciato lo stesso tipo di traccia. Aggiunta
una voce "preferiti" ai termini, una frase su "Anteprima ripristino" alla descrizione della scheda
Report, e — trovato leggendo lo stesso file per questo fix, non un'osservazione originale
dell'audit — l'esito `6` (F65, spazio insufficiente) mancava dalla tabella degli esiti di una run,
aggiunto anch'esso.

### f) Cosa invece regge bene, verificato dal vivo

I Livelli 1-3 del rifacimento visivo (§10) restano solidi contro un caso reale con dati: layout
contenuto, colonne esplicite, badge di provenienza accanto al valore, sidebar con icone, card. F66
(preferiti) funziona correttamente end-to-end, glitch (c) incluso ora corretto — aggiunta, rimozione,
e il pulsante ★ che riflette subito lo stato — verificato aggiungendo e rimuovendo un preferito reale.
F62 (`--list-schedules`) è confermato **non ancora agganciato a nessun elemento visibile della
console** (`gui_api::list_all_schedules` esiste, nessuna scheda lo chiama) — coerente con quanto
`ROADMAP.md` già dichiarava, non una sorpresa di questo audit.

### Priorità consigliata

1. ~~**D26** (P0)~~ ✅ **corretto il 6 Set 2026**, stesso giorno della scoperta — vedi (a) sopra.
2. ~~**(b)**, D27, il conteggio file~~ ✅ **corretto il 6 Set 2026** — vedi (b) sopra.
3. ~~**(c)**, il glitch del pannello~~ ✅ **corretto il 6 Set 2026** — vedi (c) sopra.
4. ~~**(e)**, il contenuto di Aiuto~~ ✅ **corretto il 6 Set 2026** — vedi (e) sopra.
5. **(d)** resta l'unica voce non affrontata in questo giro — miglioramento di rifinitura, non
   bloccante, nessun rischio di sicurezza o di dati; richiede una decisione sul costo di un
   auto-caricamento per cambio scheda prima di implementarlo, non solo il tempo per scriverlo.

## 14. Confronto con TeraCopy e Cobian Reflector: cosa manca alla GUI (analisi del 6 Set 2026)

Richiesta dall'utente dopo l'audit di §13, con una domanda specifica: guardando le GUI reali di
TeraCopy (copia interattiva) e Cobian Reflector (backup schedulato), cosa manca alla nostra console
per essere altrettanto funzionale? `ROADMAP.md` ha già un confronto di parità a livello di **motore**
(righe 157-204, F38/F40/F42-F51/F57/F58) — questa sezione lo rilegge da un angolo diverso: non "cosa
sa fare il motore" ma "cosa può fare un operatore dalla finestra", verificato contro il codice
attuale della console, non contro la memoria di quando quel confronto fu scritto.

### 14.1 Cosa offre la GUI di TeraCopy

Un dialogo di copia interattivo: barra di progresso globale, nome del file in corso, velocità
istantanea/media ed ETA; pulsanti **Pausa/Riprendi/Stop** attivi durante il trasferimento; una
finestra **Salta/Riprova/Salta tutto/Interrompi** quando un file fallisce; una **coda** di copie
visibile e riordinabile; un interruttore Copia/Sposta; integrazione nel menu contestuale di
Explorer; una cronologia dei trasferimenti; un limitatore di banda con slider nelle impostazioni;
un'icona in system tray.

### 14.2 Cosa offre la GUI di Cobian Reflector

Un elenco di Task con icona di stato (riuscito/fallito/avviso); un wizard multi-scheda per
crearne/modificarne uno (Generale, File, **Pianificazione con calendario**, Avanzate —
compressione/cifratura/numero copie da conservare, **Eventi** pre/post, FTP/cloud); un pulsante
"Esegui ora" per task; un visualizzatore di log colorato; notifiche a icona tray; impostazioni SMTP
per le notifiche via email.

### 14.3 Confronto, per categoria

**Genuinamente mancante, costruibile ora, nessun conflitto architetturale:**
- Nessun controllo di riordino dei job **nel file**, prima di eseguirlo — verificato in
  `Editor.svelte`: `drafts` è già un array multi-job (`+ Nuovo job` lo estende), ma non esiste alcun
  controllo sposta-su/sposta-giù sulle schede esistenti. `job_editor.rs`/`write_proposal` serializza
  l'array nell'ordine in cui la GUI glielo passa, quindi il percorso di scrittura già esiste — manca
  solo l'interazione lato Svelte.
- Nessun limitatore di banda con slider in Modifica — il campo (`bandwidth_limit_mbps`) esiste ed è
  scrivibile, ma come input testuale, non come controllo dedicato.

**Genuinamente mancante, bloccato da una decisione architetturale già presa e motivata:**
- **Pausa/Riprendi/Salta-per-file durante un trasferimento** (F47/F48/F58, `ROADMAP.md` righe
  383-385) — robocopy è un processo esterno opaco, non pilotabile a runtime dopo l'avvio.
  Richiederebbe un motore di copia nativo per i job interattivi. Segnato "da prototipare prima di
  impegnarsi", milestone 8.0.0 condizionale.
- **Estensione shell in Explorer** (F51) — deliverable separato (DLL COM registrata + installer
  proprio), il costo più alto della roadmap.

**Presente in TeraCopy/Cobian ma deliberatamente escluso dalla nostra GUI per un confine di
sicurezza già scritto, non per lavoro mancante — qui la differenza conta, non va confusa con un
gap:**
- **Un wizard che crea pianificazioni/servizi dalla GUI** — Cobian lo fa liberamente; la nostra
  console non può, per il vincolo esplicito di `runner.rs` ("la console può riferire un'operazione,
  mai autorizzarla" — installare un servizio o una pianificazione è nell'elenco esplicito dei
  divieti, §7). F62 mostra le pianificazioni esistenti apposta senza offrire di crearne di nuove: non
  è un gap, è la ragione per cui F62 è stato disegnato così.
- **Ruoli admin/operatore** (F57) — presenti in Cobian, valutati e scartati qui (§6): "utile come
  prevenzione degli errori, non come confine di sicurezza" in un'app desktop dove chi ha la sessione
  può comunque eseguire l'eseguibile direttamente.

### 14.4 Rianalisi critica — dove la prima lettura era imprecisa

Rileggendo il confronto appena scritto contro il codice reale, due affermazioni non reggevano:

**F49 ("coda di job gestibile") non è una voce sola — la sua stessa formulazione in `ROADMAP.md`
("riordinare/accodare job **prima o durante** l'esecuzione") mescola due capacità di costo
completamente diverso.** Riordinare **prima** di premere Avvia è a buon mercato (§14.3, sopra: manca
solo l'interazione in `Editor.svelte`). Riordinare **durante** un batch già in esecuzione, invece,
sbatte contro lo stesso muro architetturale di F47/F58: verificato in
`crates/rustcopy-cli/src/main.rs::run_jobs` (righe 335+), l'elenco dei job è un semplice `for` letto
una volta all'avvio del processo CLI — non esiste alcun canale con cui la console, che ha solo
avviato quel processo e può soltanto fermarlo (scrivendo il file di stop), possa fargli rileggere un
ordine diverso a metà corsa. La prima stesura di questa sezione presentava "coda gestibile" come il
secondo candidato più economico dopo il pulsante di ripristino — falso per la metà "durante
l'esecuzione": quella metà è cara quanto F47/F58, non a buon mercato.

**Un visualizzatore di log grezzo colorato (proposto nella prima stesura come gap da chiudere) non è
chiaramente un miglioramento, riletto contro il principio che questo progetto ha già applicato altrove
(`ANALYSIS.md`/`ROADMAP.md`, il rifiuto di `--print-schema` in §12.2 punto 5: "scartata perché
sposterebbe... verso uno schema-driven generico, un cambio di paradigma sproporzionato rispetto al
problema reale").** Cobian e TeraCopy hanno un log grezzo perché i loro motori non producono altro;
questo progetto produce già un report JSON strutturato più un'analisi deterministica (`--advise`) —
un log grezzo scorrevole sarebbe parità di forma con uno strumento diverso, non una capacità che
manca davvero. Retrocesso da "gap da colmare" a "nessun bisogno concreto dimostrato", stessa barra
già applicata a F38 (compressione) e F40 (cloud/FTP) nel backlog esistente.

Una terza cosa, verificata e **non** un errore della prima stesura ma degna di nota qui perché il
dubbio era legittimo: la velocità di trasferimento **dal vivo** (non solo nel report finale) è già
mostrata — `ProgressSample.throughput_mbps` esiste da prima di questa sessione e `Run.svelte` (riga
433) la rende come "— N MB/s" accanto a file/byte in corso. Non un gap. L'assenza reale, verificata,
è solo il **nome del file in corso** (TeraCopy lo mostra, la console no) — deliberata, non
dimenticata: `PIANO_GUI.md` §2.1 documenta che il campionamento a 200ms tramite contatori atomici
evita apposta un evento IPC per file, proprio per non pagare il costo che un nome-file dal vivo
imporrebbe.

### 14.5 Priorità raffinata

1. ~~**Riordino dei job prima dell'esecuzione**~~ ✅ **completato 7 Set 2026 (F67)** — due pulsanti
   sposta-su/sposta-giù alle schede di `Editor.svelte`, un solo controllo per l'intera striscia
   (opera sul job selezionato, non uno per scheda). **Il percorso di scrittura non "esisteva già"
   come previsto qui**: `job_editor::build_proposal` ignorava del tutto l'ordine di `drafts` per i
   job già noti, aggiornandoli sempre sul posto alla loro posizione originale nel file — trovato
   scrivendo davvero il file e rileggendolo, non fidandosi della sola UI (le schede si scambiavano
   correttamente a schermo, il file no). Corretto in `build_proposal`; dettaglio completo, causa e
   verifica nella riga F67 di `ROADMAP.md`. Resta backlog, deliberatamente, solo la metà "durante
   l'esecuzione" — vedi punto 2.
2. **Motore pilotabile** (F47/F48/F58, e con esso la metà "durante l'esecuzione" di F49) — il gap
   che pesa di più sull'esperienza utente reale rispetto a TeraCopy, ma il più costoso: richiede un
   prototipo prima di una decisione, come già scritto in `ROADMAP.md`. Non affrontarlo con una stima
   di sforzo prima di quel prototipo.
3. **Limitatore di banda con slider** — rifinitura di poco valore, nessun rischio; non prioritaria.
4. Wizard di pianificazione/servizio dalla GUI e ruoli admin/operatore **non entrano in questa
   lista**: non sono lavoro rimandato, sono confini già decisi e motivati altrove (§6, §7). Riproporli
   richiederebbe prima riaprire quella decisione con l'utente, non implementarli.
5. Visualizzatore di log grezzo **rimosso dal piano**: nessun bisogno concreto dimostrato, stessa
   barra di F38/F40.

## 15. Motore pilotabile (F47/F48/F58): analisi di rischio, sospesa (7 Set 2026)

Richiesta dall'utente prima di impegnarsi sul punto 2 di §14.5. Non un'implementazione: un'analisi
per decidere se e come procedere, che si è conclusa con la sospensione esplicita di entrambi i
livelli — registrata qui perché la decisione di **non** procedere ha bisogno della stessa
motivazione scritta di una decisione di procedere, altrimenti la prossima sessione la riapre da
zero senza sapere che è già stata valutata.

### 15.1 Due livelli, non uno

- **Livello A — pausa/riprendi l'intero trasferimento**, senza sostituire il motore. Il PID di
  robocopy è già tracciato oggi (`child_pid: Arc<AtomicU32>`, popolato da
  `RobocopyEngine::new_with_pid_slot`, usato oggi solo da `kill_active_child`). Sospensione a
  livello di processo (`NtSuspendProcess`/`NtResumeProcess`, non `SuspendThread` per-thread — vedi
  §15.2) congelerebbe tutti i thread del processo, ripristinabile esattamente da dove si trovava.
- **Livello B — salta/riprova un file specifico**. Richiede il motore naive
  (`engine::naive::NaiveCopyEngine`, già esistente e usato da incrementale/differenziale) al posto
  di robocopy per i job interattivi, perché solo un ciclo file-per-file scritto da noi può decidere
  di abbandonare un file a metà. Robocopy, come processo esterno, non offre alcun modo di
  comunicargli "salta questo file".

### 15.2 Livello A — criticità reali, non ipotetiche

- **Meccanismo corretto**: `NtSuspendProcess`/`NtResumeProcess` (chiamata NTAPI singola e atomica),
  non l'enumerazione dei thread via `CreateToolhelp32Snapshot`+`SuspendThread` — quest'ultima, pur
  essendo l'API pubblica e documentata, introduce una finestra non atomica fra thread diversi, ed è
  esplicitamente sconsigliata da Microsoft per la sincronizzazione ("primarily for debuggers")
  proprio per il rischio di deadlock descritto sotto. Con `/MT` (il default di questo progetto salvo
  limite di banda, D23) robocopy ha un pool di thread: sospenderli uno alla volta non è atomico.
  `NtSuspendProcess` lo è, congela tutto nel kernel in una sola chiamata — lo stesso meccanismo con
  cui Task Manager sospende un processo da Windows 8 in poi. **Non documentata da Microsoft**
  (esposta da `ntdll.dll`, stabile da NT4, nessuna garanzia formale) — prima volta che questo
  progetto dipenderebbe da un'API non pubblica, diverso da `GetDiskFreeSpaceExW` (F65) o dagli
  shell-out a strumenti nativi (vssadmin/schtasks/taskkill, tutti pubblici e documentati).
- **Rischio di deadlock, non eliminabile**: se un thread di robocopy viene sospeso nell'istante
  esatto in cui tiene una lock (una critical section, anche solo l'heap lock del CRT durante
  un'allocazione qualunque) necessaria a un altro thread per proseguire, alla ripresa il processo
  può restare bloccato per sempre — non un bug nostro, come funziona la sospensione di un processo
  qualunque a livello di sistema operativo. Mitigabile, non eliminabile: un timeout sulla ripresa
  (se il contatore di byte non avanza entro N secondi da "Riprendi", trattarlo come processo
  bloccato, terminarlo, proporre la ripresa da checkpoint — F31, già esistente, nessun meccanismo
  nuovo da inventare per la rete di sicurezza).
- **Rischio di rete, non verificabile da qui**: una pausa lunga su una condivisione SMB (lo
  scenario di `examples/smb-nas-mirror.toml`) rischia che il server rilasci l'oplock o chiuda la
  sessione per inattività — alla ripresa la prima syscall potrebbe fallire. Richiede un test reale
  contro un'infrastruttura di rete vera, con pause di durata crescente, prima di dichiararlo sicuro.
- **Incoerenza fra motori**: per robocopy la pausa è istantanea ma rischiosa (sopra); per il motore
  naive sarebbe invece un controllo cooperativo fra un file e l'altro — sicuro per costruzione (zero
  rischio di deadlock) ma a grana più grossa (mai a metà di un file). Il Livello A non è quindi un
  meccanismo uniforme fra i due motori, e questo va dichiarato esplicitamente se mai implementato,
  non presentato come una sola funzionalità.
- **Performance, verificata nel codice, non assunta**: `ProcessRunner::run` legge lo stdout di
  robocopy con un `read_until` bloccante su un thread dedicato — durante una sospensione nessun
  nuovo byte arriva sulla pipe, quel thread resta bloccato in attesa, zero CPU consumata, nessun
  errore spurio. Il costo del solo controllo del segnale di pausa (stesso ciclo di poll del file di
  stop già esistente) è trascurabile. Nessuna criticità di prestazioni qui — l'unica reale è di
  correttezza (sopra), non di velocità.
- **Cosa comporterebbe per la CLI**: un nuovo `--pause-file <PATH>`, simmetrico a `--cancel-file`,
  utilizzabile anche da terminale (coerente con "la GUI è un livello sottile sopra la CLI, mai il
  contrario") — non un flag GUI-only. Un nuovo task asincrono, separato dal thread bloccante che
  legge lo stdout, che sorveglia il file e agisce sul PID già tracciato. Una razza da gestire
  esplicitamente: una richiesta di pausa arrivata prima che il PID sia disponibile (durante il
  prescan) deve restare pendente e applicarsi non appena il processo parte, non essere persa.

### 15.3 Verdetto

**Nessuno dei due livelli viene implementato ora.** Non per mancanza di fattibilità tecnica — il
meccanismo di base del Livello A è collaudato in produzione su milioni di macchine — ma perché il
rischio di deadlock non è eliminabile, solo mitigabile, e la validazione contro rete reale non è
stata fatta. Se ripreso in futuro: **Livello A prima**, come funzionalità **opt-in** con la rete di
sicurezza del checkpoint già descritta, mai come "pausa garantita"; un vero prototipo (non solo
questa analisi) prima di qualunque stima di sforzo; il Livello B resta una decisione separata e più
grande, condizionata all'uso reale del Livello A. `ROADMAP.md` (righe F47/F48/F58) rimanda qui.

## 16. Selezione di percorsi e campi di configurazione irraggiungibili dalla GUI (analisi del 7 Set 2026)

Richiesta dall'utente dopo la sospensione del motore pilotabile (§15): messo da parte il gap più
costoso, cosa migliora davvero l'esperienza dell'operatore senza toccare `runner.rs` o il confine
F61? Ogni affermazione qui è stata verificata contro il sorgente reale, non assunta — stesso metodo
di §12/§14.

### 16.1 Sorgente e Destinazione si digitano, non si scelgono

**Prima di F68 (analisi originale, 7 Set 2026 — stato storico, superato dall'implementazione più
sotto):** `Editor.svelte`, righe 242-246: `Sorgente` e `Destinazione` sono due `<input>` di solo testo,
`bind:value={draft.source}`/`{draft.dest}`. Verificato con una ricerca diretta: zero occorrenze di
"Sfoglia" nelle vicinanze, contro l'unico "Sfoglia…" di tutto il file — quello del percorso della
*proposta in uscita* (riga 365), non di sorgente o destinazione. È l'unico punto della console dove
un percorso va digitato a mano: `PathBar.svelte`, usata in ogni altra scheda, ha già sia un
selettore nativo sia recenti/preferiti.

**Tecnicamente a costo quasi zero**: `@tauri-apps/plugin-dialog` (già in uso) espone `directory:
true` nei propri tipi (`node_modules/@tauri-apps/plugin-dialog/dist-js/index.d.ts`, riga 57) — un
selettore di cartella nativo, zero dipendenze nuove, due chiamate dirette `open({ directory: true })`
in `Editor.svelte`, stesso *meccanismo* di `PathBar.svelte::browse()`. **Non** `PathBar` come
componente riusato: i suoi elenchi recenti/preferiti sono per percorsi di *file* (config/report, con
un `kind` che li distingue), non per cartelle sorgente/destinazione di un job — mescolarli
confonderebbe due liste concettualmente diverse. Nessuna nuova superficie di sicurezza: scegliere da
un dialogo non è un rischio diverso dal digitare, è il contrario — "un percorso digitato male è
indistinguibile da uno assente" (commento già presente nel codice, la stessa ragione per cui
`PathBar` esiste). Spec tecnica completa: **F68**, riga corrispondente in `ROADMAP.md`.

**✅ Implementato e verificato 7 Set 2026** contro il binario compilato: entrambi i pulsanti
"Sfoglia…" aprono il dialogo nativo e la selezione aggiorna correttamente `draft.source`/
`draft.dest` (letto nel campo dopo la conferma). Un comportamento non anticipato in fase di
proposta, scoperto proprio durante questa verifica: il dialogo nativo di Windows rifiuta un
percorso non ancora esistente ("Percorso non esistente" su `examples\demo-out\copia`, una
destinazione tipica per una prima sincronizzazione) — comportamento standard del selettore di
cartelle, identico in Explorer, non un difetto di questa implementazione. Il campo di testo resta
sempre editabile in parallelo al pulsante, quindi una destinazione non ancora creata si digita
come prima o si crea con "Nuova cartella" dentro il dialogo. Dettaglio completo: riga F68 in
`ROADMAP.md`.

### 16.2 Un confronto diretto: 34 campi, 17 raggiungibili dalla GUI

`JobConfig` ha 34 campi (contati nel sorgente, `crates/rustcopy-core/src/config.rs`). **Stato al 7
Set 2026, dopo F68/F69/F70** — `Editor.svelte` ne referenzia 17 (`grep -oE "draft\.[a-z_]+"
Editor.svelte`): `name, source, dest, pattern, threads, retries, exclude_files, exclude_dirs,
report_path, verify_integrity, fast_verify, dry_run, exclude_junctions, preserve_acl, mirror,
keep_generations, backup_type` — i due precedenti (`mirror`/`keep_generations`) bloccati/sola-lettura
o vincolati per un motivo di sicurezza già scritto (F54), `webhook_url`/`pre_command`/`post_command`
esclusi con nota esplicita (F55 non deciso, §5a). **Restano 14 campi mai renderizzati, in nessuna
forma, senza alcuna nota**: `retry_wait_seconds`, `ignore_transient_missing`, `html_report_path`,
`hash_algo`, `compare_baseline`, `log_path`, `min_age_days`, `max_age_days`,
`bandwidth_limit_mbps`, `no_prescan`, `skip_space_check`, `space_safety_margin_percent`,
`long_paths`, `preserve_timestamps`.

Due di questi meritano una voce a sé, verificata più a fondo, non solo elencati:

- **`keep_generations` era più restrittivo in GUI di quanto il core richiedesse. ✅ Implementato e
  verificato 7 Set 2026** (dettaglio completo, incluso il verificato "mai vuoto": riga F69 di
  `ROADMAP.md`). **Prima di F69 (analisi originale — stato storico, superato dall'implementazione)**:
  mostrato in sola
  lettura in Modifica, ma `job_editor.rs` accetta già di **alzarlo** — verificato nel test esistente
  `retention_can_be_neither_introduced_nor_lowered`: `raise.keep_generations = Some(12)` da un
  valore di partenza di 7 è esplicitamente atteso come accettato ("keeping more deletes less"), solo
  introdurlo da zero o abbassarlo sono rifiutati. La regola F54 ("restringere il rischio, mai
  allargarlo") è già interamente rispettata dal core per ogni valore ≥ quello attuale — la GUI non
  offriva il campo per omissione, non per un vincolo mancante. **La UI non deve però permettere di
  svuotare il campo** una volta impostato — vedi §16.3, un controllo che aggirerebbe il divieto
  esistente senza toccare alcun codice del core. Ora implementato esattamente così: un input che
  compare solo quando il job risolve già a un valore, mai svuotabile. Spec tecnica: **F69**.
- **`backup_type` era il più vistoso dei 15. ✅ Implementato e verificato 7 Set 2026**: full/
  incremental/differential è una delle feature bandiera del motore (F34) e non era raggiungibile
  dalla GUI in alcun modo — impostarla richiedeva modificare il file a mano. La verifica ha trovato
  una lacuna reale nel core stesso (non solo nella GUI): `apply_draft` non controllava affatto la
  combinazione `mirror`+`backup_type`, colmata con lo stesso pattern già usato per `no_prescan`.
  Spec tecnica: **F70**, dettaglio completo (incluso il fix del core) in `ROADMAP.md`.

Gli altri 13 campi restano backlog senza F-number dedicato, in ordine di valore stimato per un
operatore reale (non misurato, giudizio): `min_age_days`/`max_age_days` (filtro comune) e
`hash_algo` (scelta dell'algoritmo di verifica) sopra `bandwidth_limit_mbps` (già proposto come
slider in §14.5 punto 3) e `retry_wait_seconds` (naturale accanto a "Tentativi", già presente);
`long_paths`/`preserve_timestamps` sono interruttori semplici, stesso pattern già in uso, a basso
costo ma basso valore; `no_prescan`/`skip_space_check`/`space_safety_margin_percent` sono gli unici
due campi di F65 (la settimana scorsa) mai collegati all'editor — un'omissione propria, non
ereditata; `compare_baseline`/`html_report_path`/`log_path` restano niche/diagnostici.

### 16.3 Criticità trovate rileggendo questa stessa sezione

Prima di scrivere il piano di priorità, la prima stesura di questa sezione (e delle righe
corrispondenti in `ROADMAP.md`) è stata riletta cercando errori — stesso metodo di §14.4, per lo
stesso motivo: una sezione che propone lavoro futuro merita lo stesso scetticismo di una già
implementata, prima che qualcuno la usi come base per scrivere codice.

**Trovata una criticità reale, di sicurezza, non solo editoriale.** F69 come proposto inizialmente
diceva "un campo numerico che accetta valori ≥ al corrente, o vuoto per ereditare". Rileggendo
`apply_draft`/`pin()` (`job_editor.rs`) più a fondo: il controllo `EditorCannotLowerRetention`
confronta solo la coppia `(Some(from), Some(to))` con `to < from` — la coppia `(Some(from), None)`
(il caso "svuota il campo") cade nel ramo di default e **passa senza errore**. Una UI che permettesse
di svuotare il campo aggirerebbe il divieto silenziosamente: svuotare produce lo stesso effetto di
digitare un numero minore (retention ridotta), senza mai passare dal ramo che lo rifiuta. Non un
difetto nel core da correggere — il core non ha mai promesso di intercettare ogni possibile input di
una UI non ancora scritta — ma un vincolo che la **spec** di F69 deve dichiarare esplicitamente:
nessuna opzione per svuotare una volta che il campo risolve a un valore impostato. Corretto nella
riga F69 di `ROADMAP.md` e nel punto corrispondente sopra (§16.2).

**Trovato un errore aritmetico**: la prima stesura diceva "33 campi" — il conteggio reale nel
sorgente (`awk '/pub struct JobConfig \{/,/^\}/' | grep -c "pub [a-z_]*:"`) è **34**. Il totale dei
16 raggiungibili più i 3 esclusi deliberatamente più i 15 mancanti tornava già a 34 nella lista
dettagliata — solo l'affermazione del totale era sbagliata, non l'inventario che la seguiva.
Corretto ovunque comparisse, sia qui sia in `ROADMAP.md`.

**Due chiarimenti, non errori**: F68 diceva "stesso pattern di `PathBar.svelte::browse()`" in un
modo che si poteva leggere come "riusa il componente `PathBar`" — non è l'intenzione: gli elenchi
recenti/preferiti di `PathBar` sono per percorsi di file, non per cartelle di un job, e vanno
tenuti distinti. F70 affermava "nessun conflitto con F54" senza affrontare l'obiezione più seria
(attivare `backup_type` cambia dove finiscono i file, non è neutro) — la risposta resta la stessa
conclusione, ma ora con la motivazione esplicita: il confine F54 riguarda la cancellazione, non ogni
cambio di comportamento, e `source`/`dest` sono già liberamente modificabili con lo stesso tipo di
impatto. Entrambi corretti nella stessa riga di `ROADMAP.md`.

### 16.4 Priorità

1. **F68** — selettori di cartella per Sorgente/Destinazione. Il gap più visibile, il più semplice
   tecnicamente (nessuna dipendenza nuova). ✅ Completato 7 Set 2026.
2. **F69** — `keep_generations` editabile per alzarlo. Non una funzionalità nuova: allinea la GUI a
   un permesso che il core ha già. ✅ Completato 7 Set 2026.
3. **F70** — `backup_type` selezionabile. Chiude la lacuna più vistosa fra le feature bandiera del
   motore e la loro raggiungibilità dalla GUI. ✅ Completato 7 Set 2026.
4. Gli altri 13 campi (§16.2, ultimo paragrafo) — nessun F-number dedicato finché uno di questi non
   emerge come richiesta concreta, stesso criterio già applicato a F38/F40/F42 nel backlog storico.

## 17. Sincronizzazione rapida senza un file di configurazione esistente (analisi del 7 Set 2026)

Richiesta dall'utente: "una semplice copia con controllo dei file originali e copia solo di quelli
aggiornati, dalla GUI" — con il sospetto giusto che esistesse già nella CLI. Verificato prima di
proporre qualunque cosa, non assunto.

**✅ Implementato e verificato 7 Set 2026** — esattamente come speccato in §17.3/§17.5 sotto: nuovo
`QuickSync.svelte`, collegato da un link nell'empty state iniziale di `Jobs.svelte` (nessuna sesta
scheda in sidebar, `Jobs.svelte` continua a non scrivere/eseguire nulla di suo). Un'unica scelta non
anticipata in fase di analisi: dopo `start_job`, il pannello **non** reimplementa il poll/notifica di
`Run.svelte` (un `setTimeout` incatenato con contatore di generazione per evitare risposte fuori
ordine, oltre alla notifica desktop di fine run) — naviga invece a Esegui con `session.configPath`
già impostato, lasciando che sia quella scheda, già scritta e verificata, a occuparsene con un
"Esamina" in più. Un secondo motore di polling per lo stesso stato avrebbe rischiato di divergere da
quello esistente, un rischio giudicato peggiore del click in più. Verificato end-to-end contro il
binario ricompilato: job creato da zero, scritto, avviato, 5/5 file copiati; stesso file rieseguito
dalla scheda Esegui, secondo run "no files copied, source and destination already in sync" —
confermato dal vivo, non solo per costruzione. Dettaglio completo: riga F71 di `ROADMAP.md`.

### 17.1 La capacità esiste già — il gap è solo nel raggiungerla dalla GUI

"Copia solo i file nuovi/aggiornati" **è** il comportamento di default di ogni copia senza
`--mirror`: robocopy stesso salta i file identici per dimensione e data — non una funzionalità di
rustcopy, un comportamento nativo dello strumento che orchestra. Verificato empiricamente più volte
in questa stessa sessione (D26/D27), non per sentito dire: una run su una destinazione già
sincronizzata produce "no files copied, source and destination already in sync".

Il gap reale è diverso: la GUI non permette di raggiungere nemmeno questo caso più semplice senza
avere già un file TOML pronto. `Editor.svelte::load()` chiama `read_drafts` (`job_editor.rs`, riga
537), che chiama `IngestConfig::load_from` (`config.rs`, riga 176) — `fs::read_to_string(path)`
fallisce su un percorso che non esiste. Modifica può solo **modificare** un file già presente, mai
crearne uno da zero: un operatore senza alcun TOML esistente non ha alcun punto d'ingresso.

### 17.2 Il core già supporta la creazione da zero — verificato, non assunto

`job_editor::build_proposal` (righe 412-434) gestisce esplicitamente `existing: None`: un singolo
draft con nome/sorgente/destinazione produce una `IngestConfig` valida a job singolo (i campi vivono
a livello di primo piano, non in `[[jobs]]` — la forma più semplice possibile di file). Le regole di
sicurezza di F54 restano intere anche partendo da zero: `effective.mirror` per un draft senza nulla
da cui ereditare parte da `None`, quindi un tentativo di attivare `mirror` in un draft "vuoto" viene
comunque rifiutato da `apply_draft` con lo stesso `EditorCannotEnableMirror` di ogni altro caso — non
serve una nuova eccezione, la protezione esistente copre già questo scenario.

### 17.3 Implementazione proposta — interamente lato GUI

Un modulo minimo, **non dentro `Jobs.svelte`**: solo un collegamento nell'empty state della scheda
Job (oggi rimanda solo a `examples/demo-locale.toml`, un secondo invito accanto ad esso:
"Sincronizza due cartelle adesso") che porta a un pannello a sé — vedi §17.5 sul perché questa
distinzione non è pedanteria. Il pannello: solo Sorgente e Destinazione, con i selettori di cartella
nativi di F68 — **deliberatamente nessuna opzione per mirror, backup_type o retention**, per restare
più semplice del form completo di Modifica, non un modo per aggirarlo. Un pulsante "Sincronizza" che:

1. Chiede dove salvare il job (stesso dialogo di salvataggio già usato in Modifica per l'output
   della proposta) — non un file "usa e getta" come lo scratch di F64: qui il backup è reale, non
   una simulazione, e l'operatore deve poterlo ritrovare per rilanciarlo o modificarlo in seguito.
2. Chiama `write_proposal` (già esistente) con quel percorso.
3. Chiama `start_job` (già esistente) sullo stesso percorso appena scritto.

**Zero nuovi comandi Tauri, zero nuove righe di core** — la funzionalità è la concatenazione di due
percorsi già scritti, testati e in uso, non un terzo meccanismo di scrittura o di esecuzione. Dopo
l'avvio, riusa il collegamento "Apri il report di questa run" già esistente (Livello 1, §10) per
portare l'operatore al risultato senza un passaggio manuale. **Per rilanciarlo in seguito**: nessun
meccanismo nuovo — il file scritto al punto 1 è un job come ogni altro, si riapre in Job/Esegui/
Modifica esattamente come un TOML scritto a mano.

### 17.4 Criticità considerate

- **Un file introvabile in seguito**: risolto chiedendo dove salvarlo prima di eseguire (punto 1
  sopra), non scegliendo una cartella temporanea per conto dell'operatore.
- **Il modulo minimo che diventa una scorciatoia attorno a Modifica**: mitigato non offrendo affatto
  i campi distruttivi nel modulo — chi ha bisogno di mirror o generazioni passa comunque da Modifica,
  questo resta il percorso per il caso più comune e più sicuro (una copia semplice).
- **Una scheda nuova per una funzionalità che si appoggia solo a due comandi già esistenti**: evitata
  deliberatamente — nessuna voce nuova nella barra laterale, solo un collegamento scoperto dall'empty
  state di Job già esistente.

### 17.5 Criticità trovata rileggendo questa stessa sezione

Stesso metodo di §14.4/§16.3. La prima stesura di §17.3 metteva l'intera logica (scrittura +
esecuzione) **dentro `Jobs.svelte`**, raggiungibile dal suo empty state. Rileggendo contro §3 di
questo stesso documento — la tabella "Cosa la console fa oggi" caratterizza esplicitamente Job come
"Scrive? No" — mettere lì un'azione che scrive un file e ne avvia l'esecuzione avrebbe contraddetto
quella caratterizzazione già dichiarata, anche se i due comandi sottostanti (`write_proposal`,
`start_job`) restano individualmente sicuri: non un rischio di sicurezza, ma un'imprecisione di
confine che avrebbe reso falsa una riga di documentazione già esistente il giorno stesso in cui
sarebbe stata implementata. Corretto in §17.3: l'empty state di Job resta solo un **collegamento**
verso un pannello a sé, che possiede la logica di scrittura/esecuzione — `Jobs.svelte` continua a
non scrivere né eseguire nulla di suo, esattamente come la tabella di §3 dichiara.

## 18. Modifica: usabilità per un operatore che non conosce già il form (analisi del 7 Set 2026)

Richiesta puntuale dall'utente dopo aver usato la console per la prima volta in modo estensivo:
undici osservazioni concrete su Modifica (righe 18.1-18.10 sotto), più una lacuna trasversale
(§18.11) e la richiesta di controllare se le altre schede hanno bisogno delle stesse migliorie
(§18.12). Ogni punto è stato verificato contro il sorgente reale prima di essere valutato fattibile
o meno — stesso metodo di ogni altra sezione di questo documento, non un'accettazione alla lettera
della lista. **Analisi soltanto**: nessuna riga di codice è stata toccata scrivendo questa sezione,
come richiesto esplicitamente.

### 18.1 L'errore "cannot split the single-job configuration" — cosa significa e perché esiste

Riprodotto: `job1` di `examples/demo-locale.toml` non ha `[[jobs]]` — è un file **a job singolo**,
i cui campi vivono a livello di primo piano del TOML. `job_editor::build_proposal` (righe 420-434)
tratta esplicitamente questo caso: un solo draft il cui nome coincide con quello del job esistente
resta nella forma a job singolo; **qualunque altra combinazione** (compreso "+ Nuovo job" seguito da
"Scrivi proposta", che produce due draft) viene rifiutata con
`IngestError::EditorCannotSplitSingleJobConfig`. Non è un bug: il commento del modulo lo dichiara
("turning it into a multi-job file changes what every one of those fields means, so the editor
declines rather than doing it silently") ed è coerente con la regola F54 — un cambiamento di
struttura del file così ampio non deve accadere in silenzio dietro un click. **La criticità reale è
nella comunicazione, non nella regola**: il messaggio ("add the [[jobs]] section by hand first") è
in inglese tecnico e presume che l'operatore sappia già cosa significhi "aggiungere `[[jobs]]` a
mano" — esattamente il tipo di conoscenza che una GUI dovrebbe evitare di richiedere. Verificato che
**non esiste oggi alcun percorso nel core** per convertire esplicitamente un file a job singolo in
formato `[[jobs]]` (il `match` in `build_proposal` è esaustivo: vuoto, singolo-che-combacia, o
rifiuto) — offrire un pulsante "Converti" richiederebbe una nuova capacità del core, non solo un
messaggio migliore, ed è una decisione di design a sé (vedi F78 sotto: solo la metà "messaggio
comprensibile" è proposta ora).

### 18.2 Campo Nome — nessuna validazione

Verificato in `job_editor::apply_draft`: `draft.name` viene usato letteralmente (`.clone()`) senza
alcun controllo sui caratteri, e finisce dentro `lib.rs::namespaced_path` che lo interpola
direttamente in un nome di file (`format!("{stem}.{name}.{ext}")`). Un nome contenente `\ / : * ? "
< > |` (caratteri riservati Windows) o uno dei nomi di dispositivo riservati (`CON`, `PRN`, `AUX`,
`NUL`, `COM1`-`9`, `LPT1`-`9`) produce un errore di I/O criptico al primo report/cache/manifest
scritto — non alla scrittura della proposta, ma ore dopo, alla prima esecuzione pianificata: esattamente
la classe di problema che il commento di `InvalidThreads` in questo stesso file (`job_editor.rs`)
avverte di non lasciare aperta. **Fattibile e a basso rischio**: nessun helper di sanificazione esiste
già nel progetto (verificato, `grep` vuoto), quindi va scritto da zero, ma è un controllo puramente
per caratteri vietati, non un'euristica. Proposto in due metà, stesso schema di F70
(`BackupTypeAndMirrorConflict`): un controllo proattivo in `apply_draft` (nuovo `IngestError`, il
core lo rifiuta comunque un giorno se qualcuno modifica il TOML a mano) più la disabilitazione
lato form con un messaggio immediato.

**✅ Implementato e verificato 7 Set 2026**, esattamente come proposto: `validate_job_name` accanto
a `namespaced_path` in `lib.rs`, chiamato come primissimo controllo di `apply_draft`. **Perimetro
tenuto deliberatamente identico a F70/F80**: "Scrivi proposta" non scansiona tutti i job del batch
per un nome non valido in un job non visualizzato al momento — solo il campo corrente ha un
messaggio immediato, il core resta l'unico vero backstop. Dettaglio completo nella riga F72 di
`ROADMAP.md`.

### 18.3/18.4 Sorgente e Destinazione — verifica esistenza e conteggio (stesso F-number, due campi)

Verificato: **non esiste oggi alcun comando Tauri** che ispezioni un percorso arbitrario (elenco
completo in `main.rs`, 17 comandi, nessuno fa statistica su una cartella scelta a mano) — andrebbe
scritto da zero, ma senza nuova logica di scansione: `scan::inventory(root, pattern, follow_links,
exclude_dirs, exclude_files, min_age_days, max_age_days) -> InventorySummary { total_files,
total_bytes }` esiste già e cammina l'albero **senza materializzare la lista dei file** — esattamente
lo scopo con cui F2.2/F2.6 lo hanno scritto (il conteggio per la barra di progresso). Un comando
`inspect_path` è un involucro sottile, stesso pattern `off_thread(move || scan::inventory(...))`
di ogni altro comando bloccante in `main.rs`. **Criticità reale, esplicitamente anticipata
dall'utente stesso** ("se non richiede troppo tempo"): questo progetto ha un profilo reale a 1.34M
file (`_ops_reports/full-profile-test.json`) su cui una scansione completa richiede minuti, non
secondi — il pulsante deve essere un'azione manuale esplicita (mai automatica ad ogni tasto premuto),
mostrare uno stato di attesa onesto, e la copia deve dire chiaramente che un albero molto grande può
richiedere tempo, non implicare un risultato istantaneo. **Asimmetria da tenere nella scrittura dei
testi**: una Destinazione inesistente è il caso **normale** per un primo backup (F68 lo ha già
mostrato per il picker di cartelle) — "non esiste" lì è un'informazione neutra, non un avviso; per la
Sorgente invece un percorso inesistente è quasi sempre un errore reale da segnalare con più
enfasi. Nessun conteggio di *cartelle* separato da quello dei file: `InventorySummary` espone solo
`total_files`/`total_bytes` — aggiungerlo è un contatore in più nello stesso walk, a costo marginale,
non una lacuna che blocca l'implementazione.

**✅ Implementato e verificato 7 Set 2026.** Il conteggio cartelle è stato aggiunto (`total_dirs`),
esattamente come previsto — costo marginale nello stesso walk. Il risultato di un controllo è
tenuto valido solo finché descrive ancora il percorso e il job per cui è stato lanciato: modificare
il campo o cambiare job dopo un click su Verifica scarta silenziosamente la risposta invece di
mostrarla accanto a un percorso che non descrive più — non previsto esplicitamente in questa
analisi, emerso durante l'implementazione come la conseguenza ovvia di un controllo manuale su un
form che il resto del tempo resta modificabile. **Bug reale trovato al primo click dal vivo**: il
percorso veniva controllato contro la working directory del processo della console, non contro la
cartella del file di configurazione — su `demo-locale.toml` (percorsi relativi per convenzione)
"Verifica" rispondeva "non esiste" per `demo-data`, presente e corretto. Stesso difetto già visto
una volta in `report_path_for_summary`; corretto con lo stesso schema (un parametro `anchor`).
Dettaglio completo nella riga F73 di `ROADMAP.md`.

### 18.5/18.9 Pattern ed Escludi file — suggerimenti

Nessun ostacolo tecnico: un `title=""` sul campo più una didascalia statica sotto, stesso linguaggio
visivo già in uso per `keep_generations`/Mirror. Pattern: valori comuni da mostrare come suggerimento
(`*` tutti i file, `*.pdf`, `*.jpg;*.png;*.gif`, per estensione singola/multipla). Escludi file:
pattern comuni da offrire come scorciatoie cliccabili oltre alla didascalia, non solo testo statico
— `*.tmp`, `*.log`, `Thumbs.db`, `desktop.ini`, `~$*` (file temporanei Office) — senza costringere a
digitarli, un click li aggiunge alla lista già presente (mai sostituisce quanto scritto a mano).

### 18.6/18.7 Thread e Tentativi — default e guida

Verificato in `cli.rs`: `--threads` di default è il conteggio di CPU logiche (`default_threads()`,
clampato a 1-128), `--retries` è `3`, `--retry-wait-seconds` è `5` — nessuno di questi default è
oggi visibile nel form (il campo è vuoto quando `draft.threads`/`draft.retries` sono `None`, che
significa "usa il default", ma un campo vuoto non comunica quale sia quel default). **Fattibile e a
costo quasi zero**: un `placeholder` che mostra il valore di default reale (letto una volta dal
comando `read_job_drafts` esistente, non serve un nuovo comando) invece di un campo che sembra
"niente" — coerente con come `report_path` dovrebbe comportarsi (§18.8 sotto). **Riserva sulla
richiesta di un `<select>` con preset per Thread**: il valore giusto dipende dal tipo di destinazione
(un NAS/condivisione SMB spesso *peggiora* con più thread, un disco locale SSD ne beneficia) — un
menu con preset del tipo "aggressivo/conservativo" darebbe una falsa sicurezza su un valore che la
GUI non può conoscere con certezza (non sa se la destinazione è locale o di rete). Proposta più onesta:
tenere il campo numerico libero, aggiungere una didascalia con la guida testuale che le note esistenti
già danno altrove (`scripts/benchmark-threads.ps1`, `RUNBOOK.md`) — "verificato empiricamente meglio"
resta una misura, non un default che la GUI può indovinare. Tentativi: stessa idea, placeholder con
`3` più una riga che spiega quando alzarlo (destinazioni di rete instabili) o abbassarlo (velocità
di fallimento su un errore reale, non transitorio).

### 18.8 Campo Report — non comunica il proprio default

`report_path` vuoto (`null`) è già il comportamento corretto — significa "usa il default del core"
(`./robocopy_ingest_report.json`, risolto contro la cartella del file di configurazione una volta
avviato) — ma un campo visivamente vuoto non lo dice. **Non riempirlo con un valore letterale**:
scrivere un valore esplicito nella proposta cambierebbe la semantica da "eredita/usa il default" a
"questo job impone questo percorso", una differenza reale che `pin()` distingue apposta. Il fix
corretto è solo di presentazione: un `placeholder` col percorso di default reale, il campo resta
vuoto finché l'operatore non digita qualcosa di suo.

**✅ Implementato e verificato 7 Set 2026**, esattamente come proposto: `placeholder` HTML col
percorso letterale (duplicato in JS, stesso schema di F72 per le liste di validazione), campo mai
riempito con un valore letterale. Dettaglio completo nella riga F76 di `ROADMAP.md`.

### 18.10 backup_type e le checkbox — nessun aiuto inline

Le spiegazioni **esistono già**, ma solo in Aiuto (`Help.svelte`, sezione "Termini che la console
usa": mirror, generazione/ciclo, verifica rapida) — un operatore che non ha mai aperto quella scheda
non le vede mai mentre compila il form. Fattibile a costo quasi zero: `title=""` su ciascun controllo
con lo stesso testo già scritto per Aiuto (non va riscritto da zero, va **riusato** — la stessa
disciplina di F71 verso `Run.svelte`), più una riga di didascalia per `backup_type` che dice in una
frase la differenza fra full/incremental/differential (oggi assente sia in Modifica che in Aiuto:
anche Aiuto non spiega i tre valori, solo il concetto generale di "generazione, ciclo").

### 18.11 Nessuna cartella d'esempio raggiungibile per chi ha installato il prodotto

**✅ Implementato e verificato 7 Set 2026** — esattamente coi vincoli descritti sotto: cartella
fissa (`Documenti\rustcopy-demo`), rifiuto atomico di sovrascrittura (`std::fs::create_dir`, non
`create_dir_all`), contenuto fisso e dichiarato, nessun parametro che lo generalizzi. Un modulo core
a sé (`example_workspace.rs`), non dentro `gui_api.rs` che è documentato read-only nel proprio
header — stessa ragione per cui `crypto.rs` (F56) è separato. Dettaglio completo: riga F79 di
`ROADMAP.md`.

**Prima di F79 (analisi originale — stato storico, superato dall'implementazione):**
Verificato in `installer/rustcopy.iss`, sezione `[Files]`: l'installer impacchetta **solo** i tre
eseguibili più `README.md`/`RUNBOOK.md`/`CLAUDE.md` (come `NOTES.md`) — `examples/` non compare da
nessuna parte (`grep -n "examples\|demo" installer/rustcopy.iss` non trova nulla). Questo significa
che **ogni** riferimento a `examples/demo-locale.toml` nel prodotto installato punta a un file che
non esiste su quella macchina:
- L'empty state di Job (`Jobs.svelte`): "Non hai un file? Prova examples/demo-locale.toml...".
- La prima voce di `Help.svelte`, sezione "Da dove si comincia": la identica indicazione, con
  istruzioni su come lanciarlo dalla CLI.

Per chiunque abbia scaricato l'installer (non un checkout del repository) questo è un vicolo cieco:
il prodotto stesso indica un punto di partenza che non può raggiungere. È la causa diretta
dell'osservazione dell'utente ("non si sa come iniziare"). **Fattibile**: un pulsante "Crea un
esempio in Documenti" che genera, in una cartella fissa e dedicata (`Documenti\rustcopy-demo\`, mai
la cartella Documenti stessa), una manciata di file finti di pochi byte più un TOML che li punta
l'uno all'altro — stesso spirito di `examples/demo-locale.toml` ma generato invece di distribuito.
**Categoria di azione nuova rispetto a tutto il resto della console**: ogni altra scrittura della
console (`write_proposal`, F56 credenziali, F71 QuickSync) scrive **un file che l'operatore ha
scelto**, mai contenuto arbitrario scelto dalla console stessa — questo pulsante creerebbe file *e*
cartelle il cui contenuto non è mai stato negoziato con l'operatore, per quanto minuscolo e innocuo.
Non è vietato da F61 (non cancella, non pianifica, non installa, non richiede privilegi), ma merita
di essere trattato come una decisione a sé, non incluso implicitamente in una riga di "migliorie
varie" — per questo resta proposto con vincoli espliciti nella riga F79 di `ROADMAP.md`, non deciso
qui: cartella fissa, mai sovrascrive (stesso `create_new` rifiuta-se-esiste di `write_proposal`),
contenuto fisso e dichiarato, nessun parametro che lo generalizzi.

### 18.12 Le altre schede hanno bisogno delle stesse migliorie?

Controllate `Settings.svelte`, `Run.svelte`, `Jobs.svelte`, `Report.svelte`/`History.svelte` (già
lette per intero in sessioni precedenti di questo documento) contro lo stesso criterio — un campo o
un concetto senza alcuna spiegazione raggiungibile senza aprire Aiuto. **Risultato per lo più
negativo, non per pigrizia della verifica**: `Settings.svelte` mostra già una didascalia (`caution`)
per ogni impostazione che ha una conseguenza, letta dal core (`gui_api::read_settings`), non
inventata lato frontend; `Run.svelte` ha già testo esplicativo sopra i pulsanti (fermare non uccide
il processo, spiegato inline) e l'empty state di Job già lo dichiara ("Non può accendere il mirror,
forzare un purge..."). Nessuna di queste schede ha campi di **immissione libera** paragonabili a
quelli di Modifica (Pattern, Thread, Escludi file...) — sono tutte lettura o, per Esegui, un solo
percorso già gestito da `PathBar`. **La vera eccezione è proprio `Jobs.svelte`**, per il motivo di
§18.11: la sua stessa didascalia rimanda a un file che l'installer non porta. Non trovata alcuna
seconda scheda con lo stesso genere di lacuna di Modifica — l'istinto dell'utente ("tutta la sezione
Modifica mi sembra da rivedere") era corretto nel puntare lì come area concentrata di intervento,
non generalizzabile automaticamente al resto della console.

### 18.13 Priorità proposta

In ordine di rapporto valore/rischio, non di apparizione nella lista originale:

1. **F79** — generatore di esempio in Documenti + correzione dei due rimandi rotti (`Jobs.svelte`,
   `Help.svelte`). Sblocca "come inizio" per chiunque non sia uno sviluppatore col repository
   clonato — il gap più bloccante di tutti quelli trovati. ✅ Completato 7 Set 2026.
2. **F80** — `encrypt_aes256` per job in `JobConfig`, raggiungibile da Modifica. Prima
   dell'implementazione la cifratura era irraggiungibile dalla GUI e dal TOML per qualunque job in
   un batch — non attrito, un'assenza totale. Priorità alta e richiedeva una decisione esplicita
   (§18.14) perché toccava il core (`JobConfig`), non solo la GUI, a differenza di ogni altra riga
   di questa lista — decisione presa e implementazione chiusa. ✅ Completato 7 Set 2026.
3. **F73** — verifica Sorgente/Destinazione (esistenza + conteggio). Il valore pratico più alto fra
   le richieste originali di solo-GUI, zero nuova logica di scansione da scrivere.
   ✅ Completato 7 Set 2026.
4. **F72** — validazione Nome. Piccolo, ma previene un errore che altrimenti si scopre solo ore
   dopo, alla prima esecuzione pianificata. ✅ Completato 7 Set 2026.
5. **F76** — placeholder Report col default reale. Costo quasi nullo. ✅ Completato 7 Set 2026.
6. **F74/F75/F77** — suggerimenti Pattern/Escludi file/Thread/Tentativi/backup_type/checkbox. Stesso
   tipo di intervento (didascalie/tooltip), raggruppabile in un solo giro di lavoro.
7. **F78** — messaggio comprensibile per l'errore di split job singolo. Non blocca nessun flusso
   esistente (l'errore compare solo tentando l'azione non supportata), ma chiude il punto di
   partenza di questa stessa analisi (§18.1).

### 18.14 Le credenziali di Impostazioni non sono raggiungibili da alcun job

Osservazione aggiuntiva dell'utente, arrivata dopo aver già letto la prima stesura di questa
sezione (§18.1-§18.13): la numerazione qui sotto continua da dove l'analisi era arrivata, non
riparte da zero. Verificato prima di rispondere, stesso metodo di ogni altro punto sopra.

Aprendo `job1` in Modifica non c'è alcuna correlazione visibile con le credenziali salvate in
Impostazioni ("Gestione credenziali", F56). aprendo `job1` in Modifica non c'è alcuna
correlazione visibile con le credenziali salvate in Impostazioni ("Gestione credenziali", F56).
Verificato il motivo esatto, non assunto: **`JobConfig` non ha affatto un campo `encrypt_aes256`
o `decrypt`** (i 34 campi della struct, contati in `config.rs`, non li includono — verificato
leggendone l'elenco completo). `--encrypt-aes256`/`--decrypt` esistono **solo** su `cli.rs::Args`,
popolati dalla riga di comando reale con cui il processo è stato invocato. Questo non è
un'omissione della sola GUI: è vero anche scrivendo il TOML a mano. Conseguenza pratica, verificata
in `main.rs`: `run_jobs` (la pipeline `[[jobs]]`) ricostruisce l'`Args` di ogni job da un clone
dell'invocazione CLI **originale**, e poiché `encrypt_aes256`/`decrypt` non sono in `JobConfig`,
`merged_over` non ha nulla da cui popolarli per job — restano quindi quelli dell'invocazione
originale, **identici per ogni job del batch**. Oggi non esiste alcun modo, né da GUI né a mano nel
TOML, di cifrare `job1` con una chiave e `job2` con un'altra nello stesso file: la cifratura è
un'impostazione dell'intera invocazione, non del singolo job. Una credenziale salvata in
Impostazioni è quindi raggiungibile **solo** digitando `--encrypt-aes256 keyring:NOME` sulla riga
di comando ad ogni lancio — mai da un campo di Modifica, perché quel campo non esiste in nessuna
scheda della console oggi.

**Da non confondere con un gap già noto e già deciso**: `webhook_url`/`pre_command`/`post_command`
**sono** già in `JobConfig` (per job, non condivisi) ma volutamente esclusi da `JobDraft` — F55,
metà scrittura, decisione ancora aperta per il rischio di iniezione di comandi (§2.3, vincolo
permanente 2). Questo è un caso diverso e più a monte: qui il campo non esiste proprio nel formato
TOML, non è solo escluso dal form.

**Fattibilità**: richiede una modifica reale del core, non solo della GUI — a differenza di F72-F79
sopra. Servirebbe: (1) aggiungere `encrypt_aes256: Option<String>` a `JobConfig` (probabilmente
**non** `decrypt`: quel flag è concepito per accompagnare `--restore-from`, un'operazione singola e
deliberata, non una voce di routine in un batch `[[jobs]]` — estenderlo per-job aggiungerebbe
complessità senza un caso d'uso chiaro, a differenza di "cifra il backup di questo job con questa
chiave" che è un bisogno reale e ricorrente); (2) wiring in `merged_over`/`apply_draft`, stesso
schema di `backup_type` (F70); (3) in `run_jobs`, usare il valore effettivo **del job**, non più
quello condiviso dell'invocazione, per la chiamata a `encrypt_destination`; (4) validazione
equivalente a `Args::validate()`'s `EncryptAndDecryptConflict`, per job invece che per invocazione.
**Vincolo di sicurezza da preservare nel disegno della UI**: il campo in Modifica dovrebbe accettare
**solo** la forma `keyring:NOME` (mai una chiave letterale) — scrivere una chiave in chiaro nel TOML
vanificherebbe l'intero scopo di F56, che esiste apposta perché una chiave letterale è visibile
nella process list e ora anche in un file su disco. Non è una decisione da prendere implicitamente
insieme alle altre migliorie: cambia la forma di `JobConfig`, tocca `run_jobs`, e introduce la prima
vera dipendenza visibile fra Impostazioni e Modifica — merita una conferma esplicita a sé, come F79.

**✅ Implementato e verificato 7 Set 2026.** Punto (3) sopra si è rivelato non necessario: `run_jobs`
già passa per `Args::apply_job_config`, lo stesso punto unico condiviso dalla pipeline a singolo job
— una volta che `encrypt_aes256` è in `JobConfig`, quella funzione lo copia già sull'`Args` per-job
corretto, senza bisogno di toccare `run_jobs` stesso. Stesso discorso per (4): `job_args.validate()`
è già chiamato per ogni job (non solo una volta per l'intera invocazione), quindi
`EncryptAndDecryptConflict` si applica già per job senza una nuova regola. Il vincolo di sicurezza
sulla UI è rispettato: `Editor.svelte` scrive solo `keyring:NOME`, un valore già impostato a mano in
una delle altre tre forme resta in sola lettura e sopravvive intatto a una modifica non correlata —
verificato dal vivo contro il binario ricompilato in entrambi gli scenari. Dettaglio completo e i due
nuovi unit test nella riga F80 di `ROADMAP.md`.

### 18.15 Audit completo delle sette schede (richiesto dall'utente dopo §18.1-§18.14)

Rilette per intero, riga per riga, con la lente più ampia di "qualunque criticità", non solo la
lente originale "manca un aiuto inline": `Jobs.svelte`, `Settings.svelte`, `Run.svelte`,
`Report.svelte`, `History.svelte`, `PathBar.svelte` (condiviso da tutte le schede con un percorso).
Due criticità reali trovate, entrambe verificate leggendo il codice, non ipotizzate:

**`History.svelte` colora di rosso un codice di uscita che il suo stesso commento dichiara non
essere un errore.** Riga 63-64: *"Exit codes are a contract with schedulers... so the console
shows what each one means rather than colouring non-zero red. A 4 is not a failed copy."* Riga
230-232, poche righe sotto lo stesso commento: `run.exit_code === 0 ? 'emerald' : 'red'` — **ogni**
codice diverso da zero, incluso il 4 (copiato ma verifica fallita, l'esempio che il commento cita
esplicitamente), diventa rosso, la stessa colorazione di un fallimento totale. Confronto diretto con
`Run.svelte` (riga 386-388): lì lo stesso schema usa **amber**, non rosso, per "diverso da zero" —
`History.svelte` non è coerente nemmeno con la convenzione che il resto della console già segue.
Non un'osservazione stilistica: un operatore che scorre lo storico e vede una riga rossa la legge
come "questo backup non è andato bene", quando l'esito 4 significa "i dati sono arrivati, solo la
verifica ha trovato una differenza" — l'esatta distinzione che questo progetto ripete più volte
essere la ragione per cui l'exit code 4 esiste (`Help.svelte`, la tabella degli esiti). **Legata a un
debito già tracciato**: `EXIT_MEANING` (riga 65-73 dello stesso file) è una seconda copia hardcoded
della mappa exit-code→significato che vive nel core (`runner::exit_code_meaning`), già segnalata in
`CLAUDE.md` ("va tenuta manualmente sincronizzata ad ogni nuovo exit code finché non viene sostituita
con una vera chiamata... esposta via `gui_api`") ma mai risolta. Le due criticità condividono la
stessa causa (questo file reinventa localmente ciò che il core già sa) e la stessa correzione:
esporre `runner::exit_code_meaning` a un nuovo comando Tauri, usarlo al posto della mappa locale, e
allineare la colorazione alla convenzione già stabilita da `Run.svelte` (amber per "diverso da zero
ma non necessariamente un fallimento", non rosso).

**✅ Implementato e verificato 8 Set 2026**, esattamente come proposto: nuovo comando Tauri
`exit_code_meaning`, `History.svelte` lo chiama per ogni codice distinto nella cronologia appena
caricata invece di tenere una copia locale, colorazione allineata all'amber di `Run.svelte`.
Dettaglio completo nella riga F81 di `ROADMAP.md`.

**`Report.svelte`: "Anteprima ripristino" può fallire senza spiegazione se non è mai stato aperto un
config in questa sessione.** Verificato in `previewRestore()` (riga 33-51): passa `session.
configPath` al comando `preview_restore` — e per D26 (già corretto, riga corrispondente in
`ROADMAP.md`) la cartella di quel config è la cwd che rende leggibili i percorsi relativi di un
report. Se l'operatore ha aperto Report direttamente (mai toccato Job/Esegui/Modifica in questa
sessione), `session.configPath` è vuoto — l'anteprima parte comunque, senza cwd, e su un report a
percorsi relativi fallisce con lo stesso "fatal error, no files copied" che D26 aveva già trovato,
stavolta non per un bug del codice ma per l'assenza silenziosa del prerequisito. Nessun avviso nella
scheda lo dice prima del click. Fattibile a basso costo: un avviso quando `session.configPath` è
vuoto ("l'anteprima potrebbe fallire su percorsi relativi: apri prima il file di configurazione di
questa run in un'altra scheda"), o mostrare quale config verrà usato quando non è vuoto — coerente
con quanto già fa D26 stesso, solo reso visibile prima del click invece che scoperto dopo.

**Nessun'altra criticità di rilievo trovata** in `Jobs.svelte`, `Settings.svelte`, `PathBar.svelte`:
`PathBar.svelte` in particolare è il componente più curato dell'intera console (drag&drop, chiusura
al click esterno, tasto Escape, collasso dei margini già risolto — §16.1/F68) e non ha mostrato
alcuna lacuna nuova alla rilettura. `Settings.svelte` ha già una didascalia per ogni impostazione
che porta una conseguenza, letta dal core. Coerente con quanto già concluso in §18.12: Modifica resta
l'area con la concentrazione più alta di criticità reali, non l'unica, ma le uniche altre due trovate
in questo giro (`History.svelte`, `Report.svelte`) sono comunque puntuali e isolate, non sistemiche
come in Modifica.

### 18.16 Priorità aggiornata con F81/F82

Entrambe le nuove voci sono isolate e a basso rischio — non richiedono la stessa cautela di F80
(che tocca `JobConfig`/`run_jobs`). Inserite nella sequenza già proposta in §18.13:

- **F81** (colorazione/`EXIT_MEANING` di `History.svelte`) — stesso ordine di grandezza di F76,
  subito dopo. ✅ Completato 8 Set 2026.
- **F82** (avviso "Anteprima ripristino" senza config) — stesso ordine di grandezza di F78, in coda
  al gruppo di didascalie/messaggi.

Ordine complessivo aggiornato: F79 → F80 → F73 → F72 → F76 → **F81** → F74/F75/F77 → F78 → **F82**.

### 18.17 Criticità trovate rileggendo questa stessa sezione

Stesso metodo di §14.4/§16.3/§17.5, applicato prima di presentare l'analisi. Quattro correzioni,
le prime due dal primo giro (§18.1-§18.14), le ultime due dal giro di audit completo (§18.15-18.16):

- La prima stesura di §18.6 proponeva senza riserve il `<select>` con preset di thread richiesto
  dall'utente. Rileggendo insieme alla nota già scritta altrove in questo stesso documento sul
  throughput SMB/NAS (§1, `RUNBOOK.md`), un preset fisso avrebbe dato una falsa certezza su un
  valore che dipende dal tipo di destinazione, non deducibile lato GUI — corretto in una
  raccomandazione esplicita di **non** implementarlo come richiesto alla lettera, con la
  motivazione, lasciando la decisione finale all'utente invece di implementare silenziosamente
  quanto sembrava più comodo.
- La prima stesura di §18.11 non specificava alcun vincolo per il generatore di esempio, trattandolo
  come un'estensione naturale di F71 (QuickSync). Riletta contro la disciplina F61/F54 di questo
  stesso documento ("l'editor scrive solo ciò che l'operatore ha scelto"), è una categoria diversa
  — genera contenuto, non solo un file di configurazione — e va proposta con vincoli espliciti
  (cartella fissa, mai sovrascrive, contenuto dichiarato) invece che come una riga in più nella
  stessa lista di migliorie a basso rischio.
- **Errore di processo, non di contenuto**: la prima stesura di §18.15/§18.16 (l'audit completo
  richiesto dall'utente) è stata scritta fisicamente **prima** di §18.15 già esistente (la sezione
  "Criticità" di questo stesso paragrafo), risultando in un file con `### 18.16` e `### 18.17` che
  comparivano prima di `### 18.15` nell'ordine fisico — lo stesso identico errore già commesso e
  corretto scrivendo F68 in questa stessa sessione (vedi la nota corrispondente in `CLAUDE.md`).
  Corretto rinumerando in ordine fisico prima di presentare l'analisi, non dopo che qualcuno lo
  notasse. Non un errore che si autocorregge da solo: va controllato esplicitamente ogni volta che
  un inserimento avviene "prima di" una sezione esistente invece che in coda al documento.
- **Limite dichiarato della proposta F81**: allineare `History.svelte` alla convenzione amber/
  emerald di `Run.svelte` è una correzione di **coerenza interna**, non un'affermazione che quello
  schema binario (0 contro diverso-da-zero) sia il migliore possibile — `Run.svelte` stesso non
  distingue un 2 (errore d'uso) da un 4 (verifica fallita), li tratta entrambi come "attenzione".
  Un'eventuale colorazione per-codice più fine (rosso solo per 1/2, amber per 3/4/5/6) sarebbe una
  proposta diversa e più ampia, non fatta qui: F81 chiude solo la contraddizione fra il commento e
  il codice nello stesso file, non ridisegna la semantica dei colori per l'intera console.

## Riferimenti

- [`CLAUDE.md`](CLAUDE.md) — regole operative per `runner.rs`, `job_editor.rs`, `gui_api.rs`.
- [`ROADMAP.md`](ROADMAP.md) — righe F53-F60 (console), F49/F51/F46 (backlog TeraCopy-parity),
  milestone 8.0.0 (motore pilotabile, condizionale).
- `docs/archive/PIANO_GUI_TAURI.md` — il piano pre-implementazione
  che ha portato alla console attuale, archiviato perché eseguito per intero (§2 sopra ne riassume
  l'esito).
- [`ANALYSIS.md`](ANALYSIS.md) — D1-D27, per il tipo di difetto che questo progetto trova più spesso
  (nel meccanismo di sicurezza attorno alla funzione, non nella funzione stessa) — la stessa cautela
  vale per ogni voce dell'Onda 3, e D26 (§13a sopra) è l'esempio più recente di quanto costi non
  verificare un'anteprima contro un caso con percorsi relativi, non solo assoluti — e di quanto costi
  fidarsi di un primo tentativo di fix senza riverificarlo dal vivo.
