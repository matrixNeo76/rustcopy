---
name: molecule-9-notify
version: 1.0.0
category: molecule
parent: rustcopy-flow
tags: [rustcopy, notifiche, webhook, ntfy, notify-server, servizio-windows]
description: "Configura e verifica il recapito delle notifiche di fine backup: --webhook-url sul lato client, notify-server sul lato ricezione (ntfy, webhook generico, log). Colma l'unico agente specializzato che mancava alla skill."
---

# Molecola 9: 📣 Notify — recapito delle notifiche

> Colma il gap rilevato in `VALUTAZIONE_AI.md` §2.2: fra gli agenti specializzati *scan, backup,
> verify, restore, notify*, quest'ultimo era l'unico senza una molecola — la notifica esisteva
> come flag e come binario, ma non come passo orchestrato.

## Quando usarla

- L'utente vuole essere avvisato quando un backup finisce o fallisce
- Una notifica attesa non è arrivata e va diagnosticata
- Va montato o verificato `notify-server` come destinatario persistente

**Non** usarla per interpretare l'esito di un backup: quello è la Molecola 7.

## Precondizione da conoscere

`notify-server` è un **binario separato e feature-gated**. Non esiste nel build di default:

```
cargo build --release --features notify-server
```

Se non ti serve un fan-out multicanale, `--webhook-url` da solo basta e non richiede alcun
server aggiuntivo: punta a un endpoint HTTP che l'utente già possiede.

## Input

- L'intento: solo avviso, oppure fan-out su più canali
- Se fan-out: quali canali (`ntfy`, webhook generico, log)

## Steps

1. **Stabilisci se serve `notify-server`** — la maggior parte dei casi non lo richiede.
   - Input: la risposta dell'utente su quanti/quali canali
   - Output: decisione fra "solo `--webhook-url`" e "client + server"
   - Output metric: la scelta è motivata dai canali richiesti, non dal default
   - Un solo endpoint HTTP già esistente → **niente server**, salta agli step 4-5.

2. **Prepara la configurazione del server** (solo nel percorso fan-out) — TOML con i canali.
   - Input: URL dei canali scelti
   - Output: file TOML con `bind` e le sezioni `[ntfy]` / `[generic_webhook]`, ciascuna con
     `enabled` e il proprio URL
   - Output metric: il file è parsabile e ogni canale voluto ha `enabled = true`
   - **Non inserire segreti nel repo**: il TOML sta fuori dall'albero di progetto.

3. **Avvia il server e verifica che risponda** — prima di collegarci un backup.
   - Comando: `notify-server --config <file.toml>` (in foreground, per la verifica)
   - Input: il TOML dello step 2
   - Output: server in ascolto sull'indirizzo configurato
   - Output metric: il processo resta attivo e logga l'avvenuto bind
   - Per l'esecuzione persistente esiste `notify-server --install-service`
     (servizio `RustcopyNotifyServer`, **distinto** da quello di `robocopy_ingest`). Richiede
     privilegi di Amministratore: **è un checkpoint umano**, non eseguirlo di iniziativa.

4. **Collega il backup** — `--webhook-url` sul comando del job.
   - Comando: `robocopy_ingest.exe ... --webhook-url <URL>`
   - Input: l'URL del server dello step 3, oppure l'endpoint già esistente dello step 1
   - Output: il comando di backup completo
   - Output metric: l'URL è raggiungibile dall'host che esegue il backup — non da qui
   - In un TOML con `[[jobs]]`, `webhook_url` può stare per singolo job.

5. **Verifica il recapito end-to-end con un backup vero e innocuo** — un `--dry-run` non è
   sufficiente da solo se l'obiettivo è provare il recapito reale.
   - Comando: un backup piccolo e reale verso una destinazione di prova, con `--webhook-url`
   - Input: sorgente/destinazione di prova, mai dati di produzione
   - Output: notifica ricevuta sul canale
   - Output metric: il canale mostra il messaggio **e** il report JSON non contiene
     `webhook_error`

6. **Se la notifica non è arrivata, distingui i due fallimenti possibili** — hanno cause opposte.
   - Input: il report JSON della run e i log del server
   - Output: diagnosi
   - Output metric: sai se il problema è a monte o a valle
   - `webhook_error` **valorizzato** nel report → il client non è riuscito a consegnare (rete,
     URL, TLS, server spento).
   - `webhook_error` **assente** ma nessun messaggio ricevuto → la consegna è riuscita e il
     problema è nel fan-out del server: controlla `enabled` dei canali e i log del server.

## Output Finale

- Comando di backup completo di `--webhook-url`
- Se richiesto: TOML del server e istruzioni di avvio (o proposta di installazione come servizio)
- Esito della verifica end-to-end

## Failure Modes

- **`notify-server` non esiste / non parte**: il binario è feature-gated. Ricompila con
  `--features notify-server`. Non è un bug.
- **Un backup fallisce e sospetti la notifica**: non può essere lei. Un errore di consegna è
  **deliberatamente non fatale** — viene solo registrato in `webhook_error`, e non cambia mai
  l'exit code del backup. Se il backup è fallito, la causa è altrove: vai alla Molecola 7.
- **L'utente chiede Telegram o email**: non sono supportati oggi. Un webhook generico **non** è
  sufficiente per Telegram (formato del payload diverso) né per l'email (SMTP è un altro
  protocollo). Sono F43/F44 in backlog: dillo, non proporre un aggiramento che non funziona.
- **L'installazione del servizio fallisce con accesso negato**: serve una shell con privilegi di
  Amministratore. Non è aggirabile e non va aggirato.
- **URL con token nel path**: trattalo come un segreto. Non ripeterlo nei riepiloghi né nei log
  che mostri all'utente.
