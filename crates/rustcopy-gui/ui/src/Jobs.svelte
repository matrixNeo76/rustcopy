<script>
  import { invoke } from "@tauri-apps/api/core";
  import PathBar from "./PathBar.svelte";
  import EmptyState from "./EmptyState.svelte";
  import QuickSync from "./QuickSync.svelte";
  import NewJobWizard from "./NewJobWizard.svelte";
  import { session } from "./session.svelte.js";
  import {
    ShieldAlert,
    FileQuestionMark,
    ListChecks,
    CircleCheck,
    CircleX,
    CircleDashed,
    CalendarClock,
    Lock,
    RotateCcwClock,
    Funnel,
    Cpu,
    Settings as SettingsIcon,
    Clock,
    SquarePen,
  } from "@lucide/svelte";

  // The frontend does not decide: it asks the library and renders what comes back
  // (docs/archive/PIANO_GUI_TAURI.md §4.1). No judgement about mirroring, verification or outcomes here.
  let jobs = $state([]);
  let error = $state(null);
  let loading = $state(false);
  let loaded = $state(false);
  // F71: local to this pane, not a `session.activeTab` entry -- this stays a link discovered from
  // Job's own empty state, not a sixth sidebar tab. Job's own characterization in PIANO_GUI.md §3
  // ("Scrive? No") stays true: the write/start logic lives in QuickSync.svelte, this pane only
  // decides whether to show it (PIANO_GUI.md §17.3/§17.5).
  let showQuickSync = $state(false);
  // F83: same local-not-a-tab pattern as `showQuickSync` above, for the same reason -- this is a
  // link discovered from Job's own empty state, not a sixth sidebar tab, and Job's "Scrive? No"
  // characterization stays true: the write logic lives in NewJobWizard.svelte, this pane only
  // decides whether to show it.
  let showNewJobWizard = $state(false);
  // F79: the installer never ships `examples/` (PIANO_GUI.md §18.11) -- "prova
  // examples/demo-locale.toml" below only ever meant something for a repository checkout. This
  // generates the same kind of thing under the operator's own Documents folder instead, so the
  // empty state has a real answer regardless of how rustcopy got onto the machine.
  let exampleError = $state(null);
  let exampleBusy = $state(false);

  // F86: per-job last-run status, keyed by job.name -- one of "unavailable" (report_path is
  // still `null`, e.g. an unresolved `{timestamp}` placeholder), "never" (a resolvable path with
  // no run recorded yet), or the most recent `RunRecord` from `read_history`. Fetched once per
  // `load()`, not reactively per row: the console never asks the engine to do anything just by
  // rendering, same as every other read in this pane.
  let historyByJob = $state({});
  // File-level only (PIANO_GUI.md §19.2): a scheduled task always invokes `--config <file>`,
  // which runs every `[[jobs]]` entry via `run_jobs` -- there is no per-job schedule flag, so this
  // is a single count shown once above the table, never attributed to one row.
  let scheduleCount = $state(0);
  // For the Onda 2 thread icon: only worth showing when a job's own/inherited `threads` differs
  // from what an unset field would actually resolve to.
  let defaultThreads = $state(null);
  invoke("default_threads").then((value) => {
    defaultThreads = value;
  });

  async function createExample() {
    exampleError = null;
    exampleBusy = true;
    try {
      // Loading it straight away is the success signal: the empty state this button lives in
      // disappears the moment `jobs` is non-empty, replaced by the table showing what was just
      // generated -- no separate "fatto!" banner needed on top of that.
      session.configPath = await invoke("create_example_workspace");
      await load();
    } catch (e) {
      exampleError = String(e);
    } finally {
      exampleBusy = false;
    }
  }

  async function load() {
    error = null;
    loading = true;
    try {
      jobs = await invoke("list_jobs", { configPath: session.configPath });
      loaded = true;
      await loadOperationalStatus();
    } catch (e) {
      error = String(e);
      jobs = [];
    } finally {
      loading = false;
    }
  }

  // Kept separate from load() above: a failure here (an unreadable history index, a schedule
  // query that errors out) must never blank the table of jobs load() just populated -- the
  // configuration is the fact this pane exists to show; the status atop it is an enrichment.
  async function loadOperationalStatus() {
    scheduleCount = 0;
    try {
      const schedules = await invoke("schedules_referencing", { configPath: session.configPath });
      scheduleCount = schedules.length;
    } catch {
      scheduleCount = 0;
    }

    const entries = await Promise.all(
      jobs.map(async (job) => {
        if (job.report_path == null) return [job.name, "unavailable"];
        try {
          const history = await invoke("read_history", {
            reportPath: job.report_path,
            jobName: job.history_job_name,
            limit: 1,
          });
          return [job.name, history.runs[0] ?? "never"];
        } catch {
          return [job.name, "unavailable"];
        }
      }),
    );
    historyByJob = Object.fromEntries(entries);
  }
