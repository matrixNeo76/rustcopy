---
type: Reference
title: Piano di ampliamento della console rustcopy
description: Inventario di ciò che la console (F52-F60) espone oggi rispetto alla CLI, analisi delle lacune per categoria, un piano prioritizzato in tre onde funzionali, e un audit visivo/di usabilità con un piano di rifacimento a tre livelli.
status: draft
generated:
  by: process:claude-code
  at: 2026-09-03T14:12:16Z
verified:
  by: process:claude-code
  at: 2026-09-04T08:00:00Z
---

# Piano di ampliamento della console rustcopy

La milestone 7.0.0 (la console) ha chiuso sette voci su otto — F52/F53/F54/F55-lettura/F56/F59/F60 —
e ha lasciato scritte due decisioni deliberatamente aperte: la metà in scrittura di F55 (script
pre/post) e F57 (ruoli), quest'ultima non raccomandata. Questo documento non riparte da zero: legge
cosa la console fa oggi, la confronta campo per campo con quello che la CLI sa fare, e organizza la
distanza fra le due in un piano. Non riapre nessuna decisione già presa — le richiama e basta, dove
sono rilevanti — e non propone nulla che allarghi il vincolo strutturale di `runner.rs`: **la console
può riferire un'operazione distruttiva, mai autorizzarla** (§6 sotto per il dettaglio).

**Aggiornamento 4 Set 2026**: dopo che l'Onda 1 e l'Onda 2 sono state spedite (§7), l'utente ha
segnalato che la console resta "poco intuitiva" e "con una UI eccessivamente spartana" — un giudizio
sulla *forma*, non sulla copertura funzionale che §2-§7 misurano. §9-§10 sono un secondo audit,
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

## 2. Cosa la console fa oggi

| Scheda | Comandi Tauri | Cosa fa | Scrive? |
|---|---|---|---|
| **Job** | `list_jobs` | Elenca i job di un TOML: sorgente, destinazione, tipo di backup, se verifica/mirror sono attivi. | No |
| **Impostazioni** | `read_settings`, `set_credential`, `delete_credential` | Ogni job risolto (vista merged), raggruppato, con provenienza per campo e avviso sulle impostazioni che hanno una conseguenza (mirror cancella, ecc.). `webhook_url` troncato a schema+host; `pre_command`/`post_command` verbatim. **Onda 2**: sezione credenziali (F56) — salva/elimina un segreto nel Windows Credential Manager, il segreto viaggia solo su IPC. | Sì (solo credenziali, eccezione dichiarata) |
| **Modifica** | `read_job_drafts`, `suggest_proposal_path`, `write_proposal` | Crea o modifica job (26 dei 29 campi di `JobConfig`) e scrive **sempre un file nuovo**, mai quello in uso. Rifiuta di allargare il rischio (mirror spento→acceso, `keep_generations` abbassato, ecc. — regola in `job_editor.rs`). | Sì, ma solo su un file di proposta |
| **Esegui** | `list_jobs`, `run_status`, `start_job`, `stop_job`, `schedules_referencing` | Avvia la stessa CLI come processo separato con `runner::run_arguments` (forma fissa: solo `--config`/`--cancel-file`/`--progress-file`), mostra fase/percentuale/output catturato, ferma scrivendo il file che la run sorveglia. **Onda 2**: coda job in sola lettura durante un batch (F49) e badge se una pianificazione punta già a questo file. Notifica di sistema a fine run. Una run alla volta per finestra. | Esegue (via CLI esterna), non scrive configurazione |
| **Report** | `read_report_page` | Report JSON paginato: riepilogo, mismatch, errori. | No |
| **Storico** | `read_history`, `read_advice` | Run passate con il significato dell'exit code, più l'analisi deterministica di `--advise`. | No |
| **Aiuto** | — | Contenuto statico. | No |

## 3. Copertura CLI → GUI, per categoria

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
| Job multipli | `[[jobs]]` in un TOML | ✅ (elenco) | ✅ | ✅ (posizione, Onda 2) | Vedi §4d — la posizione nel batch si vede (F49), l'esito per-job resta nel Report/Storico di quella run |

## 4. Lacune, per categoria

### a) Editor — script e notifiche (F55, scrittura)

