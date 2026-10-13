---
type: Reference
title: Controlli manuali della console Slint
description: Elenco, con passi e risultato atteso, delle prove che solo una persona o una macchina adatta può fare sulla console Slint (lampeggio, RDP, Narrator, scala, tema scuro, Server senza VC++, chiavetta, posta, installazione vera), più le decisioni che restano all'utente.
status: draft
generated:
  by: process:claude-code
  at: 2026-10-09T14:00:00Z
---

# Controlli manuali della console Slint

Tutto ciò che si poteva provare da qui è stato provato e sta in [PIANO_GUI_SLINT.md](PIANO_GUI_SLINT.md) §0. Questo
documento raccoglie **solo ciò che resta a te**: servono una macchina diversa, una chiavetta, un server di posta, un
lettore di schermo o semplicemente i tuoi occhi su una scheda grafica vera. Ogni voce dice **come provarla** e **che
cosa deve succedere**; se il risultato è diverso, annota cosa vedi (meglio con uno screenshot) e dimmelo.

Spunta la casella quando la voce è fatta. Il piano riporta lo stesso elenco in forma breve.

Prima di cominciare: installa la versione da provare con l'installer (`rustcopy-<versione>-setup.exe`) e
lascia selezionati «CLI e notify-server» e «Console grafica». Molte voci dipendono da come l'installer ha
registrato le cose, quindi **vanno provate sulla copia installata, non su `target\release`**.

## A. Installazione e avvio

