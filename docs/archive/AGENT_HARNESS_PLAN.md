---
type: Reference
title: Piano di Implementazione — AgentHarnesses (agentharnesses.io)
description: Proposta di framework di test YAML/harness per rustcopy, creata il 10 Agosto 2026, mai messa in esecuzione. Archiviata, non attiva.
status: deprecated
generated:
  by: process:claude-code
  at: 2026-08-20T00:00:00Z
---

# Piano di Implementazione: integrazione AgentHarnesses (agentharnesses.io)

> **Archiviato il 20 Agosto 2026**: file orfano trovato in root senza frontmatter OKF e senza alcun riferimento da nessun documento attuale del progetto. Creato il 10 Agosto 2026, la proposta non è mai stata messa in esecuzione (termina con una domanda all'utente mai risposta). **Distinto da `ROADMAP.md` F61** ("Server MCP feature-gated") — quel piano riguarda un server MCP per host agentici, questo riguarda un framework di test a scenari YAML; non sono la stessa proposta. Spostato qui senza modifiche al contenuto, per disciplina e coerenza con `docs/archive/PIANO_NOTIFY_SERVER.md`. Se in futuro serve testing end-to-end più strutturato di quello attuale (`tests/cli_smoke.rs`, `tests/notify_server_e2e.rs`), questo documento resta un punto di partenza valido da rivalutare, non da eseguire alla lettera.

Creato da: GitHub Copilot Chat Assistant

Obiettivo
- Integrare un set di harness automatizzati (AgentHarnesses) per migliorare testing end-to-end, riproducibilità, benchmarking e validazione dei flussi critici di rustcopy.
- Fornire un workflow documentato e ripetibile che permetta di aggiungere nuovi scenari, eseguirli in CI e mantenere una suite di regressione leggera e, opzionalmente, una di carico notturna.

Perché (benefici)
- Copertura end-to-end: testare l'interazione reale tra prescan, invocazione robocopy, retry, verifica dei checksum e notify-server.
- Regressioni mitigate: snapshot dei report JSON/HTML per prevenire regressioni di formato e semantica.
- Facilita contributi di agenti/automation: gli agenti che suggeriscono fix/PR possono essere verificati in scenari ripetibili.
- Benchmarking controllato: scenari sintetici per misurare performance e memoria senza dati sensibili.

Principi di progetto
- Isolare side-effect reali: usare mocks e loopback per reti; non caricare dati reali.
- Gradualità: iniziare con 2–3 harness critici (parse logs, notify-server, verify integrity) e poi estendere.
- Documentazione e template: chiunque deve poter aggiungere uno scenario seguendo un template.
- CI-friendly: harness leggeri su ogni PR, harness pesanti in workflow notturno o manuale.

Struttura dei file proposta
- .agent_harnesses/ (directory principale dei harness)
  - README.md (come scrivere harness)
  - parse-robocopy.yaml
  - notify-server-integration.yaml
  - verify-integrity.yaml
  - fixtures/ (file di esempio e log sintetici)
- tests/harness_runner.rs (helper Rust per eseguire harness e validare assertions)
- tests/fixtures/* (log, JSON, small file-metadata)
- .github/workflows/agent-harness.yml (CI: esegue harness leggeri)
- .github/workflows/agent-harness-nightly.yml (CI: harness di carico/benchmark)
- AGENT_HARNESS_PLAN.md (questo file)

Formato di uno scenario (YAML - schema suggerito)
- id: short-id
- title: Descrizione breve
- description: Test case e obiettivo
- steps: array di step eseguibili (comandi o nome di helper_functions)
- inputs:
  - fixtures: elenco di file fixture richiesti
  - env: variabili d'ambiente da impostare
- mocks: descrizione di server/mocking richiesto (es. mock_notify_server: true)
- assertions:
  - type: json-schema | contains | exit-code | snapshot
  - target: path/file o field JSON
  - expected: valore o riferimento a snapshot
- tags: [fast, nightly, ci, heavy]

Esempio minimale (parse-robocopy.yaml)
```yaml
id: parse-robocopy
title: Parse Robocopy Logs & map exit codes
description: Verifica che i log vengano parsati e che i retry siano pianificati quando necessario.
inputs:
  fixtures:
    - tests/fixtures/robocopy_example_8.log
    - tests/fixtures/robocopy_example_0.log
steps:
  - run: cargo test --test harness_runner -- parse-robocopy --fixture tests/fixtures/robocopy_example_8.log
assertions:
  - type: contains
    target: output:report.json
    expected: '"exit_code": 8'
  - type: contains
    target: output:report.json
    expected: '"retries_attempted": true'
tags: [ci, fast]
```

Runner e libreria di supporto
- tests/harness_runner.rs:
  - Carica lo YAML dello scenario.
  - Prepara l'ambiente (copie fixture in tmp dir, set env vars).
  - Esegue comandi (spawn processi con timeout controllato).
  - Colleziona output (stdout, stderr, report JSON/HTML prodotti).
  - Esegue assertions: comparazione semplice, JSON schema validation, snapshot comparison.
- Dipendenze consigliate:
  - serde_yaml, serde_json (parsing)
  - insta o similar per snapshot testing (facoltativo)
  - tempfile per workspace isolati

CI: workflow consigliato
- .github/workflows/agent-harness.yml (on: [pull_request, push])
  - Step: checkout, setup Rust, cargo build --tests, run harness tag:ci (harness runner filtra gli scenario per tag)
  - Timeout: breve (5–15 min) per non bloccare PR
- .github/workflows/agent-harness-nightly.yml (schedule: nightly)
  - Esegue harness tag:nightly o tag:heavy (benchmarking, memory profiling)
  - Artifacts: upload dei report JSON, HTML e dei log di profiling

Feature flag e conditional builds
- Feature: harness-heavy per test che richiedono axum/notify-server o altre dipendenze pesanti.
  - Esempio: cargo test --features harness-heavy -- --ignored
- Per harness leggero usare test standard o un binario helper con feature minimal.

Check di sicurezza e privacy
- Non includere dati reali né token nei fixtures; usare esempi sintetici.
- Per webhook usare loopback (127.0.0.1) o mock server. Salvare nel CI solo report senza segreti.

Metriche e benchmarking
- Per scenari di performance, raccogliere: durata totale, peak RSS, throughput (MB/s), CPU time.
- Salvare i risultati nightly in /bench-results/YYYY-MM-DD/*.json e aggiungere grafici nel README dei harness.

Manutenzione e contributi
- AGENTS.md: aggiungere sezione "Come scrivere un harness" con template YAML e checklist.
- Template PR per harness: descrizione, fixtures, tags, cost estimate del tempo CI.
- Owners: assegnare uno o due maintainers responsabili dell'aggiornamento dei harness.

Piano temporale suggerito (roadmap a 4 settimane)
- Settimana 1 (Kickoff)
  - Creare struttura directory (.agent_harnesses, tests/fixtures)
  - Implementare harness_runner skeleton e parse-robocopy scenario
  - Aggiungere small CI workflow per tag:ci
- Settimana 2
  - Implementare notify-server integration scenario
  - Aggiungere fixtures e mock notify-server helper
  - Documentare README harness + template
- Settimana 3
  - Implementare verify-integrity scenario e snapshot tests per report JSON
  - Aggiungere nightly workflow e artifacts
- Settimana 4
  - Rifinire harness pesanti, aggiungere benchmark e grafici, onboarding documentazione per contributori

Stima costi di lavoro (indicativa)
- Setup iniziale + 1 scenario semplice: 4–8 ore
- 2 scenario aggiuntivi + CI: 8–16 ore
- Harness di carico/benchmark + dashboard: 8–16 ore

Criteri di accettazione (definition of done)
- Esistono almeno 3 scenario YAML con fixtures e assertions
- Il runner esegue gli scenario tag:ci e ritorna exit code 0 su successo
- Documentazione per aggiungere nuovi harness presente in .agent_harnesses/README.md
- CI esegue harness leggeri su PR e pubblica artifacts per nightly

Rischi principali
- CI runtime troppo lungo: mitigare limitando harness eseguiti per PR e spostando i test pesanti su schedule
- Mantenimento: richiedere governance (owners) e template chiari
- Falsi positivi dipendenti da snapshot non aggiornati: usare snapshot review (PR) con convenzioni

Azioni immediate che posso eseguire ora
- Creare i file base (directory .agent_harnesses/, README, uno YAML d'esempio) e il runner skeleton in tests/ per aprire una PR.

Vuoi che proceda a creare subito i file iniziali (AGENT_HARNESS_PLAN.md è già presente in questo commit) come PR oppure preferisci prima modificare il contenuto del piano?
