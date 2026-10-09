---
type: Reference
title: Catalogo dei comportamenti della console da portare in Slint
description: Elenco verificabile dei comportamenti e dei divieti già pagati dalla console Tauri (ricavati dalle prescrizioni di CLAUDE.md, ciascuna nata da un difetto reale), con per ciascuno dove vive oggi, se resta nel core o va riprodotto nell'interfaccia, come si porta in Slint e come si verifica. Prerequisito della Fase 2 di PIANO_GUI_SLINT.md.
status: draft
generated:
  by: process:claude-code
  at: 2026-10-08T11:15:00Z
---

# Catalogo dei comportamenti della console

Fase 2 di [PIANO_GUI_SLINT.md](PIANO_GUI_SLINT.md), prerequisito prima di scrivere schermate. [CLAUDE.md](CLAUDE.md)
contiene decine di prescrizioni sulla console attuale; ciascuna esiste perché un difetto vero è stato trovato, quasi
sempre **cliccando sul binario compilato** e non leggendo il codice. Riscrivere l'interfaccia senza questo elenco
significa pagarli di nuovo.

**Come leggerlo.** *Dove vive*: **Core** = logica già in `rustcopy-core` (la nuova interfaccia la eredita chiamando le
stesse funzioni, nulla da riscrivere ma da **non aggirare**); **UI** = comportamento dell'interfaccia, da
riprodurre; **Tauri** = specifico di Tauri/Svelte, senza equivalente da portare (con il motivo); **Nuovo** = richiesto
da specifica o piano, non esiste oggi. *Verifica*: come si prova che la nuova interfaccia lo rispetta; "dal vivo" vuol
dire sul binario compilato con Windows-MCP, come per la console attuale.

Le regole di sicurezza chiave (nessun comando che copi, cancelli, pianifichi o installi senza il confine descritto in
[SPEC_GUI_SLINT.md](SPEC_GUI_SLINT.md) §10) valgono per ogni riga.

## 1. Confini e architettura

| ID | Comportamento | Perché (difetto o scelta) | Dove vive | Come si porta in Slint | Verifica |
|---|---|---|---|---|---|
| G01 | I comandi della console sono **involucri sottili** su `gui_api`: convertono argomenti, chiamano una funzione, mappano l'errore. Se un ramo decide semantica di backup, va nel core | È ciò che tiene vera la regola "il frontend non decide" | Core | I callback Slint chiamano solo `gui_api`/`runner`. Nessuna decisione di backup nel crate `rustcopy-ui`. Revisione di codice: ogni `if` su mirror, retention, purge, pianificazione è un campanello | Revisione + `grep` in CI per divieti noti (G02) |
| G02 | La console **non** copia, cancella, pianifica o installa fuori dal confine; i divieti (`--force-purge`, `--mirror` non presidiato, install/uninstall) non possono comparire negli argomenti | Il confine di sicurezza è verificato da un test, non ricordato | Core (`runner::run_arguments` a forma fissa) | Usare `run_arguments`/`resume_arguments`/`restore_preview_arguments`; **mai** un parametro che inoltri flag. I livelli di sicurezza (SPEC §10.1) cambiano cosa si può *preparare*, non questa forma | Test `the_argument_list_cannot_carry_a_destructive_flag` + test del livello |
| G03 | `cli_beside`: la CLI si cerca **accanto** all'eseguibile della console, mai su `PATH` | Un altro rustcopy su `PATH` sarebbe avviato per errore | Core | Chiamare `runner::cli_beside(current_exe)` | Prova dal vivo con una CLI diversa su `PATH` |
| G04 | **Fermare = scrivere il file di stop**, mai uccidere il processo | Il checkpoint lo scrive il ramo `--cancel-file`; un kill lo salta | UI + Core | Il pulsante "Ferma" scrive il file restituito da `cancel_file_for_now` (già nel prototipo) | Dal vivo: fermare una copia, controllare il checkpoint |
| G05 | Il figlio parte con **stdout/stderr catturati su file** e **cwd = cartella della configurazione**; i percorsi relativi in un TOML valgono rispetto al file | `null` nascondeva errori veri; ereditare la cwd faceva risolvere percorsi dal posto sbagliato | UI | `spawn_run` (già nel prototipo): `output_file_for`, `current_dir(parent)`; `--config` assoluto con `std::path::absolute` | Dal vivo con `examples/demo-locale.toml` avviato da un'altra cartella |
| G06 | Ogni processo figlio a console parte con **`CREATE_NO_WINDOW`** (CLI, `schtasks.exe`) | Altrimenti Windows apre una console nera davanti alla finestra | UI + Core | Già nel prototipo per la CLI; ogni nuovo `Command::new` verso strumenti a console porta la stessa flag | Dal vivo: nessuna finestra nera |
| G07 | Una sola esecuzione per volta in questa finestra: il controllo "c'è già una run?" e l'assegnazione del figlio sono **un'unica sezione critica** | Due clic rapidi sovrascrivevano il primo figlio, lasciando una copia non fermabile | UI | Lo stato della run vive in un solo posto sul thread grafico (`RefCell`), mai in due passi separati | Test dell'unità di stato + doppio clic dal vivo |
| G08 | Le **costanti degli exit code** e `exit_code_meaning` stanno solo in `runner`; History legge il significato da lì | Erano duplicate fra CLI e mappa JS | Core | `runner::exit_code_meaning(code)`; nessuna mappa locale | Revisione: nessuna stringa "riuscito/…" nella UI |
| G09 | **Nessuna icona o pass/fail da `exit_code_meaning` né da `exit_code == 0`**; si usa `ReportView::exit_code_is_success` (il codice 1 di robocopy è un successo) e `integrity_status` | Icona sempre falsa, trovata caricando un report vero | Core + UI | Badge e frase d'esito partono da `exit_code_is_success` | Test con report di codice 1, 0, 8 |
| G10 | **Ambra, mai rosso** per tutto ciò che non è pulito; l'icona dice "fallito" | Decisione di prodotto F81/F93 | UI | Token di colore `Theme.warning` unico; nessun rosso nei componenti | Revisione dei token + schermate |
| G11 | `read_settings` **tronca `webhook_url`** a schema e host; i comandi pre/post si mostrano **verbatim** | L'URL è la credenziale e la pagina finisce negli screenshot; un comando mezzo redatto sembra sicuro | Core | La UI mostra ciò che `read_settings` restituisce, senza ricomporre | Test sul core + prova dal vivo |
| G12 | Le **`caution`** (avvisi di rischio) sono semantica di backup e arrivano dal core | Il frontend non decide cosa è rischioso | Core | Mostrare, non calcolare | Revisione |
| G13 | I **segreti** (chiavi di cifratura) non viaggiano mai come argomento di processo; `keyring:NOME` | Un argomento è visibile nella lista dei processi | Core + UI | `gui_api::set_credential/delete_credential` in-process; il campo non è mai scritto in log né argomenti | Revisione + dal vivo |

