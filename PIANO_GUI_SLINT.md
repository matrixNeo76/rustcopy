---
type: Reference
title: Piano di realizzazione della console rustcopy su Slint
description: Piano a fasi con cancelli di conferma per valutare e, solo se la prova tecnica lo giustifica, costruire la nuova console "mista" TeraCopy + Cobian Reflector su Slint: prova a tempo limitato con criteri go/no-go misurabili, catalogo dei comportamenti da portare, onde di costruzione, rischi, ritorno indietro e decisioni che spettano all'utente.
status: draft
generated:
  by: process:claude-code
  at: 2026-10-08T10:30:00Z
---

# Piano: la nuova console rustcopy su Slint

Documento 3 di 3 del pacchetto "GUI Slint": [studio delle GUI di riferimento](STUDIO_GUI_RIFERIMENTO.md),
[specifica](SPEC_GUI_SLINT.md) e questo piano. **Bozza per approvazione: niente di quanto segue è
autorizzato finché l'utente non conferma.** Ogni fase termina con un **cancello**: ci si ferma, si riferisce
con i numeri e si aspetta una conferma esplicita prima della fase successiva.

## 0. Registro di avanzamento (si aggiorna a ogni PR)

Elenco spuntabile, **l'unico posto dove si tiene il conto di cosa è fatto, cosa manca e cosa è un limite noto**. Una voce si
spunta solo dopo che è in `main`, con la CI verde e, se è un comportamento, provata sul binario compilato. Legenda: `[x]` fatto ·
`[ ]` da fare · `[~]` fatto in parte (la nota dice cosa manca) · `[?]` serve una persona o una macchina che qui non ho.

### Fasi
- [x] Fase 0 — studio, specifica, piano, catalogo dei comportamenti (77 voci)
- [x] Fase 1 — prova a tempo, esito GO condizionato
- [x] Fase 2 — fondamenta (tema, stringhe, componenti, istanza unica, icona di notifica, job CI)
- [x] Fase 3 — lavoro immediato: elenco dei lavori, ripeti, salva come attività, verifica, tempo residuo, grafico, apri cartella
- [x] Fase 4 — attività salvate (la creazione di pianificazioni è nella Fase 6)
  - [x] 4a attività salvate, esecuzione sul posto, esegui un file di configurazione
  - [x] 4b griglia delle proprietà di un'attività, di sola lettura, con da dove viene ogni valore (pulsante «Proprietà» nella pagina Attività; tutto da `gui_api::read_settings`)
  - [x] 4c editor a schede (Base, Copia, Filtri, Conservazione, Sicurezza, Avanzate; scrive solo proposte accanto al file; può restringere il rischio, mai allargarlo; il nome non si cambia). Provato dal vivo: abbassare i cicli da 7 a 3 è rifiutato dal core, alzarli a 9 salva, e il job con mirror e `pre_command` resta intatto
  - [x] 4d storico delle esecuzioni e osservazioni del motore (`--advise`) per un'attività: pulsante «Storico». Provato dal vivo con 8 esecuzioni vere e tre osservazioni con le prove
  - [x] 4e report completo di una copia (fasi, file, verifica, elenchi di problemi, avvisi, computer e versione): «Mostra i dettagli tecnici» nel dettaglio di un lavoro. Provato dal vivo su una copia verificata; i casi con problemi sono coperti da test sul modulo (`report_rows`), non provati dal vivo
