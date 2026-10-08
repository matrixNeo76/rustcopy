---
type: Reference
title: Studio delle GUI di riferimento (TeraCopy 4 e Cobian Reflector)
description: Anatomia, funzioni e schemi di interazione delle due interfacce che la nuova console vuole unire, ricavati dall'immagine fornita dall'utente, dalla documentazione ufficiale e dall'uso diretto dei due programmi installati (a riposo); matrice di copertura rispetto a rustcopy, difetti da non copiare e checklist di verifica dal vivo. Base della specifica e del piano della GUI Slint.
status: draft
generated:
  by: process:claude-code
  at: 2026-10-08T10:30:00Z
---

# Studio delle GUI di riferimento: TeraCopy 4 e Cobian Reflector

Documento 1 di 3 del pacchetto "GUI Slint": questo studio, la specifica ([SPEC_GUI_SLINT.md](SPEC_GUI_SLINT.md)) e il piano ([PIANO_GUI_SLINT.md](PIANO_GUI_SLINT.md)).
Nessuno dei tre autorizza sviluppo: servono la conferma dell'utente dopo ogni rielaborazione.

## 1. Scopo, metodo e limiti dello studio

**Domanda.** L'obiettivo dichiarato dall'utente fin dall'inizio è una GUI "mista" fra due programmi che
fanno lavori diversi: **TeraCopy** (copia interattiva, immediata) e **Cobian Reflector** (backup
configurato e pianificato), ma costruita sul nostro motore (`robocopy` avvolto in Rust) per essere più
veloce, robusta e intuitiva. Questo studio scompone le due interfacce in ciò che mostrano, ciò che
permettono e ciò che presuppongono, per poi dire cosa prendere, cosa adattare e cosa lasciare.

**Fonti, in ordine di affidabilità.**