- [ ] **Aggiornare con la console aperta, e chiuderla con la X.**
  Passi: 1) apri la console, premi la X: con nessuna copia in corso deve **sparire anche dall'icona vicino all'orologio** (Gestione attività non deve più mostrare `rustcopy-gui.exe`). 2) avvia una copia lunga e premi la X: la finestra si nasconde ma la copia continua (l'icona resta); a copia finita, dopo una decina di secondi, anche il programma si chiude da solo. 3) riapri la console e lancia l'installatore **senza chiuderla**: prima di toccare qualsiasi file compare «rustcopy è ancora aperto...» con «Riprova» e «Annulla»; chiudi la console, premi «Riprova» e l'installazione prosegue; con «Annulla» si ferma senza cambiare nulla.

- [ ] **Installazione sul Windows Server senza Visual C++ (D30).**
  Passi: su un Server 2016/2019/2022 pulito, esegui l'installer; poi avvia la console dal menu Start.
  Atteso: nessuna richiesta di runtime, nessun errore di DLL mancanti; la console si apre. Il rapporto di installazione
  (cartella indicata dall'installer) non segnala errori. Se non si apre, copia il contenuto del rapporto.
- [ ] **Il collegamento del menu Start e la notifica di sistema.**
  Passi: dopo l'installazione, avvia una copia lunga (qualche GB), poi porta in primo piano un'altra finestra prima che finisca.
  Atteso: a copia finita compare una notifica «rustcopy» con una riga («src → out: copia riuscita, N file (X GB)») e il
  pulsante nella barra delle applicazioni lampeggia. Se **non** compare la notifica ma lampeggia, il collegamento con
  l'identità `rustcopy.console` non è stato registrato: dimmelo.
- [ ] **Il pulsante nella barra delle applicazioni ha l'icona di rustcopy** (non quella generica) e le proprietà del file
  `rustcopy-gui.exe` mostrano nome «rustcopy» e la versione giusta.
- [ ] **Disinstallazione.** Dopo averla fatta non devono restare il collegamento nel menu Start né la chiave
  `HKLM\SOFTWARE\Classes\AppUserModelId\rustcopy.console`.

## B. Aspetto e accessibilità

- [ ] **Lampeggio del pulsante nella barra delle applicazioni** a fine copia con la finestra senza focus (è la prova più
  semplice: sta già nel punto A, ma controllala anche **senza** la notifica, per esempio con le notifiche disattivate in Windows).
- [ ] **Scala di Windows al 125 %, 150 %, 200 %.**
  Passi: Impostazioni di Windows → Schermo → Scala; riavvia la console a ogni valore e apri, nell'ordine: Nuova copia,
  Attività, il dettaglio di una copia, Proprietà, Modifica, Storico, Chiavi e password, Pianifica.
  Atteso: nessun testo tagliato o sovrapposto, nessun pulsante fuori dalla finestra.
- [ ] **Tema scuro che segue il sistema, barra del titolo compresa.**
  Passi: con Windows in modalità scura apri la console.
  Atteso: sfondo scuro, testo leggibile, anche la barra del titolo scura. Poi passa a chiaro: deve seguire.
- [ ] **Narrator (lettore di schermo).**
  Passi: attiva Narrator (Win + Ctrl + Invio), apri la console e naviga con Tab.
  Atteso: legge i nomi dei pulsanti e dei campi di Nuova copia, Attività, dettaglio di una copia e la pagina Livello di sicurezza;
  i messaggi di errore o di avviso vengono letti con il loro prefisso («Avviso», «Controllo», ...).

## C. Macchine particolari

- [ ] **Sessione RDP e macchina virtuale senza accelerazione grafica.**
  Passi: collegati in Desktop remoto a una macchina con la console installata e, se puoi, avvia una VM senza GPU.
  Atteso: la finestra si disegna e risponde; il grafico della velocità, la barra e le pagine funzionano come in locale.
  (La console usa il rendering software, quindi non dovrebbe dipendere da OpenGL né da Direct3D: qui si vuole la prova.)
- [ ] **Server con Desktop Experience e Remote Desktop Session Host.** Se ci installi l'estensione Shell, ricorda l'avviso
  dell'installer: si carica nella sessione di ogni utente collegato.

## D. Funzioni che toccano dischi o rete

- [ ] **Comandi prima e dopo la copia nell'editor.**
  Passi: livello **Prudente**: Attività → «Modifica» → scheda «Avanzate»: le due caselle dei comandi sono **grigie** con «Richiede il livello Esperto». Passa a **Esperto** (pagina Sicurezza) e riapri «Modifica»: scrivi in «Comando prima della copia» `cmd /c echo prova > C:\\prova\\prima.txt` e in «dopo» un altro; «Salva come proposta»: atteso **nessun file scritto**, ma un riquadro con il testo esatto e «Confermo: scrivi la proposta»; «Annulla» torna indietro senza scrivere. Rifai e conferma: compare il percorso della proposta; aprila e verifica che contenga i due comandi. Sostituisci l'attività con la proposta e lancia «Esegui»: il file `prima.txt` deve comparire **prima** della copia. Poi torna a Prudente: i comandi sono visibili ma non modificabili, e «Togli» funziona.

- [ ] **«Esegui tutte» e «Sospendi».**
  Perché a mano: la coda gira nella finestra vera e le copie sono vere (la logica della coda è provata a parte, ma non il collegamento con la finestra). Passi: crea tre attività piccole con «Nuova attività» (cartelle di prova); premi «Esegui tutte». Atteso: partono **una alla volta** nell'ordine dell'elenco (mai due insieme) e alla fine compare «Esegui tutte: 3 attività, 3 riuscite.». Poi «Sospendi» la seconda (la riga diventa attenuata e dice «Sospesa») e rilancia «Esegui tutte»: partono solo le altre due. «Riattiva» la rimette. Durante la coda premi «Ferma»: la copia in corso si interrompe e **le altre non partono** (il messaggio dice «Interrotta»). Con una attività pianificata: «Sospendi» la mette in «Disabilitata» in Utilità di pianificazione e «Riattiva» la riabilita; con il livello Prudente «Riattiva» una pianificazione dice che serve Standard.

- [ ] **Procedura guidata «Nuova attività» con i selettori veri.**
  Perché a mano: usa i dialoghi nativi «Aggiungi cartelle/file» e «Sfoglia...», che l'automazione non pilota. Passi: Attività → «Nuova attività»; passo 1: un nome, «Aggiungi cartelle...» con due cartelle di prova, togline una con «Togli»; passo 2: «Sfoglia...» per la destinazione e scegli «Incrementale»; passo 3: «Solo a mano» (poi, dal livello Standard, anche «Pianificata» con un'ora); passo 4: lascia tutto; passo 5: leggi il riepilogo e premi «Crea attività».
  Atteso: l'attività compare nell'elenco con «Incrementale»; «Esegui» fa **una copia completa** la prima volta (non un errore) e dalla seconda copia solo le novità. Riprova lo stesso nome: «Esiste già un'attività...» e l'attività di prima non cambia. Con il livello Prudente la voce «Pianificata» è grigia con «Richiede il livello Standard».

- [ ] **Espelli l'unità alla fine, con una chiavetta.**
  Passi: 1) scegli una destinazione sulla chiavetta (`E:\...`): deve comparire la casella «Espelli l'unità alla fine»; con una
  destinazione su un disco fisso la casella **non** deve comparire. 2) spunta la casella e copia pochi file: a copia finita il
  messaggio «L'unità E: è stata espulsa: puoi staccarla» e la chiavetta sparisce da Esplora file. 3) ripeti tenendo **un file aperto**
  sulla chiavetta (per esempio in Blocco note): atteso «ancora in uso... non l'ho espulsa» e la chiavetta resta com'era.
