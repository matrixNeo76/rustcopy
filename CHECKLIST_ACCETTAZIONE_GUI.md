---
type: Reference
title: Checklist di accettazione della console Slint (cancello per rimuovere Tauri)
description: Confronto onesto fra console Slint, console Tauri, TeraCopy e Cobian Reflector, elenco delle lacune ancora aperte e prove da fare fianco a fianco; la rimozione di Tauri si decide solo quando questa lista è chiusa.
status: draft
generated:
  by: process:claude-code
  at: 2026-10-09T16:00:00Z
---

# Checklist di accettazione della console Slint

**Scopo.** Decidere se la console Slint può sostituire la Tauri senza perdere nulla. Non basta che «le fasi del piano
siano chiuse»: il piano misurava *ciò che avevamo deciso di costruire*, non *tutto ciò che la Tauri sa fare*. Questo documento
confronta le due direttamente, e confronta la Slint con i due riferimenti da cui siamo partiti (TeraCopy e Cobian Reflector).

**Regola del cancello.** Tauri si rimuove solo quando (1) ogni lacuna della §2 è **colmata o accettata da te per iscritto**,
(2) le prove della §5 sono tutte fatte fianco a fianco, (3) i controlli di [CONTROLLI_MANUALI_GUI.md](CONTROLLI_MANUALI_GUI.md)
sono fatti. Fino ad allora Tauri resta nel repository. L'installer installa già la Slint come «console grafica»: per le prove
fianco a fianco la Tauri si costruisce a parte (vedi §5).

## 1. Verdetto onesto

La Slint **non è ancora equivalente** alla Tauri. Fa cose che la Tauri non fa (§3), ma mancano alcune cose che la Tauri ha (§2).
Rispetto ai riferimenti: ha **quasi tutto ciò che ci interessava di TeraCopy** (§4.1) e **la parte di Cobian Reflector che il
motore rustcopy sa fare** (§4.2); non ha le funzioni di Cobian che il motore non ha mai avuto (compressione, FTP, gruppi, ecc.).

## 2. Lacune verso la console Tauri (verificate leggendo il codice delle due console)

La colonna «Blocca?» dice se la lacuna impedisce di togliere Tauri senza una tua accettazione esplicita.

| # | Cosa fa la Tauri e la Slint no | Dove si vede | Blocca? |
|---|---|---|---|
| L1 | **Preferiti** dei percorsi (oltre ai recenti), con etichetta, che restano finché non li rimuovi | casella percorso in ogni scheda | No, comodità |
| L2 | **«+ Nuovo job»** nell'editor: aggiungere un job a una configurazione (multi-job) | Modifica | ~~Sì~~ **colmata** (9 Ott 2026): «+ Nuovo job», provato dal vivo (L38) |
| L3 | **«Verifica»** accanto a Origine e Destinazione nell'editor: conta file e dimensione di quel percorso | Modifica | ~~No~~ **colmata** (9 Ott 2026): «Verifica» per campo, provata dal vivo (L40) |
| L4 | **Problemi del report a pagine** (100 per volta, «Precedenti/Successivi») e **Esporta CSV** dell'elenco dei problemi | Report | ~~Sì~~ **colmata** (9 Ott 2026): pagine da 100 e CSV di tutti i problemi, provati dal vivo (L39) |
| L5 | **Procedura guidata «Nuovo job»** e «Sincronizza ora» dalla scheda vuota | Job | No: il caso comune è coperto da Nuova copia → «Salva come attività» |
| L6 | **Scheda Esegui**: scegliere **fra più checkpoint** (la Slint offre il più recente) e vedere per ogni job della coda *in attesa / in corso / concluso* (la Slint dice «Job N di M») | Esegui | **Metà colmata** (9 Ott 2026): lo stato per job in coda c'è (L43, provato dal vivo); resta la scelta fra più checkpoint (la Slint offre il più recente) |
| L7 | **Scheda Job per job**: ogni job di una configurazione in una tabella, con **ultima esecuzione per job** e icone (cifrata, retention, esclusioni, thread) | Job | ~~Sì~~ **colmata** (9 Ott 2026): pulsante «Job» per ogni attività, un job per riga con il suo esito (L38) |
| L8 | Un **tooltip su ogni controllo dell'editor** (la Slint ha le descrizioni per l'accessibilità solo su alcuni) | Modifica | ~~No~~ **colmata nella forma, non nella lettera** (9 Ott 2026): ogni casella ha una frase visibile e una descrizione accessibile; Slint standard non ha il tooltip al passaggio del mouse (L41) |