- [~] Fase 5 — parità con la console Tauri, installer, **toast di sistema (con AppUserModelID creato dall'installer)**, rimozione di Tauri (**con tua conferma esplicita**)
  - [x] 5a verifica di parità: confronto scheda per scheda con la console Tauri (tabella sotto)
  - [~] 5b lacune di parità, nell'ordine della tabella
    - [x] 1 aprire un file di configurazione e vederne i job (aggiunto all'elenco, mai copiato; elenco ricordato; «Togli» lo toglie dall'elenco e lascia il file; badge «pianificata» da `schtasks`)
    - [x] 2 coda di più job (etichetta «Job N di M» dal progresso; esito aggregato). Provato dal vivo con 3 job da 8,4 GB totali
    - [x] 3 ripresa da checkpoint («Riprendi» nel dettaglio di una copia interrotta; il core rifiuta un punto di ripresa che non sia di quella copia; uno già portato a termine non si offre più). Provato dal vivo: 26 GB, Ferma a 350 file, Riprendi ha copiato i 551 rimanenti con esito pulito
    - [x] 4 «Controlla prima» (file e dimensione di ogni cartella, spazio libero della destinazione con lo stesso margine del controllo preliminare della CLI, rifiuti di `plan_copy`; solo su pressione e fuori dal thread della finestra; il risultato sparisce se cambiano cartelle o destinazione). Provato dal vivo
    - [x] 5 report e storico di un file qualsiasi, con filtro per esito ed esportazione CSV (lo storico di un file di configurazione qualsiasi era già coperto da 5b.1; qui «Apri un report...» da Attività mostra il report tecnico di un `.json` e il suo storico; filtro Tutte/Riuscite/Da controllare/Di prova; «Esporta CSV...» scrive esattamente le righe filtrate). Provato dal vivo
    - [x] 6 gestione credenziali (pagina «Credenziali...» da Attività: Nome + Segreto mascherato, Salva/Elimina su Gestione credenziali di Windows; il campo si svuota appena il segreto ha fatto il suo lavoro). Provato dal vivo con una credenziale di prova, poi rimossa
    - [x] 7 selettori di cartella nell'editor (pulsante «Sfoglia...» accanto a origine e destinazione: scrive solo nel modulo, mai su disco finché non si preme «Scrivi proposta»; avvisi in linea per mirror, verifica e cifratura insieme al tipo di copia a generazioni, il vero rifiuto resta del core). Provato dal vivo
    - [x] 8 recenti per origine, destinazione e configurazioni (cartelle e destinazioni recenti dalla Nuova copia, ricavate dal registro dei lavori senza una seconda lista da tenere allineata; le configurazioni recenti sono l'elenco di Attività, già ricordato da 5b.1). Provato dal vivo
    - [x] 9 Aiuto ed esempio guidato (pagina «Aiuto» dalla barra laterale: guida breve, termini, esiti; «Crea un esempio in Documenti» usa `example_workspace` del core, che non riscrive mai una cartella esistente, e aggiunge l'esempio alle attività). Provato dal vivo, comprese le due pressioni di seguito
    - [x] 10 anteprima di ripristino (pulsante «Anteprima ripristino» nel dettaglio di una copia e nella pagina di un report: lancia `--restore-from --dry-run` verso un report di servizio tramite `gui_api::preview_restore`, ora nel core e usata anche dalla console Tauri). Provato dal vivo: cancellato un file dalla sorgente, l'anteprima ne ha contato 1 da ripristinare e la sorgente è rimasta com'era
  - [x] 5c installer (script compilato con ISCC in locale; installazione e disinstallazione vere provate dal job `installer-smoke` su windows-2022, passato; icona e informazioni di versione incorporate in `rustcopy-ui.exe` con `winresource`, una dipendenza usata solo in compilazione): il componente «console» installa `rustcopy-ui.exe` (stesso nome della console attuale, `rustcopy-gui.exe`, per non rompere Shell e `runner::gui_beside`), senza WebView2; installer smoke aggiornato
  - [x] 5d toast di sistema (`toast.rs`: notifica WinRT solo se la finestra non ha il focus; identità `rustcopy.console` data dal collegamento del menu Start e dalla chiave di registro che crea l'installer; senza identità Windows scarta la notifica e resta il lampeggio). Provato dal vivo con un collegamento di prova: «src → out: copia riuscita, 600 file (17,6 GB).» comparso nel centro notifiche, pulsante della barra che lampeggia con la nuova icona; la prova con l'installer vero resta nei controlli finali
  - [ ] 5e rimozione di Tauri e della toolchain JS: **solo dopo la tua conferma esplicita**, in una PR separata e annullabile
- [ ] Fase 6 — funzioni che toccano il core (vedi sotto)

### Parità con la console Tauri (verifica del 9 Ott 2026)

Confronto con le otto schede della console attuale (`App.svelte`) e i suoi 24 comandi. **Fatto** = c'è e provato nella console Slint; **manca** = va costruito prima della rimozione di Tauri.

| Scheda Tauri | Cosa fa | Stato in Slint | Lacuna da colmare |
|---|---|---|---|
| Copia | Cartelle → destinazione, controllo, verifica | **Fatto** (più: verifica, apri cartella, grafico, tempo residuo, drop da Explorer) | «Controlla prima» (conteggio file e dimensione prima di copiare, `inspect_path`) |
| Job | Elenco dei job di un file di configurazione con badge (ultimo esito, pianificato, cifrato, ...) | **Parziale**: pagina Attività per le attività *salvate dalla console* e «Esegui un file di configurazione» | Aprire **un file di configurazione qualsiasi** e vederne i job con esito dell'ultima esecuzione e badge «pianificato» (F62) |
| Impostazioni | Griglia delle impostazioni con origine | **Fatto** (pulsante Proprietà) | Gestione **credenziali** (`set_credential`/`delete_credential`): l'unica scrittura di quella scheda |
| Modifica | Editor a sezioni, proposta accanto al file | **Fatto** (schede) | Selettori di cartella per origine/destinazione; avvisi in-linea mirror+tipo di copia |
| Esegui | Avvio di un file di configurazione con coda di job, stop, ripresa | **Parziale**: avvio, stop, avanzamento, un lavoro alla volta | **Coda di più job** con posizione (in attesa / in corso / concluso); **ripresa da checkpoint** (`list_checkpoints`/`resume_job`) |
| Report | Apertura di un report | **Fatto** per le copie della console (dettagli tecnici) | Aprire **un report qualsiasi** da file |
| Storico | Esecuzioni, filtri, esportazione CSV, analisi | **Parziale**: per attività, con osservazioni | Filtro per esito, **esportazione CSV**, storico di un file qualsiasi |
| Aiuto | Guida | **Manca** | Pagina Aiuto (testi) |
| (fuori scheda) | Anteprima di un ripristino (`preview_restore`), esempio guidato (`create_example_workspace`), procedura nuovo job (`NewJobWizard`), `QuickSync` | **Mancano** | Esempio guidato per chi non ha nulla; anteprima di ripristino |
| (fuori scheda) | Recenti e preferiti dei percorsi (`PathBar`) | **Manca** | Recenti per origine, destinazione e configurazioni |

Ordine proposto per colmare le lacune (dal più usato): 1 aprire un file di configurazione e vederne i job; 2 coda di più job; 3 ripresa da checkpoint; 4 «Controlla prima»; 5 report e storico di un file qualsiasi con CSV; 6 credenziali; 7 selettori di cartella nell'editor; 8 recenti; 9 Aiuto ed esempio guidato; 10 anteprima di ripristino.

### Limiti noti e come li tratto
| # | Limite | Cosa faccio | Stato |
|---|---|---|---|
| 1 | Con **pochi file enormi** la barra resta indeterminata e tempo residuo/grafico non dicono nulla fino alla fine di ciascun file | Voce da indagare nel core: verificare se il poller della destinazione può dare byte a metà file senza toccare il parser di robocopy. Se sì, lo propongo come modifica del core con test; se no, resta documentato | [ ] da indagare (dopo 4b) |
| 2 | Notifica di sistema (toast) a fine copia non c'è; c'è solo il lampeggio sulla barra | **Decisione mia (9 Ott 2026): va fatta, ma nella Fase 5, non ora.** Un toast di un'applicazione non pacchettizzata funziona solo se l'applicazione ha un identificativo registrato (AppUserModelID) legato a un collegamento nel menu Start, e quel collegamento lo crea l'**installer**: senza, Windows scarta il toast in silenzio. Costo a regime: nessuno in prestazioni (una chiamata a fine copia, nessun processo residente); in compilazione e dimensione sì, perché porta con sé i binding WinRT. Sui Server con desktop funziona (Server 2016 e successivi); non su Server Core, dove la console non gira. Nel frattempo copre il caso il lampeggio della barra | [ ] Fase 5 (con l'installer) |
| 3 | Il **lampeggio** della barra delle applicazioni non l'ho osservato | Nei **controlli finali** (§0, ultima sezione): copia lunga, passare a un'altra finestra, osservare il pulsante di rustcopy | [ ] controllo finale |
| 4 | **RDP / macchina senza GPU**, **Narrator**, **DPI reale** non provati | Stesso trattamento del lampeggio: nei **controlli finali** (§0), da eseguire su una macchina adatta prima di dichiarare chiusa la Fase 5. Il rendering software non usa né OpenGL né Direct3D, quindi l'attesa è che funzioni, ma non è una prova | [ ] controllo finale |
| 5 | Stato *Interrotta* | Provato dal vivo: finestra chiusa a metà di una copia da 8 GB, al riavvio il lavoro risulta *Interrotta* | [x] |
| 6 | `.toml` scritto a mano con percorsi relativi | Provato dal vivo da «Esegui un file di configurazione...»: `source = "src"`, `dest = "out"`, 5 file copiati in `out` accanto al file, report accanto al file | [x] |
| 7 | Espelli disco e spegni il PC «al termine» | **Spegni il PC: scartato** (lavori su server, dove non serve e sarebbe pericoloso). **Espelli disco: rimandato alla Fase 6**, con la casella visibile solo se la destinazione è su un'unità rimovibile; richiede una chiamata al sistema o al guscio di Windows e la gestione del caso «il disco è ancora in uso», che non vale la fase corrente per un uso raro su server | [ ] Fase 6 |
| 8 | Allentare la regola di sicurezza (livelli Prudente / Standard / Esperto, creare pianificazioni, Sposta) | Fase 6, una funzione per volta, con i tre livelli come da §6 e SPEC §10.1 | [ ] |
| 9 | D28: destinazione UNC con prefisso di percorso lungo errato nel core | **Analizzato il 9 Ott 2026: era già corretto e chiuso il 21 Set** (`normalize_path_arg` produce `\\?\UNC\server\share\...`, con test su Windows); era la nota di `CLAUDE.md` a essere rimasta indietro, ora corretta. Provato anche dal vivo con una destinazione `\\localhost\C$` | [x] |

### Controlli finali (su macchina adatta, prima di chiudere la Fase 5)
- [ ] lampeggio del pulsante nella barra delle applicazioni quando una copia finisce e la finestra non ha il focus
- [ ] avvio e uso in una sessione RDP e in una macchina virtuale senza accelerazione grafica
- [ ] Narrator legge le schermate principali (Nuova copia, Attività, dettaglio di un lavoro)
- [ ] scala di Windows al 125 %, 150 %, 200 %: nessun testo tagliato
- [ ] tema scuro che segue il sistema, barra del titolo compresa
- [ ] installazione su un Windows Server senza Visual C++ (D30)

### Fase 6
- [x] livello di sicurezza nelle Impostazioni (predefinito Prudente): `safety.rs` nel core (illeggibile = Prudente, mai in un file di job, alzarlo chiede conferma, ogni cambio nel `safety.log`, nessun livello avvia un mirror o una pulizia non presidiati); pulsante «Sicurezza: ...» in fondo alla barra laterale. Provato dal vivo: Prudente → Standard con conferma. **Da solo non sblocca ancora nulla**: lo useranno pianificazioni e Sposta, una funzione per volta
- [x] creare/modificare pianificazioni (pulsante «Pianifica» sull'attività, solo dal livello Standard; il core, `schedule::install_arguments`, rilegge il livello dal disco, rifiuta mirror, generazioni con pulizia e percorsi relativi, e la riga di comando installa). Provato dal vivo: attività creata e tolta in Utilità di pianificazione, e con il livello abbassato a mano mentre la finestra era aperta il core ha rifiutato. **Nota:** la pianificazione è dell'utente corrente e parte solo quando è connesso, quindi non c'è nessuna richiesta UAC
- [x] Sposta a due passi con conferma (casella «Sposta» nella Nuova copia dal livello Standard, verifica sempre attiva; a copia riuscita e verificata, «Controlla gli originali da cancellare...» mostra cosa si cancellerebbe e cosa resta, poi «Cancella gli originali»; `moves.rs` nel core: serve una prova di verifica, il livello, la conferma, e ogni file è riletto subito prima di cancellarlo). Provato dal vivo: 5 originali cancellati, il file nuovo creato dopo la copia è rimasto, le copie intatte. **Non provato**: l'avvio di una copia «Sposta» dalla casella (il dialogo nativo ha dato problemi con la finestra del mio strumento), coperto da test di `begin_move`; la prova dal vivo ha usato una copia vera verificata registrata come spostamento
- [~] pausa/riprendi (Livello A: `suspend.rs` sospende tutti i thread dell'albero di processi del lavoro, mai un kill; riprende da sola dopo 10 minuti e «Ferma» riprende prima di chiedere lo stop). **Provato in locale**: 28 GB, in pausa la destinazione è rimasta ferma a 358 file per 8 s, alla ripresa è finita pulita con 900 file. **Manca la prova su una condivisione SMB vera**, come previsto dal piano: va fatta con il NAS e voglio il tuo permesso a scriverci
- [x] file singoli come origine («Aggiungi file...» e trascinamento di file nella Nuova copia; `runner::plan_copy_with_files`: un file va **dentro** la destinazione col suo nome, scritto nella configurazione come cartella + modello a un nome; vale anche per «Sposta» e per «Controlla prima»; l'estensione Shell resta sul `plan_copy` solo lessicale perché gira dentro Explorer). Provato col motore: un modello a un nome copia e verifica esattamente quel file. **Non provato dal vivo** nella finestra (il dialogo nativo)
- [ ] notifica via posta
- [ ] «espelli il disco alla fine» (solo unità rimovibili)

## 1. Principi del piano

1. **Prima si prova, poi si decide, poi si costruisce.** Slint non è ancora stato usato in questo progetto; le
   affermazioni a suo favore (leggerezza, avvio, assenza di WebView) sono ipotesi da misurare, non fatti.
2. **La console attuale resta in uso** (Tauri/Svelte, 7.8.1) finché la nuova non ha raggiunto la parità
   verificata. Nessun passo di questo piano la tocca, salvo la rimozione finale (Fase 5) e solo dopo conferma.
3. **Il motore non cambia** nelle fasi 1-5. Le funzioni che richiedono il core (§Fase 6) sono lavoro a parte,
   ciascuna con la propria decisione.
4. **Una modifica = una PR**, con i cancelli di CI già esistenti (fmt, clippy `-D warnings`, test, `check-static-crt`,
   installer smoke, `check-versions`). Nessuna scorciatoia per far tornare verde un job.
5. **Misurare con uno script salvato**, stessa macchina, stessi stati (finestra aperta, elenco vuoto). **Lo script
   oggi non esiste** (le prime misure erano comandi occasionali): è la prima cosa della Fase 1 (M0). I valori di
   partenza sono in [SPEC_GUI_SLINT.md](SPEC_GUI_SLINT.md) §2.1.
6. **Il confine di sicurezza non si allarga per inerzia**: ogni funzione che lo tocca ha una decisione propria (§6).

### 1.1 Priorità dell'utente che orientano ogni scelta

Dichiarate l'8 Ott 2026: **(1) prestazioni elevate, (2) robustezza, (3) funzioni tutte raggiungibili in modo
semplice e intuitivo, con un livello più approfondito e tecnico quando serve** (dettaglio in
[SPEC](SPEC_GUI_SLINT.md) §1.1). Nel piano diventano: le soglie della Fase 1 sono tutte di prestazioni e
robustezza (M1-M4, M7, M16, M17); l'interfaccia ha tre livelli (Semplice / Dettagli / Tecnico) nelle Fasi 3-4; e
ogni funzione nuova della Fase 6 entra solo con un predefinito non distruttivo.

## 2. Panoramica delle fasi

| Fase | Contenuto | Tempo | Cosa produce | Cancello |
|---|---|---|---|---|
| **0** | Studio, specifica, piano; completamento dello studio dal vivo | questo pacchetto | Tre documenti | **Conferma dell'utente sulle decisioni di §6 e sull'avvio della Fase 1** |
| **1** | Prova tecnica a tempo (spike) su ramo isolato | 3-5 giorni | Un prototipo **usa e getta** di Copia + Report e una tabella di misure | **Go / No-go** secondo §3. ✅ **Eseguita il 8 Ott 2026: GO condizionato, §3.5** |
| **2** | Fondamenta e catalogo dei comportamenti | 1-2 settimane | Crate `rustcopy-ui`, tema, componenti, estrazione dei comportamenti validati dal vivo | Conferma prima delle schermate vere. ✅ **Fondamenta pronte l'8 Ott 2026** (catalogo di 65 voci, token, stringhe, componenti, istanza unica, tray, job CI); revisione del catalogo contro D1-D30 fatta |
| **3** | **Lavoro immediato** (ingresso TeraCopy) | 2-3 settimane | Nuova copia, esecuzione, elenco dei lavori con esito, istanza unica | Prova dal vivo + conferma. 🟡 **3a, 3b e 3c fatte e 3d iniziata l'8 Ott 2026** (elenco dei lavori, ripeti, salva come attività, istanza unica, tempo residuo, grafico di velocità, verifica a fine copia); **Fase 3 chiusa**: «al termine» ha apri-cartella e lampeggio sulla barra (le collisioni non richiedono nulla, vedi §6 decisione 10; espelli disco e spegni il PC sono una decisione a parte) |
| **4** | **Lavori salvati e pianificati** (ingresso Cobian) | 3-4 settimane | Salva come attività, griglia proprietà, editor a schede, storico | Prova dal vivo + conferma. 🟡 **4a fatta l'8 Ott 2026** (attività salvate, esegui sul posto, esegui un file di configurazione); restano 4b griglia proprietà, 4c editor a schede, 4d storico |
| **5** | Parità con la console attuale e rimozione di Tauri | 1-2 settimane | Installer unico con la nuova console; Tauri rimosso | **Conferma esplicita prima di cancellare** |
| **6** | Funzioni nuove che dipendono dal core | a richiesta | Pausa, collisioni, sposta, creazione pianificazioni, posta… | Una decisione per funzione |

I tempi sono stime di ordine di grandezza fatte da chi non ha ancora scritto una riga di Slint: valgono
come relativi fra le fasi, non come impegni. **La Fase 1 serve proprio a sostituirli con dati.**

## 3. Fase 1 — Prova tecnica a tempo (con criteri go/no-go)

**Obiettivo.** Rispondere con misure a una sola domanda: *Slint, per questo prodotto su Windows, è migliore
della console attuale abbastanza da giustificare una riscrittura?* Ramo isolato (`spike/slint`), nessun merge
su `main`, nessun rilascio. Tempo massimo **5 giorni di lavoro**: scaduto, si riferisce con quel che c'è.

### 3.1 Che cosa si costruisce (e solo questo)

- Un crate `rustcopy-ui` minimo con **due schermate**: **Copia** (scelta cartelle, destinazione, avvio, avanzamento)
  e **Report** (le quattro schede numeriche e l'esito in una frase). Sono le due già disegnate in F94/F95 e quelle
  su cui il confronto è più diretto.
- Chiamata diretta a `robocopy_ingest::gui_api` e `runner` (nessun IPC): è anche la prova che il core si usa
  senza involucri.
- I tre punti ad alto rischio, ciascuno con una prova isolata:
  1. **Trascinare cartelle da Explorer sulla finestra** (R1): **prima** vedere se arriva l'evento winit
     `DroppedFile` tramite `on_winit_window_event` (feature `unstable-winit-030`); **solo se non arriva**, scheggia
     Win32 `IDropTarget` sull'`HWND` (`raw-window-handle`), ricordando che winit può aver già registrato il
     proprio bersaglio (revocare prima). È la funzione che TeraCopy rende primaria.
  2. **Selettore nativo di cartelle** con `rfd` (Slint non ne ha uno).
  3. **Icona nell'area di notifica** (`SystemTrayIcon`, nuovo nella 1.17) e notifica di fine.
- **Accessibilità**: albero UIA letto con Windows-MCP (`Snapshot`), prova con Narrator.
- **Renderer**: software predefinito; poi provare FemtoVG e, se serve, Skia, annotando la richiesta del Visual C++.

### 3.2 Che cosa si misura

**M0 — prima di ogni misura.** Salvare in `scripts/` uno script di misura (avvia l'eseguibile, attende il primo contenuto
leggibile letto dall'albero UIA, somma memoria privata e working set dell'**albero di processi**, misura
tempo a finestra e a contenuto) e rimisurare con quello **tutti** i valori di partenza, console attuale e
TeraCopy compresi. Senza M0 i confronti sotto non sono ripetibili. Valori di partenza provvisori (8 Ott 2026,
finestra aperta, elenco vuoto, metodo occasionale):

| ID | Misura | Console attuale | Soglia go | Soglia stretch |
|---|---|---|---|---|
| M1 | Memoria privata, albero di processi | 163-168 MB di serie · **113 MB con WebView2 alleggerito** | ≤ 60 MB | ≤ 40 MB |
| M2 | Working set | 378 MB | ≤ 120 MB | — |
| M3 | Processi | 7 | 1 | 1 |
| M4 | Tempo al primo contenuto leggibile (non alla sola finestra) | 0,70 s con contenuto; 0,12-0,24 s alla sola finestra: **metodi diversi, da ripetere con M0** | ≤ 0,4 s | ≤ 0,25 s |
| M5 | Dimensione exe | 12,4 MB | ≤ 20 MB | ≤ 12 MB |
| M6 | Dipendenze esterne richieste | WebView2 | nessuna | — |
| M7 | Avvio e uso in RDP / VM senza GPU | n/d | funziona, nessun errore | fluido |
| M8 | `check-static-crt.ps1` | passa | passa | — |
| M9 | Testo a 100 / 150 / 200 % DPI | n/d | leggibile, nulla tagliato | — |
| M10 | Tema chiaro/scuro che segue il sistema | solo nel browser | segue il sistema | — |
| M11 | Albero UIA (nomi, ruoli) e Narrator | n/d | ogni controllo ha nome e ruolo; Narrator legge la schermata Copia | — |
| M12 | Righe di codice per la schermata Copia (vs `Copy.svelte`, 251) | 251 | **indicativo**, non un criterio di decisione | — |
| M13 | Drop da Explorer sulla finestra | non supportato | **funziona** (evento winit o scheggia) | — |
| M14 | Tempo per cambiare un testo e rivederlo | non misurato | ≤ 10 s con anteprima | — |
| M16 | **Lista lunga**: 100.000 righe (elenco file di un report) scorrono senza scatti e senza far crescere la memoria in modo proporzionale | non misurato | scorrimento fluido, memoria stabile | — |
| M17 | **Robustezza della finestra**: la finestra resta reattiva mentre si legge un'unità di rete lenta e mentre un figlio viene terminato | n/d | nessun blocco, nessun crash | — |
| M15 | Istanza unica: un secondo avvio consegna il lavoro alla finestra aperta | nessuna istanza unica dichiarata | funziona, con `--auto-config` | — |

### 3.3 Criteri di decisione (unica fonte delle soglie)

| Esito | Condizione | Cosa succede |
|---|---|---|
| **GO** | M1 ≤ 60 MB, M3 = 1, M4 ≤ 0,4 s, M6, M7, M8, M11, M13 e M15 soddisfatti; nessun difetto bloccante in tema o DPI | Si propone la Fase 2, con le misure nel documento |
| **GO condizionato** | M1 fra 60 e 100 MB **oppure** uno solo fra M13 / M11 / M15 non raggiunto ma con soluzione delimitata | Si decide con l'utente se il compromesso vale la riscrittura, mettendo a confronto il costo della Fase 2-5 con quello di alleggerire Tauri (§7) |
| **NO-GO** | M1 > 100 MB, **oppure** M7 fallisce, **oppure** M11 fallisce senza rimedio, **oppure** M4 > 0,8 s, **oppure** il drop da Explorer è impraticabile e l'utente lo considera essenziale | Si resta su Tauri e si **alleggerisce** quella console (§7), niente riscrittura |

**Perché 60 e 100.** Il riferimento realistico è TeraCopy a 23 MB. Tauri **di serie** pesa 168 MB e **alleggerito**
113 MB (§7): una console Slint a 100 MB varrebbe poco, quindi 100 è il limite oltre cui non ha senso procedere e 60
il valore che dimostra un vantaggio vero. La prima stesura aveva tre soglie diverse (60 / 80 / 100) in due
documenti; questa tabella le sostituisce tutte.

### 3.4 Cosa non si fa nella Fase 1

Nessun editor, nessuna scheda Job, nessuna pianificazione, nessun installer, nessuna traduzione. Se la
prova dimostra che serve altro per decidere, lo si dice e si chiede.

### 3.5 Risultati della Fase 1 (8 Ott 2026, ramo `spike/slint`, non committato)

**Cosa è stato costruito** (usa e getta): crate `rustcopy-ui`, 308 righe di Rust e 378 di Slint, Slint 1.18.1 con
rendering software e backend winit. Scelta cartelle (`rfd`), destinazione, **Copia** che usa `runner::plan_copy`,
`write_shell_drop_config` e avvia la **stessa CLI** della console attuale, avanzamento da `ProgressSample`, pagina
Report con quattro schede numeriche (`gui_api::read_report`), lista di prova da 100.000 righe, icona nell'area di
notifica, palette scura. **Una copia vera di 66 file e 120 MB è partita e si è conclusa dal prototipo** con "Copia
riuscita" e le schede popolate.

**Misure con lo script salvato** (`scripts/measure-ui-footprint.ps1`, M0; mediane su 3-5 avvii, stessa macchina):

| ID | Misura | Tauri di serie | Tauri alleggerito | TeraCopy 4 | **Slint (prova)** | Soglia GO | Esito |
|---|---|---|---|---|---|---|---|
| M1 | Memoria privata, albero | 165 MB | 116 MB | 23 MB | **6 MB** | ≤ 60 | ✅ |
| M2 | Working set | 369 MB | 369 MB | 53 MB | **25 MB** | ≤ 120 | ✅ |
| M3 | Processi | 7 | 7 | 1 | **1** | 1 | ✅ |
| M4 | Al primo contenuto leggibile | 0,79 s | 0,85 s | 1,10 s | **0,19 s** | ≤ 0,4 | ✅ |
| M4' | Alla sola finestra | 0,09 s | 0,08 s | 0,92 s | 0,08 s | — | — |
| M5 | Eseguibile | 12,4 MB | 12,4 MB | 6,0 MB | **13,9 MB** | ≤ 20 | ✅ |
| M6 | Dipendenze esterne | WebView2 | WebView2 | — | **nessuna** (nessun modulo GL/D3D/WebView caricato; 41 moduli di sistema) | nessuna | ✅ |
| M8 | `check-static-crt.ps1` | ok | ok | — | **ok** (21 DLL di sistema) | passa | ✅ |
| M16 | 100.000 righe | — | — | — | **+8 MB** (7→15 MB) e **stabile dopo 100 scatti di rotella**; il processo risponde | stabile | ✅ |

**Funzioni verificate sul binario compilato:**
- **M13 — drop da Explorer: FUNZIONA, senza alcuna scheggia COM.** Trascinando la cartella `src2` da Esplora
  file sulla finestra, è comparsa nell'elenco. Lo fa l'evento winit `DroppedFile` letto con
  `on_winit_window_event` (feature `unstable-winit-030`). **R1, il rischio più grande, è risolto.**
- **M11 — UI Automation: l'albero è leggibile** con nomi e ruoli giusti (Pulsante "Copia", "Report", "Aggiungi
  cartelle...", Modifica "Cartella di destinazione" con il suggerimento): gli strumenti che leggono Cobian WPF
  leggono anche questa finestra. **Narrator non è stato provato**: la macchina non ha un dispositivo audio.
- **M9/M10:** a `SLINT_SCALE_FACTOR=2` testo nitido e leggibile; palette scura che segue lo schema
  (forzata con una variabile di prova). Il titolo di finestra resta chiaro in scuro (la barra nativa non segue la
  palette: da sistemare con il tema winit).
- **M17:** mentre il selettore cartelle di Windows impiegava ~25 s ad apparire (dischi di rete), la finestra Slint
  è rimasta disegnata e viva; il dialogo gira su un thread a parte.

**Cosa NON ha funzionato o non è stato provato (da dire chiaramente):**
- **Icona nell'area di notifica: non è comparsa.** Il codice compila e `show()` non dà errore, ma né nell'area
  visibile né nel menu "icone nascoste" c'è la nostra icona. Causa sconosciuta; fallback pronto: il crate `tray-icon`.
- **M15 istanza unica: non realizzata** nel prototipo (da costruire in Fase 2 con un mutex nominato e il passaggio
  dell'argomento `--auto-config`).
- **M7 RDP / macchina virtuale senza GPU: non provato.** Il rendering software non carica moduli OpenGL né Direct3D,
  quindi non dovrebbe averne bisogno, ma non l'ho visto girare in RDP.
- **DPI reale a 125/150/200 %** non provato (non modifico impostazioni di sistema): solo il fattore di scala di Slint.
- **Notifica toast di fine, avanzamento dal vivo:** la copia di prova è finita in meno di un secondo, quindi
  la barra di avanzamento e il nome del file non sono stati visti in azione.
- **Prototipo minuscolo:** 6 MB oggi, ma l'applicazione vera avrà più schermate, font e dati. Il margine è enorme
  rispetto alla soglia (60 MB), non rispetto a niente.

**Verdetto proposto: GO condizionato**, nei termini del §3.3: tutti i valori numerici superano di molto le soglie
(la memoria è un decimo del limite e un quarto di TeraCopy, l'avvio è quattro volte più veloce di Tauri), il drop
funziona e l'albero di accessibilità è leggibile; restano **quattro verifiche economiche** da chiudere all'inizio
della Fase 2: icona nell'area di notifica (con `tray-icon` se serve), istanza unica, una sessione RDP, una prova
di Narrator su una macchina con audio. **Nessuna delle quattro, se fallisse, cambierebbe il verdetto sulla scelta
del toolkit**: sono funzioni da costruire, non limiti della libreria.

**Decisione sul confronto con Tauri alleggerito (§7):** Tauri alleggerito resta a 116 MB e 7 processi: la strada
"alleggerire" non raggiunge l'obiettivo, e quindi non è l'alternativa a cui tornare per le priorità dichiarate
(prestazioni elevate).

### 3.5b Chiusura delle quattro verifiche (Fase 2, 8 Ott 2026)

- **Icona nell'area di notifica: risolta.** Causa: avevo disattivato le feature predefinite di Slint e con esse `system-tray`.
  Riattivata, l'icona compare; chiudendo la finestra il processo resta vivo (tray-residente) e un clic sull'icona
  riporta la finestra.
- **Istanza unica: realizzata e provata.** Mutex nominato più pipe nominata; un secondo avvio con `--auto-config` consegna la
  richiesta alla finestra aperta e termina (un solo processo rimasto); la prima ha copiato i 30 file e mostrato il report.
  La pipe accetta solo `activate` e `auto-config <file>` e il file deve essere un `shell-drop-*.toml` nella cartella
  temporanea di rustcopy; ogni altra richiesta è rifiutata **e mostrata**. Provato in 14 test, uno dei quali a due istanze.
- **RDP e Narrator: ancora non provati** (nessuna sessione RDP né audio su questa macchina); restano condizioni da chiudere
  su una macchina adatta, non bloccano la costruzione.

### 3.5c Fase 3, prima parte (8 Ott 2026)

Costruito e provato sul binario compilato:

- **Elenco dei lavori** nella barra laterale, con vita ed esito a colpo d'occhio (*Riuscita, Simulazione, Da controllare,
  Interrotta, In corso*), data breve e, a copia finita, file e dimensione. Il giudizio "come è finita" è del core
  (`sessions::SessionLog`, nuovo modulo con 10 test): pulita secondo la lettura di successo di robocopy, mai dal codice di
  uscita; nessun report non è mai "riuscita"; una copia partita e mai finita è "interrotta".
- **Dettaglio di un lavoro** con le quattro schede numeriche, il motivo se c'è qualcosa da controllare (letto dal report su un
  thread a parte, scartato se nel frattempo si apre un altro lavoro) e i due gesti **Ripeti** (stesse cartelle, di nuovo
  pianificate dal core) e **Salva come attività** (stesso testo di configurazione di un drop più un nome validato; non
  sovrascrive mai; non può contenere mirror, pulizia, verifica né cifratura: lo prova un test).
- **Un lavoro nasce allo stesso modo** dalla finestra, da un secondo avvio e da Esplora file (`begin_from_config`).
- Prova dal vivo: tre lavori in elenco, Ripeti (0 file, già aggiornata), salvataggio in un file `.toml` con solo nome, origine
  e destinazione, copia di 2,2 GB su una condivisione di rete a 291 MB/s con la riga che passa da *In corso* a *Riuscita*.
- Impronta dopo questa fase: 1 processo, 7 MB privati, 27 MB di working set, 0,23 s al primo contenuto, 14,9 MB di eseguibile.

Non ancora: ETA e grafico di velocità (3b), opzioni della sessione dentro la sessione (3d), la verifica a fine copia dalla
scheda Copia. Quest'ultima richiede di cambiare una prescrizione di CLAUDE.md (`prepare_copy` non inoltra la verifica):
non è un divieto di sicurezza, la verifica non cancella nulla, ma lo faccio con un tipo che **non può** portare mirror né
pulizia, non con un parametro libero.

### 3.5d Fase 3, seconda parte (8 Ott 2026): verifica, tempo residuo, grafico

- **Verifica a fine copia** dalla scheda Nuova copia: una casella e tre algoritmi (xxHash3 veloce, SHA-256, BLAKE3), con la
  frase onesta accanto: xxHash3 scopre gli errori di copia ma non è crittografico. È **l'unica opzione che una copia della
  console può aggiungere**, ed è un tipo (`Option<HashAlgorithm>`), non un campo libero: non esiste un parametro da cui
  possano entrare mirror, pulizia o altro (`runner::shell_drop_config_text`, con test). Il lavoro ricorda la verifica, la
  mostra nel dettaglio, la riporta in **Ripeti** e nell'attività salvata. Provata dal vivo: 14 file, 840 MB, verificata con
  xxHash3, anche verso una destinazione di rete.
- **Tempo residuo** calcolato dal core (`ProgressSample::eta_seconds`): solo durante la copia, mai nei primi 2 secondi, mai
  con totale ignoto o a zero, mai oltre una settimana, sulla velocità media dall'inizio (si muove piano e non salta).
- **Grafico della velocità** degli ultimi 30 secondi (campioni a 250 ms), disegnato da una funzione testata
  (`format::chart_path`); **non disegna nulla finché non si è mosso qualcosa**, perché una linea piatta sul fondo direbbe «fermo»,
  che è un'affermazione, non l'assenza di dati.
- **Limite onesto, visto dal vivo:** il motore pubblica i byte quando un file finisce, quindi con **pochi file enormi**
  (due da 3 GB) la barra resta indeterminata e tempo residuo e grafico non hanno nulla da dire fino alla fine di ogni file.
  Con molti file funzionano; con file giganti serve un avanzamento a metà file, che non è nel motore e non è una scelta della
  console. Resta dichiarato, non nascosto.
- **Errore trovato e corretto:** `format::throughput_mbps` usava 1024² byte per MB mentre il core usa 10⁶ (`progress`): la
  velocità di una sessione nell'elenco e quella del report avrebbero potuto differire del 5 %.
- Il blocco «in corso» dice **quale lavoro** sta girando: prima mostrava il modulo con le scelte di un'altra copia mentre ne
  girava una avviata da Esplora file.

### 3.5e «Al termine» (8 Ott 2026)

Una casella «Apri la cartella di destinazione alla fine»: si apre in Esplora file **solo se la copia è finita pulita** (dopo un problema si legge prima l'esito) ed è una scelta di *quella* copia, non una impostazione salvata con l'attività. Se la finestra non ha il focus, il pulsante nella barra delle applicazioni lampeggia. Provato dal vivo l'apertura della cartella; **il lampeggio no** (non l'ho osservato). **Notifica di sistema (toast) non fatta**: richiederebbe una dipendenza nuova da valutare con `cargo audit`; il lampeggio copre il caso «guardavo altrove» senza aggiungere nulla.

### 3.5f Fase 4a: attività salvate ed esecuzione sul posto (8 Ott 2026)

- **Ogni attività salvata ha una cartella sua** (`attivita\<nome>\<nome>.toml`): la CLI gira con quella cartella come cartella di lavoro, quindi report e storico delle esecuzioni finiscono lì. Con un'unica cartella condivisa due attività avrebbero scritto lo **stesso** `robocopy_ingest_report.json` e si sarebbero sovrascritte; l'ho visto leggendo il codice prima che accadesse, non dopo.
- **Pagina Attività**: elenco delle attività salvate con l'esito dell'ultima esecuzione da questa console («Mai eseguita» se non c'è), pulsante **Esegui**, e **Esegui un file di configurazione...** per i `.toml` già esistenti (si eseguono dove sono, con le loro impostazioni; mirror e pulizie restano protetti dalla conferma della CLI, che senza terminale si rifiuta).
- **Un'attività eseguita è un lavoro** come gli altri: compare nell'elenco, **Ripeti** la rilancia dal suo file (non la ripianifica da cartelle) e il lavoro ricorda quale attività era.
- **Il report conta solo se è fresco**: per un'attività il core cerca i report dove li scrive la sua configurazione e accetta solo quelli modificati dopo l'inizio dell'esecuzione (test con un report vecchio di un'ora: non rende pulita una run che non ha scritto nulla).
- Provato dal vivo: salvata una copia da 25 file, aggiunto un file nuovo nella sorgente, **Esegui** dalla pagina Attività: ha copiato **1 file (32 B)** e saltato i 25 già presenti, cioè l'aggiornamento incrementale di robocopy che è il punto di forza di rustcopy.
- Limiti: l'editor, la griglia delle proprietà e la creazione di pianificazioni non ci sono ancora (4b-4d e Fase 6). Un file `.toml` con percorsi relativi funziona perché la CLI parte nella sua cartella, ma non l'ho provato con un file «vero» dell'utente.

### 3.6 Consegna del cancello

Un breve rapporto (stesso formato di questo pacchetto, OKF) con la tabella M1-M14 compilata, schermate del
prototipo, i problemi incontrati e una raccomandazione. **Poi si aspetta.**

## 4. Fasi 2-5 — costruzione (solo dopo il GO)

### 4.1 Fase 2 — Fondamenta e catalogo dei comportamenti

1. **Catalogo dei comportamenti validati dal vivo.** [CLAUDE.md](CLAUDE.md) contiene decine di prescrizioni nate
   da difetti reali della console attuale: contatore di generazione per scartare risposte fuori ordine, flag
   monouso `pendingRunAttach`, sezioni `<details>` che si aprono da sole con valori non predefiniti, fraction
   di avanzamento limitata a `0.99`, `PathBar` con margine che collassa, controlli che non dicono mai il
   valore di un webhook intero, ecc. **Prima di scrivere schermate**, ogni voce diventa un caso di prova nel
   catalogo (cosa, perché, come si verifica) così che la riscrittura non reintroduca un difetto già pagato.
2. **Crate `rustcopy-ui`** con dipendenza diretta da `rustcopy-core`; `rustcopy-gui` (Tauri) non si tocca.
3. **Sistema visivo**: token (colori chiaro/scuro, tipografia 14 px minimo 12, spaziature, raggi), componenti
   `Badge`, `StatCard`, `Card`, `PathBar`, `Banner`, `Tooltip` con descrizione accessibile (RF-Y07).
4. **Infrastruttura**: stato di sessione condiviso, runner che avvia la CLI **accanto** al supervisore (mai su
   `PATH`), polling con generazione, notifiche, tray, schede Win32 (drop, finestra di scelta cartelle).
5. **Stringhe in un solo posto** (RF-Y09), anche se la sola lingua resta l'italiano.
5b. **Contratti da non rompere** ([SPEC §9.1](SPEC_GUI_SLINT.md)): nome `rustcopy-gui.exe`, argomento `--auto-config`,
   installer, `check-versions`, `check-static-crt`, job di CI. **Istanza unica** (RF-Y11): un secondo avvio consegna
   il lavoro alla finestra aperta; da provare anche con la console attuale, dove non è dichiarata.
6. **CI**: job `windows-latest` per `rustcopy-ui`, fuori da `--workspace` cross-platform come `rustcopy-gui`.

Cancello: catalogo completo e componenti pronti, **prima** delle schermate.

### 4.2 Fase 3 — Lavoro immediato (ingresso TeraCopy)

Onde, ciascuna una PR con prova dal vivo sul binario compilato:

- **3a** Sessione: cartelle (scelta e drop), destinazione con recenti/preferiti, anteprima ("Controlla prima"),
  `plan_copy` con i rifiuti già esistenti, avvio con `prepare_copy`.
- **3b** Esecuzione: avanzamento, file in corso, velocità, ETA (da calcolare nel core, non nel frontend),
  grafico di velocità dai campioni a 200 ms, ferma/riprendi da checkpoint.
- **3c** **Elenco dei lavori** (RF-S04) con vita (*una tantum / salvato / pianificato*) ed esito (✓ / ! / ✗) a colpo
  d'occhio, "ripeti" e **"Salva come attività"** (RF-C12); **istanza unica** (RF-Y11). Cobian non mostra l'esito né
  in elenco né in Storia (studio §3.5): da noi sì.
- **3d** Opzioni della sessione dentro la sessione (RF-C13): conflitti già esprimibili, algoritmo, thread,
  verifica, "al termine"; "Salva come predefinito".

### 4.3 Fase 4 — Lavori salvati e pianificati (ingresso Cobian)

- **4a** Lo stesso elenco con filtro (*Tutti / Salvati / Recenti*), ultima esecuzione e prossima pianificata (F62), riordino.
- **4b** Griglia Proprietà/Valore con la **provenienza** di ogni valore e schede Storia/Registro.
- **4c** Editor a schede **Base · Quando · Conservazione · Filtri · Sicurezza · Prima e dopo · Avanzate**,
  ciascuna mostrando solo ciò che le appartiene (RF-Y10); tooltip su ogni controllo; opzioni non applicabili
  grigie con il motivo; **solo scrittura di proposte** (`job_editor`: restringere il rischio, mai allargarlo).
- **4d** Storico e `--advise`, Report con le schede numeriche.

### 4.4 Fase 5 — Parità e rimozione di Tauri

1. **Checklist di parità** compilata voce per voce contro la console attuale (ogni scheda, ogni comportamento
   del catalogo), con prova dal vivo.
2. **Installer**: il componente "console" installa `rustcopy-ui.exe`; WebView2 non più richiesto; l'estensione
   Shell continua a trovare la console accanto alla DLL; installer smoke aggiornato.
3. **Rimozione** di `rustcopy-gui` e della toolchain JS **solo dopo conferma esplicita** dell'utente, in una PR
   separata e annullabile.

## 5. Fase 6 — Funzioni nuove che dipendono dal core

Nessuna parte della riscrittura le richiede; ognuna ha una **decisione propria** (§6) e il suo giro di prova.

| Funzione | Cosa serve nel core | Confine | Note |
|---|---|---|---|
| Pausa/Riprendi (RF-E06) | Livello A: sospensione del processo robocopy, con timeout | regola di sicurezza non toccata | Prova su SMB reale prima del rilascio |
| Opzione facoltativa «non toccare i file più recenti in destinazione» (RF-C07) | `/XO`, spenta di base; nessun cambio del predefinito | regola di sicurezza non toccata | "Mantieni entrambi" richiede il motore naive |
| Sposta (RF-C09) | Copia → verifica → conferma → elimina origine, mai non presidiato | **regola di sicurezza toccata** | Due passi, con conferma |
| Creare pianificazioni (RF-A09) | La GUI prepara, la **CLI** installa dopo UAC visibile | **regola di sicurezza toccata** | Mai privilegi permanenti nella GUI |
| Scrittura di eventi pre/post (RF-A10) | Validazione e conferma | regola di sicurezza | Decisione F55 aperta |
| Notifica via posta (RF-S07) | Sink SMTP | — | Oggi webhook soltanto |
| File singoli (RF-C03) | Sorgente non-cartella | — | Estende `plan_copy` e il runner |
| Algoritmi di checksum aggiuntivi | Nuovi `HashAlgorithm` | — | Oggi tre |
| Forza completo / Reimposta (RF-A06) | Operazione sul manifest | — | Economico |
| Impedire la sospensione, non a batteria (RF-E10, RF-A13) | `SetThreadExecutionState`, controllo alimentazione | — | Economico |

## 6. Decisioni che spettano all'utente 🔒

Per ciascuna do la mia raccomandazione. **Stato al 8 Ott 2026 (risposte dell'utente):** **chiuse** la 2 (un'app, un'entità "lavoro"), la 3 (badge) e la 13
(studio dal vivo, eseguito). **Risolte con la proposta dei livelli di sicurezza** la 8 e la 9 ([SPEC §10.1](SPEC_GUI_SLINT.md)); l'attuazione è lavoro della
Fase 6 e non blocca la Fase 1. Le altre sono aperte, con la mia raccomandazione.

| # | Decisione | Opzioni | Raccomandazione | Perché |
|---|---|---|---|---|
| 1 | **Avviare la Fase 1** (prova a tempo) | sì / no | **Sì**, dopo la tua conferma di questi documenti | Costa pochi giorni e sostituisce le ipotesi con misure |
| 2 | **Forma del prodotto** | due app / una app con due modalità / **una app con un'unica entità "lavoro"** | **Una app, un'entità, due ingressi** (SPEC §5.0) — ✅ **approvata dall'utente il 8 Ott 2026** | Sessione di TeraCopy e attività di Cobian sono lo stesso oggetto a profondità diverse; evita l'interruttore di modalità |
| 3 | **Come citare Slint** | schermata "Informazioni" / badge sulla pagina di download / GPLv3 | ✅ **Badge sulla pagina di download** (scelta dell'utente, 8 Ott 2026); una schermata Informazioni resta facoltativa | La forma più leggera; non cambia la licenza del codice. Prima di distribuire rileggo i termini |
| 4 | **Renderer e Visual C++** | software predefinito / Skia | **Software** | Skia richiede il VC++ Redistributable e contraddice D30 |
| 5 | **Trascinamento da Explorer** | evento winit, poi scheggia Win32 / rinunciare | **Evento winit, poi scheggia**, con criterio go/no-go | È la funzione primaria di TeraCopy |
| 6 | **Migrazione** | affiancare e poi rimuovere / sostituire subito | **Affiancare** fino alla parità | Nessun rischio per gli utenti attuali |
| 7 | **Lingua** | italiano soltanto / italiano + inglese | **Italiano ora, struttura per tradurre** | Costo contenuto, evita la lingua mista |
| 8 | **Allentare la regola di sicurezza**: creare pianificazioni dalla GUI | livelli scelti dall'utente nelle impostazioni | ✅ **Impostazione a tre livelli** (Prudente predefinito / Standard / Esperto) proposta dall'utente il 8 Ott 2026, [SPEC §10.1](SPEC_GUI_SLINT.md); resta da confermare che Esperto escluda i mirror non presidiati | Mantiene il sicuro come norma e dà potenza a chi la vuole |
| 9 | **Sposta** (copia, verifica, poi cancella l'originale) | livello Standard, due passi con conferma | ✅ **Disponibile dal livello Standard** | Valore reale, sicurezza nell'ordine e nel livello |
| 10 | **Collisioni** | nessuna scelta / opzione facoltativa «non toccare i file più recenti in destinazione» / motore interattivo | ✅ **Nessuna nuova scelta e nessun cambio di predefinito** (chiarito con l'utente l'8 Ott 2026): resta il comportamento di robocopy, copia nuovi e cambiati e salta gli identici. L'opzione facoltativa si valuta solo se serve; «Salta se esiste già» è scartato perché non aggiornerebbe più i file modificati | Rustcopy è un aggiornatore incrementale, non un copiatore una tantum: il valore è la velocità del secondo passaggio. Una versione precedente di questo piano proponeva il predefinito *Salta* di TeraCopy: era un errore, nato dal confronto con un programma che risolve un altro problema |
| 11 | **Tray** | sì / no | **Sì** (stato + apri); serve anche all'istanza unica | Presenza senza finestra, come entrambi i riferimenti |
| 12 | **File singoli** | estendere il motore / solo cartelle | **Estendere**, come lavoro di Fase 6 | TeraCopy è usato soprattutto su file singoli |
| 13 | **Studio dal vivo ulteriore** | sì / no | **Fatto** (8 Ott): resta aperto solo l'elenco di §8 | — |

**Nota sulla decisione 3.** Con il badge si assolve l'obbligo di attribuzione della licenza *Royalty-free* di Slint
(il badge va su una pagina pubblica, preferibilmente dove si scaricano i binari: il README e la pagina della release). Il
codice del progetto resta MIT. Non è una consulenza legale: rileggo i termini sul sito ufficiale prima di distribuire.

## 7. Alternativa se la Fase 1 dà NO-GO: alleggerire la console attuale

Se Slint non regge i criteri, il lavoro non è sprecato: le Fasi 0-1 hanno prodotto numeri e una specifica
di prodotto (un'entità "lavoro" con due ingressi, requisiti RF-C13 e RF-Y07-RF-Y11) applicabili a Tauri. Le leve concrete, da
misurare:

- **Processi**: ridurre i sette processi WebView2 (impostazioni di `--process-per-site`, finestra nascosta alla chiusura).
- **Memoria**: caricare le schede a richiesta, liberare lo stato delle schede non visibili.
- **Avvio**: rimandare il lavoro del core a dopo il primo disegno.
- **Funzioni**: portare comunque in Svelte la cronologia laterale, le opzioni di sessione, i tooltip su ogni controllo.

**Misurato il 8 Ott 2026** sulla console 7.8.1 installata, un'esecuzione per configurazione, albero di processi
completo, 6 s dopo l'avvio:

| Configurazione | Processi | Memoria privata | Working set |
|---|---|---|---|
| Di serie | 7 | 168 MB | 369 MB |
| `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--renderer-process-limit=1 --disable-gpu` | 7 | **113 MB** (-33 %) | 359 MB |

Quindi l'alleggerimento è reale (un terzo di memoria privata) ma **non porta a 60 MB né a un processo solo**: il
limite sta nell'architettura WebView2. Una misura per configurazione, nessuna ripetizione, nessun effetto di
`--disable-gpu` sulla resa verificato: serve prova prima di usarlo davvero.

## 8. Studio dal vivo: fatto e ancora aperto

**Fatto il 8 Ott 2026** (autorizzato dall'utente, solo cartelle temporanee create e poi rimosse): due copie vere in
TeraCopy con conflitto e file bloccato; creazione, esecuzione, Storia, Registro ed eliminazione di un'attività in
Cobian. Dettagli nello [studio](STUDIO_GUI_RIFERIMENTO.md) §2.3, §3.5, §8.

**Ancora aperto** (non blocca nulla): tooltip delle icone di TeraCopy, pausa/sposta/rete lenta/trascinamento da
Explorer, più attività e gruppi in Cobian, pianificazioni e comportamento a finestra chiusa. Vanno fatti solo se
la Fase 6 li richiede; **nessuno richiede di creare pianificazioni reali sulla macchina senza il tuo ok**.

## 9. Rischi del piano e ritorno indietro

| ID | Rischio | Mitigazione | Ritorno indietro |
|---|---|---|---|
| P1 | La prova a tempo si allunga | Limite di 5 giorni; riferire con ciò che c'è | Ramo isolato, si butta |
| P2 | Il drop da Explorer non funziona senza codice unsafe fragile | Scheggia piccola, isolata, con test sul binario; criterio go/no-go | Rinunciare al drop o restare su Tauri |
| P3 | Accessibilità insufficiente | Prova UIA/Narrator in Fase 1 | No-go o condizionato |
| P4 | Regressioni silenziose nella riscrittura | Catalogo dei comportamenti (Fase 2); parità verificata (Fase 5) | La console Tauri resta fino alla parità |
| P5 | Licenza incompatibile con il progetto | Leggere i termini in Fase 0-1, decisione 3 | Cambiare toolkit |
| P6 | Slint evolve e cambia API | Fissare la versione, un solo crate ne dipende | Isolamento nel crate `rustcopy-ui` |
| P7 | Costo di mantenere due console in parallelo | Parità come obiettivo, nessuna funzione nuova su Tauri nel frattempo | Fermare il lavoro e restare |
| P8 | L'installer cresce o perde la proprietà "senza prerequisiti" | `check-static-crt` e installer smoke come cancelli | Tornare al renderer software |
| P9 | Si copiano i difetti dei due riferimenti | Lo [studio](STUDIO_GUI_RIFERIMENTO.md) §5 come lista di regole | Revisione di specifica |
| P10 | Si rompono i contratti nascosti (nome eseguibile, `--auto-config`, installer, CI) | [SPEC §9.1](SPEC_GUI_SLINT.md) come lista di controllo; tenere il nome `rustcopy-gui.exe` | Revert della PR |
| P11 | Il vantaggio di memoria è sopravvalutato perché si confronta con Tauri di serie | Confronto anche con Tauri alleggerito (113 MB), §7 | No-go motivato |

**Come si torna indietro, in ogni fase:** fino alla Fase 5 non cambia nulla per chi usa la 7.8.1; ogni fase è
una o più PR a sé e si annulla con un `revert`. La Fase 5 è l'unica con un passo non banale (rimozione di
Tauri) ed è preceduta da una conferma esplicita; Tauri resta recuperabile dalla storia git.

## 10. Prossimo passo, se l'utente conferma

1. ✅ L'utente ha confermato i documenti e dato il via alla Fase 1 (8 Ott 2026).
2. Si rielabora ciò che emerge.
3. Solo allora, e con una nuova conferma, si apre il ramo `spike/slint` per la Fase 1, **cominciando da M0** (lo script di misura).

## 11. Criticità trovate rileggendo questo piano

- La prima stima assegnava un tempo alla riscrittura prima di aver provato Slint: l'ho convertita in "relativo
  fra le fasi" e ho legato ogni tempo reale al risultato della Fase 1.
- Il criterio di NO-GO per la memoria era "peggiore della console attuale"; è troppo permissivo: una console
  Slint a 150 MB non giustifica una riscrittura. La soglia è ora **60 MB** (M1), con riferimento reale a TeraCopy.
- Inizialmente la Fase 6 era dentro la costruzione. L'ho separata: le funzioni che toccano la regola di sicurezza non devono
  viaggiare dentro una migrazione tecnica, dove passerebbero senza una decisione propria.
- Il piano non dice quanto costi *non* migrare: l'alternativa di §7 non è stimata. È dichiarato, non risolto.
- **Tre soglie diverse per la stessa decisione** (60/80/100 MB) in due documenti: ora una sola tabella (§3.3).
- **Il termine di paragone era sbagliato**: Slint va confrontato con Tauri **alleggerito** (113 MB), non solo di serie
  (168 MB). Aggiunto M1 e §7.
- **Misure non ripetibili**: nessuno script e due metodi d'avvio diversi (0,70 s vs 0,12-0,24 s). M0 è ora la prima
  attività della Fase 1.
- **Il drop da Explorer partiva dalla soluzione più difficile** (scheggia COM) invece che dall'evento winit; e non
  considerava la registrazione già fatta da winit.
- **"Due modalità"** rivalutata in "un'entità, due ingressi" (decisione 2, SPEC §5.0).
- **La decisione sulla licenza era incomprensibile** per chi non conosce i termini: riscritta in parole semplici.
- **M12 (righe di codice) non è un criterio** di decisione: reso indicativo. **M14** non aveva un valore di partenza.
- **Contratti nascosti non elencati** (P10): nome eseguibile, `--auto-config`, installer, CI.