- [ ] **Pianificazione con privilegi elevati (VSS).**
  Perché a mano: la richiesta di Windows (UAC) è una finestra sicura che nessuna automazione può premere. Passi: livello **Esperto**
  (pagina «Sicurezza»); Attività → «Pianifica» di un'attività **senza** mirror né comandi, con percorsi completi; spunta «Con privilegi
  elevati» → «Installa»: compare **una** richiesta di Windows, accettala. Atteso: «Pianificazione elevata ... installata», l'attività
  compare con «pianificata» in Utilità di pianificazione con «Esegui con i privilegi più elevati» e **non** chiede più nulla;
  `C:\ProgramData\rustcopy\elevated\<nome>\config.toml` esiste e, da un prompt normale, **non** puoi modificarlo (accesso negato).
  Controlla anche che nella stessa cartella ci siano `robocopy_ingest.exe`, `config.toml` e (dopo la prima esecuzione) il report e il registro, e che
  da un prompt normale non riesci a modificarne nessuno. Prova poi con un file di Office aperto nella cartella di origine: dopo «Esegui» dall'Utilità di pianificazione il file si copia.
  Infine «Togli la pianificazione con questo nome»: una richiesta, e task e `config.toml` spariscono (i report restano).
  Se rifiuti la richiesta di Windows: «Hai annullato la richiesta», nulla cambia.
- [ ] **Avvio di uno «Sposta» dalla casella, e file singoli, dalla finestra.**
  Perché a mano: sono le due funzioni che passano dal dialogo nativo «Aggiungi cartelle/file», che non sono riuscito a pilotare.
  Passi: livello Standard (pagina «Sicurezza»); Nuova copia → «Aggiungi file...» e scegli un paio di file; spunta «Sposta»;
  destinazione di prova; Copia. Poi apri il dettaglio e usa «Controlla gli originali da cancellare...» e «Cancella gli originali».
  Atteso: la verifica è attiva da sola; il controllo elenca i file con copia identica; dopo la conferma spariscono **solo** quelli,
  le copie restano. Prova anche a creare un file nuovo nella cartella di origine tra la copia e la conferma: deve restare.
- [ ] **Posta con un server vero.**
  Passi: dalla console apri Attività → «Notifiche per posta...», compila server, protezione, utente, mittente e destinatario, scrivi la password
  nel campo mascherato con un nome (es. `smtp-rustcopy`), «Salva» e **«Invia una prova»**: atteso «Messaggio di prova inviato a ...» e in casella un
  messaggio con «PROVA». Poi la prova completa: avvia `notify-server.exe --config` con il file indicato dalla pagina e lancia una copia con
  `--webhook-url http://127.0.0.1:3000/notify`.
  Atteso: arriva un messaggio con oggetto «[rustcopy] OK <computer>: <origine>» e il riepilogo. Prova anche con la password sbagliata:
  l'errore deve comparire nel registro del server, e il backup **non** deve fallire.
- [ ] **Il PC non va in sospensione durante una copia.**
  Perché a mano: serve un prompt con privilegi di Amministratore e una copia abbastanza lunga. Passi: avvia una copia grande dalla console
  (o «Esegui» su un'attività); in un prompt **elevato** esegui `powercfg /requests`. Atteso: sotto «SISTEMA» compare `rustcopy-gui.exe`
  (o il suo percorso); a copia finita, rieseguendo il comando, non c'è più. Non tiene acceso lo **schermo**: può spegnersi, il sistema no.
- [ ] **Pausa lunga oltre 10 minuti** (facoltativa). Metti in pausa una copia sul NAS e lasciala: dopo 10 minuti la console deve
  riprenderla da sola e dirlo. (La pausa di 3 minuti sul NAS è già provata.)

## E. Esplora risorse (estensione Shell)

- [ ] **Trascinamento da Esplora file con la console nuova.**
  Passi: con la console installata, trascina una cartella su un'altra cartella tenendo premuto il tasto destro e scegli la voce «Copia con RustCopy».
  Atteso: si apre la console e parte la copia con la sua barra. (Il comando dell'estensione cerca `rustcopy-gui.exe` accanto alla sua DLL:
  ora è la console Slint con lo stesso nome.)

## F. Decisioni che restano a te

- [ ] **Rimozione di Tauri e della toolchain JS (fase 5e).** È l'ultimo passo del piano. Dopo averla confermata la farò in una PR separata,
  annullabile con un `revert`: toglie `crates/rustcopy-gui`, `ui/` (Node/npm), i job CI `gui` e `gui-npm-audit` e le dichiarazioni di versione
  di Tauri in `check-versions.sh`. Conviene confermarla **dopo** i controlli di A, B e E.
- [ ] **Licenza di Slint.** Ho messo il badge «Made with Slint» nel README, come chiede la licenza royalty-free, ma **non è una
  consulenza legale**: rileggi i termini sul sito di Slint prima di distribuire a terzi, soprattutto perché il codice del progetto è MIT.
- [ ] **Se ti serve l'inglese**: oggi la console è solo in italiano (struttura pronta per tradurre, nessuna traduzione scritta).

## Come segnalarmi un problema

Dimmi: la voce (per esempio «B – scala 150 %»), cosa hai fatto, cosa ti aspettavi, cosa è successo, e se puoi uno screenshot. Per i problemi
di installazione allega il rapporto di installazione (la cartella è scritta nell'ultima schermata dell'installer).