Cose che avevo temuto mancassero e invece **non** sono lacune: l'output della CLI in tempo reale e l'elenco di tutte le pianificazioni con
la prossima esecuzione non esistono nemmeno nella Tauri (usa solo la domanda «c'è una pianificazione per questo file?», come la Slint).

## 3. Cose che la Slint fa e la Tauri no

Livello di sicurezza e blocchi nel core · creare e togliere pianificazioni · **Sposta** a due passi con verifica · **pausa**/ripresa ·
**file singoli** · espelli l'unità · **notifica di sistema** e lampeggio · icona nell'area di notifica · una sola finestra (un secondo avvio
consegna il lavoro alla prima) · grafico della velocità e tempo residuo · «Controlla prima» · posta (dal notify-server) · recenti per origine
e destinazione · cronologia laterale di tutti i lavori con esito · un processo solo, ~7 MB contro 165 MB, avvio in ~0,2 s.

## 4. Rispetto ai riferimenti

### 4.1 TeraCopy

| Funzione di TeraCopy | Slint | Nota |
|---|---|---|
| Trascinare cartelle e file nella finestra | ✅ | cartelle e file |
| Voce nel menu di Explorer (trascinamento con tasto destro) | ✅ | estensione Shell esistente, ora apre la Slint |
| Copia, **Sposta**, verifica con checksum | ✅ | Sposta solo dal livello Standard, con anteprima e conferma |
| Sessione immediata con avanzamento, velocità, tempo residuo, grafico | ✅ | |
| Pausa / riprendi | ✅ | sospende l'albero di processi; riprende da sola dopo 10 min |
| Fermare e **riprendere** una copia interrotta | ✅ | checkpoint |
| Coda di più copie | 🟡 | più cartelle = un lotto in sequenza; non si riordina |
| Cronologia delle sessioni | ✅ | barra laterale, con esito |
| Azioni «al termine» (apri cartella, espelli, notifica) | ✅ | spegni il PC **scartato** (lavori su server) |
| **Scelta sui conflitti** (salta, sovrascrivi, rinomina) | ❌ per scelta | si tiene il comportamento di robocopy: copia nuovi e cambiati, salta gli identici |
| Saltare un file in corsa, riprovare i falliti | ❌ | richiederebbe un motore diverso |
| File di checksum esportabile | ❌ | |
| Sostituire il copia/incolla di Explorer | ❌ | fuori ambito (costo alto) |

### 4.2 Cobian Reflector

| Funzione di Cobian | Slint | Nota |
|---|---|---|
| Elenco attività con esito e ultima esecuzione | ✅ | per configurazione e, col pulsante «Job», per job (L7 colmata) |
| Creare / modificare un'attività | ✅ | modifica per proposte, anche «+ Nuovo job» (L2 colmata); nuova da «Salva come attività» (L5) |
| Completo / Incrementale / Differenziale | ✅ | |
| Pianificazione (creare, vedere, togliere) | ✅ | dal livello Standard; non si vede la «prossima esecuzione» |
| Eventi prima/dopo | 🟡 | si leggono in Proprietà; **non si scrivono** dalla console (decisione F55 aperta) |
| Conservazione per cicli, forza completo | ✅ | cicli sì (si può solo alzare); «Forza completo» dal livello Standard (L44) |
| Cifratura | ✅ | AES-256, chiave nel Gestore credenziali |
| Notifica via posta | ✅ | dal notify-server |
| Copia di file in uso (VSS) | 🟡 | nel motore sì, **non esposta** nella console |
| Storico, registro | ✅ | storico con analisi e CSV; il registro grezzo della CLI non si legge dalla console |
| Compressione (zip/7z) | ❌ | il motore non l'ha mai avuta |
| FTP / cloud | ❌ | l'invio al cloud è un segnaposto che rifiuta |
| Gruppi di attività, attività «fittizia», clonare, esegui come altro utente, backup mancati | ❌ | |

## 5. Prove fianco a fianco (da fare tu)

Per ogni riga: fai la stessa cosa nella Tauri e nella Slint e spunta se il risultato è **uguale o migliore**.
La Tauri attuale si avvia da `target\release\rustcopy-gui.exe` se la costruisci (`cargo build --release -p rustcopy-gui`, serve Node: `npm --prefix
crates/rustcopy-gui/ui ci` e poi `run build`); dimmi se vuoi che te la prepari io in una cartella a parte, così non tocca l'installazione.

| # | Prova | Tauri | Slint | Cosa guardare |
|---|---|---|---|---|
| P1 | Copiare una cartella vera (qualche GB) con verifica | ☐ | ☐ | avanzamento, esito finale, tempo |
| P2 | Fermare a metà e **riprendere** | ☐ | ☐ | riparte da dove si era, nessun file doppio |
| P3 | Aprire un tuo `.toml` **multi-job** vero | ☐ | ☐ | si vedono tutti i job con il loro stato (L7) |
| P4 | Dal file della P3, **modificare** un job (e aggiungerne uno) | ☐ | ☐ | la proposta si scrive accanto, l'originale intatto (L2) |
| P5 | Aprire il report di una copia **con errori** (una cartella senza permessi) | ☐ | ☐ | elenco dei problemi completo e esportabile (L4) |
| P6 | Storico di un'attività con più esecuzioni | ☐ | ☐ | stesse esecuzioni, stessi esiti, osservazioni del motore |
| P7 | Proprietà/Impostazioni: confronta 3 impostazioni a caso | ☐ | ☐ | stesso valore e stessa provenienza |
| P8 | Credenziali: salva e togli una credenziale | ☐ | ☐ | compare/sparisce in Gestione credenziali |
| P9 | Trascinare una cartella da Esplora file con tasto destro | ☐ | ☐ | si apre la console con la copia già in corso |
| P10 | Anteprima di ripristino da un report | ☐ | ☐ | stesso elenco, nulla copiato |
| P11 | Pianificare un'attività (Standard) e togliere la pianificazione | — | ☐ | compare in Utilità di pianificazione e sparisce |
| P12 | Sposta di qualche file con un file nuovo creato nel frattempo | — | ☐ | il file nuovo resta |

## 6. Come si chiude

1. Colmo le lacune che marchi «da colmare» (proposta: L2, L4, L7 sicuramente; L1, L3, L6, L8 se le vuoi).
2. Tu fai le prove P1–P12 e i controlli manuali.
3. Per ogni lacuna rimasta mi dici «accettata» o «da colmare».
4. Solo allora ti chiedo la conferma per la PR di rimozione di Tauri (separata, annullabile con un `revert`).
