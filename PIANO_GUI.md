---
type: Reference
title: Piano della console rustcopy
description: Documento unico e vivo per la console (F52-F60) — consolida il piano pre-implementazione (stack, ambito, distribuzione, vincoli permanenti) con l'inventario di ciò che espone oggi rispetto alla CLI, le lacune funzionali con un piano in tre onde, un audit visivo/di usabilità con un piano di rifacimento a tre livelli (chiuso), e una valutazione di una metodologia a workspace più cinque funzionalità CLI non ancora costruite. Sostituisce PIANO_GUI_TAURI.md (archiviato) e PIANO_GUI_ESPANSIONE.md (questo stesso file, rinominato).
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
toccare in alcun modo prestazioni, robustezza o potenza del motore. Non è ancora implementato: resta
una proposta.

**Effetto collaterale di questa analisi**: vale la pena che l'utente sappia che il repository ha
oggi due sistemi di "profili" paralleli e disconnessi (PowerShell e, potenzialmente domani, i
preferiti GUI proposti sopra andrebbero costruiti come *etichette su percorsi `[[jobs]]` esistenti*,
mai come un terzo formato equivalente a `profiles.json`) — non una richiesta di azione immediata,
ma una cosa da tenere a mente la prossima volta che si tocca l'uno o l'altro.

### 12.2 Funzionalità CLI non ancora valutate, utilizzabili anche in GUI

Cinque idee, ciascuna verificata contro il codice reale — non proposte a vuoto. Le prime tre
riusano quasi per intero logica **già scritta e testata**, solo mai esposta con questo scopo.

1. **`--list-schedules`** — lacuna già dichiarata (`CLAUDE.md`, riga F36: "Known gap: no
   `--list-schedules`") ma mai colmata. `schedule::referencing_config`
   (`crates/rustcopy-core/src/schedule.rs`) interroga già `schtasks.exe /Query /FO CSV /V` e
   filtra le attività il cui comando cita un `config_path` specifico — usata oggi solo dalla GUI
   (`gui_api::schedules_referencing`, il badge "un'attività punta già qui" in Esegui). Una
   variante che filtra invece sul percorso del **binario** (ogni attività che invoca
   `robocopy_ingest.exe`, non solo quelle che citano un file preciso) chiuderebbe la lacuna CLI
   riusando lo stesso motore di parsing CSV già coperto da test contro output reale catturato. La
   GUI guadagnerebbe un vero elenco al posto del semplice badge booleano di oggi.
2. **Anteprima di un mirror/purge, di sola lettura** — `check_mirror_safety`
   (`crates/rustcopy-cli/src/main.rs`) **calcola già** l'elenco esatto (`extraneous: Vec<&Path>`)
   dei file che `--mirror` cancellerebbe, ma oggi lo tronca a 5 voci e lo stampa solo su
   `stderr` quando sta per abortire in modo interattivo. L'avviso mirror della console
   (`Run.svelte`) dice letteralmente "eseguila dalla CLI, dove la conferma mostra quali file
   verrebbero eliminati" — un'opzione che scrive la lista **intera**, strutturata (JSON), senza
   mai eseguire il purge, chiuderebbe quella frase con un pulsante invece che con un rimando alla
   riga di comando. Il vincolo F61 resta intatto: leggere un elenco non è autorizzare una
   cancellazione, stessa distinzione già usata per il badge di pianificazione.
3. **Anteprima di ripristino** — `--restore-from` e `--dry-run` non risultano in conflitto in
   `cli.rs` (nessun `conflicts_with` fra i due), quindi la combinazione **probabilmente** già
   funziona oggi — non verificato con un'esecuzione reale in questa sessione, va confermato prima
   di costruirci sopra. Se confermato, è il primo mattone naturale per il flusso di ripristino
   guidato già in cima al backlog (§5b, §8 Onda 3): elenco report → anteprima (questo comando) →
   conferma esplicita → avvio.
4. **Controllo preventivo di spazio libero in destinazione** — verificato: **non esiste in
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

**Nessuna di queste cinque voci è stata implementata in questa sessione**: sono proposte, verificate
contro il codice reale dove possibile, in attesa di una decisione sulla priorità — stesso metodo
già usato per l'Onda 3 (proporre con `AskUserQuestion` prima di scrivere codice).

## Riferimenti

- [`CLAUDE.md`](CLAUDE.md) — regole operative per `runner.rs`, `job_editor.rs`, `gui_api.rs`.
- [`ROADMAP.md`](ROADMAP.md) — righe F53-F60 (console), F49/F51/F46 (backlog TeraCopy-parity),
  milestone 8.0.0 (motore pilotabile, condizionale).
- `docs/archive/PIANO_GUI_TAURI.md` — il piano pre-implementazione
  che ha portato alla console attuale, archiviato perché eseguito per intero (§2 sopra ne riassume
  l'esito).
- [`ANALYSIS.md`](ANALYSIS.md) — D1-D25, per il tipo di difetto che questo progetto trova più spesso
  (nel meccanismo di sicurezza attorno alla funzione, non nella funzione stessa) — la stessa cautela
  vale per ogni voce dell'Onda 3, e D24 è l'esempio più recente di un difetto trovato mentre si
  cercava altro.
