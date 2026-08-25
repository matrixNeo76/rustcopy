---
name: molecule-8-diagnose
version: 1.0.0
category: molecule
parent: rustcopy-flow
tags: [rustcopy, diagnosi, storico, advise, anomalie, linguaggio-naturale]
description: "Risponde a domande sullo storico dei backup ('quanto durano di solito?', 'quando è fallito l'ultimo?', 'perché è lento da martedì?') interrogando l'indice delle run e --advise. Nessuna esecuzione di backup, nessuna operazione distruttiva."
---

# Molecola 8: 🔍 Diagnose — domande sullo storico

> **Sola lettura.** Questa molecola non lancia backup, non cancella nulla e non modifica
> configurazioni. Se la richiesta dell'utente implica un'azione (ripianificare, cambiare
> retention, ripristinare), questa molecola **produce la proposta** e passa la palla alla
> molecola competente, che ha i suoi checkpoint umani.

## Quando usarla

L'utente fa una domanda sul **passato** anziché chiedere un'operazione:

- "quanto durano di solito i backup del NAS?"
- "quando è fallito l'ultimo backup?"
- "perché ci mette di più da martedì?"
- "questo backup è andato bene?"
- "ogni quanto posso schedularlo senza sovrappormi?"
- "quanto spazio mi serve se tengo 7 generazioni?"

Distinzione utile: se la frase contiene un verbo al futuro o all'imperativo ("fai", "esegui",
"ripristina", "pianifica"), **non** è questa molecola — è la 1 (Plan) o una di esecuzione.

## Input

- Il `--report-path` usato dalle run di quel job (o il default, se non è mai stato personalizzato).
  L'indice `.rustcopy_history.jsonl` sta **accanto ai report**, non dentro `--dest`.
- Facoltativo: il nome del job, se la configurazione usa `[[jobs]]`.

## Steps

1. **Individua l'indice dello storico** — senza, non c'è nulla da diagnosticare.
   - Comando: `ls <report-dir>/.rustcopy_history.jsonl` (Bash) o
     `Test-Path <report-dir>\.rustcopy_history.jsonl` (PowerShell)
   - Input: la directory dei report di quel job
   - Output: percorso confermato, oppure la constatazione che non esiste
   - Output metric: il file esiste ed è non vuoto
   - **Se manca**: non è un errore. Significa che nessuna run è ancora stata registrata con una
     versione che scrive l'indice. Dillo esplicitamente e fermati qui: **non inventare stime**.

2. **Esegui l'analisi deterministica** — è la fonte primaria, non un ripiego.
   - Comando: `robocopy_ingest.exe --advise --report-path <report-path>`
   - Input: lo stesso `--report-path` usato dalle run
   - Output: elenco di rilievi con severità (`ATTENZIONE` / `PROPOSTA` / `INFO`) e le evidenze
     numeriche di ciascuno
   - Output metric: exit code `0`
   - Aggiungi `--job-name` non esiste come flag: se il job usa `[[jobs]]`, l'indice è già
     namespacizzato nel nome file (`.rustcopy_history.<job>.jsonl`).

3. **Rispondi alla domanda posta, non a tutte** — `--advise` produce ogni rilievo disponibile;
   l'utente ne ha chiesto uno.
   - Input: l'output dello step 2
   - Output: risposta in italiano, che cita **i numeri** dell'evidenza
   - Output metric: la risposta contiene almeno un valore misurato, mai solo un aggettivo
   - **Non riformulare "0.09s mediana" in "molto veloce" e basta**: il numero è ciò che rende la
     risposta verificabile dall'utente.

4. **Se serve dettaglio oltre gli aggregati, apri il report della run specifica** — l'indice
   contiene aggregati per riga, non dati per-file.
   - Comando: leggi il JSON indicato dal campo `report_path` della riga di interesse
   - Input: la riga dello storico selezionata
   - Output: mismatch specifici, path non leggibili, timing per fase
   - Output metric: hai trovato il campo che risponde alla domanda
   - Per interpretare exit code e campi del report, **usa la Molecola 7**, non reimplementarla qui.

5. **Se emerge un'azione, formulala come proposta e fermati** — la decisione è dell'utente.
   - Input: i rilievi di severità `ATTENZIONE` o `PROPOSTA`
   - Output: una proposta concreta con il comando esatto, **non eseguito**
   - Output metric: l'utente ha davanti comando e motivazione numerica
   - **Vietato** eseguire in questo turno: `--force-purge`, `--mirror` non presidiato, purge di
     retention, `--install-schedule`/`--uninstall-schedule`, `--install-service`/`--uninstall-service`.
     Valgono qui integralmente i divieti della skill madre.

## Output Finale

- Risposta in italiano alla domanda posta, con le evidenze numeriche
- Eventuale proposta di azione, con il comando esatto e la ragione — da eseguire solo se l'utente
  lo chiede esplicitamente nel turno successivo

## Failure Modes

- **L'indice non esiste**: nessuna run registrata. Dillo e fermati; non stimare nulla dai report
  sciolti (è esattamente il lavoro manuale che l'indice esiste per eliminare).
- **`--advise` esce con codice diverso da `0`**: è un errore d'uso, non un problema di backup.
  Il caso più probabile è un `--report-path` che punta a una directory diversa da quella delle run.
  Verifica con lo step 1 prima di ipotizzare altro.
- **`--advise` dice "campione — N righe non leggibili"**: lo storico ha righe danneggiate (tipico:
  un append interrotto). I backup già fatti **non** sono compromessi; è solo il campione a essere
  incompleto. Riportalo all'utente, non nasconderlo.
- **`--advise` dice "servono almeno 3 run"**: non forzare una risposta. Con due run non esiste una
  distribuzione, e una stima inventata su due punti è peggio di un "non lo so ancora".
- **L'utente chiede una previsione che i dati non sostengono** (es. "quanto durerà il backup del
  mese prossimo con il doppio dei file?"): dichiara l'estrapolazione come tale, e dai il dato
  osservato da cui parte.
