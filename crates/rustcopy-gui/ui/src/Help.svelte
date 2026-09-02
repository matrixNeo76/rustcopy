<script>
  // What the console shows is meaningless without these. The knowledge existed only in
  // docs/cli-reference.md and never reached the window, so an operator faced four blank panes and
  // no way to tell what they wanted.
  const SECTIONS = [
    {
      title: "Da dove si comincia",
      items: [
        [
          "Non hai ancora niente da aprire",
          "Nel repository c'è examples/demo-locale.toml: copia cinque file finti in una cartella accanto e scrive lì report e log, quindi non può toccare dati veri. Lanciala dalla CLI (`robocopy_ingest.exe --config examples/demo-locale.toml`), poi apri il report che ha prodotto nella scheda Report. Gli altri esempi di quella cartella sono modelli da adattare: la scheda Job li segnala come tali.",
        ],
        [
          "Un file di configurazione TOML",
          "È il file che descrive uno o più job di backup. Le schede Job, Impostazioni e Modifica partono da lì. Se non ne hai uno, la CLI funziona anche con soli argomenti da riga di comando: la console serve a leggere ciò che un file già descrive.",
        ],
        [
          "Un report JSON",
          "Ogni run conclusa ne scrive uno (per impostazione predefinita `ingest-report.json`). La scheda Storico parte da lì, perché l'indice delle run vive accanto al report, non nella destinazione del backup.",
        ],
      ],
    },
    {
      title: "Cosa significano le schede",
      items: [
        ["Report", "Il dettaglio di una singola run: esito, volumi, durata, e i file che la verifica ha segnalato, a blocchi di cento."],
        ["Job", "I job che il file descrive, con sorgente, destinazione e tipo. Un job che cancella in destinazione è segnalato in modo distinto."],
        ["Impostazioni", "Ogni impostazione risolta, raggruppata, con da quale strato viene il valore che vince e quali scelte portano una conseguenza."],
        ["Modifica", "L'unico punto che scrive. Produce una proposta in un file nuovo: la configurazione in uso non viene mai toccata."],
        ["Storico", "Le run passate con il significato del loro esito, più l'analisi deterministica che la CLI stampa con --advise."],
      ],
    },
    {
      title: "Termini che la console usa",
      items: [
        [
          "mirror",
          "Rende la destinazione identica alla sorgente, quindi cancella lì ciò che nella sorgente non c'è più. È l'impostazione più distruttiva che un job possa avere, e la console non può accenderla: va scritta a mano nel file.",
        ],
        [
          "generazione, ciclo",
          "Con --backup-type il backup diventa una storia: un Full più gli Incremental o Differential che lo seguono formano un ciclo. La retention elimina cicli interi e non singole generazioni, per non lasciare un incrementale senza il full da cui dipende.",
        ],
        [
          "verifica rapida (fast-verify)",
          "Salta i file la cui sorgente è immutata dall'ultima verifica riuscita. Si fida dell'identità della sorgente invece di rileggere i byte in destinazione: una corruzione nata in destinazione può sfuggire.",
        ],
        [
          "ereditato",
          "In un file con più job, un valore non scritto nel job viene dai valori di primo livello. La scheda Impostazioni dice per ogni voce se l'ha chiesta il job, se l'ha ereditata, o se nessuno l'ha impostata.",
        ],
      ],
    },
    {
      title: "Gli esiti di una run",
      items: [
        ["0", "Riuscito."],
        ["1", "Il trasferimento è fallito: qualcosa non è stato copiato."],
        ["2", "Errore d'uso o di configurazione."],
        ["3", "La cancellazione di --mirror è stata annullata."],
        ["4", "I dati sono stati copiati, ma la verifica ha trovato una differenza. Non è una copia fallita, ed è la distinzione per cui questo codice esiste."],
        ["5", "La cancellazione della retention è stata annullata."],
      ],
    },
  ];
</script>

<section class="p-4">
  <p class="max-w-3xl text-sm text-slate-600 dark:text-slate-400">
    Questa console <strong>legge</strong> ciò che rustcopy ha già scritto e prepara proposte di
    configurazione. Non esegue backup, non copia e non cancella nulla.
  </p>

  {#each SECTIONS as section}
    <h2 class="mt-5 text-xs font-semibold uppercase tracking-wide text-slate-500">
      {section.title}
    </h2>
    <dl class="mt-1 max-w-3xl">
      {#each section.items as [term, meaning]}
        <div class="border-b border-slate-200 py-1.5 dark:border-slate-800">
          <dt class="font-mono text-xs font-semibold">{term}</dt>
          <dd class="text-xs text-slate-600 dark:text-slate-400">{meaning}</dd>
        </div>
      {/each}
    </dl>
  {/each}
</section>