| Fonte | Che cosa dà | Limite |
|---|---|---|
| **Uso diretto dei due programmi installati** (8 Ott 2026, Windows-MCP) | Struttura reale di dialoghi, schede, tooltip, valori predefiniti; impronta di memoria | A riposo, senza dati: vedi limiti sotto |
| Immagine allegata dall'utente (8 Ott 2026) con le due finestre affiancate | Anatomia reale delle schermate principali, in italiano | Una sola schermata per programma, entrambe vuote o quasi (nessun trasferimento in corso) |
| Manuale ufficiale di Cobian Reflector: [attività](https://cobiansoft.com/crHelp/tasks.html), [menu](https://cobiansoft.com/crHelp/menus.html), [opzioni](https://cobiansoft.com/crHelp/options.html), [tutorial](https://cobiansoft.com/crTutorial/page6.html) | Elenco completo di menu, schede e opzioni | Descrive, non mostra; non dice com'è fatto il dialogo |
| Pagina ufficiale di [TeraCopy](https://www.codesector.com/teracopy) e blog del produttore ([finestra di sostituzione](https://blog.codesector.com/2015/10/01/teracopy-replace-dialog/)) | Elenco di funzioni Free/Pro, comportamento delle collisioni | Marketing più che manuale; il pannello Opzioni non è elencato |

**Aggiornamento, stesso giorno.** La prima stesura di questo studio era scritta prima di avere i
programmi. Poi l'utente ha installato **TeraCopy 4** e **Cobian Reflector 2.7.2.0** su questa macchina e li
ho aperti con Windows-MCP (albero UIA + schermate). Le sezioni 2.3, 2.4 e 3.5 riportano **ciò che ho visto**; il
resto del documento, dove contraddetto, è stato corretto (§9).

**Limiti, dichiarati.**

- **Cobian**: oltre a menu, "Nuova attività" (otto schede) e "Impostazioni" (dieci), ho **creato un'attività di
  prova** su cartelle temporanee, **l'ho eseguita**, ho letto Storia e Registro e poi **l'ho eliminata** (senza
  spuntare "elimina anche i file di backup"; le cartelle di prova sono state rimosse a parte). Non ho provato:
  gruppi, più di un'attività, il Gestore code in uso, pianificazioni, parcheggio. Il pulsante "Servizio e
  applicazione" chiede UAC (`consent.exe`) e blocca la cattura dello schermo: l'ho lasciato stare.
- **TeraCopy**: ho eseguito **due copie vere** (45 file, 40 MB) da riga di comando verso cartelle temporanee, la
  seconda con un conflitto e con un file bloccato apposta. Ho visto la finestra dei conflitti, il riepilogo
  con i filtri, il grafico e il registro. Non ho provato: pausa, "Sposta", "Elimina", copie verso più
  destinazioni (funzione Pro), trascinamento da Explorer, area di notifica.
- **Misure**: una macchina, una sessione, programmi a riposo. Sono ordini di grandezza, non un benchmark; il
  **metodo non era stato salvato in uno script**, quindi non è ancora ripetibile (vedi PIANO §3.2, M0).
- **Linguaggio dei tooltip UIA**: nell'albero UIA di Cobian i testi d'aiuto perdono le lettere accentate
  ("attivit", "verr"); non so se è un difetto del programma o dello strumento di lettura, quindi non lo uso
  come prova.

## 2. TeraCopy 4: anatomia della finestra

Dall'immagine (tema chiaro, italiano). Una finestra unica, **non modale**, con una colonna di cronologia
a sinistra e un'area di lavoro a destra.

```
+--------------------------------------------------------------------------------------+
| [pannello] [+] [copie] [orologio] [pin]   TeraCopy 4     [chiave][carta][tema][utente][ingranaggio][i][v] |
+----------------------+---------------------------------------------------------------+
| ott 08, 2026 11:21   | [Filtro]  Elabora file: ( Nessun file aggiunto )        0 / 0   |
|  Nessun file agg.  0 | [Origine]                                              0 B / 0 B |
|  Nessuna destin.  0B | [Destinaz.]                                                      |
|----------------------| [Copia...] [Sposta...] [Verifica] [Verifica] [Elimina...] [xxHash3-64] |
| ott 08, 2026 07:40 v |---------------------------------------------------------------|
|  C:\..\SOTA-OS.zip 1 | Elenco files | Destinaz. | Opzioni | Stato | Registro          |
|  C:\Users\..  42 MB  | [menu] [impronta] [stampa]      [x] Scorrimento automatico  [Tutte le voci v] |
|----------------------|                                                               |
| giu 25, 2024 09:58 v |                  Trascina qui i file o le cartelle             |
|  C:\Users\m..iso   1 |                                                               |
|  D:\            5,1 GB|                                                              |
+----------------------+---------------------------------------------------------------+
```

### 2.1 Che cosa si vede, zona per zona

- **Barra del titolo come barra degli strumenti.** A sinistra cinque icone (mostra/nascondi pannello,
  nuovo, copie/coda, cronologia, fissa in primo piano); a destra sette icone (licenza, carta, aspetto,
  profilo utente, impostazioni, informazioni, spunta). Nessuna etichetta: si capisce per convenzione, non
  per testo (vedi §5).
- **Colonna di cronologia (sinistra).** Ogni voce è una *sessione di copia*: data e ora, ✓ se conclusa,
  riga con origine abbreviata (`C:\..\SOTA-Agentic-OS.zip`), destinazione (`C:\Users\matrix\Desktop`),
  numero di file (`1`) e dimensione (`42 MB`). La voce in alto, con l'icona "espandi", è la sessione
  **corrente/nuova**, vuota: "Nessun file aggiunto 0 / Nessuna destinazione 0 B".
- **Intestazione della sessione.** Un'etichetta a pillola "Elabora file: *Nessun file aggiunto*" che cambia
  testo in base al contenuto; due contatori allineati a destra (`0 / 0` file, `0 B / 0 B` byte). I pulsanti
  **Filtro**, **Origine** e **Destinaz.** aprono la scelta di cosa copiare e dove.
- **Riga delle azioni.** `Copia...`, `Sposta...`, `Verifica`, `Verifica`, `Elimina...` e, a destra, una
  pillola con l'algoritmo di verifica (`xxHash3-64`). I puntini di sospensione indicano che l'azione apre
  una scelta (di destinazione); i due `Verifica` identici dell'immagine sono uno **stato transitorio** della
  finestra vuota: dopo una copia ne resta uno (verificato dal vivo, §2.3).
- **Cinque schede** sotto le azioni: *Elenco files*, *Destinaz.*, *Opzioni*, *Stato*, *Registro*.
  Sotto, una striscia con tre pulsanti (menu delle voci, impronta/checksum, stampa), la casella
  **Scorrimento automatico** e un filtro **Tutte le voci**.
- **Area vuota con invito.** "Trascina qui i file o le cartelle": il trascinamento è il modo primario di
  riempire la sessione.

### 2.2 Funzioni dichiarate dal produttore (Free e Pro)

Da [codesector.com/teracopy](https://www.codesector.com/teracopy):

- Copia e sposta integrate in Explorer (sostituiscono la copia nativa); un errore su un file **non
  interrompe** il trasferimento: il file si salta e si può riprovare a fine corsa; salto manuale per
  accorciare i tempi.
- Verifica con 17 algoritmi (50+ varianti: CRC32, MD5, SHA1, BLAKE3, xxHash3…), generazione e convalida di
  file di checksum, confronto origine/destinazione.
- Coda di più trasferimenti in sequenza; attività e liste file conservate alcuni giorni; l'elenco file fa
  da registro (nome, dimensione, checksum di origine e destinazione, problemi).
- File bloccati copiati tramite servizio elevato e Volume Shadow Copy; cancellazione sicura; opzioni di
  metadati (date, stream, permessi); replica della struttura delle cartelle.
- Informazioni di avanzamento complete: grafico della velocità, contatori di saltati/falliti, velocità
  media, tempo residuo.
- **Solo Pro/Business:** più destinazioni, trasferimenti con 3+ thread (2 nella versione gratuita),
  modifica dell'elenco file, cartelle preferite, esportazione HTML/CSV, licenza commerciale.
- **Collisioni** ([blog del produttore](https://blog.codesector.com/2015/10/01/teracopy-replace-dialog/)):
  finestra con *Sovrascrivi tutti*, *Salta tutti*, *Mantieni entrambi* (rinomina il nuovo), *Sovrascrivi i
  più vecchi*; in trasferimenti non presidiati usa *Salta tutti* dopo 30 s senza input.

### 2.3 Osservato dal vivo (TeraCopy 4, finestra 846x512)

**Schede, tutte viste.**

| Scheda | Contenuto reale |
|---|---|
| **Elenco files** | Striscia con tre pulsanti (`≡`, *Checksums*, *Export*), casella **Scorrimento automatico** (attiva), filtro **Tutte le voci**; area vuota con "Trascina qui i file o le cartelle" |
| **Destinaz.** | Campo percorso con `+` e *Sfoglia…*; due colonne **Preferiti & Recenti** e **Gestori di file** (integrazione con altri file manager) |
| **Opzioni** | Tre colonne: *Trasferimento* (File & cartelle: Dati, Data e ora, Attributi, Flussi; Sicurezza: ACL, Proprietario↑, Audit↑), *Opzioni di trasferimento* (conflitti nomi file, algoritmo di hash, buffer "10 x 1,0 MB", thread "2 · 20 MB per target", verifica dopo il trasferimento, verifica dei file saltati, copia shadow del volume↑, "Opzioni Test": checksum in ADS), *Al termine* (esporta come HTML, salva file di checksum, **Run scripts**: `Eject drives.ps1`, `Notify via IFTTT.ps1`, `Notify via Ntfy.ps1`). In fondo *Salva come predefinito* e *Ripristina ai valori predefiniti* |
| **Stato** | Tabella Categoria · Folders · File · Size · Tempo · Speed (vuota a riposo) |
| **Registro** | Pannello "Log" con righe orarie (`11:36:17: Creato: 261008-….db`) |

**Preferenze** (ingranaggio): **una pagina sola, senza schede**, in blocchi: *Integrazione shell* (registra
come gestore di copia, conferma trascina e rilascia, usa come gestore predefinito, "solo quando Scroll Lock
è disattivato", aggiungi al menu contestuale, verifica singoli file con un checksum negli Appunti), *Queue*
(job attivi massimi = 3, elenchi "forza dispositivi veloci/lenti", con esempio `C:, C:\Path, \\Server\Share`),
*Finestre* (sempre in primo piano, notifiche, suono), *Lingua*, *Verifica sempre dopo la copia*, *Controlla lo
spazio libero*, *Usa la cache di scrittura di sistema*, *File managers* (Total Commander, Directory Opus),
*Aggiornamenti automatici…*. Dodici caselle (undici attive e una, "solo quando Scroll Lock è disattivato", subordinata) più un campo numerico, in una finestra di circa 650x380.

**Che cosa insegna.**

1. **"Opzioni" sta dentro la sessione, non in una finestra a parte**: le scelte che cambiano *questa* copia
   (conflitti, hash, thread, buffer, verifica, cosa fare al termine) sono nella scheda della sessione. Le
   Preferenze contengono solo ciò che è globale. È una separazione che la nostra console non ha.
2. **Il contrassegno ↑** accanto ad alcune opzioni (Proprietario, Audit, copia shadow) è, **ipotesi non verificata**,
   un'indicazione "solo versione Pro": non l'ho confermato con un tooltip. Se lo è, l'idea utile per noi è
   **mostrare un'opzione non disponibile e dire perché**, non il contrassegno commerciale.
3. **"Al termine" con script**: tre `.ps1` pronti (espelli dischi, notifica IFTTT, notifica Ntfy). Equivale al
   nostro `--post-command`, ma **presentato come elenco di azioni scelte da un menu**, non come campo libero.
4. **Densità**: l'intera finestra di lavoro mostra scheda + contatori + azioni in ~500 px di altezza.
5. **Lacune viste**: i due `Verifica` identici restano senza spiegazione in finestra; le icone della barra del
   titolo non hanno etichetta; il pannello Stato a riposo è una tabella vuota senza testo di aiuto; la lingua
   mescola italiano e inglese (*Folders*, *File*, *Size*, *Speed*, *Run scripts*, *Save checksum file*, *Verify
   skipped files after copying*).

**Dopo aver eseguito copie vere** (45 file, 40 MB, due sessioni nella stessa finestra):

- **Una sola istanza**: la seconda `TeraCopy.exe Copy …` non ha aperto un secondo processo ma ha aggiunto la
  sessione alla finestra esistente (stesso PID, due voci nella colonna a sinistra). È ciò che rende istantaneo
  il trascinamento da Explorer: il programma è già lì.
- **Finestra di conflitto** ("File di destinazione già esistente"): mostra i due file affiancati con data e
  dimensione e, per file identici, la dicitura "Stessa data / Stessa dimensione". Cinque azioni grandi
  (*Sovrascrivi*, *Sovrascrivi tutti*, **Salta** come predefinita, *Salta tutti*, *Tieni entrambi*) e sei politiche
  "applicabili a tutti" (sovrascrivi i più vecchi, sostituisci i più piccoli, sostituisci se la dimensione
  differisce, rinomina i copiati / le destinazioni / le destinazioni più vecchie). Il predefinito è l'azione
  **non distruttiva**. Le etichette dei pulsanti sono in italiano ma i loro nomi interni in inglese (*Keep both*,
  *Skip all*), un'ulteriore incoerenza linguistica.
- **Riepilogo a fine copia**: riga di *chip* filtrabili — `Saltati: 44`, `Copied: 2` (**inglese**), `Falliti: 1` —
  contatore `3 / 47`, percorsi di origine e destinazione, spazio libero in destinazione (`Free: …`), e il
  pulsante **Successivo**. Il secondo "Verifica" dell'inizio **è scomparso**: i due pulsanti identici erano uno
  stato transitorio della finestra vuota, non una vera ambiguità permanente (corretto in §5).
- **Errore su file bloccato**: il file fallito resta nella lista con un'icona di ripetizione; il **motivo non
  compare sulla riga**, solo nella scheda *Registro* ("Impossibile accedere al file. Il file è utilizzato da un
  altro processo"). La sessione porta un ⚠ nella colonna a sinistra.
- **Scheda Registro** = **grafico della velocità** (asse tempo e MB/s) + elenco delle sessioni + log testuale con
  orari e *Replace mode: Salta tutti*. **Scheda Stato** = tabella Categoria/Folders/File/Size/Tempo/Speed.
- **Chiusura automatica**: la prima copia, riuscita, ha chiuso da sola la finestra; quella con un errore l'ha
  lasciata aperta. Comportamento sensato: il successo non richiede attenzione.

### 2.4 Impronta misurata

| | Processi | Working set | Memoria privata | Eseguibile |
|---|---|---|---|---|
| TeraCopy 4 (a riposo) | 1 | 56 MB | 23 MB | 6,0 MB |
| Cobian Reflector (elenco vuoto) | 4 (Application, Starter, UserInterface, VSCRequester) | 238 MB | 154 MB | — |
| Console rustcopy 7.8.1 (Tauri, a riposo) | 7 | 378 MB | 163 MB | 12,4 MB |

La cifra di TeraCopy è il **termine di paragone**: un programma di copia interattivo può stare in ~25 MB
privati e un processo. Cobian, pur avendo meno funzioni a video di quanto sembri, occupa quanto la nostra
console. Questo sostiene l'obiettivo di leggerezza della specifica, ma è una misura sola, a riposo.

### 2.5 Schemi di interazione da ricordare (T1…T9)

| ID | Schema | Perché funziona |
|---|---|---|
| T1 | **La sessione è l'unità di lavoro**: si crea, si riempie, si lancia; resta in cronologia con esito | Nessuna configurazione da salvare; il passato è consultabile e ripetibile |
| T2 | **Trascina per cominciare** | Zero passaggi per il caso più comune |
| T3 | **Un solo punto d'azione con le azioni vicine** (Copia, Sposta, Verifica, Elimina) | Il verbo viene scelto *dopo* aver scelto gli oggetti |
| T4 | **Errore = saltare e proseguire**, riprovare a fine corsa | La copia lunga non muore per un file bloccato |
| T5 | **Contatori sempre visibili** (file e byte, fatti/totali) | Il progresso è leggibile senza aprire nulla |
| T6 | **Verifica come cittadino di prima classe**, con algoritmo visibile e cambiabile | La fiducia nel risultato è il prodotto |
| T7 | **Cronologia laterale persistente** con esito a colpo d'occhio (✓) | Risponde a "è andata a buon fine?" senza aprire report |
| T8 | **Collisioni con politica "per tutti"** invece di una domanda per file | Evita centinaia di clic |
| T9 | **Presenza nel menu di Explorer e nell'area di notifica** | Si usa senza aprire il programma |

## 3. Cobian Reflector: anatomia della finestra

Dall'immagine (italiano). Una finestra **classica a due pannelli** con barra dei menu, barra degli
strumenti a icone grandi, barra di stato.

```
+--------------------------------------------------------------------------------------+
| Elenco  Attivita  Azione  Cronologia  Registro  Strumenti  Aiuto                      |
| [>] [>] [x] [||] [+] [monitor] [ingranaggio] [rete] [pacco] [?]                       |
+----------------------------------+---------------------------------------------------+
| Nome dell'attivita    Dimensione | Proprieta                  | Valore               |
|                                  | * Attivita selezionata     | 0                    |
|                                  |                                                   |
|  (elenco delle attivita)         |  (griglia proprieta dell'attivita selezionata)    |
|                                  |                                                   |
| [ Tutti i gruppi ] [ordina]      | [Proprieta] [Storia] [Registro]                   |
+----------------------------------+---------------------------------------------------+
| Elenco: MainList.lst | File: 0 | Dimensione: 0 bytes | Inattivo | [=====] [=====]     |
+--------------------------------------------------------------------------------------+
```

### 3.1 Che cosa si vede

- **Barra dei menu a sette voci** (completa al §3.2) e, sotto, **dieci icone grandi** senza etichetta:
  esegui tutte, esegui selezionate (disattivata senza selezione), annulla, pausa, nuova attività, e
  cinque strumenti (lista/monitor, opzioni, servizio/rete, note/pacco, aiuto).
- **Pannello sinistro: elenco delle attività** con colonne *Nome dell'attività* e *Dimensione*; in basso un
  filtro per **gruppi** ("Tutti i gruppi") con un controllo di ordinamento.
- **Pannello destro: griglia Proprietà/Valore** dell'attività selezionata (qui vuota: "Attività
  selezionata 0"), con tre schede in basso: **Proprietà**, **Storia**, **Registro**.
- **Barra di stato** con lista corrente (`MainList.lst`), numero di file, dimensione, stato (*Inattivo*) e
  due barre di avanzamento (attività corrente e complessivo).

### 3.2 Menu completi ([manuale](https://cobiansoft.com/crHelp/menus.html))

| Menu | Comandi |
|---|---|
| **Elenco** | Nuovo elenco · Apri elenco · Salva con nome · Aggiorna · Ordina · Icone grandi · Importa elenco precedente (Cobian Backup 11) · Gestore della coda |
| **Attività** | Nuova · Modifica · Clona · Elimina (con i file di backup, a scelta) · Attiva/Disattiva · Reimposta (forza il prossimo backup a completo) |
| **Azione** | Esegui tutte · Esegui selezionate · Esegui il gruppo corrente · Forza completo per tutte/selezionate/gruppo · Annulla · Pausa/Riprendi |
| **Cronologia** | Backup più vecchi/più nuovi · Elimina selezionati · Elimina cronologia dell'attività · **Parcheggia** (protegge un backup dalla rotazione) · Rimuovi tutti i parcheggi · Proprietà del backup |
| **Registro** | Seleziona tutto · Copia · Pulisci · Stampa · Aggiungi riga a mano · Apri file di log · Elimina file di log · A capo automatico |
| **Strumenti** | Aggiornamenti · Ottimizza database · Decompressore · Traduttore · **Deleter** (file con percorsi troppo lunghi) · Ripara permessi · Servizio e applicazione · Note · Opzioni |
| **Aiuto** | Indice · Tutorial · Sito · Forum · Donazioni |

### 3.3 Il dialogo dell'attività ([manuale](https://cobiansoft.com/crHelp/tasks.html))

Si crea chiedendo **nome** e **tipo**: *Completo*, *Incrementale*, *Differenziale* o *Fittizio* (nessun
file, serve a lanciare eventi). Poi otto schede:

| Scheda | Contenuto |
|---|---|
| **Generale** | Nome, ID (non modificabile), Abilitata, Gruppo, Includi sottocartelle, Crea backup separati (con data), Usa gli attributi di archivio, Usa le copie shadow dei volumi |
| **File** | Origini (file, cartelle, posizioni SFTP/FTP) e destinazioni (locali o remote) |
| **Pianificazione** | Una volta · Giornaliera · Settimanale · Mensile · Annuale · **Timer** (ogni X minuti) · Manuale |
| **Dinamica** | Priorità, copie complete da tenere, "fai un completo ogni N backup", copie differenziali da tenere, giorno fisso per il completo |
| **Archivio** | Nessuna compressione · per file · archivi monolitici separati · archivio globale; commento; **cifratura con passphrase**; livello 0-9 |
| **Filtro** | Inclusioni/esclusioni per maschera, **espressione regolare**, **data** (con marcatori dinamici) e **dimensione** |
| **Eventi** | Prima/dopo il backup; "annulla se un evento fallisce"; "non eseguire i successivi se i precedenti sono falliti" |
| **Avanzate** | Azzera l'attributo d'archivio · **Attività mirror** (cancella in destinazione ciò che manca in origine) · percorsi assoluti · cartella genitore sempre · tipo di backup nel nome · ignora cartelle vuote · **esegui come altro utente** (utente/dominio/password) · annulla se l'impersonificazione fallisce |

### 3.4 Le opzioni del programma ([manuale](https://cobiansoft.com/crHelp/options.html))

Nove schede: **Generale** (aggiornamenti, lingua, tasto rapido, esegui tutto all'avvio, cartella
temporanea), **Registro** (livello, un file al giorno, tempo reale, elimina i più vecchi di N giorni,
invio via posta), **Posta** (SMTP completo: server, porta, SSL, autenticazione, proxy, timeout),
**Archivio** (Zip64, Unicode, metodo di cifratura, buffer, estensioni non compresse, verifica dell'archivio),
**Aspetto** (palloncini, icone grandi, avanzamento sulla barra delle applicazioni, suono a fine lavoro,
colori, font del registro), **Funzionalità** (carica attività completa, converti tutto in UNC, **esegui i
backup mancati**, **avvisa se il mirror è troppo grande**, numero di voci di cronologia), **Motore**
(primo backup completo, non duplicare in coda, **impedisci la sospensione**, annulla se la copia shadow
fallisce, thread a bassa priorità, **non eseguire a batteria**, copia attributi/date/permessi NTFS, ignora
reparse point, salva backup vuoti, parcheggia il primo), **Sicurezza** (proteggi l'interfaccia con
password) e **Avanzate** (formato data nei nomi, timeout di uscita, copia di file grandi senza buffer,
riconnessione remota, **cartella mirror sicura**, opzioni di resa grafica).

### 3.5 Osservato dal vivo (Cobian Reflector 2.7.2.0, finestra 947x600, WPF)

**Menu reali, con tasti rapidi** (differenze dal manuale in corsivo): *Elenco*: Nuovo, Apri, Salva con nome,
Aggiorna (F5), Organizza, Icone grandi, Importa legacy, Gestore code (Ctrl+M). *Attività*: Nuova (Ctrl+N),
Modifica (Invio), Clona, Elimina (Ctrl+Canc), Attiva/Disattiva, Reimposta. *Azione*: Esegui tutte (Ctrl+B),
Esegui selezionate (Maiusc+B), Esegui gruppo, Forza completo per tutte/selezionate/gruppo, *"Simulate a full
backup"* (voce in inglese: una **simulazione**, non nel manuale), *Spegnimento al termine* (non nel manuale),
Pausa, Annulla.

**Il dialogo "Nuova attività" è un'unica finestra con una barra verticale di otto icone** (Generale, File,
Pianifica, Dinamiche, Archivio, Filtro, Eventi, Avanzata), un'immagine-banner, e OK/Annulla in basso a destra.
Non è una sequenza guidata a due campi come suggerisce il tutorial: nome, gruppo, **cinque** caselle (tutte attive
di partenza: Abilitato, sottocartelle, backup separati, attributi d'archivio, copie shadow) e quattro tipi (Completo
predefinito, Incrementale, Differenziale, Fittizio) stanno **nella stessa pagina**.

- **Ogni controllo ha un tooltip esplicativo** (lo espone anche l'albero UIA come `help_text`). È la scelta di
  design più utile dell'intero programma: l'interfaccia è densa ma **si spiega da sola al passaggio del mouse**.
- **Le opzioni non applicabili restano visibili ma disattivate** (es. "Attività di mirror" con tipo Completo),
  invece di sparire: si capisce che esistono e perché non valgono ora.
- **Pianifica mostra tutti i controlli sempre**, qualunque sia il tipo scelto (Manuale predefinito): giorni della
  settimana, impostazioni data, mesi, timer con limiti 00:00-23:59 convivono. È il difetto più evidente
  del programma: il tipo scelto in alto non filtra ciò che sta sotto.
- **Archivio**: tipo di compressione (Nessuna), metodo (Zip), cifratura con passphrase + conferma + suggerimento
  + **misuratore di robustezza**, livello (cursore, 6), suddivisione (solo 7zip, 524288000 byte), commento.
- **Eventi**: due elenchi (prima/dopo) con due caselle "annulla se un evento fallisce" / "non eseguire se
  quelli prima sono falliti".
- **Avanzata** include l'esecuzione come altro utente con campi utente/dominio/password.

**Impostazioni: dieci schede, non nove** (Generale, Registro, Posta, Archivio, Visualizzazioni, Funzionalità,
Motore, **Remote**, Sicurezza, Avanzate). *Remote* non è nel manuale: server di controllo remoto con porta
(44044), password e indirizzo di ascolto, con un **avviso di sicurezza rosso in inglese** nella stessa pagina.
È un controllo remoto di un programma di backup: per noi è fuori perimetro (regola di sicurezza), ma la **formula
dell'avviso inline accanto al controllo rischioso** è buona.

**Finestra principale.** Elenco a sinistra (Nome dell'attività · Dimensione), "Tutti i gruppi" + pulsante di
riordino, a destra griglia **Proprietà/Valore** (con voce "Attività selezionate 0"), tre schede in basso
(Proprietà, Storia, Registro), barra di stato (Elenco: `MainList.lst`, File, Dimensione, stato *Inattivo*,
due barre). Nessun testo d'aiuto nello stato vuoto.

**Costi di interazione, contati su un'attività reale** (cartella → cartella locali, tipo Completo, nessuna
pianificazione): Ctrl+N · scrivere il nome · scheda *File* · *Aggiungi* fonte · *Manuale* · scrivere il percorso ·
*Aggiungi* destinazione · *Manuale* · scrivere il percorso · *OK* = **10 interazioni**, e per eseguirla Ctrl+B più
una **finestra di conferma** ("Eseguire tutte le attività sulla tua lista?"). Con i selettori nativi al posto di
"Manuale" si aggiungono due dialoghi di Windows. In TeraCopy lo stesso lavoro da Explorer è un trascinamento e una
scelta di menu; dall'applicazione, *Origine*, *Destinaz.* e *Copia…*.

**Risultato di quell'esecuzione.** L'elenco mostra solo nome, **icona-sveglia** e dimensione: nessuna colonna di
esito né di ultima esecuzione. La scheda **Storia** ha una riga per backup (data, tipo "Backup completo",
"Separato: Sì", file, dimensione) e **nessun esito**: per sapere se è andata bene bisogna leggere la scheda
**Registro**, un testo lungo (circa venti righe per un backup banale) che termina con "Il backup è stato
completato senza errori". Un *balloon* di Windows ha avvisato a fine corsa ("backup completato"). Dettagli
positivi: il gestore chiede conferma di eliminazione con **"elimina anche i file di backup" non spuntato**
per impostazione predefinita (predefinito non distruttivo), e ha creato e rimosso da solo la copia shadow del
volume usata per leggere i file.

**Difetti confermati, oltre a quelli del §5.** Lingua mista (*Simulate a full backup*, tutta la scheda
*Remote*, parte dell'Archivio); pulsante "Servizio e applicazione" che richiede UAC **senza avvisarlo
sull'etichetta**; quattro processi per un elenco vuoto; **nessun esito visibile nell'elenco né nella Storia** (si legge solo nel Registro).

### 3.6 Schemi di interazione da ricordare (C1…C10)

| ID | Schema | Perché funziona |
|---|---|---|
| C1 | **L'attività è l'unità di lavoro**: si definisce una volta, si esegue molte volte | La ripetizione è il caso d'uso |
| C2 | **Elenco + griglia Proprietà/Valore + schede Storia/Registro** per l'attività selezionata | Tutto sulla stessa attività resta a un clic, senza aprire dialoghi |
| C3 | **Gruppi** come filtro dell'elenco e come bersaglio di "esegui il gruppo" | Scala da 3 a 300 attività |
| C4 | **Tipo di backup scelto alla creazione** con spiegazione, **Fittizio** per i soli eventi | Il modello mentale precede i dettagli |
| C5 | **Dialogo ad otto schede ordinato per rilevanza** (Generale, File, Pianificazione prima) | L'ordine delle schede è un tutorial implicito |
| C6 | **Forza completo / Reimposta** accanto a "Esegui" | Il caso eccezionale ha un comando, non un'opzione nascosta |
| C7 | **Parcheggia**: un backup può essere protetto dalla rotazione | La retention non cancella ciò che l'operatore ha dichiarato prezioso |
| C8 | **Gestore della coda** esplicito | Cosa sta per partire è visibile |
| C9 | **Barra di stato con due barre** (attività corrente / totale) e stato testuale | Il programma dice sempre cosa sta facendo |
| C10 | **Servizio e motore separati dall'interfaccia** | I backup partono anche senza finestra aperta |

## 4. Due filosofie, a confronto

| Aspetto | TeraCopy 4 | Cobian Reflector |
|---|---|---|
| Domanda a cui risponde | "Copia questo, adesso, bene" | "Tieni questi dati al sicuro, ogni giorno" |
| Unità di lavoro | Sessione (monouso, in cronologia) | Attività (riusabile, pianificata) |
| Come si comincia | Trascinando | Creando un'attività (nome + tipo) |
| Configurazione | Quasi nessuna, opzioni a parte | Molta, a schede |
| Tempo del lavoro | Primo piano, si guarda mentre corre | Sullo sfondo, si guarda dopo |
| Feedback chiave | Velocità, tempo residuo, file saltati | Stato dell'attività, storia, registro |
| Fiducia | Verifica con checksum visibile | Storia dei backup, parcheggio, registro |
| Punto debole | Poca profondità per backup ricorrenti | Interfaccia datata, molta rigidità, nessuna immediatezza |
| Aspetto | Moderno, chiaro, a pillole | Classico a menu, icone grandi senza testo |

**La sintesi che cerchiamo** non è una finestra con due programmi dentro, ma un'unica idea di *lavoro*
che nasce **immediata** (TeraCopy: trascino, scelgo, parte) e può **diventare permanente** (Cobian: "ripeti
questo ogni notte"). Il passaggio fra le due vite di una copia è il vero disegno di prodotto, ed è ciò
che nessuno dei due fa bene: TeraCopy non ricorda una sessione come regola, Cobian non parte dal gesto.

## 5. Che cosa non copiare

Osservazioni dall'immagine, dai manuali e da una sessione di uso a riposo (non un uso prolungato): le voci
confermate dal vivo lo dicono.

1. **Icone senza etichetta** in entrambe le barre (quindici in totale). Si imparano, non si capiscono: è
   esattamente il problema già risolto in F89 con le etichette visibili. Regola: ogni icona ha testo o
   tooltip immediato.
2. **Due pulsanti "Verifica" identici** in TeraCopy a finestra vuota. **Corretto dal vivo**: a copia conclusa ne
   resta uno solo (e compare *Successivo*), quindi è una disposizione transitoria, non un'ambiguità permanente.
   Resta la regola: un verbo, un significato, e un pulsante inattivo si mostra disattivato, non duplicato.
3. **"Tutti i gruppi" con un controllo di ordinamento accanto** in Cobian: filtro e ordinamento nello
   stesso gesto, senza dire quale sia attivo.
4. **Terminologia di motore esposta**: *attributo d'archivio*, *reparse point*, *Zip64*, *Rfc2047*, *GDI-
   compatible font metrics*. Sono opzioni per chi conosce il file system; vanno dietro "Avanzate" con una
   riga che dice cosa cambia per i dati.
5. **Funzioni a rischio nascoste fra le opzioni** (mirror come casella in "Avanzate", cancellazione dei
   file di backup in "Elimina attività"). Da noi il mirror è già trattato come *l'* impostazione
   distruttiva (F54, regola di sicurezza): resta così.
6. **Sedici schede di opzioni in totale** (otto più nove di Cobian meno sovrapposizioni): troppe per
   scoprire qualcosa. Raggruppare per intenzione ("Notifiche", "Sicurezza", "Prestazioni").
7. **Stato vuoto muto**: entrambe le finestre, appena aperte, non dicono cosa fare (TeraCopy ha almeno
   "Trascina qui…"; la sua scheda Stato è una tabella vuota). La nostra console ha già stati vuoti con
   spiegazione; resta la regola.
8. **Lingua mista** (confermata dal vivo in entrambi): colonne *Folders/Size/Speed*, *Run scripts*, *Simulate a
   full backup*, la scheda *Remote*. Una GUI nostra deve avere **una** lingua per volta, con le stringhe
   in un solo posto.
9. **Il tipo non governa il resto della pagina** (Cobian, scheda Pianifica): tutti i controlli visibili per
   qualunque tipo. Regola: la scelta in alto **mostra solo ciò che le appartiene**.
10. **UAC a sorpresa** dietro un pulsante senza avviso (Cobian, "Servizio e applicazione"). Regola: ciò che
    richiede elevazione lo dice sull'etichetta (scudo) e prima del clic.

## 6. Matrice di copertura rispetto a rustcopy

Legenda: ✅ c'è · 🟡 parziale · ❌ manca · ⛔ vietato oggi da un confine esplicito (regola di sicurezza, vedi
[PIANO_GUI.md](PIANO_GUI.md) §7) · 🔒 richiede una decisione dell'utente.

"Motore" = ciò che CLI/core sanno fare ora. "Console" = ciò che la console Tauri attuale espone.

### 6.1 Funzioni "alla TeraCopy"

| Funzione | Motore | Console oggi | Nota |
|---|---|---|---|
| Copiare cartelle | ✅ | ✅ scheda Copia (F95) | Solo cartelle, non file singoli |
| Copiare file singoli | ❌ | ❌ | La sorgente è sempre una cartella: serve estendere il motore 🔒 |
| Trascinare sulla finestra | — | ❌ | Vedi limite di Slint, SPEC §7 |
| Trascinare in Explorer (menu "Copia con RustCopy") | ✅ | ✅ (estensione Shell, F85) | Con protezione di annidamento (7.8.1) |
| Sostituire la copia nativa di Explorer | ❌ | ❌ | TeraCopy intercetta Ctrl+C/V e trascinamenti: costo alto 🔒 |
| Spostare | ❌ (F46) | ❌ | Copia→verifica→elimina origine; tocca la regola di sicurezza ⛔🔒 |
| Verificare dopo la copia | ✅ SHA-256, BLAKE3, xxh3 | ✅ | 17 algoritmi in TeraCopy; da noi 3 |
| Scegliere l'algoritmo in finestra | ✅ (campo) | 🟡 in Modifica | Pillola come TeraCopy |
| File di checksum (generare/convalidare) | ❌ | ❌ | |
| Pausa / riprendi | ❌ (F47) | ❌ | Analizzato e sospeso: PIANO_GUI §15 🔒 |
| Saltare un file in corsa | ❌ (F48/F58) | ❌ | Richiede il motore naive per i job interattivi 🔒 |
| Politica di collisione scelta dall'utente | 🟡 (robocopy: salta uguali) | ❌ | TeraCopy: 5 azioni + 6 politiche "a tutti", predefinito *Salta*. "Mantieni entrambi" non è esprimibile in robocopy 🔒 |
| Errore = salta e riprova a fine corsa | 🟡 (retry robocopy, riprendi da checkpoint) | 🟡 | |
| Velocità e tempo residuo dal vivo | ✅ campioni a 200 ms | 🟡 velocità sì, ETA no | |
| Grafico della velocità | ❌ | ❌ | Dato disponibile (campioni). In TeraCopy sta nella scheda *Registro* |
| Nome del file in corso | ✅ | ✅ (F-live, 8 Set) | |
| Elenco file con checksum per file | 🟡 (report JSON per mancanze) | ❌ | Oggi solo anomalie, non ogni file |
| Coda sequenziale di più copie | ✅ `[[jobs]]` | 🟡 posizione sola lettura (F49) | Riordino prima di avviare: F67 |
| Cronologia laterale con ✓ | ✅ indice NDJSON | ✅ scheda Storico | In TeraCopy è la colonna fissa, con ⚠ se c'è stato un errore |
| Una sola istanza che accoglie nuove copie | ❌ | ❌ | **TeraCopy lo fa** (verificato). La console attuale non dichiara un plugin single-instance: da verificare cosa fa un secondo avvio |
| Più destinazioni | ❌ | ❌ | Solo Pro in TeraCopy |
| Copiare file bloccati (VSS) | ✅ `--vss-snapshot` | 🟡 non esposto | Richiede amministratore |
| Esportare HTML/CSV | ✅ HTML report | 🟡 CSV dalla Storico | |
| Icona nell'area di notifica | ❌ | ❌ | |
| Notifica di fine | ✅ | ✅ | |
| Cartelle preferite / recenti | ✅ (F66) | ✅ | |
| Cancellazione sicura | ❌ | ❌ | ⛔ distruttivo, fuori perimetro salvo decisione 🔒 |

### 6.2 Funzioni "alla Cobian Reflector"

| Funzione | Motore | Console oggi | Nota |
|---|---|---|---|
| Elenco di attività con esito | ✅ | ✅ scheda Job (F86, F94) | |
| Gruppi di attività | ❌ | ❌ | Un file = un elenco; niente gruppi 🔒 |
| Più elenchi (file `.lst`) | ✅ più TOML | ✅ scelta del file | |
| Griglia proprietà dell'attività | ✅ | ✅ Impostazioni (sola lettura) | |
| Modifica attività a schede | ✅ | ✅ Modifica a sezioni (F54, F89) | Scrive una *proposta*, mai il file in uso |
| Clona attività | ❌ | ❌ | Economico |
| Tipi: completo/incrementale/differenziale | ✅ (F34) | ✅ | Più "Fittizio" (solo eventi) ❌ |
| Forza completo / reimposta | ❌ | ❌ | Economico: manifest delle generazioni |
| Retention per cicli | ✅ (F35) | 🟡 sola salita | |
| Parcheggio di un backup | ❌ | ❌ | Va con la retention 🔒 |
| Pianificazione | ✅ via `schtasks` (F36) | 🟡 solo elenco (F62) | Crearla dalla GUI è ⛔🔒 |
| Timer ogni X minuti | ✅ `hourly@N` (min 1 h) | ❌ | |
| Eventi prima/dopo | ✅ `--pre/--post-command` | 🟡 sola lettura | Scriverli dalla GUI è una decisione aperta (F55) |
| Filtri: maschera | ✅ | ✅ | |
| Filtri: regex, data, dimensione | 🟡 età sì; regex e dimensione no | 🟡 | |
| Compressione / archivio | ❌ (F38) | ❌ | 🔒 |
| Cifratura | ✅ AES-256-GCM (non con generazioni) | ✅ keyring | |
| FTP/SFTP/cloud | ❌ (F40) | ❌ | 🔒 |
| Mirror | ✅ con conferma | ✅ segnalato, mai autorizzato | ⛔ per l'avvio non presidiato |
| Copie shadow (VSS) | ✅ | 🟡 | |
| Esegui come altro utente | ❌ | ❌ | 🔒 |
| Servizio separato dall'interfaccia | 🟡 servizio inerte (F37) | ❌ | Lo Scheduler fa già il lavoro |
| Esegui i backup mancati | ❌ | ❌ | |
| Impedisci sospensione / non a batteria | ❌ | ❌ | Economico (`SetThreadExecutionState`) |
| Notifiche via posta | 🟡 webhook; email non implementata (F44) | ❌ | 🔒 |
| Registro colorato in tempo reale | 🟡 log + report | 🟡 coda output | Retrocesso in PIANO_GUI §14.4 |
| Protezione dell'interfaccia con password | ❌ | ❌ | Valutata e scartata (F57) |
| Lingua | solo italiano | solo italiano | Slint ha traduzioni; vedi SPEC |
| Strumenti: ottimizza DB, deleter percorsi lunghi, ripara permessi | ❌ | ❌ | Il nostro motore gestisce già percorsi lunghi (`\\?\`) |

### 6.3 Dove rustcopy è già avanti

- **Verifica e integrità** più ricche (parole d'ordine: report strutturato, `--fast-verify`, `--advise`).
- **Percorsi UNC e lunghi** corretti (D28) e **check dello spazio libero prima di partire** (F65).
- **Ripresa da checkpoint** con la stessa configurazione (D25) e **cronologia indicizzata** accanto al report.
- **Analisi deterministica** delle prestazioni (`--advise`), assente in entrambi i riferimenti.

## 7. Schemi da adottare, adattare, scartare

**Adottare quasi tali e quali.** T1 sessione come unità immediata · T2 trascinare per cominciare · T5
contatori sempre visibili · T7 cronologia laterale con esito · C2 elenco + griglia + schede Storia/
Registro · C9 barra di stato con stato testuale e doppia barra · C6 "forza completo" accanto a "esegui" ·
**tooltip esplicativo su ogni controllo** (Cobian, §3.5) · **opzioni della sessione dentro la sessione, non
nelle preferenze** (TeraCopy, §2.3) · **opzioni non applicabili visibili e disattivate con il motivo**
(Cobian) · **"Salva come predefinito" / "Ripristina predefiniti"** accanto alle opzioni (TeraCopy).

**Adattare.** T3 (verbi vicini, ma con **Sposta** e **Elimina** dietro conferma e dietro la decisione sulla regola di sicurezza) · T6 (la verifica come pillola visibile, con 3 algoritmi invece di 17, e dire quale è "forte" e
quale solo "veloce", come già facciamo per xxh3) · T8 (collisioni: scegliere una politica **prima** di
avviare, perché robocopy non può chiedere durante) · C1/C4 (l'attività come "sessione promossa": il
pulsante *Salva come attività* sulla sessione appena completata) · C5 (le schede del dialogo, ma
raggruppate per intenzione e con "Semplice/Avanzata", come F89) · C7 (parcheggio, se la retention resta).

**Adattare, aggiunte dal vivo.** "Al termine" come **elenco di azioni scelte** (espelli, notifica, apri
cartella) invece di un campo libero: copre il caso comune senza esporre `--post-command` a chi non vuole
scrivere uno script (TeraCopy) · avviso inline rosso accanto a un controllo rischioso (Cobian, scheda Remote)
· "Simulazione" come azione di primo livello accanto a "Esegui" (Cobian; da noi già `--dry-run`).

**Scartare.** Menu classici a sette voci come navigazione primaria (la nostra sidebar già funziona e
scala meglio) · icone senza etichetta · "Deleter", "Ripara permessi", "Ottimizza database" (problemi del
loro motore, non del nostro) · protezione con password dell'interfaccia (F57) · sedici schede di opzioni.

## 8. Verifica dal vivo: fatto e ancora aperto

**Fatto (8 Ott 2026, Windows-MCP, solo cartelle temporanee, poi rimosse).**

| Domanda | Esito |
|---|---|
| I due "Verifica" di TeraCopy? | Transitori: a copia conclusa ne resta uno (§2.3) |
| Contenuto di Destinaz./Opzioni/Stato/Registro, Preferenze | Visto (§2.3) |
| Comportamento durante/dopo una copia vera, conflitto, file bloccato | Visto (§2.3): finestra di conflitto, chip di riepilogo, grafico, registro, una sola istanza |
| Dialogo attività di Cobian e le sue otto schede; Impostazioni a dieci schede | Visti (§3.5) |
| Creazione, esecuzione, Storia, Registro, eliminazione di un'attività | Fatti: 10 interazioni + conferma a ogni esecuzione; nessun esito in elenco né in Storia (§3.5) |
| Impronta di memoria a riposo | Misurata (§2.4); metodo da salvare in uno script |

**Ancora aperto.**

1. Tooltip delle icone della barra del titolo di TeraCopy; significato esatto del contrassegno ↑.
2. TeraCopy: pausa, "Sposta", copia verso rete lenta, trascinamento da Explorer, area di notifica.
3. Cobian: più attività e gruppi, Gestore code in uso, parcheggio, pianificazioni, comportamento a finestra chiusa.
4. Conteggio dei clic di "pianifica ogni notte" (non fatto: crea una pianificazione reale) in entrambi.
5. Metodo di misura riproducibile (script) per memoria e avvio.

Nessuno di questi blocca la specifica; i punti 2-4 informano la Fase 6, il 5 la Fase 1.

## 9. Criticità trovate rileggendo questo studio

- La prima stesura leggeva il secondo "Verifica" di TeraCopy come "verifica da file di checksum". Non è
  verificabile dall'immagine; resta ipotesi dichiarata e il difetto vero è l'ambiguità.
- La matrice contava "coda sequenziale" come ✅ pieno. Lo è per il motore (`[[jobs]]`), ma la console oggi
  offre posizione in sola lettura e riordino **prima** dell'avvio; il riordino **durante** l'esecuzione è
  l'ostacolo già documentato in PIANO_GUI §14.4. Corretto in 🟡.
- Il paragone sulla velocità ("più veloce") non è oggetto di questo studio: nessuna misura è stata fatta
  su TeraCopy o Cobian. Il vantaggio prestazionale del motore va dimostrato con benchmark, non assunto
  (strumenti già presenti: `scripts/benchmark-threads.ps1`, `--compare-baseline`).
- **Corretto dopo l'uso dal vivo.** La prima stesura descriveva il dialogo di Cobian come "otto schede" dal
  manuale e le impostazioni come "nove". Dal vivo sono una finestra con barra verticale di icone e **dieci**
  schede di impostazioni (*Remote* mancava). Il tutorial ufficiale descrive inoltre una creazione guidata a due
  campi che non corrisponde alla versione 2.7.2.0 installata: **il manuale non è una fonte affidabile per
  l'aspetto**, solo per l'elenco delle funzioni.
- **Confronto di memoria.** L'idea che TeraCopy sia "leggero" e Cobian "pesante" era un'intuizione: misurato,
  Cobian pesa quanto la nostra console (238 MB contro 378 MB di working set, 154 contro 163 MB di memoria
  privata) e TeraCopy un quarto. La soglia della specifica (≤ 60 MB privati) è quindi ambiziosa ma ha un
  riferimento reale (23 MB).
- **Conteggi dei processi.** Cobian ne usa quattro anche a elenco vuoto: la separazione fra interfaccia e
  motore (C10) ha un costo di memoria che per noi non vale, dato che lo Scheduler fa già il lavoro.

### Criticità trovate nella rilettura del 8 Ott 2026 (pomeriggio)

Verificate contro il codice, il repository e i programmi; ciascuna è già stata corretta nel testo sopra.

1. **Data di generazione nel futuro.** Il frontmatter dei tre documenti riportava `14:00Z` mentre erano le `~10:00Z`
   (UTC). È un'attestazione di provenienza falsa, anche se di poco; ora è l'ora vera.
2. **"Costi di interazione misurati" non misurati.** La prima versione di §3.5 li dichiarava misurati senza aver mai
   creato un'attività. Ora sono contati su un'attività reale.
3. **"Cobian ha meno funzioni a video di quanto sembri"**: affermazione senza dato; rimossa.
4. **"Password in chiaro nel dialogo"** di Cobian: non verificato; rimosso.
5. **Quattro caselle / cinque elencate**, **tredici interruttori / dodici**: errori di conteggio, corretti.
6. **Il contrassegno ↑ di TeraCopy come "solo Pro"** era un'inferenza presentata come fatto; ora è dichiarata ipotesi.
7. **Un'osservazione corretta dal vivo**: i due "Verifica" non sono un difetto permanente.
8. **Metodo di misura non salvato**: le cifre di memoria/avvio sono state prese con comandi occasionali. Il
   "stesso script" invocato dalla specifica non esisteva.
