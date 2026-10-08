---
type: Reference
title: Specifica della nuova console rustcopy su Slint
description: Specifica di prodotto e tecnica della console "mista" TeraCopy + Cobian Reflector costruita su Slint e sul motore rustcopy: visione, obiettivi misurabili, schermate, requisiti funzionali e non funzionali con i numeri di partenza misurati, valutazione di Slint con rischi e alternative, confini di sicurezza da riaprire, strategia di verifica e decisioni aperte.
status: draft
generated:
  by: process:claude-code
  at: 2026-10-08T10:30:00Z
---

# Specifica: la nuova console rustcopy su Slint

Documento 2 di 3 del pacchetto "GUI Slint": [studio delle GUI di riferimento](STUDIO_GUI_RIFERIMENTO.md),
questa specifica e il [piano](PIANO_GUI_SLINT.md). **Bozza per approvazione: nessuna riga di codice della nuova
console è autorizzata.** Ogni sezione marcata 🔒 è una decisione che spetta all'utente.

## 1. Visione

Una sola applicazione Windows, leggera e nativa, in cui **una copia nasce immediata** (si trascina, si
sceglie, parte, come in TeraCopy) e **può diventare permanente** (si salva come attività, si pianifica, si
conserva con una politica di retention, come in Cobian Reflector), appoggiata a un motore che è già il
punto di forza del progetto: `robocopy` avvolto in Rust, con verifica, ripresa, report strutturati e
analisi delle prestazioni.

Tre promesse al prodotto:

1. **Immediata.** Dal gesto alla copia in avvio in pochi secondi, senza file da salvare e senza scegliere
   un tipo di backup che non interessa.
2. **Profonda quando serve.** Tutto ciò che Cobian permette di configurare e che il nostro motore sa
   fare, raggiungibile da una scheda dell'attività; nulla di nascosto in menu classici.
3. **Leggera e affidabile.** Un processo, poca memoria, nessun componente esterno da installare (oggi:
   WebView2), che funzioni anche su un Windows Server senza scheda grafica, e che non faccia mai più di
   quello che l'operatore ha chiesto.

### 1.1 Le tre priorità dichiarate dall'utente (8 Ott 2026)

Quando due requisiti confliggono, decidono questi tre, **in quest'ordine di importanza pari ma con questi significati**:

1. **Prestazioni elevate.** Della **finestra** (poca memoria, avvio immediato, mai bloccata: RNF-07, soglie in
   PIANO §3.3) e della **copia** (non peggio di `robocopy` puro; il paragone "più veloce di TeraCopy e Cobian"
   va **dimostrato con un benchmark**, non assunto: `scripts/benchmark-threads.ps1`, `--compare-baseline`).