`webhook_url`, `pre_command`, `post_command` sono già letti e round-trippati intatti da un edit (un
campo che il form non disegna torna com'era — regola di `job_editor.rs`), ma non sono **scrivibili**.
Questa non è una svista: ROADMAP la lascia esplicitamente non decisa perché morde l'avviso di
sicurezza 2 — *script configurabili più servizio privilegiato uguale escalation locale* (rilevante
da quando F37 rende `rustcopy_ingest` installabile come servizio Windows). Un campo di testo libero
che poi esegue con i privilegi del servizio è una superficie diversa da un mirror che si ferma da
solo con l'exit 3. **Non la riapro come già decisa** — la porto in §6 come Onda 3, con la stessa
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

## 5. Cosa resta fuori da questo piano, e perché

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

## 6. Il vincolo che ogni onda deve rispettare

Ogni proposta sotto passa lo stesso test che `runner.rs` già impone e verifica con un test
(`the_argument_list_cannot_carry_a_destructive_flag`): la console può **eseguire** solo attraverso la
stessa CLI, con `run_arguments` a forma fissa, e può **riferire** un'operazione distruttiva ma mai
**autorizzarla** in modo non presidiato. Nessuna proposta qui sotto tocca `run_arguments`, aggiunge un
parametro che inoltra flag, o esegue codice di backup dentro il processo della console invece che
dentro la CLI.

## 7. Piano prioritizzato

### Onda 1 — Rischio basso, nessuna nuova superficie di scrittura

Tutte lettura pura o piccole aggiunte al formato interno del progresso; nessuna tocca `job_editor.rs`
né `runner.rs`.

1. **Notifica di sistema a fine run** — `run_status` già sa quando una run finisce; basta collegare
   `tauri-plugin-notification` (nuova dipendenza, nessun nuovo permesso pericoloso) al punto in cui
   `RunStatus.running` passa a `false`.
2. **Etichetta "job N di M" durante un batch** — richiede un campo in più in `ProgressSample`
   (`job_index`/`job_name`, `Option` come gli altri totali) scritto da `run_jobs` in `main.rs` (CLI),
   letto e mostrato da `Run.svelte`. Piccola estensione additiva, non tocca il formato per chi non
   fa batch (resta `None`).
3. **Filtro e ricerca in Report e Storico** — puramente client-side sui dati già restituiti da
   `read_report_page`/`read_history`, nessun nuovo comando Tauri.
4. **Esportazione CSV** di report e storico — trasformazione lato frontend dei dati già in memoria.
5. **Trascina-e-rilascia un TOML sulla finestra** per popolare `PathBar` — evento nativo di Tauri,
   nessuna nuova superficie verso il core.
6. **Badge di sola lettura** "pianificazione/servizio installati per questo job" — un nuovo comando
   `gui_api` che esegue `schtasks /Query`/`sc query` e basta, mai un'azione.

### Onda 2 — Valore medio, nuova superficie ma dentro i limiti esistenti

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
   `JobConfig` lato core (non lavoro di frontend, vedi §4f). Fino ad allora questo punto resta bloccato.

### Onda 3 — Valore alto, richiede una decisione esplicita prima di progettare

10. **Flusso di ripristino guidato (`--restore-from`)** — la lacuna più sentita, ma `--restore-from`
    inverte sorgente e destinazione e può sovrascrivere dati: prima di un solo pixel di disegno serve
    la stessa disciplina che ha retto il resto della console — un meccanismo di conferma esplicita
    pari a quello del mirror (l'esecuzione non presidiata continua ad autofermarsi da sola; la console
    può proporre il ripristino, non forzarlo). Proposta di massima: elenco dei report disponibili →
    anteprima di cosa verrebbe ripristinato (letta, non eseguita) → conferma esplicita → avvio via
    `start_job` con un `--config` derivato, mai un flag forzato in `run_arguments`.
11. **Ripresa da checkpoint (`--resume-from`)** — stesso pattern del punto 10 ma rischio minore
    (non inverte sorgente/destinazione): elenco dei checkpoint `.checkpoint.json` trovati accanto ai
    report, avvio della stessa CLI con `--resume-from` risolto dal core (`checkpoint::build_resume_args`,
    già esistente), mai costruito lato frontend.
12. **Scrittura di `webhook_url`/`pre_command`/`post_command` in Modifica (F55, metà in scrittura)** —
    **non la marco pronta**: la porto qui esattamente perché ROADMAP la lascia esplicitamente non
    decisa, con la ragione già scritta (avviso 2, script + servizio privilegiato = escalation locale).
    Se e quando si decide di procedere, la forma più cauta è probabilmente: mostrare uno stato
    "invariato dal file" per il campo finché non viene toccato (coerente con "l'omissione non cancella
    mai"), e non permettere di *aggiungere* un hook a un job che non ne aveva uno senza un avviso
    esplicito a schermo — ma questa è una discussione di sicurezza da avere prima, non un dettaglio
    di implementazione.

## 8. Come leggere le tre onde

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

Diviso per rischio/sforzo come le tre onde funzionali di §7, ma ortogonale ad esse: nessuna di queste
voci tocca `runner.rs`, `job_editor.rs` o il confine F61 — sono tutte CSS/markup/organizzazione dei
contenuti Svelte esistenti, non nuova superficie verso il core.

### Livello 1 — correzioni puntuali, nessun rischio, poche righe ciascuna

1. **Contenere e centrare il contenuto** entro una larghezza massima leggibile (es. un contenitore
   `max-w-6xl` centrato sul `<main>`), invece di lasciarlo ancorato in alto a sinistra su qualunque
   larghezza di finestra. Risolve (a) alla radice con poche righe di CSS, senza toccare nessuna
   scheda individualmente.
2. **Colonne di tabella esplicite** (Job, Storico) invece di lasciare il motore di rendering
   distribuire lo spazio in eccesso sulla prima colonna. Risolve (d).
3. **Avvicinare il badge di provenienza** in Impostazioni al valore a cui appartiene — sulla stessa
   riga del nome del campo invece che in una colonna a distanza fissa dal bordo destro. Risolve (e).
4. **Tradurre `exit_code_meaning`/`integrity_status` in Report** con la stessa tabella `EXIT_MEANING`
   già presente in `History.svelte` — da estrarre in un modulo condiviso, per non tenere due copie
   che possono divergere. Risolve (f).
5. **"Apri il report di questa run"** — un collegamento in Esegui che appare a run conclusa e
   naviga a Report/Storico con `session.reportPath` già impostato dal `report_path` del job appena
   eseguito. Risolve (g). Verificare prima se il percorso serve un piccolo campo aggiuntivo su
   `RunStatus`/`gui_api`, o se è già derivabile da `jobs` (letto da `list_jobs`) senza toccare il core.

### Livello 2 — un sistema di design minimo, tocca ogni scheda ma senza logica nuova

6. **Una vera scala tipografica** (2-3 dimensioni oltre l'attuale 11-12px onnipresente) applicata in
   modo coerente fra titoli di sezione, etichette e valori.
7. **Card per ogni gruppo di contenuto imparentato** (bordo leggero + sfondo distinto da
   `bg-slate-50`) al posto di elenchi/tabelle che fluttuano sullo sfondo — tabella Job, griglia
   statistiche Report, gruppi di Impostazioni. Risolve (c).
8. **Un set minimo di icone** (valutare peso di una libreria SVG contro icone inline scritte a mano —
   la toolchain JS è deliberatamente leggera, 52 pacchetti/0 vulnerabilità, criterio già scritto in
   `CLAUDE.md`) per: le 7 voci di navigazione, gli stati di run (riuscito/fallito/in corso), il badge
   mirror. Non decorazione: è la differenza principale fra "pannello operativo" e "modulo di testo",
   e la causa singola più citata nel giudizio "spartana". Risolve (b) insieme al punto 6.
9. **Larghezza dei campi dell'editor proporzionata al contenuto atteso** (nome/pattern corti restano
   corti, percorsi restano larghi) invece di `w-full` uniforme dentro la griglia a due colonne.
   Completa (a) sulla scheda Modifica.

### Livello 3 — struttura, richiede una decisione di design prima di cominciare

10. **Navigazione a barra laterale verticale** (con le icone del punto 8) al posto della riga di
    pulsanti testuali in testa — libera la testata per il contesto del file attivo (nome, tipo,
    ultimo esito) invece di ripetere la stessa frase statica identica su ogni scheda.
11. **Dimensione di apertura della finestra**: oggi apre piccola (~1080×615), costringendo a
    massimizzare manualmente ogni volta. Da decidere fra ricordare l'ultima dimensione (richiede un
    salvataggio locale lato Tauri, superficie nuova per quanto piccola) o semplicemente aprire più
    grande di default (nessuna superficie nuova, ma non risolve chi la ridimensiona comunque).
12. **Empty state con un'ancora visiva** oltre al solo paragrafo di testo — oggi ogni scheda vuota è
    tre righe di prosa dentro un riquadro tratteggiato (già meglio di niente, vedi il commento in
    `EmptyState.svelte`), ma resta un muro di testo come primo contatto con ciascuna scheda.

### Cosa resta fuori da questo rifacimento

Nessuna di queste voci riapre le scelte già motivate altrove: la sola-lettura salvo le due eccezioni
dichiarate (Modifica, Credenziali), il vincolo di `runner.rs`, la densità come principio — l'obiettivo
è renderla leggibile, non trasformarla in un'app consumer. Nessuna introduce una dipendenza pesante:
qualunque libreria scelta per il punto 8 va verificata contro lo stesso criterio che ha scelto Svelte
su React+shadcn (`CLAUDE.md`) — pacchetti e vulnerabilità aggiunte, non solo funzionalità offerta.

## Riferimenti

- [`CLAUDE.md`](CLAUDE.md) — regole operative per `runner.rs`, `job_editor.rs`, `gui_api.rs`.
- [`ROADMAP.md`](ROADMAP.md) — righe F53-F60 (console), F49/F51/F46 (backlog TeraCopy-parity),
  milestone 8.0.0 (motore pilotabile, condizionale).
- [`PIANO_GUI_TAURI.md`](PIANO_GUI_TAURI.md) — il piano operativo che ha portato alla console attuale.
- [`ANALYSIS.md`](ANALYSIS.md) — D1-D24, per il tipo di difetto che questo progetto trova più spesso
  (nel meccanismo di sicurezza attorno alla funzione, non nella funzione stessa) — la stessa cautela
  vale per ogni voce dell'Onda 3, e D24 è l'esempio più recente di un difetto trovato mentre si
  cercava altro.