## 2. Esecuzione, avanzamento, report

| ID | Comportamento | Perché | Dove vive | Come si porta | Verifica |
|---|---|---|---|---|---|
| E01 | Le risposte asincrone **fuori ordine** si scartano (contatore di generazione sul polling) | Una risposta lenta sovrascriveva una più recente | UI (Tauri: `setTimeout` a catena + generazione) | Un solo `Timer` sul thread grafico; ogni lettura da thread di lavoro riporta il proprio numero di generazione e il risultato più vecchio si scarta | Test di unità con risposte invertite |
| E02 | Il **polling** è a catena (il prossimo parte a fine del precedente), mai sovrapposto | Letture ravvicinate si accavallavano | UI | `Timer` unico; nessun secondo timer per la stessa risorsa | Revisione |
| E03 | **Progresso limitato a 0,99** mentre la run è attiva; "100 %" compare solo a run finita | Robocopy conta voci di cartella che l'inventario non conta: `bytes_done` supera il totale prima della fine | UI | `fraction.min(0.99)` finché `running` (già nel prototipo) | Test di unità + prova su run con molte cartelle |
| E04 | La **posizione nel batch** (`in attesa / in corso / concluso`) si legge da `ProgressSample.batch_index/total`; **non si azzera** quando si riesamina; resta visibile solo se `config_path` coincide; a batch concluso senza `wasStopped` l'ultimo job passa a "concluso" | La coda spariva proprio quando serviva; un ultimo job veloce restava "in attesa" | UI | Stato del batch separato dalla selezione; confronto di `config_path` | Dal vivo con un batch di 3 job |
| E05 | Il **nome del file in corso** (`current_file`) viene dal progresso, **non** da log a livello `debug` | Il log per file costa 76× (D18) | Core + UI | Mostrare `ProgressSample.current_file` | Dal vivo |
| E06 | La sezione "Dettagli — file in copia" **resta aperta** mentre il polling aggiorna | Con `open={expr}` a una via si richiudeva a ogni tick | UI (Tauri: `bind:open`) | Lo stato "aperto" è una proprietà dell'interfaccia **non ricostruita** dal modello a ogni tick | Dal vivo: aprire e attendere 5 s |
| E07 | Il **report** mostra un banner di **simulazione** quando `dry_run`; non si deduce dall'`exit_code_meaning` | Un `--dry-run` sembrava "79 GB in 34 s" | Core + UI | Banner da `ReportView::dry_run` (già nel prototipo) | Report di prova con dry-run |
| E08 | I report con **placeholder** `{timestamp}` irrisolto mostrano "non disponibile", mai un'euristica | `report_path` può essere `None` | Core | Gestire `None` | Test dedicato |
| E09 | **Notifica di sistema a fine run** al passaggio `running → false` | Non serve tenere la finestra in primo piano | UI (Tauri: plugin) | Notifica toast di Windows o palloncino del tray; **da realizzare** | Dal vivo |
| E10 | **Esegui** si aggancia a una run già in corso lanciata dalla Copia o da Explorer (`pendingRunAttach`) e ne mostra lo stato senza un secondo clic | Segnali monouso fra schede | UI (Tauri: flag monouso) | Evento esplicito "apri la run X" verso lo stato condiviso; mai un flag da ricordare di azzerare | Dal vivo |
| E11 | `--auto-config <toml>`: la console avvia subito il lavoro **dopo aver esaminato** la configurazione, perché è l'esame che popola la schermata | Una copia partiva con la finestra su "scegli un file" | UI | In avvio: carica → mostra → avvia, in quest'ordine | Dal vivo da Explorer |
| E12 | Un secondo avvio **consegna il lavoro all'istanza aperta** e si chiude | TeraCopy lo fa; evita due finestre su una stessa copia | Nuovo | Mutex nominato + passaggio dell'argomento (RF-Y11) | Dal vivo: due `--auto-config` consecutivi |

## 2b. Lavori (elenco, ripeti, salva come attività)