2. **Robustezza.** Nessun dato perso o corrotto se il processo viene interrotto; un errore su un file non ferma
   il resto (T4); predefiniti **non distruttivi** (TeraCopy propone *Salta*, Cobian non spunta "elimina i file di
   backup"); scritture atomiche; ripresa da checkpoint; la finestra non si blocca mai; istanza unica.
3. **Semplicità con profondità.** *Tutte* le funzioni raggiungibili in modo intuitivo, e **più tecnico quando serve**.
   Tre livelli per lo stesso lavoro, senza cambiare schermata:
   - **Semplice** (predefinito): cosa copiare, dove, verifica sì/no, **Copia**. Il caso comune in tre gesti.
   - **Dettagli**: opzioni per chi le cerca (filtri, conservazione, pianificazione, cifratura, eventi), ciascuna con
     tooltip e avviso se rischiosa; le sezioni con valori non predefiniti **si aprono da sole**.
   - **Tecnico**: parametri del motore (thread, tentativi, banda, giunzioni, percorsi lunghi), **anteprima del
     comando equivalente** in sola lettura (RF-C16), registro completo, report JSON.

## 2. Obiettivi misurabili e non-obiettivi

### 2.1 Obiettivi (da validare nella prova tecnica, [PIANO_GUI_SLINT.md](PIANO_GUI_SLINT.md) Fase 1)

| ID | Obiettivo | Misura | Punto di partenza misurato (8 Ott 2026) |
|---|---|---|---|
| O1 | Meno memoria a riposo | Memoria privata totale dell'albero di processi, finestra aperta, elenco vuoto | Console attuale 163-168 MB (369-378 MB di working set, 7 processi) · **con WebView2 alleggerito** (`--renderer-process-limit=1 --disable-gpu`) **113 MB** privati, sempre 7 processi · TeraCopy 23 MB (56 MB, 1 processo) · Cobian 154 MB (238 MB, 4 processi) |
| O2 | Un solo processo di interfaccia | Numero di processi, escluse le CLI figlie durante una copia | 7 (console) · 1 (TeraCopy) · 4 (Cobian) |
| O3 | Avvio rapido | Tempo fino alla **prima schermata con contenuto leggibile** (non alla sola comparsa della finestra) | Console attuale 0,70 s con contenuto (primo rilievo); alla sola comparsa della finestra 0,12-0,24 s (rilievo ripetuto). **I due metodi non coincidono: va fissato uno script, PIANO §3.2 M0** |
| O4 | Nessuna dipendenza esterna da installare | Elenco dei componenti richiesti oltre al sistema | Oggi WebView2 (verificato dall'installer) |
| O5 | Eseguibile compatto | Dimensione dell'exe dell'interfaccia | Console attuale 12,4 MB · TeraCopy 6,0 MB |
| O6 | Funziona senza GPU | Avvio e uso in una sessione RDP / VM senza accelerazione | Da misurare (la console attuale usa il rendering del browser) |
| O7 | Nessuna regressione di sicurezza | I test della regola di sicurezza, di `runner`/`plan_copy`/`job_editor` restano gli unici cancelli | 468 test unitari della libreria `rustcopy-core` (più i test d'integrazione della CLI) |

**Soglie di accettazione: un'unica fonte, [PIANO_GUI_SLINT.md](PIANO_GUI_SLINT.md) §3.3.** In una prima stesura
questa specifica diceva 60 MB come obiettivo e 80 MB come limite di rifiuto, mentre il piano diceva 100 MB:
tre numeri per la stessa decisione. Qui restano solo gli obiettivi; i numeri di decisione sono nel piano.

### 2.2 Non-obiettivi (per questa specifica)

- Riscrivere il motore o la CLI: restano invariati (vedi §9).
- Sostituire la copia nativa di Explorer a livello di sistema (TeraCopy lo fa): resta una decisione 🔒
  a parte, costo alto.
- Portare l'interfaccia su macOS/Linux: il prodotto è Windows-nativo.
- Un servizio residente sempre attivo come in Cobian: lo Scheduler di Windows continua a fare il lavoro.

## 3. Utenti e scenari

| ID | Chi | Scenario | Cosa gli serve |
|---|---|---|---|
| U1 | Operatore non tecnico | "Copia queste cartelle sul NAS e dimmi se è tutto lì" | Trascinare, vedere il progresso, ricevere un esito in una frase |
| U2 | Amministratore di un file server | "Ogni notte copia questo, tieni sette generazioni, avvisami se fallisce" | Attività, pianificazione, retention, notifica, storico |
| U3 | Tecnico avanzato | "Questa copia deve escludere X, usare 16 thread, verificare con xxHash, partire dopo uno script" | Tutte le opzioni, ma dopo le basi |
| U4 | Chi installa su un Server 2016/2019/2022 | "Non ho Visual C++, non ho GPU, sono in RDP" | Un exe che parte, senza prerequisiti |

## 4. Principi di prodotto

1. **Il frontend non decide.** Ogni giudizio sul backup (sicurezza di un purge, significato di un codice di
   uscita, validità di una copia) resta nel core ([CLAUDE.md](CLAUDE.md), regola su `gui_api`).
2. **La console riferisce, mai autorizza** un'operazione distruttiva non presidiata (confine di sicurezza, §10).
3. **Il caso comune non richiede configurazione.** Le opzioni compaiono quando servono (F89: Semplice/Avanzata).
4. **Una parola, un significato; nessuna icona senza testo** (difetti osservati in entrambi i riferimenti,
   [studio](STUDIO_GUI_RIFERIMENTO.md) §5).
5. **Ogni controllo spiega cosa fa**, con un tooltip immediato: lo fa bene Cobian (ogni casella ha una
   spiegazione, verificato dal vivo) e va ripreso.
6. **Opzioni a rischio con avviso inline** nel punto in cui si attivano (Cobian: "usare con cura").
7. **Voci non applicabili grigie, non assenti**, con il motivo (Cobian: "mirror" grigio per un backup completo).
8. **Stato sempre leggibile**: cosa sta facendo, a che punto è, cosa farà dopo.
9. **Il passato è consultabile e ripetibile**: cronologia con esito, ripetizione di una copia, promozione ad attività.
10. **Accessibile da tastiera e da screen reader** ([§7](#7-requisiti-non-funzionali)).

## 5. Architettura d'informazione

### 5.0 Un'app, un'idea: il "lavoro" (e perché non "due modalità")

Una prima stesura proponeva *una app con due modalità* (Copia e Attività). Rivalutata:

| Forma | Pro | Contro | Giudizio |
|---|---|---|---|
| **Due app** (copia rapida, gestore di attività) | La copia rapida può essere piccola e partire all'istante; l'altra può essere più ricca | Due installazioni di interfaccia, due impostazioni, due cronologie; "salva come attività" attraversa un confine fra programmi; due finestre aperte = più memoria | Scartata: spezza proprio il passaggio che è il valore del prodotto |
| **Una app, due modalità** (un interruttore) | Semplice da spiegare | "Dove sono?": sessioni e attività in elenchi separati, cronologia divisa; il salvataggio attraversa il confine fra modalità; due modelli mentali | Accettabile, ma introduce un interruttore che l'utente deve capire |
| **Una app, un'entità "lavoro"** con ciclo di vita: *una tantum* → *salvato* → *pianificato* | Sessione di TeraCopy e attività di Cobian sono **lo stesso oggetto** a profondità diverse (le opzioni della sessione di TeraCopy e il dialogo di Cobian sono lo stesso insieme di impostazioni); un solo elenco, un solo editor, una sola cronologia; "salva" è un gesto, non un cambio di programma | Serve una buona divulgazione progressiva; l'elenco misto di recenti e salvati va filtrato di default | **Raccomandata** |

**Decisione proposta:** una sola app e **una sola entità**, il *lavoro*, con **due punti d'ingresso** e nessun
interruttore. L'ingresso rapido (trascina, *Nuova copia*, Explorer) porta direttamente a un lavoro in
esecuzione; l'ingresso completo (*Nuova attività*, *Modifica*) apre lo stesso lavoro con tutte le impostazioni.
Ciò che nei riferimenti sono "due programmi" per noi è lo stesso lavoro visto con più o meno dettaglio. Il
ripiego, se il prototipo lo smentisce, è economico: l'elenco si divide in due sezioni senza cambiare il modello.

**Conseguenza di processo, osservata in TeraCopy** ([studio](STUDIO_GUI_RIFERIMENTO.md) §2.3): una **sola istanza**
che accoglie nuove copie. Un trascinamento da Explorer deve arrivare alla finestra già aperta, non avviare un
secondo processo (RF-Y11). Tutto ciò è più importante con un'unica app che con due.

### 5.1 Navigazione

Una **barra laterale** (come oggi, scala meglio dei menu classici di Cobian) con tre gruppi, più una
**barra dei comandi** con pulsanti etichettati e una **barra di stato**.

```
+------------------------------------------------------------------------------------+
| rustcopy    [+ Nuova copia] [> Esegui] [|| Pausa] [x Ferma]                [!] [?]   |
+----------------+-------------------------------------------------------------------+
| LAVORI         |                                                                   |
|  > in corso    |            (area di lavoro: cambia con la voce scelta)            |
|  * Documenti   |                                                                   |
|    ogni notte  |                                                                   |
|  ! Foto        |                                                                   |
|    salvato     |                                                                   |
|  . Archivio    |                                                                   |
|  v 11:21 Foto  |                                                                   |
|    una tantum  |                                                                   |
|  [filtro: tutti|                                                                   |
|   salvati rec.]|                                                                   |
| STRUMENTI      |                                                                   |
|  Report        |                                                                   |
|  Storico       |                                                                   |
|  Impostazioni  |                                                                   |
|  Aiuto         |                                                                   |
+----------------+-------------------------------------------------------------------+
| Inattivo  | 3 attivita | Prossima: stanotte 02:00 (Documenti) | [========    ] 62%  |
+------------------------------------------------------------------------------------+
```

- **Lavori**: un solo elenco. Ogni voce dice **che vita ha** (*una tantum*, *salvato*, *pianificato: ogni notte
  02:00*) e **com'è andata** (✓ / ! / ✗, data, riepilogo). In cima ciò che è in corso. Il filtro (*Tutti ·
  Salvati · Recenti*) tiene sotto controllo l'elenco; di default compaiono i salvati e gli ultimi dieci
  recenti. Un lavoro una tantum si **ripete** e si **salva** (diventa *salvato*); uno salvato si **pianifica**
  (decisione sulla regola di sicurezza, §10). I gruppi di Cobian (🔒) sono un secondo filtro.
- **Strumenti**: Report (una run), Storico (tutte le run, analisi `--advise`), Impostazioni, Aiuto.
- **Barra dei comandi**: le azioni globali con etichetta (mai solo icona). Pausa è visibile **solo se** il
  motore la supporta 🔒 (§10).
- **Barra di stato**: stato testuale, numero di attività, prossima esecuzione pianificata (dallo
  Scheduler, già letta da F62), avanzamento complessivo.

### 5.2 Schermata Copia (sessione immediata)

```
+------------------------------------------------------------------------------------+
|  Trascina qui cartelle (o file)                          [ Aggiungi cartelle... ]  |
|  +--------------------------------------------------------------------------+     |
|  | D:\Dati\Foto               1.820 file   7,6 GB      [x]                 |     |
|  | D:\Dati\Video                312 file   1,2 GB      [x]                 |     |
|  +--------------------------------------------------------------------------+     |
|  Dove:  [ \\nas01\backup                              ]  [ Sfoglia... ]  < recenti >|
|  Verifica: ( ) nessuna  (o) dopo la copia   Algoritmo: [ xxHash3  v ] veloce, non crittografico |
|  Se il file esiste gia: [ Salta se uguale v ]                                       |
|  [ COPIA ]   [ Sposta ]  [ Verifica soltanto ]        < Opzioni avanzate v >        |
|-----------------------------------------------------------------------------------|
|  Elenco file | Destinazioni | Opzioni | Stato | Registro                            |
```

- **Un solo punto d'azione** con i verbi vicini (T3). *Sposta* e *Elimina* sono visibili **solo dopo** la
  decisione sulla regola di sicurezza e su F46 🔒; fino ad allora non compaiono, non restano disattivati come promessa vuota.
- **Anteprima** ("Controlla prima", già esistente in F95) integrata nell'elenco: file, cartelle, byte, spazio libero.
- **Politica di collisione scelta prima di avviare** (T8), perché robocopy non può chiedere durante 🔒.
- **Dopo la copia**: il riepilogo in una frase (già F93), con il pulsante **Salva come attività** (C1/C4) e
  **Ripeti**.

### 5.3 Schermata Esecuzione (durante)

Progresso complessivo e per elemento; velocità istantanea e media con **grafico**; tempo residuo; file in
copia; contatori *copiati / saltati / falliti*; coda delle cartelle successive; pulsanti **Ferma** (scrive il
file di stop: nessun kill, come oggi) e, se deciso, **Pausa**/**Salta questo file** 🔒. Gli errori sono un
elenco consultabile, non una finestra per file (T4).

### 5.4 Schermata Attività (Cobian-like)

```
+--------------------------------------------------------------------------+
| Documenti           [ Esegui ora ] [ Forza completo ] [ Modifica ] [ ... ] |
| Ultima: riuscita ieri 02:03, 1.820 file, 7,6 GB, 210 MB/s                  |
|--------------------------------------------------------------------------|
|  Proprieta | Storia | Registro                                           |
|  Sorgente      D:\Documenti                                              |
|  Destinazione  \\nas01\backup\Documenti                                  |
|  Tipo          Incrementale, completo ogni 7                             |
|  Pianificata   Ogni giorno alle 02:00 (Task Scheduler)                   |
|  Conserva      7 cicli                                                   |
|  Cifratura     si (chiave: backup-nas)                                   |
+--------------------------------------------------------------------------+
```

La griglia Proprietà/Valore con la **provenienza di ogni valore** (default, file, job) esiste già in
Impostazioni (F55); qui diventa il corpo dell'attività. *Storia* mostra le run con esito e velocità; *Registro*
il testo della run selezionata.

### 5.5 Editor dell'attività (scheda per scheda)

Una finestra o pannello con **rotaia verticale di schede con icona e testo** (come Cobian, che funziona), ma
con l'ordine e il raggruppamento di F89:

| Scheda | Contenuto | Origine in Cobian |
|---|---|---|
| **Base** | Nome, gruppo 🔒, tipo (Completo / Incrementale / Differenziale / Fittizia 🔒), sorgenti, destinazioni | Generale + File |
| **Quando** | Manuale, a orari, ogni N ore; finestra oraria; prossima esecuzione | Pianifica |
| **Conservazione** | Cicli da tenere, "completo ogni N", giorno fisso, parcheggio 🔒 | Dinamiche |
| **Filtri** | Includi/escludi per maschera, età; regex/dimensione 🔒 | Filtro |
| **Sicurezza** | Cifratura (chiave dal Credential Manager), verifica e algoritmo, VSS | Archivio (parte) + Generale |
| **Prima e dopo** | Comandi prima/dopo, "annulla se fallisce" | Eventi |
| **Avanzate** | Thread, tentativi, banda, giunzioni, percorsi lunghi, utente alternativo 🔒 | Avanzata |

Regole prese dal dialogo reale di Cobian: tooltip su ogni controllo (P5); opzioni a rischio con avviso
inline (P6); voci non applicabili grigie con motivo (P7); un'unica finestra, non una sequenza di dialoghi.
Regole nostre già in vigore: l'editor scrive una **proposta** in un file nuovo e **può restringere il rischio,
mai allargarlo** (F54); non si può mettere mirror con tipo di backup (F70) né cifratura con generazioni (F80).

### 5.6 Impostazioni

Raggruppate per intenzione, non per sezione tecnica: **Notifiche** (toast, webhook, posta 🔒), **Sicurezza**
(credenziali, F56), **Prestazioni** (thread, banda, buffer), **Aspetto** (tema, lingua, tray), **Avanzate**.
Cobian ne ha dieci; qui l'obiettivo è cinque, con ricerca.

### 5.7 Area di notifica

Icona nel tray (Slint 1.17 ha l'elemento `SystemTrayIcon`, giovane: da verificare, in alternativa il crate
`tray-icon`): stato corrente, apri, pausa/riprendi 🔒, esci. Notifiche toast di fine run con esito.

## 6. Requisiti funzionali

Legenda priorità: **M**ust, **S**hould, **C**ould, **W**on't (per ora). Stato: ✅ il motore o la console attuale lo fanno ·
🟡 parziale · ❌ da costruire · 🔒 decisione dell'utente richiesta. Il confine è sempre quello di §10.

### 6.1 Copia immediata

| ID | Requisito | Prio | Stato |
|---|---|---|---|
| RF-C01 | Aggiungere cartelle con selettore nativo multiplo e rimozione dall'elenco | M | ✅ (F95) |
| RF-C02 | Trascinare cartelle da Explorer sulla finestra | M | ❌ limite di Slint, vedi R1 |
| RF-C03 | Aggiungere **file singoli** | S | ❌ 🔒 estende il motore |
| RF-C04 | Destinazione con recenti e preferiti, anche di rete | M | ✅ (F66) |
| RF-C05 | Anteprima: file, cartelle, byte, spazio libero in destinazione | M | 🟡 (spazio libero solo a run avviata, F65) |
| RF-C06 | Verifica dopo la copia con algoritmo visibile (pillola) e nota "veloce/non crittografico" | M | ✅ |
| RF-C07 | Politica di collisione scelta prima (salta se uguale, sovrascrivi più recenti, sovrascrivi tutto) | S | 🟡 🔒 |
| RF-C08 | "Mantieni entrambi" (rinomina) | C | ❌ 🔒 non esprimibile in robocopy |
| RF-C09 | Sposta (copia, verifica, poi elimina l'origine) | S | ❌ 🔒 tocca la regola di sicurezza |
| RF-C10 | Verifica soltanto (confronto origine/destinazione senza copiare) | S | 🟡 |
| RF-C11 | Generare/convalidare un file di checksum | C | ❌ |
| RF-C12 | Ripetere una sessione dalla cronologia; salvarla come attività | M | ❌ |
| RF-C13 | **Opzioni della sessione dentro la sessione** (conflitti, algoritmo, thread, verifica, cosa fare al termine), con "Salva come predefinito" e "Ripristina predefiniti" | M | ❌ nuova; osservato in TeraCopy (studio §2.3) |
| RF-C14 | **"Al termine" come elenco di azioni scelte** (apri la cartella, notifica, espelli l'unità, spegni) oltre al comando libero | S | 🟡 motore: `--post-command`; elenco ❌ 🔒 (spegnimento e espulsione toccano il sistema) |
| RF-C16 | **Anteprima del comando equivalente** (la riga di `robocopy_ingest` che la console avvierebbe), in sola lettura e copiabile, nel livello Tecnico | S | ❌ nuova; serve all'utente avanzato e alla fiducia. Va costruita dal core (`runner`), mai dalla GUI |
| RF-C15 | **Simulazione** (`--dry-run`) come azione visibile accanto a "Copia", con esito etichettato come simulato | S | ✅ motore; banner già in Report (F84) |

### 6.2 Esecuzione

| ID | Requisito | Prio | Stato |
|---|---|---|---|
| RF-E01 | Progresso complessivo, file in corso, byte, velocità, ETA | M | 🟡 (ETA manca) |
| RF-E02 | Grafico della velocità | S | ❌ dato già campionato a 200 ms |
| RF-E03 | Contatori copiati / saltati / falliti | M | 🟡 (da `copy_detail`) |
| RF-E04 | Fermare scrivendo il file di stop (checkpoint garantito) | M | ✅ |
| RF-E05 | Riprendere da checkpoint con la stessa configurazione | M | ✅ (D25) |
| RF-E06 | Pausa / Riprendi | S | ❌ 🔒 vedi §10, [PIANO_GUI.md](PIANO_GUI.md) §15 |
| RF-E07 | Saltare un file in corsa; riprovare i falliti a fine corsa | C | ❌ 🔒 richiede il motore naive |
| RF-E08 | Coda di più copie, riordinabile prima dell'avvio | M | ✅ (F67) |
| RF-E09 | Riordino durante l'esecuzione | W | ❌ costo come F47 |
| RF-E10 | Impedire la sospensione del PC durante la copia | S | ❌ economico |
| RF-E11 | Copiare file bloccati (VSS), con richiesta di amministratore esplicita | S | 🟡 non esposto |

### 6.3 Attività e pianificazione

| ID | Requisito | Prio | Stato |
|---|---|---|---|
| RF-A01 | Elenco attività con esito, ultima esecuzione e filtro per gruppo | M | 🟡 (gruppi ❌ 🔒) |
| RF-A02 | Griglia proprietà con provenienza dei valori; schede Storia e Registro | M | ✅ in parte (F55, F86) |
| RF-A03 | Creare / modificare / clonare un'attività da schede | M | ✅ (F54) / clone ❌ |
| RF-A04 | Tipi Completo, Incrementale, Differenziale | M | ✅ (F34) |
| RF-A05 | Tipo **Fittizio** (solo eventi) | C | ❌ |
| RF-A06 | "Esegui ora", "Forza completo", "Reimposta" | M | 🟡 Forza completo ❌ |
| RF-A07 | Retention per cicli, "completo ogni N", giorno fisso, parcheggio di un backup | S | ✅ in parte (F35) |
| RF-A08 | Vedere le pianificazioni esistenti e la prossima esecuzione | M | ✅ (F62) |
| RF-A09 | **Creare/modificare/eliminare** una pianificazione dalla GUI | S | ⛔ 🔒 oggi vietato (regola di sicurezza) |
| RF-A10 | Eventi prima/dopo con "annulla se fallisce" | S | ✅ motore; scrittura GUI 🔒 (F55) |
| RF-A11 | Esegui come altro utente | W | ❌ 🔒 |
| RF-A12 | Eseguire i backup "mancati" (PC spento all'ora prevista) | C | ❌ |
| RF-A13 | Non eseguire a batteria | C | ❌ economico |

### 6.4 Esiti, storico, notifiche

| ID | Requisito | Prio | Stato |
|---|---|---|---|
| RF-S01 | Esito in una frase con motivi (F93) | M | ✅ |
| RF-S02 | Report completo con schede numeriche (F84, F94) | M | ✅ |
| RF-S03 | Storico con analisi deterministica (`--advise`) | M | ✅ |
| RF-S04 | Cronologia laterale persistente con esito a colpo d'occhio | M | ❌ nuova |
| RF-S05 | Esportare CSV/HTML | S | 🟡 |
| RF-S06 | Notifica toast di fine run | M | ✅ |
| RF-S07 | Notifica via posta | C | ❌ 🔒 (F44) |
| RF-S08 | Icona nel tray con stato | S | ❌ |
| RF-S09 | Registro colorato in tempo reale | C | 🟡 retrocesso ([PIANO_GUI.md](PIANO_GUI.md) §14.4) |

### 6.5 Sistema

| ID | Requisito | Prio | Stato |
|---|---|---|---|
| RF-Y01 | Estensione Shell di Explorer (voce nel menu del trascinamento) | M | ✅ (F85, protetta 7.8.1) |
| RF-Y02 | Sostituire copia/incolla nativo di Explorer | W | ❌ 🔒 costo alto |
| RF-Y03 | Tema chiaro/scuro che segue il sistema | M | 🟡 da verificare in Slint |
| RF-Y04 | Italiano; inglese come seconda lingua | S | ❌ 🔒 |
| RF-Y05 | Attribuzione richiesta dalla licenza di Slint: schermata "Informazioni" con `AboutSlint` raggiungibile dal menu principale **oppure** il badge Slint sulla pagina pubblica da cui si scaricano i binari | M | ❌ vincolo di licenza, §8.1 (la scelta fra le due forme è la decisione 3 del piano) |
| RF-Y06 | Esecuzione da amministratore solo quando serve, mai di default | M | ✅ |
| RF-Y07 | **Ogni controllo ha un tooltip esplicativo** (testo e accessibile come descrizione) e le opzioni non applicabili restano visibili e disattivate con il motivo | M | ❌ nuova; osservato in Cobian (studio §3.5) |
| RF-Y08 | Ciò che richiede elevazione (UAC) lo dice **sull'etichetta**, prima del clic | M | ❌ nuova; difetto osservato in Cobian |
| RF-Y09 | **Una sola lingua per volta**, stringhe in un unico posto, nessuna etichetta mista | M | ❌ difetto osservato in entrambi i riferimenti |
| RF-Y12 | **Primo avvio e Impostazioni: livello di sicurezza** (Prudente / Standard / Esperto), predefinito Prudente, nelle impostazioni dell'utente e non nei file di job; visibile nella barra di stato | M | ❌ nuova, §10.1 |
| RF-Y10 | In ogni scheda, la scelta in alto **mostra solo i controlli che le appartengono** (es. tipo di pianificazione) | M | ❌ difetto osservato in Cobian (scheda Pianifica) |
| RF-Y11 | **Una sola istanza**: un secondo avvio (anche da Explorer) consegna il lavoro alla finestra già aperta e si chiude | M | ❌ nuova; osservato in TeraCopy. La console attuale non dichiara un plugin single-instance: da verificare |

## 7. Requisiti non funzionali

| ID | Requisito | Verifica |
|---|---|---|
| RNF-01 | Soglie O1-O6 di §2.1 | Prova tecnica: misura con lo stesso script usato per i valori di partenza |
| RNF-02 | Avvio e uso senza GPU: rendering software come predefinito sui Server | Avvio in sessione RDP e in VM senza accelerazione; nessun errore OpenGL |
| RNF-03 | Nessun runtime C dinamico, nessun Visual C++ richiesto | `scripts/check-static-crt.ps1` sul binario (regola D30) |
| RNF-04 | Scala DPI 100-200 % senza testo tagliato, finestra ridimensionabile | Prova a 100/150/200 % |
| RNF-05 | Tastiera: ogni azione raggiungibile, ordine di tab sensato, scorciatoie (Ctrl+N, Invio, Ctrl+Canc come Cobian) | Prova senza mouse |
| RNF-06 | Screen reader: ruoli e nomi esposti via UI Automation | Albero UIA letto con gli strumenti già usati (Windows-MCP) e prova con Narrator |
| RNF-07 | Interfaccia sempre reattiva: nessuna operazione di disco o rete sul thread grafico | Revisione + prova con unità di rete lenta (il selettore nativo impiega fino a 40 s sull'unità Z: di questa macchina) |
| RNF-08 | Tutta l'interfaccia in italiano corretto, senza mescolare lingue (difetto osservato in Cobian) | Revisione dei testi |
| RNF-09 | L'applicazione non scrive mai nei file in uso né sulla destinazione senza una richiesta esplicita | Test esistenti + nuovi sui comandi |

## 8. Scelta tecnologica: Slint

### 8.1 Fatti verificati (8 Ott 2026, con fonte)

- **Versione e novità.** Ultima pubblicata su crates.io: **1.18.1** (usata nella prova). Novità della 1.17 (24 giu 2026): trascinamento **solo dentro l'applicazione**; elemento
  `SystemTrayIcon`; `Tooltip`; `RadioGroup`; albero di accessibilità ispezionabile; `StyledText` con markdown.
  Fonte: [annuncio 1.17](https://slint.dev/blog/slint-1.17-released).
- **Rilascio di file da Explorer sulla finestra: non supportato.** Citazione: "Dragging to other applications
  and receiving drops from them is in development upstream in winit". Stessa fonte.
- **Widget standard:** Button, CheckBox, ComboBox, ProgressIndicator, RadioGroup, Slider, SpinBox, Spinner,
  Switch, LineEdit, ListView, ScrollView, StandardListView, **StandardTableView**, TabWidget, TextEdit,
  GroupBox, DatePickerPopup, TimePickerPopup, AboutSlint. Stili: fluent (predefinito), material, cupertino,
  native, qt. Fonte: [documentazione](https://docs.slint.dev/latest/docs/slint/reference/std-widgets/overview/).
  Selettore di file nativo: **non incluso** (serve un crate, p. es. `rfd`).
- **Backend e renderer.** Backend predefinito winit. Renderer: software ("lightweight, no GPU"), FemtoVG
  (richiede **OpenGL**), Skia (GPU, "heavy disk-footprint", e **richiede il Visual C++ Redistributable** su
  Windows), Vello (sperimentale). Scelta con `SLINT_BACKEND`. Fonte:
  [backend e renderer](https://docs.slint.dev/latest/docs/slint/guide/backends-and-renderers/backends_and_renderers/).
- **Licenza** ([termini](https://slint.dev/terms-and-conditions)): GPLv3; **Royalty-free 2.0** per applicazioni
  desktop, mobile e web (non embedded), con obbligo di mostrare il widget `AboutSlint` in una schermata
  "Informazioni" raggiungibile dal menu principale (o l'attribuzione su una pagina web pubblica da cui si
  scaricano i binari) e divieto di distribuire applicazioni che espongano le API di Slint; licenza commerciale a
  pagamento. **Riletto sul testo ufficiale (8 Ott 2026)**: l'attribuzione si assolve in **uno** dei due modi
  (schermata Informazioni *oppure* badge sulla pagina di download), quindi non obbliga a una schermata in app. **Scelta dell'utente (8 Ott 2026): badge** sulla pagina di download (README e pagina della release su GitHub); una schermata Informazioni resta facoltativa.
  Con GPLv3, distribuire un binario che unisce il codice MIT e Slint richiede di concedere l'**intero
  lavoro** sotto GPLv3 e di fornire il sorgente. **Il repository è MIT**: la scelta è una decisione 🔒 e questa
  non è una consulenza legale.
- **Accessibilità.** Esiste un albero di accessibilità e proprietà `accessible-*` sui widget; la qualità con
  Narrator/NVDA sul backend winit **non l'ho verificata**: è un punto della prova.
- **Tray.** `SystemTrayIcon` (1.17) oppure crate dedicati; la maturità dell'elemento integrato va provata.
- **Eventi winit.** Con la feature `unstable-winit-030` Slint espone `on_winit_window_event`, che consegna alla
  nostra applicazione gli eventi `WindowEvent` di winit (fonte: [documentazione](https://docs.slint.dev/latest/docs/rust/slint/winit_030/trait.WinitWindowAccessor)). Se winit su Windows invia `DroppedFile` per i file trascinati da Explorer (da **verificare**, non l'ho provato), il drop non richiede alcuna scheggia COM.

### 8.2 Rischi

| ID | Rischio | Effetto | Mitigazione proposta |
|---|---|---|---|
| R1 ✅ **risolto dalla prova (8 Ott 2026): il drop funziona con l'evento winit `DroppedFile`, nessuna scheggia COM** | Nessun drop di file da Explorer **esposto da Slint** | Perde il gesto primario di TeraCopy (T2) | **Primo tentativo (da provare):** `on_winit_window_event` (feature `unstable-winit-030`) e l'evento winit `DroppedFile`, che winit su Windows implementa da tempo. **Secondo:** scheggia Win32 con `IDropTarget` sull'`HWND` (`raw-window-handle`): attenzione che winit può avere già registrato il proprio bersaglio di drop sulla finestra, e una seconda `RegisterDragDrop` fallisce (`DRAGDROP_E_ALREADYREGISTERED`) se non si revoca prima. **Fallback:** pulsante "Aggiungi" + estensione Shell esistente. Fase 1 |
| R2 | Nessun selettore di file nativo | Serve una dipendenza | `rfd` (usa i dialoghi di Windows). Stesso comportamento lento del dialogo attuale su unità di rete scollegate: non peggiora |
| R3 | Renderer GPU su Server/RDP | FemtoVG può non avviarsi o essere lento senza OpenGL; Skia richiede VC++ Redistributable (contraddice D30) | **Default: rendering software.** Misurare qualità del testo e fluidità; Skia escluso salvo prova di collegamento statico |
| R4 | Accessibilità non verificata | Esclude utenti con screen reader; limita anche la nostra automazione di prova | Prova UIA e Narrator in Fase 1; criterio go/no-go |
| R5 | Widget complessi (tabella con barre di avanzamento nelle celle, griglia proprietà, menu contestuali) | Lavoro di costruzione | `StandardTableView` + componenti propri; valutare nella prova su Storico e Attività |
| R6 | Tray e notifiche | **Nella prova l'icona di `SystemTrayIcon` non è comparsa** (causa ignota, nessun errore); toast Windows non inclusi | Crate `tray-icon` e `winrt-notification`/equivalente; provare |
| R7 | Perdita del ciclo di prova rapido (browser con dati finti) | Più lento verificare l'aspetto | Anteprima dal vivo di Slint (strumento `slint-viewer`/LSP) e catture automatiche (1.17 cita screenshot via albero di accessibilità); da misurare |
| R8 | Riscrittura di 4.415 righe di Svelte e dei comportamenti validati dal vivo | Regressioni silenziose | Catalogo dei comportamenti prima di scrivere ([PIANO_GUI_SLINT.md](PIANO_GUI_SLINT.md) Fase 2); la console attuale resta fino alla parità |
| R9 | Licenza non compatibile con gli obiettivi | Obblighi di attribuzione o GPL | Decisione 🔒 prima della Fase 2; schermata Informazioni prevista (RF-Y05) |
| R10 | Dipendenza da un progetto con rilascio rapido (1.17 a giugno 2026) | Cambi di API | Fissare la versione, aggiornare con prova, come per Tauri oggi |
| R11 | Dimensione dell'installer e del binario con il renderer software | Obiettivo O5 non raggiunto | Misura nella prova |
| R12 | Gestione asincrona: i callback Slint girano sul thread grafico | Riprodurre in altra forma le gare già risolte (contatori di generazione, segnali monouso) | Modello esplicito: operazioni su thread/runtime, risultati rientrano con `invoke_from_event_loop`; catalogo comportamenti |

### 8.3 Alternative considerate

| Alternativa | Pro | Contro | Giudizio |
|---|---|---|---|
| **Restare su Tauri/Svelte, alleggerito** | Nessuna riscrittura; tutto già verificato | WebView2 resta (processi multipli, ~160 MB privati); dipendenza esterna | Va **misurata** come base di confronto: riduzione possibile con argomenti del browser incorporato |
| **Slint** | Un processo, rendering software, nessun WebView, linguaggio dichiarativo, anteprima dal vivo | R1-R12 sopra; riscrittura | Candidata principale, **solo dopo prova** |
| **egui** | Semplice, tutto Rust, immediato | Testo e accessibilità meno curati; stile "da strumento" difficile da rendere raffinato | Scartata per l'obiettivo di resa visiva |
| **iced** | Elm-like, buono per stato complesso | Ecosistema più piccolo; accessibilità limitata | Non preferita |
| **WinUI 3 / Windows App SDK** | Aspetto nativo moderno | Binding Rust acerbi, pesante, vincola a un'unica versione di Windows | Scartata |
| **Qt** | Maturo e accessibile | Licenza e dimensioni; DLL numerose | Scartata |

### 8.4 Cosa ci aspettiamo dalla prova e cosa decide il no-go

La prova ([PIANO_GUI_SLINT.md](PIANO_GUI_SLINT.md) Fase 1) è un ramo isolato che porta in Slint la scheda Copia e la
scheda Report. **Le soglie di go / go condizionato / no-go sono solo in PIANO §3.3**, per non averne due diverse.
In sostanza si resta su Tauri se: la memoria non scende ben sotto quella di Tauri **alleggerito** (113 MB, non
solo quella di serie), il drop da Explorer non è ottenibile, il testo non è leggibile a 150 % con il rendering
software, l'albero UIA non è leggibile o Narrator tace, o la licenza non è accettabile.

## 9. Architettura software proposta

```
rustcopy-core (lib)  -- invariato: motore, gui_api, job_editor, runner, plan_copy, report
   ^
   |  (chiamate dirette, niente IPC)
rustcopy-ui   (nuovo, Slint)  -- viewmodel, stato, ponte asincrono, tema, i18n
   |-- ui/*.slint            -- componenti e schermate
   |-- platform/             -- shim Windows: IDropTarget, tray, toast, finestre di dialogo (rfd)
rustcopy-shell (DLL)  -- invariata, ora usa plan_copy
rustcopy-cli          -- invariata
rustcopy-gui (Tauri)  -- resta finché la nuova non è alla pari, poi rimossa
```

- **Niente comandi IPC.** I 25 comandi Tauri sono involucri sottili su `gui_api`: la nuova console li
  chiama direttamente. La regola "se un comando cresce un ramo sulla semantica di backup, quel ramo va nel
  core" resta, come revisione del codice e con gli stessi test.
- **Stato e asincronia.** Un modello per schermata; letture su thread di lavoro; i risultati rientrano sul
  thread grafico. I **comportamenti già risolti** (contatori di generazione, segnali monouso fra schede,
  sezioni che si aprono da sole, annullamento di richieste superate) sono requisiti da riportare, non
  dettagli di implementazione: vanno elencati prima ([PIANO_GUI_SLINT.md](PIANO_GUI_SLINT.md) Fase 2).
- **Tema.** Token di colore e tipografia in un solo file `.slint` (palette ambra mai rosso per i non-puliti,
  badge, schede numeriche: le regole F81/F93/F94), con chiaro/scuro che segue il sistema.
- **Installer.** Un solo `rustcopy-ui.exe` nel componente "console"; WebView2 non più richiesto (si
  rimuove il controllo e l'avviso). Il componente Shell continua a trovare la console accanto alla DLL.
- **CI.** Job `windows-latest` che compila `rustcopy-ui`, esegue `check-static-crt.ps1` e il test di
  installazione già esistente; esclusa dai job cross-platform come oggi la GUI.

### 9.1 Interfacce da preservare (rilevate rileggendo il codice)

La nuova console non può cambiare queste, o rompe pezzi che oggi funzionano:

| Interfaccia | Dove | Cosa implica |
|---|---|---|
| Nome del file eseguibile `rustcopy-gui.exe` | `runner.rs` (`gui_beside`), `installer/rustcopy.iss` (`MyGuiExeName`), estensione Shell (`spawn.rs`) | Se la nuova console si chiama `rustcopy-ui.exe`, vanno cambiati tutti e tre **e** il test in `runner.rs`. Più semplice: **tenere il nome** e cambiare solo il contenuto |
| Argomento `--auto-config <toml>` | `rustcopy-gui/src/main.rs`, usato dall'estensione Shell | Contratto fra due crate; la nuova console deve accettarlo (RF-Y11: consegnarlo all'istanza già aperta) |
| Testo del componente installer "Console grafica (richiede WebView2)" e il controllo `IsWebView2Installed` | `installer/rustcopy.iss` | Da rimuovere alla Fase 5, con il suo avviso |
| Quattro file con la versione | `scripts/check-versions.sh` | La nuova `Cargo.toml` dichiara la versione; `tauri.conf.json` e `package.json` spariscono con Tauri |
| `check-static-crt.ps1` su quattro artefatti | CI e procedura di rilascio | Il binario nuovo deve passare |
| Job `gui`, `gui-npm-audit`, `installer-smoke` | `.github/workflows` | Cambiano job e trigger, non vanno persi |

## 10. Sicurezza e confini 🔒

**In parole semplici — la «regola di sicurezza» della console.** Oggi la console grafica può *far partire* una copia
e *mostrare* tutto, ma **non può decidere da sola** le azioni che cancellano dati o cambiano il sistema senza che
una persona sia davanti: avviare un mirror non presidiato (che cancella in destinazione ciò che manca in origine),
forzare una pulizia (`--force-purge`), installare o togliere pianificazioni e servizi. Può *segnalarle* ("questo
job cancellerebbe 3.000 file"), mai *autorizzarle*. È una scelta di prudenza presa quando è nata la console,
scritta in [PIANO_GUI.md](PIANO_GUI.md) §7 e nell'elenco dei divieti di ROADMAP F61 (che riguarda un altro
argomento, il server MCP, ma ne riusa la lista; per questo in una prima stesura la chiamavo "F61": nome
fuorviante, ora abbandonato). Cobian e TeraCopy fanno invece queste cose senza chiedere: **per avere "tutte le
funzioni" bisognerebbe allentarla in modo controllato**, ed è ciò che questa sezione chiede di decidere.

Il confine di sicurezza ([PIANO_GUI.md](PIANO_GUI.md) §7) oggi impedisce alla console di: avviare un mirror non
presidiato, forzare una purge, installare servizi o pianificazioni. L'utente ha chiesto una console che
"includa entrambe le funzionalità" di TeraCopy e Cobian: **alcune sono esattamente quelle vietate.** Non le
decido io, le metto in fila con la mia raccomandazione:

| ID | Funzione | Oggi | Raccomandazione | Perché |
|---|---|---|---|---|
| D-F1 | Creare/modificare/eliminare pianificazioni dalla GUI | Vietato | **Riaprire**, ma in forma stretta: la GUI prepara il comando, la **CLI** lo installa dopo una richiesta UAC esplicita e visibile; mai privilegi permanenti nella GUI | È ciò che rende Cobian utile; il rischio è l'elevazione, non la funzione |
| D-F2 | Pausa/Riprendi | Sospeso (§15) | **Livello A** (sospensione del processo robocopy) con timeout di ripresa e prova su SMB reale prima del rilascio; dichiararlo come funzione "migliore possibile", non universale | Piano già scritto; rischio noto (deadlock, sessioni SMB) |
| D-F3 | Salta un file / collisioni interattive / "mantieni entrambi" | Non possibile con robocopy | **Rimandare** alla valutazione di un motore interattivo; per ora politiche prima dell'avvio | Costo alto: motore naive come motore di copia interattiva |
| D-F4 | Sposta | Non esiste (F46) | **Sì, a due passi**: copia → verifica riuscita → conferma esplicita → elimina origine, mai non presidiato | Il valore è reale; la sicurezza sta nell'ordine e nella conferma |
| D-F5 | Elimina sicura | Non esiste | **No** per ora | Distruttivo, poco legato alla copia |
| D-F6 | Mirror da GUI | Segnalato, mai autorizzato | **Mantenere** | Resta l'impostazione più distruttiva |
| D-F7 | Esegui come altro utente / password salvate | Assente | **No** per ora | Gestione di credenziali: già coperta dal Credential Manager per le chiavi |

### 10.1 Proposta dell'utente: livelli di sicurezza scelti nelle impostazioni (8 Ott 2026)

L'utente propone di non decidere noi caso per caso, ma di far scegliere **all'avvio iniziale** (e poi nelle
Impostazioni) con un selettore *sicuro / non sicuro*, preferendo di norma il modo sicuro. È una buona soluzione e la
adotto, con **quattro paletti** che la rendono robusta invece che un semplice interruttore:

1. **Predefinito = il più sicuro**, anche dopo un aggiornamento o un ripristino delle impostazioni.
2. **Il livello sblocca ciò che la console può *preparare*, non cancella le conferme.** Anche al livello più alto, ogni
   azione distruttiva mostra prima **che cosa farà** (anteprima, come `--purge-preview-path`) e chiede conferma.
3. **Il livello vive nelle impostazioni dell'utente, mai in un file di job**: un `.toml` modificabile o scambiato fra
   macchine non può far salire il livello. Stessa logica per cui `--force-purge` non è impostabile da configurazione.
4. **Windows decide sull'elevazione**: nessun livello evita l'avviso UAC per installare una pianificazione.

Tre livelli, non due, perché "non sicuro" è troppo largo:

| Livello | Che cosa la console può fare |
|---|---|
| **Prudente** (predefinito) | Solo ciò che fa oggi: avviare copie, mostrare, segnalare. Nessuna pianificazione né cancellazione dalla GUI |
| **Standard** | In più: **preparare pianificazioni** (la CLI le installa dopo l'UAC) e **Sposta** a due passi con conferma |
| **Esperto** | In più: mirror e pulizie **presidiati** (con anteprima e conferma a ogni esecuzione). **Mai** un mirror o una pulizia *non presidiati* dalla console: restano solo da CLI, per scelta esplicita |

Il livello corrente è sempre visibile nella barra di stato; salirlo richiede una conferma che spiega cosa si sblocca;
ogni cambio è registrato nel log. Requisito RF-Y12. Resta una **decisione da confermare** se anche il livello Esperto
debba escludere i mirror non presidiati (la mia raccomandazione: sì).

## 11. Verifica e collaudo

1. **Catalogo dei comportamenti** della console attuale (Fase 2): ogni voce di [CLAUDE.md](CLAUDE.md) sulla
   console diventa un caso di prova da riportare.
2. **Test automatici**: la logica resta nel core con i suoi test; per l'interfaccia, test di viewmodel senza
   finestra e, dove Slint lo consente, catture automatiche confrontate.
3. **Prova dal vivo sul binario compilato** per ogni onda, come già fatto con la console attuale, usando
   l'albero UIA (la stessa tecnica che ha funzionato su Cobian WPF) e le catture di schermo.
4. **Matrice di piattaforme**: Windows 11 (questa macchina), Server 2022 e Server 2019 (CI e, se disponibili,
   macchine reali), una VM senza GPU per RNF-02, DPI 100/150/200 %.
5. **Accessibilità**: albero UIA completo di nomi e ruoli, prova con Narrator.
6. **Misure**: lo stesso script di §2.1 prima e dopo, riportato nel piano.

## 12. Decisioni aperte

Elenco completo e proposto con raccomandazione in [PIANO_GUI_SLINT.md](PIANO_GUI_SLINT.md) §6. Le più
pesanti: forma del prodotto (una app, un'entità "lavoro", §5.0), riapertura dei confini di sicurezza (§10), licenza di Slint,
trascinamento da Explorer (R1), politica di migrazione (la console attuale resta fino alla parità).

## 13. Criticità trovate rileggendo questa specifica

- La prima stesura dava per scontato il rilascio di file sulla finestra "perché Slint ha il drag and drop".
  È falso per i file di Explorer nella 1.17: R1 è diventato rischio di prima fascia con una mitigazione,
  non un dettaglio.
- Avevo scritto "rendering software = più leggero" senza dato. Il documento ufficiale dice solo che il
  renderer software è "lightweight" e senza GPU; **qualità del testo e fluidità** a 150-200 % di DPI vanno
  misurate, non assunte.
- Il tutorial di Cobian descrive la creazione di un'attività con un dialogo a due campi (nome e tipo);
  la versione installata ne ha uno solo con otto schede e il tipo nella scheda Generale. Le schermate di
  riferimento sono quelle **dal vivo**, non quelle del manuale.
- **Soglie contraddittorie** (60/80/100 MB, 0,4/0,8 s) in tre punti: ora una sola tabella, nel piano.
- **"Stesso script di §2.1"**: lo script non esisteva e le due misure d'avvio (0,70 s e 0,12-0,24 s) misuravano cose
  diverse. Dichiarato in O3; lo script è un prerequisito della Fase 1 (PIANO M0).
- **"557 test del core"** non aveva fonte: la libreria ne ha 468 (misurato con `cargo test -p rustcopy-core --lib -- --list`).
- **Confronto con Tauri "di serie"**: il vero termine di paragone è Tauri **alleggerito**, misurato a 113 MB privati
  (da 168) con due argomenti di WebView2. La riscrittura va giustificata contro quel numero, non contro 168.
- **R1 sottovalutava winit**: probabilmente il drop arriva già come evento winit; e la scheggia COM, se serve,
  rischia di scontrarsi con la registrazione che winit ha già fatto.
- **Licenza**: avevo scritto che serviva una schermata Informazioni. Il testo ufficiale ammette in alternativa il
  badge sulla pagina di download, quindi la decisione non è "mostrare o no una schermata" ma quale forma.
- **Contratti nascosti** (nome dell'eseguibile, `--auto-config`, installer, CI): non erano elencati, ora sono in §9.1.
- **"Due modalità"**: rivalutata, vedi §5.0.
- **Mancava la singola istanza** (RF-Y11), osservata in TeraCopy e necessaria per il drop da Explorer.
- **Fatto non verificato eliminato**: "il tray non aveva API fino alla 1.4".
