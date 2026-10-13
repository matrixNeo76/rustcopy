---
type: Plan
title: Piano della console centrata sulle attività (stile Cobian)
description: Progetto per rendere la console Slint simile anche a Cobian Reflector oltre che a TeraCopy: pagina principale con la tabella delle attività, procedura guidata per crearle, voci riservate sempre visibili, attiva/sospendi, eventi e posta nell'editor; con le fasi, le regole di sicurezza e le decisioni che restano all'utente.
status: draft
generated:
  by: process:claude-code
  at: 2026-10-13T09:00:00Z
---

# Piano della console centrata sulle attività

Nasce dalla prova dal vivo del 13 Ott 2026: con il livello **Esperto** la console sembrava identica a quella del livello normale, e
nel complesso somigliava a TeraCopy ma non a Cobian Reflector. L'osservazione è corretta e ha tre cause, tutte nostre.

## 1. Che cosa non va oggi

1. **La pagina iniziale è «Nuova copia»**: una copia al volo, cioè il modello TeraCopy. Non c'è un punto da cui si governa un insieme di attività.
2. **Quasi tutto ciò che somiglia a Cobian c'è già, ma è nascosto dietro due passaggi**: bisogna fare una copia, salvarla come attività e aprire «Modifica». Tipi di
   copia (completa, incrementale, differenziale), conservazione dei cicli, pianificazione con recupero di un'esecuzione persa, copia ombra (VSS), cifratura,
   filtri, storico, osservazioni: esistono, ma chi comincia non li vede.
3. **I livelli di sicurezza nascondono le voci invece di mostrarle bloccate.** Esperto sblocca una sola casella (la pianificazione elevata); senza attività salvate
   non si vede nulla. Una funzione nascosta fa sembrare l'applicativo più povero di quello che è.

## 2. Principi

- **Attività al centro, copia al volo accanto.** Le due cose convivono: la pagina principale è la tabella delle attività (Cobian), da cui «Copia al volo» è un'azione
  rapida (TeraCopy).
- **Le voci riservate si vedono sempre**, in grigio, con scritto il livello che le sblocca. Un livello decide *che cosa si può fare*, non *che cosa esiste*.
- **Nessuna regola di sicurezza si indebolisce.** Valgono i divieti di F61 e `SafetyLevel`: il core decide, la console è un involucro sottile, nessun mirror o
  pulizia non presidiati, nessun comando preso da un file che viaggia.
- **Niente nuovo motore in questo giro.** Compressione, destinazioni multiple e spegnimento a fine copia cambiano il motore e restano fuori (sezione 7).

## 3. Schermate

La bozza visiva è stata mostrata in chat il 13 Ott 2026 (tabella delle attività e passo 2 della procedura guidata).

### 3.1 Pagina principale «Attività»

Una tabella, una riga per attività, con questi dati (la maggior parte esiste già in `TaskRow`):

| Colonna | Dato | Stato |
|---|---|---|
| Nome | nome dell'attività | esiste |
| Da → a | cartelle di origine e destinazione | esiste (`folders`) |
| Tipo | copia semplice / completa / incrementale / differenziale | da aggiungere (letto dal file con `gui_api::list_jobs`) |
| Quando | testo della pianificazione («Ogni giorno 22:00», «Non pianificata», «Sospesa») | da aggiungere (`schedule_details`) |
| Ultima volta | esito (Riuscita / Da controllare / Interrotta) e quando | esiste (`last-state`, `last-when`) |
| Prossima | prossima esecuzione, come la riporta Utilità di pianificazione | esiste (`next-run`) |

In alto: **Nuova attività** (procedura guidata), **Copia al volo**, **Esegui tutte**. Sotto la riga selezionata: Esegui, Modifica, Pianifica, Storico, Proprietà, Sospendi,
Forza completo. Lo stato vuoto resta quello spiegato il 12 Ott 2026.

«Esegui tutte» esegue le attività attive **una dopo l'altra** (coda della console, come per i job di un file), mai in parallelo: sullo stesso NAS più copie insieme
rallentano tutte.

### 3.2 Procedura guidata «Nuova attività»

Cinque passi, con barra di avanzamento e «Indietro»/«Avanti»:

1. **Cosa copiare**: cartelle e file (come «Nuova copia»), nome dell'attività.
2. **Dove metterla**: destinazione e **tipo di copia**, con una frase per ciascuno (semplice: aggiorna; incrementale: una versione per volta con solo le novità;
   differenziale: tutto ciò che è cambiato dall'ultima completa; completa: tutto, ogni volta).
3. **Quando**: mai (solo a mano), ogni giorno, ogni tot ore, nei giorni scelti. La scelta di pianificare richiede il livello Standard; sotto, il passo mostra la voce
   in grigio con la ragione.
4. **Come**: verifica dopo la copia, esclusioni, filtri per età, limite di banda. **Niente «cicli da conservare»**: la conservazione cancella le versioni vecchie e una console non la introduce mai (resta a mano nel file, come per l'editor).
5. **Riepilogo**: tutto quello che verrà scritto, in italiano, e «Crea attività». Scrive un file nuovo; non sovrascrive mai (`create_new`).

Riuso: `job_editor::build_proposal(None, &[bozza])` e `propose_config` creano il file; `schedule::install` pianifica. Cambia soltanto la procedura che li chiama.

### 3.3 Voci riservate visibili

Per ogni funzione dietro un livello (Pianifica, Sposta, Forza completo, pianificazione elevata) la voce compare sempre. Se il livello non basta è grigia, con la frase
«Richiede il livello Standard» e un pulsante «Cambia livello...» che porta alla pagina Sicurezza. Il controllo vero resta nel core: il grigio è un'indicazione, non la
difesa (`schedule::install`, `moves::execute_move` rileggono il livello dal disco).

### 3.4 Sospendi / riattiva

«Sospendi» fa due cose: disattiva la pianificazione di Windows di quell'attività (`schtasks /Change /DISABLE`, riattivabile) e la esclude da «Esegui tutte». Non cancella
nulla. La riga appare attenuata con «Sospesa». Lo stato della sospensione per le attività non pianificate sta nella cartella dati della console, mai nel file dell'attività.

### 3.5 Eventi e posta nell'editor

- **Prima e dopo la copia** (`pre_command`/`post_command`): campi nell'editor, **solo al livello Esperto**, con una conferma che mostra il comando e dice che verrà eseguito
  con i tuoi diritti a ogni esecuzione. Mai in una pianificazione elevata (il core la rifiuta già). Oggi `JobDraft` non li contiene per scelta: scriverli dalla console è
  una decisione di sicurezza, vedi sezione 6.
- **Notifica per posta**: pagina che prepara la sezione `[smtp]` del `notify-server.toml` (la password resta in Gestione credenziali, `keyring:NOME`) e un pulsante
  «Invia una prova». Oggi si fa solo a mano nel file.

## 4. Cosa si riusa

`TaskRow`, `gui_api::{list_jobs, schedule_details}`, `job_editor::{JobDraft, build_proposal, propose_config}`, `schedule::{install, list_installed}`, `queue.rs`,
`SafetyLevel`. Il core guadagna `SafetyLevel::requirement(feature)` (che livello serve a una voce) così il grigio e il messaggio vengono dal core e non dal frontend.

## 5. Fasi, una PR ciascuna

| Fase | Contenuto | Rischio |
|---|---|---|
| 1 | Pagina «Attività» come home con la tabella (colonne Tipo e Quando), «Copia al volo», voci riservate visibili in grigio, `SafetyLevel::lock_reason` | basso: solo presentazione. **Fatta il 13 Ott 2026** |
| 2 | Procedura guidata «Nuova attività» (5 passi) | medio: crea file; test di non sovrascrittura. **Fatta il 13 Ott 2026** (senza «cicli da conservare»: cancella, quindi resta solo a mano, come per l'editor) |
| 3 | «Sospendi/riattiva» e «Esegui tutte» (coda tra attività) | medio: tocca `schtasks`. **Fatta il 14 Ott 2026** |
| 4 | Eventi prima/dopo nell'editor (Esperto + conferma) | alto: scrive un comando eseguibile; richiede la tua decisione |
| 5 | Posta nella console | basso/medio |

Ogni fase: test in `ui_tests`, immagini delle pagine (`snapshot_pages`), catalogo e changelog aggiornati, controllo manuale in `CONTROLLI_MANUALI_GUI.md`.

## 6. Decisioni che restano a te

1. **Eventi prima/dopo dall'editor (fase 4).** È l'unica fase che fa scrivere alla console un comando che poi gira da solo. Proposta: solo Esperto, conferma che mostra il
   comando, mai nelle pianificazioni elevate, mai importato da un file. Va bene, o preferisci che restino da scrivere a mano nel file?
2. **«Sospendi» tocca Utilità di pianificazione.** Va bene che la console disattivi e riattivi una pianificazione che ha creato lei?
3. **Ordine delle fasi.** Proposto: 1, 2, 3, 5, 4.

## 7. Fuori da questo piano (cambiano il motore)

- **Compressione** (zip o 7z) e divisione in più file: oggi il motore non comprime.
- **Più destinazioni** per la stessa attività.
- **Spegnere il computer** a fine copia.
- **Modalità servizio** che esegue le attività senza utente connesso (`--install-service` oggi è inattivo per scelta).
- **«Solo verifica»**: controllare una copia senza copiare; serve una modalità nuova del motore e una tua decisione.

Si pianificano a parte, una volta chiuse le fasi 1-3, perché ciascuna merita una scelta di progetto propria.