</script>

<section class="p-4">
  <PathBar
    bind:value={session.configPath}
    kind="config"
    label="Percorso del file di configurazione TOML"
    placeholder="Scegli un file di configurazione TOML"
    action="Elenca job"
    busy={loading}
    onrun={load}
  />

  {#if error}
    <p
      class="mt-3 rounded border border-red-300 bg-red-50 px-2 py-1 text-sm text-red-800
             dark:border-red-800 dark:bg-red-950 dark:text-red-200"
      role="alert"
    >
      {error}
    </p>
  {/if}

  {#if jobs.length > 0}
    {#if scheduleCount > 0}
      <!-- File-level, never per-row (PIANO_GUI.md §19.2): a scheduled task invokes `--config
           <file>`, which runs every `[[jobs]]` entry -- there is no per-job schedule flag, so
           attributing this badge to one row would claim a granularity the engine doesn't have.
           Same badge classes already used elsewhere for a "job"-origin value (Settings.svelte,
           History.svelte's SEVERITY_CLASS.Suggestion) -- no new palette. -->
      <p
        class="mt-3 inline-flex items-center gap-1.5 rounded bg-blue-100 px-2 py-1 text-xs
               text-blue-900 dark:bg-blue-950 dark:text-blue-200"
      >
        <CalendarClock size={13} strokeWidth={2.25} aria-hidden="true" />
        Questo file è referenziato da {scheduleCount}
        {scheduleCount === 1 ? "pianificazione" : "pianificazioni"} — vale per l'intero file, non
        per un singolo job.
      </p>
    {/if}
    <div class="card mt-4 overflow-x-auto">
      <table class="w-full table-fixed text-left text-xs">
        <!-- Explicit widths instead of leaving the browser's default table layout put all the
             extra space on whichever column has the widest content — on a wide window that put
             nearly the whole row into "Sorgente" while "Tipo"/"Verifica" stayed cramped, unrelated
             to what either column actually needs (Livello 1, punto 2, PIANO_GUI.md §10). -->
        <colgroup>
          <col class="w-[16%]" />
          <col class="w-[19%]" />
          <col class="w-[19%]" />
          <col class="w-[8%]" />
          <col class="w-[8%]" />
          <col class="w-[16%]" />
          <col class="w-[14%]" />
        </colgroup>
        <thead class="border-b border-slate-300 dark:border-slate-700">
          <tr>
            <th class="py-1 pr-3 font-medium">Job</th>
            <th class="py-1 pr-3 font-medium">Sorgente</th>
            <th class="py-1 pr-3 font-medium">Destinazione</th>
            <th class="py-1 pr-3 font-medium">Tipo</th>
            <th class="py-1 pr-3 font-medium">Verifica</th>
            <th class="py-1 pr-3 font-medium">Ultima esecuzione</th>
            <th class="py-1 pr-3 font-medium">Impostazioni / Azioni</th>
          </tr>
        </thead>
        <tbody>
          {#each jobs as job (job.name)}
            {@const last = historyByJob[job.name]}
            <tr class="border-b border-slate-200 last:border-0 dark:border-slate-800">
              <td class="py-1 pr-3 font-mono">
                {job.name}
                <!-- `--mirror` deletes at the destination, so it must be visible as such and
                     never rendered like an ordinary copy. -->
                {#if job.unconfigured}
                  <!-- A template read as if it were a configured job is how a first look at the
                       product ends in confusion: the row looks complete and points nowhere. -->
                  <span
                    class="ml-1 inline-flex items-center gap-1 rounded bg-slate-200 px-1 text-[10px]
                           font-semibold text-slate-700 dark:bg-slate-700 dark:text-slate-200"
                  >
                    <FileQuestionMark size={11} strokeWidth={2.25} aria-hidden="true" />
                    MODELLO — percorsi da compilare
                  </span>
                {/if}
                {#if job.mirror}
                  <span
                    class="ml-1 inline-flex items-center gap-1 rounded bg-amber-200 px-1 text-[10px]
                           font-semibold text-amber-900 dark:bg-amber-900 dark:text-amber-100"
                  >
                    <ShieldAlert size={11} strokeWidth={2.25} aria-hidden="true" />
                    MIRROR — cancella in destinazione
                  </span>
                {/if}
              </td>
              <td class="truncate py-1 pr-3 font-mono text-slate-600 dark:text-slate-400" title={job.source ?? ""}>{job.source ?? "—"}</td>
              <td class="truncate py-1 pr-3 font-mono text-slate-600 dark:text-slate-400" title={job.dest ?? ""}>{job.dest ?? "—"}</td>
              <td class="py-1 pr-3">{job.backup_type ?? "copia"}</td>
              <td class="py-1 pr-3">
                <!-- fast_verify travels beside verify_integrity, never instead of it: it skips
                     files whose source is unchanged, so it is a weaker guarantee and saying only
                     "yes" would overstate it. -->
                {#if job.verify_integrity}
                  {job.fast_verify ? "sì (fast)" : "sì"}
                {:else}
                  no
                {/if}
              </td>
              <td class="py-1 pr-3">
                <!-- Three honest states (PIANO_GUI.md §19.4 Onda 1): a report_path that never
                     resolves (still carries `{timestamp}`), one that resolves but has no run
                     recorded yet, and a real last outcome -- never a guess in place of the first
                     two. Colour follows the F81 convention (History.svelte): emerald only for
                     exit code 0, amber otherwise -- never red, since a nonzero code is not
                     necessarily a failure (e.g. 4 = copied, verification mismatch). -->
                {#if last === "unavailable"}
                  <span class="inline-flex items-center gap-1 text-slate-500">
                    <CircleDashed size={12} strokeWidth={2} aria-hidden="true" />
                    non disponibile
                  </span>
                {:else if last === "never" || last === undefined}
                  <span class="inline-flex items-center gap-1 text-slate-500">
                    <CircleDashed size={12} strokeWidth={2} aria-hidden="true" />
                    mai eseguito
                  </span>
                {:else}
                  <span
                    class="inline-flex items-center gap-1 rounded px-1 text-[10px] font-semibold
                           {last.exit_code === 0
                             ? 'bg-emerald-100 text-emerald-900 dark:bg-emerald-950 dark:text-emerald-200'
                             : 'bg-amber-100 text-amber-900 dark:bg-amber-950 dark:text-amber-200'}"
                  >
                    {#if last.exit_code === 0}
                      <CircleCheck size={11} strokeWidth={2.25} aria-hidden="true" />
                    {:else}
                      <CircleX size={11} strokeWidth={2.25} aria-hidden="true" />
                    {/if}
                    {last.exit_code}
                  </span>
                  <div class="mt-0.5 text-[10px] text-slate-500">
                    {new Date(last.timestamp).toLocaleString("it-IT")} ·
                    {last.throughput_mbps.toFixed(1)} MB/s
                  </div>
                {/if}
              </td>
              <td class="py-1 pr-3">
                <div class="flex flex-wrap items-center gap-2">
                  <!-- Onda 2: at-a-glance icons for the settings that change behaviour the most,
                       never a value beyond the two counts already exposed here -- the full
                       picture with provenance stays Impostazioni's job alone (§19.2). -->
                  {#if job.encrypt_enabled}
                    <span title="Cifratura attiva">
                      <Lock size={13} strokeWidth={2} class="text-slate-500" aria-hidden="true" />
                    </span>
                  {/if}
                  {#if job.keep_generations != null}
                    <span class="inline-flex items-center gap-0.5 text-slate-500" title="Generazioni conservate">
                      <RotateCcwClock size={13} strokeWidth={2} aria-hidden="true" />
                      <span class="text-[10px]">{job.keep_generations}</span>
                    </span>
                  {/if}
                  {#if job.exclude_count > 0}
                    <span class="inline-flex items-center gap-0.5 text-slate-500" title="Esclusioni configurate">
                      <Funnel size={13} strokeWidth={2} aria-hidden="true" />
                      <span class="text-[10px]">{job.exclude_count}</span>
                    </span>
                  {/if}
                  {#if job.threads != null && job.threads !== defaultThreads}
                    <span title="Thread non-default: {job.threads}">
                      <Cpu size={13} strokeWidth={2} class="text-slate-500" aria-hidden="true" />
                    </span>
                  {/if}
                </div>
                <div class="mt-1 flex items-center gap-2">
                  <!-- Onda 3: un clic verso il dettaglio di questo job in un'altra scheda, invece
                       di ricopiare a mano percorso (e nome job) (PIANO_GUI.md §9g/§19.1). -->
                  <button
                    class="text-slate-500 hover:text-slate-800 dark:hover:text-slate-200"
                    title="Apri le impostazioni di questo job"
                    onclick={() => {
                      // session.configPath is already this file's own path -- Job just loaded it
                      // from there -- so only the signal and the tab switch are needed here.
                      session.pendingSettingsLoad = true;
                      session.activeTab = "settings";
                    }}
                  ><SettingsIcon size={14} strokeWidth={2} aria-hidden="true" /></button>
                  <button
                    class="text-slate-500 hover:text-slate-800 disabled:opacity-30 dark:hover:text-slate-200"
                    title="Apri lo storico di questo job"
                    disabled={job.report_path == null}
                    onclick={() => {
                      session.reportPath = job.report_path;
                      // Mai `job.name`: il job singolo implicito ha sempre `name` risolto al
                      // fallback "job1", che punterebbe a un indice namespaced inesistente.
                      session.jobName = job.history_job_name ?? "";
                      session.pendingHistoryLoad = true;
                      session.activeTab = "history";
                    }}
                  ><Clock size={14} strokeWidth={2} aria-hidden="true" /></button>
                  <button
                    class="text-slate-500 hover:text-slate-800 dark:hover:text-slate-200"
                    title="Modifica questo job"
                    onclick={() => {
                      session.pendingEditorJob = job.name;
                      session.activeTab = "editor";
                    }}
                  ><SquarePen size={14} strokeWidth={2} aria-hidden="true" /></button>
                </div>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {:else if loaded && !error}
    <EmptyState
      title="Il file non descrive nessun job"
      lines={["Un file senza [[jobs]] descrive comunque un job singolo nei campi di primo livello: se anche quelli sono vuoti, non c'è niente da elencare."]}
    />
  {:else if !error}
    <EmptyState
      icon={ListChecks}
      title="Scegli un file di configurazione per cominciare"
      lines={[
        "Questa scheda elenca i job che un file TOML descrive: sorgente, destinazione, tipo di backup e se la verifica è attiva.",
        "Un job che cancella in destinazione (mirror) viene segnalato in modo distinto, perché è l'impostazione più distruttiva che possa avere.",
        "Hai clonato il repository? Prova examples/demo-locale.toml: copia qualche file finto in una cartella accanto, quindi non può toccare nulla di tuo.",
      ]}
    />

    <!-- F83: the primary action. Everything below this (F79's fake example, F71's QuickSync) was
         already here and already worked, but neither creates a named, reusable job for the
         operator's own real folders -- that gap was the actual complaint (a fresh Job tab looked
         like it assumed a TOML already existed). Collapsed out of view while the wizard is open
         so a first-time screen does not show three competing calls to action at once. -->
    <div class="mt-3">
      <button
        class="rounded bg-blue-600 px-3 py-1.5 text-sm text-white"
        onclick={() => (showNewJobWizard = !showNewJobWizard)}
      >{showNewJobWizard ? "Annulla" : "Crea la tua configurazione"}</button>
    </div>

    {#if showNewJobWizard}
      <NewJobWizard onCreated={() => { showNewJobWizard = false; load(); }} />
    {:else}
      <div class="mt-3 flex items-center gap-2">
        <button
          class="rounded border border-slate-300 px-2 py-1 text-xs disabled:opacity-40
                 dark:border-slate-700"
          onclick={createExample}
          disabled={exampleBusy}
        >{exampleBusy ? "Creazione…" : "Crea un esempio in Documenti"}</button>
        <span class="text-[11px] text-slate-500">
          Genera pochi file finti e un file di configurazione già pronto in
          <code>Documenti\rustcopy-demo</code>, poi lo apre qui.
        </span>
      </div>
      {#if exampleError}
        <p
          class="mt-2 rounded border border-red-300 bg-red-50 px-2 py-1 text-xs text-red-800
                 dark:border-red-800 dark:bg-red-950 dark:text-red-200"
          role="alert"
        >{exampleError}</p>
      {/if}

      {#if showQuickSync}
        <QuickSync />
        <button
          class="mt-2 text-xs text-slate-500 hover:text-slate-700 dark:hover:text-slate-300"
          onclick={() => (showQuickSync = false)}
        >← Torna</button>
      {:else}
        <button
          class="mt-3 text-xs text-blue-700 dark:text-blue-300"
          onclick={() => (showQuickSync = true)}
        >Oppure sincronizza due cartelle adesso, senza scrivere prima un file →</button>
      {/if}
    {/if}
  {/if}
</section>
