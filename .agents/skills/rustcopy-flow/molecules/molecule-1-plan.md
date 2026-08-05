---
name: molecule-1-plan
version: 1.0.0
category: molecule
parent: rustcopy-flow
tags: [rustcopy, planning, cli-args, toml-config]
description: "Raccoglie l'intento utente e costruisce un comando rustcopy (o TOML) valido, applicando i pitfall noti del progetto. Usata dagli Scenari 1 e 2."
steps: 5
max_steps: 8
---

# Molecola 1: Plan — Costruzione del Comando/Config

## Input
- Path del binario (Molecola 0)
- Eventuali TOML riutilizzabili trovati (Molecola 0)
- Richiesta utente (source, dest, cosa includere/escludere, tipo di backup)

## Steps

1. **Raccogli i parametri obbligatori**
   - `source`, `dest` (se manca uno dei due e non c'è un TOML/`--config` che li fornisce, chiedi
     esplicitamente — non indovinare path)
   - Se `dest` è un percorso di rete (`\\host\share\...`), verifica che sia raggiungibile prima
     di costruire il comando (`Test-Path` su PowerShell, o equivalente)
   - Output: `source`, `dest` confermati

2. **Determina pattern ed esclusioni**
   - `--pattern` (default `*`, va bene per la maggior parte dei casi)
   - `--exclude-dirs`/`--exclude-files` (ripetibili) — se l'utente vuole escludere cartelle per
     nome (es. cache, cartelle cloud-only), chiedi se ci sono junction/symlink nella root che
     potrebbero duplicare quei dati sotto un altro nome: se sì, **aggiungi sempre
     `--exclude-junctions`** insieme a `--exclude-dirs`
   - Se la destinazione è un profilo cloud (OneDrive, ecc.) o contiene cartelle "Files on-demand":
     valuta di escluderle esplicitamente, altrimenti il prescan/la copia forzano il download di
     placeholder cloud
   - Output metric: nessuna esclusione ambigua, junction gestiti

3. **Determina opzioni di trasferimento**
   - `--threads` (default = CPU logiche; per SMB/NAS su link lenti, valori più bassi possono
     essere più stabili — se l'utente ha già dati di benchmark in `_ops_reports/`, usali invece
     di indovinare)
   - `--verify-integrity` (+ `--hash-algo`: `sha256` default sicuro, `blake3` più veloce ma
     comunque crittografico, `xxh3` solo se la minaccia è corruzione accidentale non manomissione)
   - `--fast-verify` solo se l'utente farà run ripetute sullo stesso albero (non sulla prima run)
   - `--mirror` SOLO se l'utente vuole esplicitamente che la destinazione rispecchi la sorgente
     (file solo-in-dest verranno CANCELLATI) — se sì, NON aggiungere `--no-prescan` insieme a
     `--mirror` (senza prescan la safety-check non ha un inventario di riferimento)
   - `--bandwidth-limit-mbps` se l'utente vuole limitare la banda (es. durante orario lavorativo)
   - Output: lista di flag opzionali attivi

4. **Determina se serve un TOML invece di flag CLI puri**
   - Se il comando ha più di ~5 flag, o la coppia source/dest verrà riusata in futuro (job
     ricorrente), preferisci scrivere/aggiornare un file
     `examples/<nome-caso-d-uso>.local.toml` (già in `.gitignore` per pattern `*.local.toml`)
     invece di un comando CLI lunghissimo da ridigitare
   - **Promemoria campi CLI-only, NON supportati nel TOML** (verificato contro `src/config.rs`):
     `--decrypt`, `--restore-from`, `--vss-snapshot`, `--resume-from`, `--force-purge`,
     `--exclude-junctions`, `--fast-verify`, `--html-report-path`, `--install-schedule`,
     `--install-service` — questi vanno sempre passati come flag CLI insieme a `--config`, mai
     scritti nel TOML
   - Output: `examples/<nome>.local.toml` (se scelto) oppure comando CLI completo come stringa

5. **Presenta il piano per checkpoint**
   - Mostra il comando/TOML completo, spiega ogni flag non ovvio in una riga
   - Chiedi conferma esplicita prima di passare alla Molecola 2 (dry-run)

## Output Finale
- Comando CLI completo (stringa pronta per l'esecuzione) oppure path del TOML + eventuali flag
  CLI complementari
- Riepilogo leggibile del piano, mostrato all'utente

## Failure Modes
- **Destinazione di rete non raggiungibile**: segnala prima di costruire il resto del piano,
  chiedi credenziali/mapping SMB se serve
- **Source e dest identici o annidati**: blocca e chiedi conferma esplicita (rischio di loop o
  sovrascrittura)
- **Utente chiede `--mirror` senza capirne le conseguenze**: spiega esplicitamente "i file
  presenti SOLO in destinazione verranno cancellati" prima di proseguire
- **TOML esistente non compatibile con la nuova richiesta**: non riusarlo silenziosamente,
  mostra il diff concettuale (cosa cambierebbe) e chiedi se aggiornarlo o crearne uno nuovo
