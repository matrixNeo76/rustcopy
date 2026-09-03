---
type: Reference
title: Piano di ampliamento della console rustcopy
description: Inventario di ciò che la console (F52-F60) espone oggi rispetto alla CLI, analisi delle lacune per categoria, e un piano prioritizzato in tre onde per ampliarla senza allargare il confine di sicurezza F53/F54.
status: draft
generated:
  by: process:claude-code
  at: 2026-09-03T14:12:16Z
---

# Piano di ampliamento della console rustcopy

La milestone 7.0.0 (la console) ha chiuso sette voci su otto — F52/F53/F54/F55-lettura/F56/F59/F60 —
e ha lasciato scritte due decisioni deliberatamente aperte: la metà in scrittura di F55 (script
pre/post) e F57 (ruoli), quest'ultima non raccomandata. Questo documento non riparte da zero: legge
cosa la console fa oggi, la confronta campo per campo con quello che la CLI sa fare, e organizza la
distanza fra le due in un piano. Non riapre nessuna decisione già presa — le richiama e basta, dove
sono rilevanti — e non propone nulla che allarghi il vincolo strutturale di `runner.rs`: **la console
può riferire un'operazione distruttiva, mai autorizzarla** (§6 sotto per il dettaglio).

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
| **Impostazioni** | `read_settings` | Ogni job risolto (vista merged), raggruppato, con provenienza per campo e avviso sulle impostazioni che hanno una conseguenza (mirror cancella, ecc.). `webhook_url` troncato a schema+host; `pre_command`/`post_command` verbatim. | No |
| **Modifica** | `read_job_drafts`, `suggest_proposal_path`, `write_proposal` | Crea o modifica job (26 dei 29 campi di `JobConfig`) e scrive **sempre un file nuovo**, mai quello in uso. Rifiuta di allargare il rischio (mirror spento→acceso, `keep_generations` abbassato, ecc. — regola in `job_editor.rs`). | Sì, ma solo su un file di proposta |
| **Esegui** | `list_jobs`, `run_status`, `start_job`, `stop_job` | Avvia la stessa CLI come processo separato con `runner::run_arguments` (forma fissa: solo `--config`/`--cancel-file`/`--progress-file`), mostra fase/percentuale/output catturato, ferma scrivendo il file che la run sorveglia. Una run alla volta per finestra. | Esegue (via CLI esterna), non scrive configurazione |
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
| Credenziali | `--set-credential/--delete-credential` | ❌ | ❌ | ❌ | Solo CLI, `resolve_key` non passa da qui |
| VSS | `--vss-snapshot` | ❌ | ❌ | ❌ | Non è nemmeno un campo di `JobConfig` |
| Ripristino | `--restore-from` | ❌ | ❌ | ❌ | **Nessun flusso in GUI** |
| Ripresa | `--resume-from` | ❌ | ❌ | ❌ | Nessun flusso in GUI |
| Automazione | `--install/uninstall-schedule/-service` | ❌ | ❌ | ❌ | Vietato di proposito (vincolo `runner.rs`) |
| Mirror non presidiato | `--force-purge` | ❌ | ❌ | ❌ | Vietato di proposito |
| Job multipli | `[[jobs]]` in un TOML | ✅ (elenco) | ✅ | Parziale | Vedi §4d — nessuna vista per-job durante l'esecuzione |

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

7. **Vista "coda job" (F49)** — elenco dei job di un batch con stato individuale (in attesa /
   in corso / riuscito / fallito), letto dagli stessi file che la CLI già scrive (report per job,
   namespacizzati da F33/D12), aggiornato mentre `start_job` esegue il batch. Nessuna esecuzione
   diretta nel processo della console: resta un pannello di lettura sopra la stessa run.
8. **UI per `--set-credential`/`--delete-credential`** — un campo mascherato che invoca un comando
   dedicato il quale, come oggi la CLI, legge il segreto e lo passa a `keyring` **senza** farlo
   transitare per un argomento di processo. Chiude la lacuna (c) per la parte già coperta da F56.
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

## Riferimenti

- [`CLAUDE.md`](CLAUDE.md) — regole operative per `runner.rs`, `job_editor.rs`, `gui_api.rs`.
- [`ROADMAP.md`](ROADMAP.md) — righe F53-F60 (console), F49/F51/F46 (backlog TeraCopy-parity),
  milestone 8.0.0 (motore pilotabile, condizionale).
- [`PIANO_GUI_TAURI.md`](PIANO_GUI_TAURI.md) — il piano operativo che ha portato alla console attuale.
- [`ANALYSIS.md`](ANALYSIS.md) — D1-D22, per il tipo di difetto che questo progetto trova più spesso
  (nel meccanismo di sicurezza attorno alla funzione, non nella funzione stessa) — la stessa cautela
  vale per ogni voce dell'Onda 3.