| ID | Comportamento | Perché | Dove vive | Come si porta | Verifica |
|---|---|---|---|---|---|
| L01 | Come **finisce** un lavoro lo decide il core: pulita secondo `exit_code_is_success` (mai `exit_code == 0`), simulazione, da controllare; **nessun report = mai pulita**; una copia avviata e mai finita, non in corso nella finestra, è **interrotta** | Lo stesso errore già fatto con i badge del Report | Core (`sessions`) | La UI mostra lo stato che il core restituisce | 10 test del core + dal vivo |
| L02 | Il log dei lavori è **append-only**; una riga troncata da un crash si salta, non rompe l'elenco | Stessa disciplina di cronologia e manifest | Core | Nessuna logica UI | Test della riga troncata |
| L03 | **Salva come attività** scrive la configurazione semplice di un drop più un nome validato, con `create_new` (**mai sovrascrive**), e non può contenere mirror, pulizia, verifica o cifratura | Una console che salva configurazioni non deve poterle allargare | Core | Chiamare `SessionLog::save_as_task` | Test: nessuna parola vietata nel file; secondo salvataggio rifiutato |
| L04 | Una copia **ripetuta** è pianificata di nuovo dal core (`plan_copy`), non riusa il vecchio file | Le cartelle possono essere cambiate nel frattempo | Core + UI | `start_copy` con le cartelle del lavoro | Dal vivo: Ripeti dà 0 file se è già aggiornata |
| L05 | Il motivo di un "da controllare" si legge dal report **su un thread a parte** e si scarta se nel frattempo si apre un altro lavoro | Un report grande non deve bloccare la finestra; risposta lenta sopra risposta nuova | UI | `Generation` + `upgrade_in_event_loop` | Test del contatore + dal vivo |
| L06 | Un lavoro che **non è partito** (un'altra copia attiva, errore di avvio) è chiuso nel log, non lasciato "avviato" | Altrimenti comparirebbe come interrotto | UI | `finish` con codice -1 | Revisione |
| L07 | La **verifica** è l'unica opzione che una copia della console può aggiungere: tipo `Option<HashAlgorithm>`, mai un campo libero; il lavoro la ricorda, la ripete e la porta nell'attività salvata | Una console che avvia copie non deve poter allargare il rischio | Core | `begin_with(..., verify)` | Test: nessuna parola vietata con la verifica attiva |
| L08 | Il **tempo residuo** lo calcola il core e sa tacere (fase diversa, totale ignoto, troppo presto, assurdo) | Una stima inventata da un numero mancante sembra conoscenza | Core | Mostrare `eta_seconds()` solo se `Some` | 3 test del core |
| L09 | Il **grafico** non disegna nulla senza dati; la velocità usa MB = 10⁶ byte come il core | Una linea piatta dice «fermo»; due unità diverse mostrano numeri diversi per la stessa copia | UI | `format::chart_path`, `throughput_mbps` | Test di unità |

| L10 | Ogni attività salvata vive in una **cartella sua** (`<nome>\<nome>.toml`) perché report e storico sono relativi alla cartella della configurazione | Una cartella condivisa farebbe sovrascrivere a tutte lo stesso report | Core | `save_as_task` | Test: due attività, due cartelle |
| L11 | Un'attività si esegue **sul posto**, senza copiare il file (percorsi relativi e cwd = cartella della configurazione, G05) | Copiarla spezzerebbe i percorsi relativi | Core | `begin_task` | Test + dal vivo |
| L12 | Il report di un'attività conta **solo se modificato dopo l'inizio** della run | Un report di una run precedente non è prova di questa | Core | `task_reports` | Test con report vecchio |

| L13 | La griglia delle proprietà **non calcola nulla**: valore, origine (nel file / ereditata / predefinita), avvisi e redazione del webhook arrivano da `gui_api::read_settings`; i comandi pre/post si mostrano verbatim | Il frontend non decide cosa è rischioso né cosa redigere (G11, G12) | Core | `property_rows` solo dispone le righe | Dal vivo con un file a due job |

| L14 | L'editor **non decide**: il modulo (`form.rs`) converte testo in campi tipizzati e un numero non valido dà un errore col nome del campo; ogni rifiuto (mirror non attivabile, conservazione solo in salita, mirror+tipo, tipo+cifratura, thread 1-128) è del core e compare nel banner | Il frontend non decide cosa è rischioso (G01, J01) | Core + UI | `FormValues::to_draft` + `job_editor::propose_config_from_path` | 8 test del modulo + dal vivo |
| L15 | Il nome del job **non si modifica** dall'editor; i controlli che il core vieterebbe sono **disattivati con il motivo** (mirror spento, conservazione assente), non nascosti | Rinominare orfana le generazioni (D12); RF-Y07 | Core + UI | `name` letto dal draft originale; `enabled:` da `mirror-was-on`/`retention-was-set` | Dal vivo |
| L16 | La cifratura scritta a mano (`env:`, `file:`, letterale) è mostrata **sola lettura** e portata invariata; il modulo scrive solo `keyring:NOME` | Un segreto non passa dal modulo (G13, F80) | UI | `encrypt_other` | Test del modulo |
| L17 | La proposta si scrive **accanto** al file (`create_new`, mai sopra); i campi che il modulo non possiede (pre/post, webhook, spazio libero) restano come nel file | Una sostituzione la fa la persona (F54) | Core | `suggest_proposal_path_now` | Dal vivo: originale intatto, `pre_command` conservato |

| L18 | Lo storico di un'attività si legge dall'indice che sta **accanto al report di ogni job**; il nome del job è `history_job_name` (non `name`: il job singolo non ha suffisso); le osservazioni sono di `gui_api::read_advice` e **propongono, non cambiano nulla**; le righe illeggibili sono dichiarate | Altrimenti si cerca l'indice sbagliato e si mostra «nessuna run» (F86); l'analisi non è giudizio del frontend | Core + UI | `history_rows` | Dal vivo con 8 run |

| L19 | I dettagli tecnici si leggono dal report **su un thread a parte** (scartati se si apre un altro lavoro, E01); le righe sono scelte da `report_rows` ma tutti i valori sono di `gui_api::ReportView`; un elenco tagliato dice **quanti altri** ce ne sono e se era già incompleto alla scrittura; i problemi sono in ambra | Un report grande non blocca la finestra; un elenco tagliato non deve sembrare completo | Core + UI | `rows_for` | 6 test + dal vivo |

| L20 | Un file di configurazione aggiunto **non si copia né si modifica**: si ricorda il percorso (`configs.txt`, riga per riga, anche con BOM); si controlla all'aggiunta che sia una configurazione con almeno una coppia di cartelle; un file sparito (unità non collegata) **si salta ma non si dimentica**; «Togli» lascia il file | Perdere un percorso perché un'unità è scollegata sarebbe una perdita silenziosa | Core | `add_config` / `list_entries` | 5 test |
| L21 | Il badge «pianificata» arriva da `schtasks.exe` su un thread a parte e si scarta se l'elenco è stato ricostruito nel frattempo | È lento (E01); non deve bloccare né mostrare dati vecchi | UI | `ctx.schedules` | Dal vivo |

| L22 | «Riprendi» compare solo nel dettaglio di una copia **interrotta** che ha lasciato un punto di ripresa nella propria cartella; il core rifiuta ogni punto di ripresa che non appartenga a quella copia e non offre più uno già portato a termine con esito pulito | Un percorso incollato altrove non deve poter avviare una run; riprendere un lavoro finito è solo una seconda run inutile | Core | `checkpoints_of` / `begin_resume` | 3 test + dal vivo |

| L23 | Il risultato di «Controlla prima» **sparisce** appena cambiano le cartelle o la destinazione, e una risposta lenta arrivata dopo il cambio si scarta; lo spazio libero usa la stessa regola e lo stesso margine (5%) del controllo preliminare della CLI | Un esito vecchio accanto a una scelta nuova è peggio di nessun esito; due regole diverse per «basta» farebbero dire all'interfaccia una cosa e alla CLI un'altra | UI + Core | `gui_api::check_copy`, `disk_space::covers` | 6 test + dal vivo |

| L24 | «Esporta CSV...» scrive **esattamente le righe che il filtro mostra**, nello stesso ordine; i campi che iniziano come una formula (`=`, `+`, `-`, `@`) portano un apice davanti; il file ha il marcatore UTF-8 e righe CRLF | Un CSV con righe appena filtrate via direbbe altro da ciò che la persona ha scelto; un percorso o nome file che comincia con `=` diventerebbe una formula in un foglio di calcolo | UI | `runs::history_csv`, `csv::to_csv` | 5 test + dal vivo |

| L25 | Il segreto di una credenziale viaggia **solo** dal campo mascherato a Gestione credenziali di Windows (mai un argomento di processo, mai un file) e il campo si **svuota** appena salvato; la scrittura va su un thread a parte | Un argomento si vede nell'elenco dei processi; un segreto rimasto nella finestra si vede in uno screenshot | UI + Core | `gui_api::set_credential` / `delete_credential` | Dal vivo |

| L26 | I selettori di cartella dell'editor **scrivono solo nel modulo**; gli avvisi in linea (mirror o cifratura con un tipo di copia a generazioni) sono un'indicazione, mentre il rifiuto vero resta di `job_editor::apply_draft` | Un selettore che scrivesse su disco farebbe dell'editor ciò che F54 vieta; un avviso non è una barriera | UI | `edit-browse` | Dal vivo |

| L27 | Le cartelle e le destinazioni **recenti** si ricavano dal registro dei lavori (più recenti prima, senza ripetizioni, confronto senza maiuscole); sceglierne una riempie solo il modulo | Una seconda lista salvata a parte potrebbe non coincidere mai col registro | Core + UI | `sessions::recent_folders` | 1 test + dal vivo |

| L28 | Il significato di un codice di uscita nella pagina Aiuto **viene da `runner::exit_code_meaning`**, non è riscritto; l'esempio guidato non riscrive mai una cartella esistente e lo dice in italiano | Una seconda copia dei significati si disallinea al primo codice nuovo (F81); un secondo clic non deve rovinare ciò che la persona ha già modificato | Core + UI | `help::entries`, `example_workspace` | 2 test + dal vivo |

| L29 | L'anteprima di ripristino **non copia nulla** (`--restore-from` con `--dry-run`, forma fissa senza flag distruttivi, report di servizio cancellato dopo) e il suo risultato è sempre dichiarato «Simulazione»; la cartella di lavoro è quella della configurazione, non del report (D26) | Una simulazione scambiata per un ripristino vero, o che risolve male i percorsi relativi, farebbe credere cose false | Core + UI | `gui_api::preview_restore` | Dal vivo |

| L30 | La notifica di sistema compare **solo se la finestra non ha il focus**, ha una riga sola (cosa è stato copiato e come è andata) e **non è mai un errore** se Windows la scarta; il testo è escapato per XML | Con la finestra davanti l'esito è già sotto gli occhi; un nome di cartella con `<` non deve poter rompere il contenuto; senza identità registrata Windows non mostra nulla e non va detto come guasto | UI | `toast::toast_xml`, `announce_finish` | 2 test + dal vivo |

| L31 | Il livello di sicurezza **si legge dalle impostazioni della persona** (mai da un file di job), **qualsiasi cosa illeggibile vale Prudente**, **alzarlo chiede una conferma che dice cosa sblocca**, abbassarlo no, ogni cambio finisce in `safety.log`, e **nessun livello** avvia un mirror o una pulizia non presidiati | Un `.toml` che passa da un computer all'altro non deve poter alzare i permessi; un aggiornamento o un file rovinato può solo rendere la console più prudente | Core + UI | `safety::{load,set}`, `SafetyLevel::can_run_unattended_purge` | 6 test + dal vivo |

| L32 | La console **non scrive mai una pianificazione**: prepara una lista di argomenti fissa per la CLI (`--config`, `--install-schedule`, `--schedule-name`), dopo che il **core** ha riletto il livello di sicurezza dal disco; rifiuta mirror, pulizia di generazioni, percorsi relativi, nomi e specifiche malformate | Un livello abbassato mentre la finestra è aperta deve valere subito; una pianificazione parte senza nessuno davanti e non deve poter cancellare | Core + UI | `schedule::{install_arguments,removal_arguments}`, `schedule_form::build_spec` | 7 test + dal vivo |

| L33 | Cancellare gli originali di uno «Sposta» richiede **tutti** insieme: livello Standard riletto dal disco, una copia **non simulata, riuscita e con verifica passata** (`VerifiedCopy`, il tipo è la prova), la conferma della persona; si cancellano solo i file con una copia di **stessa dimensione e data** e ognuno è riletto subito prima; mai in modo ricorsivo, le cartelle si tolgono solo se vuote; link e giunzioni non si toccano | Spostare è l'unica azione della console che distrugge dati: la sicurezza sta nell'ordine, nella prova e nel non fidarsi di ciò che è cambiato nel frattempo | Core + UI | `moves::{plan_move,execute_move,VerifiedCopy}`, `SessionLog::begin_move` | 12 test + dal vivo |

| L34 | Un **file singolo** come origine va *dentro* la destinazione col suo nome (cartella + modello a un nome nella configurazione); è rifiutato se la destinazione è la sua stessa cartella o se due file hanno lo stesso nome; il suo job si chiama `file-<nome>` per non condividere report e cache con una cartella omonima; l'estensione Shell **non** lo usa | Un file non ha un «nome di cartella» sotto cui mettersi; l'estensione gira dentro Explorer e non deve fare accessi al disco in più | Core + UI | `runner::plan_copy_with_files`, `shell_drop_config_text` | 5 test + col motore |

| L35 | La pausa **sospende** (tutti i thread della riga di comando e dei suoi figli), non uccide: nulla si perde; **riprende da sola dopo 10 minuti** e lo dice; «Ferma» **riprende prima** di scrivere il file di stop, perché un processo sospeso non lo vedrebbe | Una connessione di rete ferma troppo a lungo può essere chiusa dal server; uno stop dato a un processo in pausa resterebbe senza risposta | UI | `suspend::{set_suspended,descendants,PAUSE_LIMIT}` | 3 test + dal vivo (locale) |

| L36 | L'espulsione dell'unità è proposta **solo** per una destinazione su una lettera che Windows dichiara *rimovibile* (mai disco fisso, rete, ottico, percorso senza lettera), avviene **solo dopo una copia pulita**, e se il volume è ancora in uso **non cambia nulla** e lo dice; l'apertura della cartella a fine copia viene saltata quando si espelle | Espellere un disco sbagliato o uno in uso farebbe perdere dati o lavoro; aprire la cartella proprio prima terrebbe il volume occupato | UI | `eject::{drive_letter,is_removable,eject}` | 4 test + C: mai toccato; chiavetta da provare |
| L37 | La posta parte **dal notify-server**, non dalla console né dalla CLI; la password è una specifica (`keyring:`, `env:`, `file:`) letta al momento dell'invio; **senza cifratura solo verso un host locale**, altrimenti il canale non si avvia; un server morto o muto è un errore con scadenza, non un blocco | Un segreto non deve stare in un file condiviso né viaggiare in chiaro per una svista; un canale rotto non deve fermare gli altri | Core | `notify_smtp::SmtpSink` | 5 test con finto server |
| L38 | Una configurazione a **più job** si guarda **un job per riga** (cosa copia, che tipo di copia, le impostazioni che contano, l'esito dell'ultima run di *quel* job letto dal suo indice) e si estende con **«+ Nuovo job»** nell'editor: il nome è convalidato dal core, un file a job singolo **rifiuta** di diventare multi-job (cambierebbe i nomi di report, cache e storico), e nulla si scrive finché non si preme «Scrivi proposta» | Con una riga per file non si vede quale job è andato male; aggiungere un job a un file a job singolo rinominerebbe i file del job che c'è già | UI | `jobs_view::job_line`, `job_editor::build_proposal` | 4 test + dal vivo (due job, esito per job, nuovo job scritto in proposta) |
| L39 | I **problemi di un report** (differenze, mancanti, illeggibili) si sfogliano **100 per volta** e si esportano **tutti** in CSV (non solo la pagina a schermo); i nomi che iniziano con `=`, `+`, `-`, `@` sono neutralizzati; un'anteprima di ripristino **non** si sfoglia (le sue righe non sono quelle del report che il percorso nomina) | Con migliaia di errori «e altri N» non basta; un CSV con una riga formula verrebbe eseguito da un foglio di calcolo | UI | `problems::{page_info,neighbour,all_problems,problems_csv}` | 7 test + dal vivo (250 problemi, 3 pagine) |
| L40 | «Verifica» accanto a origine e destinazione dell'editor dice **cosa c'è a quel percorso** (esiste, è una cartella, quanti file e quanto pesa); una destinazione che non esiste è **normale** (la crea la prima copia), un'origine che non esiste no; il conteggio parte **solo a pulsante premuto**, su un thread, e la risposta si mostra **solo finché il testo nella casella è quello controllato** | Un albero grande richiede minuti: contare a ogni tasto bloccherebbe la finestra; una risposta su un altro percorso mentirebbe | UI | `path_check::describe`, `gui_api::inspect_path` | 4 test + dal vivo (percorso relativo risolto sul file, risposta che sparisce se cambi il testo) |
| L41 | **Ogni** casella dell'editor ha una frase che dice a cosa serve (visibile e come descrizione accessibile), compresi percorsi relativi, tentativi, date, permessi, report, registro e algoritmo di verifica; i percorsi predefiniti di report e registro sono mostrati come suggerimento nella casella vuota, mai scritti nel file | Un controllo senza spiegazione obbliga a indovinare, e uno screen reader legge solo l'etichetta | UI | `editor::TextField`/`ToggleField`, `strings.slint` | dal vivo; non esiste un tooltip al passaggio del mouse in Slint standard |
| L42 | Mentre una copia **avviata da questa finestra** è in corso il computer **non va in sospensione** (`SetThreadExecutionState`, solo il sistema: lo schermo può spegnersi); la richiesta si ritira nel tick in cui la copia finisce e sparisce da sola se la finestra si chiude. Non copre le copie pianificate: quelle sono di Task Scheduler (`StartWhenAvailable`, installazione da XML) | Una copia lunga non deve fermarsi perché un portatile ha deciso che nessuno è al computer | UI | `keep_awake::KeepAwake` | 1 test sulla decisione; la chiamata di sistema da controllare a mano (CONTROLLI_MANUALI_GUI.md) |

## 3. Copia, anteprima, Explorer

| ID | Comportamento | Perché | Dove vive | Come si porta | Verifica |
|---|---|---|---|---|---|
| C01 | `plan_copy`: rifiuta destinazione dentro/uguale alla sorgente, radice di un'unità, cartelle con lo stesso nome; **lessicale**, mai metodi di `Path` | Copiare una cartella in sé stessa; stesso risultato su Linux e Windows | Core | Chiamare `plan_copy`; mostrare il messaggio | Test esistenti + dal vivo |
| C02 | `prepare_copy`/`write_shell_drop_config`: nessun parametro per mirror/purge/verifica; destinazione UNC → `threads: Some(8)` | Sicurezza e 48 connessioni SMB inutili | Core | Stessa coppia di funzioni (già nel prototipo) | Test esistenti |
| C03 | **"Controlla prima"** conta file e dimensione **solo su pressione**, mai a ogni modifica | Su un profilo vero richiede minuti | UI + Core | Pulsante esplicito; `inspect_path` con ancora (`config_path`) | Dal vivo |
| C04 | `inspect_path` è **non filtrato** e prende l'**ancora** della configurazione; un risultato non più valido (percorso o file di config cambiati) **non si mostra** | Risposta vecchia accanto a un percorso diverso; due config con "job1" | Core + UI | Chiave di validità = (percorso, job, config); mostrare solo il risultato valido | Test di unità + dal vivo |
| C05 | I rifiuti e gli avvisi **spariscono** quando l'utente cambia cartelle o destinazione | Messaggio che non descrive più la scelta | UI | `error = ""` a ogni modifica (già nel prototipo) | Dal vivo |
| C06 | I **selettori nativi** non restituiscono percorsi non esistenti; il campo testo resta modificabile per destinazioni nuove; **nessun `create_dir_all`** per aggirare | Il selettore rifiuta cartelle inesistenti; creare cartelle dall'editor violerebbe "scrive solo proposte" | UI | `rfd` su thread di lavoro (già nel prototipo) + campo testo accanto | Dal vivo con destinazione nuova |
| C07 | Una **sola** casella di percorso condivisa per config/report (stato di sessione), **non** una per pannello | Era ciò che rendeva l'app inutilizzabile | UI | Stato di sessione unico nel viewmodel | Revisione |
| C08 | **Recenti/Preferiti** nel selettore: pannello sovrapposto che **non occupa spazio** nel layout | Il pannello copriva il testo seguente; il margine collassava | UI (Tauri: `mb-8`) | `PopupWindow` di Slint: non è nel layout, quindi il difetto non esiste; verificare comunque | Dal vivo con una voce |
| C09 | **Explorer**: la voce "Copia con RustCopy" **non compare** per un drop non sicuro (`plan_drop`); mai un dialogo né un panic; il drop non lancia mai la CLI in silenzio, passa dalla console | Annidamento; la copia silenziosa lasciava l'operatore senza feedback | Core + DLL Shell | Nessun cambiamento alla DLL; la console accetta `--auto-config` | Dal vivo da Explorer |
| C11 | Una destinazione **UNC** (`\\server\share`) funziona dalla scheda Copia come dal drop di Explorer (thread conservativi, C02); **D28 è aperto** nel core: `normalize_path_arg` costruisce un prefisso di percorso lungo non valido per UNC | Il difetto vive nel core e la nuova interfaccia lo eredita | Core (aperto) | Nessun codice UI; **provare dal vivo una destinazione UNC** prima del rilascio e non dichiarare risolto ciò che D28 non risolve | Dal vivo con `\\localhost\C$\...` |
| C10 | **Drop sulla finestra**: una cartella trascinata si aggiunge all'elenco (winit `DroppedFile`) | Gesto primario di TeraCopy | Nuovo | Già nel prototipo; aggiungere gestione file singoli (decisione 12) e conflitti con `plan_copy` | Dal vivo (fatto nella Fase 1) |

## 4. Editor e impostazioni di job

| ID | Comportamento | Perché | Dove vive | Come si porta | Verifica |
|---|---|---|---|---|---|
| J01 | L'editor **può restringere il rischio, mai allargarlo**; scrive una **proposta** in un file nuovo, mai il file in uso | F54 | Core (`job_editor`) | Usare `read_drafts`/`write_proposal` | Test esistenti |
| J02 | Il **mirror resta nella bozza**; un campo di job si scrive solo se il job lo aveva già o se differisce dall'ereditato | Senza, ogni modifica non correlata spegneva il mirror; si "appiattiva" l'ereditarietà | Core | Nessuna logica nel client | Test esistenti |
| J03 | `mirror` + `backup_type` rifiutati; la `<select>` del tipo è **disattivata** con mirror attivo; il core è l'ultimo argine. Idem `backup_type` + cifratura (F80) | Proposte che fallivano alla prima esecuzione notturna | Core + UI | Controllo disattivato con il motivo (RF-Y07) | Test + dal vivo |
| J04 | `keep_generations`: campo **solo-in-salita**, il minimo è il valore **caricato** (non quello che si digita), **mai vuoto** | `EditorCannotLowerRetention` non copre `Some→None` | UI + Core | Valore minimo memorizzato al caricamento; il campo non accetta stati intermedi non validati | Test di unità del campo + dal vivo |
| J05 | `validate_job_name`: nomi con caratteri riservati, controlli C0, nomi di dispositivo (anche `COM¹…`) rifiutati; la UI duplica le liste per un messaggio immediato, il core è il vero argine | Un nome inutilizzabile falliva al primo run pianificato | Core + UI | **Una sola lista**, esposta dal core, nella nuova UI (il duplicato JS era un debito) | Test sul core |
| J06 | Le sezioni **"Comportamento della copia" e "Opzioni avanzate" si aprono da sole** quando contengono un valore non predefinito; mai chiuse in modo incondizionato | Una sezione chiusa non deve nascondere un valore personalizzato o un avviso | UI | Stato iniziale dedotto dalla bozza + non richiuso dal rendering | Dal vivo con un job non predefinito |
| J07 | Il campo **Report** mostra il percorso predefinito come **segnaposto**, non lo scrive; **Pattern**: solo singoli (`*`, `*.pdf`) finché non diventa una lista; **Thread**: il segnaposto è `gui_api::default_threads`, non la CPU del browser | Scrivere il valore cambierebbe "eredita" in "fissa"; due estensioni insieme non copiano nulla; il motore decide i thread | Core + UI | Placeholder del `LineEdit` da `gui_api`; nessun suggerimento multi-estensione | Dal vivo |
| J08 | **F89**: Semplice/Avanzata; **F70**: avvisi su combinazioni; **F68**: cartelle con selettore nativo, campo testo accanto | Vedi `PIANO_GUI.md` | UI | Livelli Semplice / Dettagli / Tecnico (SPEC §1.1) | Dal vivo |
| J09 | **F79**: la cartella di esempio si crea in "Documenti" con rifiuto di sovrascrittura (`create_dir`); `dirs` solo nella GUI | Non toccare cartelle esistenti | Core + UI | Stessa funzione del core; `dirs` solo nel crate UI | Test esistenti |
| J10 | **Le pianificazioni** si leggono con un'unica funzione condivisa; ogni nuova variante di filtro è una funzione sottile sopra `parse_scheduled_tasks`, mai una seconda query | Duplicazione del parser CSV | Core | `gui_api::list_all_schedules`/`schedules_referencing` | Test esistenti |
| J11 | L'**anteprima di una pulizia** (`mirror_purge_candidates`) è la stessa funzione del controllo di sicurezza e **non dipende da `--force-purge`** | Non divergere su cosa "verrebbe cancellato" | Core | Mostrare l'elenco del core | Test esistenti |
| J12 | **`preview_restore`**: `--restore-from --dry-run` verso un report scratch; la cwd è la **cartella della configurazione caricata**, non quella del report | Percorsi relativi falliti | Core + UI | Passare `config_path` della sessione | Dal vivo con `demo-locale.toml` |

## 5. Aspetto e accessibilità

| ID | Comportamento | Perché | Dove vive | Come si porta | Verifica |
|---|---|---|---|---|---|
| A01 | Un chip di stato è un `Badge`, una cifra grande è una `StatCard`, la frase d'esito viene da `outcome` | Niente `<span>` improvvisati | UI | Componenti Slint `Badge`, `StatCard`, `Outcome` nella libreria di Fase 2 | Revisione dei file `.slint` |
| A02 | Testo del corpo **14 px (min 12)**; niente 10-11 px; righe più alte; barra laterale più grande | Leggibilità (F94) | UI | Token tipografici in un solo file | Dal vivo a 100/150/200 % |
| A03 | Una libreria di icone **per nome**, mai per sottopercorso; nessuna dipendenza nuova senza controllo di vulnerabilità | Peso del pacchetto e superficie d'attacco | Tauri → Nuovo | Icone come immagini/SVG incorporate; `cargo audit` e `cargo deny` per il crate | CI |
| A04 | **Chiaro/scuro che segue il sistema**; barra del titolo inclusa | Il titolo nativo non seguiva la palette nel prototipo | Nuovo | Palette di Slint + tema winit per la barra | Dal vivo con il tema scuro |
| A05 | **Tooltip su ogni controllo**; opzioni non applicabili disattivate con il motivo; avviso inline per le opzioni a rischio | RF-Y07 (osservato in Cobian) | Nuovo | Componente `Hint` + `accessible-description` | Albero UIA + dal vivo |
| A06 | **Una sola lingua** e stringhe in un solo posto | RF-Y09 | Nuovo | Global `Strings` nel `.slint` (poi `@tr`) | Revisione: nessuna stringa nel Rust |
| A07 | Ogni controllo ha **nome e ruolo UIA**; ordine di tab sensato | RNF-06 | Nuovo | `accessible-*` su ogni componente; prova con lo script UIA | Albero UIA + Narrator |

## 6. Installer, build, CI

| ID | Comportamento | Perché | Dove vive | Come si porta | Verifica |
|---|---|---|---|---|---|
| I01 | Un **solo installer**: la console è un componente opzionale; `bundle.active=false` | Un secondo setup separerebbe la console | Tauri | Il componente installa il nuovo eseguibile; `bundle` sparisce con Tauri | Installer smoke |
| I02 | **Nome `rustcopy-gui.exe`** cercato da `runner::gui_beside`, `installer/rustcopy.iss`, DLL Shell; argomento `--auto-config` | Contratto fra crate | Core + Shell + Installer | **Tenere il nome** nella Fase 5; fino ad allora la nuova app si chiama `rustcopy-ui.exe` | `check-versions` + test `runner` |
| I03 | **Runtime C statico**: `+crt-static`, mai `RUSTFLAGS` in CI; `check-static-crt.ps1` su tutti gli artefatti | D30 | Core | Già vero per il prototipo (21 DLL di sistema) | CI `static-crt` e installer smoke |
| I04 | **`Flags: regserver` mai** sulla DLL Shell; registrazione non fatale; il rapporto di installazione si scrive anche in caso di errore | Un errore di registrazione annullava l'intera installazione | Installer | Nessun cambiamento | Installer smoke |
| I05 | **Quattro file dichiarano la versione**; `scripts/check-versions.sh` | Nessun passo di build li allinea | Core | Il crate eredita `version.workspace`; `tauri.conf.json` e `package.json` spariscono con Tauri | CI |
| I06 | La GUI è **esclusa** dai job cross-platform e ha job propri; **non rientra in `--workspace`** senza le librerie | `webkit2gtk` su Linux (Tauri); Slint non lo richiede ma il prodotto è Windows-nativo | CI | `--exclude rustcopy-ui` (fatto) + job `windows-latest` | CI |
| I07 | `cargo` in CI **sempre `--locked`**; mai tolto per far tornare verde un job | Un `Cargo.lock` non allineato è un errore del ramo | CI | Stesso | CI |
| I08 | **Chiudere `rustcopy-gui.exe` prima di una build release** e controllarne `ProductVersion` prima di ISCC | Il file bloccato nascondeva un fallimento | Procedura | Vale anche per `rustcopy-ui.exe` | Procedura di rilascio |
| I10 | Il binario **installato** non dipende da risorse di sviluppo né da un server locale: parte da solo su una macchina pulita (D22: la console Tauri caricava il server di sviluppo e mostrava `ERR_CONNECTION_REFUSED`) | Una release costruita nel modo sbagliato falliva ovunque tranne sul PC dello sviluppatore | Tauri → Nuovo | Slint non ha un server di sviluppo da caricare; resta da provare **dall'installer** su una macchina senza il repository (job `install-windows-server-2022` e prova a mano) | Installer smoke + prova su macchina pulita |
| I09 | **Niente `unwrap`/`expect` in codice di produzione** senza `#[allow]` motivato | Panic in un'interfaccia o nella shell | CI | Il job clippy include `rustcopy-ui --bins` | CI |

## 7. Stato e conteggi

Righe: 13 confini, 12 esecuzione, 42 lavori, 11 copia/Explorer, 12 editor, 7 aspetto, 10 build. **107** voci. Quelle marcate **Nuovo**
(E09, E12, A04, A05, A06, A07, C10 in parte) sono i requisiti che nascono dalla specifica, non da un difetto passato.

## 8. Come si usa questo catalogo

1. Ogni voce **UI** o **Nuovo** diventa un test o una prova dal vivo prima del rilascio della schermata che la riguarda;
   la colonna *Verifica* dice quale.
2. Una voce **Core** non richiede lavoro, ma richiede di **non aggirare il core**: la revisione di codice guarda ogni
   chiamata dell'interfaccia al core.
3. Una voce **Tauri** non si porta; il suo *perché* resta (es. C08): va riverificato, non assunto risolto.
4. Quando si trova un nuovo difetto in Slint, **si aggiunge qui una riga**, non in CLAUDE.md (che resta per le
   prescrizioni operative in una riga, come da convenzione B5b).

## 9. Criticità trovate rileggendo questo catalogo

- **Non esaustivo per costruzione**: ricavato da CLAUDE.md, non dai test della console. Le prescrizioni di
  `ROADMAP.md`/`ANALYSIS.md` che non sono finite in CLAUDE.md possono mancare; la revisione del catalogo contro i
  difetti D1-D30 resta da fare quando si arriva alle schermate corrispondenti.
- **C08 è un'ipotesi favorevole**: `PopupWindow` non sta nel layout, ma non ho provato un pannello Recenti reale.
- **E09 e A04** sono funzioni che non esistono ancora; la verifica "dal vivo" dipende dal tray (non riuscito nella Fase 1).
- **Il conteggio 107** è fatto a mano e può non riflettere righe aggiunte dopo.
- **Revisione contro D1-D30 fatta (8 Ott 2026)**: dei difetti documentati, quelli che toccano la console sono D22, D24, D25, D26, D28, D29, D30; D24 (G06), D26 (J12), D30 (I03) e D25 (core, `--resume-from`) erano già coperti; D22 (I10) e D28 (C11) sono state aggiunte; D29 riguarda solo la DLL Shell, invariata. D1-D21, D23 e D27 vivono nel core o nella CLI e la nuova interfaccia li eredita senza codice proprio.
