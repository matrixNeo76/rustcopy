<script>
  import { invoke } from "@tauri-apps/api/core";
  import PathBar from "./PathBar.svelte";
  import EmptyState from "./EmptyState.svelte";
  import { session } from "./session.svelte.js";
  import { toCsv, downloadCsv } from "./csv.js";
  import { Clock, CircleCheck, CircleX } from "@lucide/svelte";

  // Read-only, like the rest of this version: this pane opens files the CLI already wrote and
  // renders them. It never asks the engine to do anything.
  // A `[[jobs]]` entry keeps its own namespaced index (`.rustcopy_history.<job>.jsonl`, D12), so a
  // hardcoded `null` here would silently show an empty history for every named job. Left blank for
  // single-job configs, where the index has no job suffix.
  let history = $state(null);
  let advice = $state([]);
  let error = $state(null);
  let loading = $state(false);
  // Client-side only, and honest about it: `history.runs` is already the whole loaded batch (the
  // fixed `limit: 100` above, not a further server-side page), so filtering it here drops nothing
  // the operator hasn't already been told about via the "le più recenti" label.
  let outcomeFilter = $state("all");

  const SEVERITY_ORDER = { ATTENZIONE: 0, PROPOSTA: 1, INFO: 2 };

  // The severities come from the library as an enum; the labels shown here are the same ones
  // `--advise` prints, so the console and the CLI cannot disagree about how loud a finding is.
  const SEVERITY_LABEL = { Warning: "ATTENZIONE", Suggestion: "PROPOSTA", Info: "INFO" };
  const SEVERITY_CLASS = {
    Warning: "bg-red-100 text-red-900 dark:bg-red-950 dark:text-red-200",
    Suggestion: "bg-blue-100 text-blue-900 dark:bg-blue-950 dark:text-blue-200",
    Info: "bg-slate-100 text-slate-700 dark:bg-slate-800 dark:text-slate-300",
  };

  async function load() {
    error = null;
    loading = true;
    // A filter left over from a different report/job would silently hide runs in the new one.
    outcomeFilter = "all";
    try {
      // Two calls rather than one combined view: the history is what happened, the advice is a
      // reading of it. Keeping them apart means a parse problem in one does not blank the other.
      // Empty means "the un-suffixed index", which is what a single-job run writes.
      const job = session.jobName.trim() === "" ? null : session.jobName.trim();
      history = await invoke("read_history", { reportPath: session.reportPath, jobName: job, limit: 100 });
      advice = await invoke("read_advice", { reportPath: session.reportPath, jobName: job });
    } catch (e) {
      error = String(e);
      history = null;
      advice = [];
    } finally {
      loading = false;
    }
  }

  function duration(seconds) {
    if (seconds < 10) return `${seconds.toFixed(2)}s`;
    const total = Math.round(seconds);
    const h = Math.floor(total / 3600);
    const m = Math.floor((total % 3600) / 60);
    return h > 0 ? `${h}h ${String(m).padStart(2, "0")}m` : `${m}m ${String(total % 60).padStart(2, "0")}s`;
  }

  // Exit codes are a contract with schedulers (AGENTS.md rule 12), so the console shows what each
  // one means rather than colouring "non-zero" red. A 4 is not a failed copy.
  const EXIT_MEANING = {
    0: "riuscito",
    1: "trasferimento fallito",
    2: "errore d'uso",
    3: "purge mirror annullata",
    4: "copiato, verifica fallita",
    5: "purge retention annullata",
    6: "spazio libero insufficiente",
  };

  const filteredRuns = $derived(
    history
      ? history.runs.filter((run) =>
          outcomeFilter === "all" ? true : outcomeFilter === "success" ? run.exit_code === 0 : run.exit_code !== 0,
        )
      : [],
  );

  // Exports exactly what the table below shows: the applied filter, in the same displayed order.
  // A CSV that quietly included rows the operator had just filtered out would misrepresent what
  // they chose to export.
  function exportCsv() {
    const headers = ["Quando", "Codice uscita", "Esito", "File copiati", "File totali", "Durata (s)", "Throughput (MB/s)"];
    const rows = [...filteredRuns].reverse().map((run) => [
      new Date(run.timestamp).toISOString(),
      run.exit_code,
      EXIT_MEANING[run.exit_code] ?? "sconosciuto",
      run.files_copied,
      run.total_files,
      run.elapsed_seconds.toFixed(2),
      run.throughput_mbps.toFixed(2),
    ]);
    downloadCsv(`rustcopy-storico-${Date.now()}.csv`, toCsv(headers, rows));
  }
</script>

<section class="p-4">
  <PathBar
    bind:value={session.reportPath}
    kind="report"
    label="Percorso del report JSON"
    placeholder="Scegli un report JSON (lo storico sta lì accanto)"
    action="Apri storico"
    busy={loading}
    onrun={load}
  />

  <label class="mt-2 flex items-center gap-2 text-xs text-slate-600 dark:text-slate-400">
    Job, se il file di configurazione usa [[jobs]]
    <input
      class="w-48 rounded border border-slate-300 px-2 py-1 dark:border-slate-700 dark:bg-slate-900"
      placeholder="vuoto = job singolo"
      bind:value={session.jobName}
    />
  </label>

  {#if error}
    <p
      class="mt-3 rounded border border-red-300 bg-red-50 px-2 py-1 text-sm text-red-800
             dark:border-red-800 dark:bg-red-950 dark:text-red-200"
      role="alert"
    >
      {error}
    </p>
  {/if}

  {#if history}
    {#if history.skipped_lines > 0}
      <!-- Never hidden: a partially readable index means the advice below rests on less data than
           the operator would assume. -->
      <p class="mt-3 rounded border border-amber-300 bg-amber-50 px-2 py-1 text-xs text-amber-900
                dark:border-amber-800 dark:bg-amber-950 dark:text-amber-200">
        {history.skipped_lines} righe dell'indice non sono leggibili e sono state ignorate: i dati qui
        sotto si basano su un campione incompleto. I backup già eseguiti non sono compromessi.
      </p>
    {/if}

    {#if advice.length > 0}
      <h2 class="mt-4 text-xs font-semibold uppercase tracking-wide text-slate-500">Analisi</h2>
      <ul class="mt-1 space-y-1">
        {#each [...advice].sort((a, b) => SEVERITY_ORDER[SEVERITY_LABEL[a.severity]] - SEVERITY_ORDER[SEVERITY_LABEL[b.severity]]) as item}
          <li class="rounded border border-slate-200 p-2 dark:border-slate-800">
            <span class="rounded px-1 text-[10px] font-semibold {SEVERITY_CLASS[item.severity]}">
              {SEVERITY_LABEL[item.severity]}
            </span>
            <span class="ml-1 text-sm">{item.headline}</span>
            {#if item.evidence.length > 0}
              <!-- The numbers travel with the claim, exactly as `--advise` prints them: advice
                   without its evidence is not reviewable. -->
              <ul class="mt-1 ml-3 list-disc text-xs text-slate-600 dark:text-slate-400">
                {#each item.evidence as line}<li>{line}</li>{/each}
              </ul>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}

    <div class="mt-4 flex flex-wrap items-center justify-between gap-2">
      <h2 class="text-xs font-semibold uppercase tracking-wide text-slate-500">
        Run ({filteredRuns.length}{filteredRuns.length !== history.runs.length ? ` di ${history.runs.length}` : ""}{history.runs.length === history.limit_applied ? ", le più recenti" : ""})
      </h2>
      {#if history.runs.length > 0}
        <div class="flex items-center gap-2">
          <label class="flex items-center gap-1 text-xs text-slate-600 dark:text-slate-400">
            Esito
            <select
              class="rounded border border-slate-300 px-1 py-0.5 dark:border-slate-700 dark:bg-slate-900"
              bind:value={outcomeFilter}
            >
              <option value="all">Tutti</option>
              <option value="success">Solo riuscite (0)</option>
              <option value="failed">Solo non riuscite</option>
            </select>
          </label>
          <button
            class="rounded border border-slate-300 px-2 py-0.5 text-xs disabled:opacity-40 dark:border-slate-700"
            onclick={exportCsv}
            disabled={filteredRuns.length === 0}
          >Esporta CSV</button>
        </div>
      {/if}
    </div>
    {#if history.runs.length === 0}
      <p class="mt-1 text-sm text-slate-500">
        Nessuna run registrata
        {#if session.jobName.trim() !== ""}
          per il job «{session.jobName.trim()}»: l'indice di un job nominato è
          <code>.rustcopy_history.{session.jobName.trim()}.jsonl</code>, accanto al report.
        {:else}
          per questo percorso. Se il file di configurazione usa <code>[[jobs]]</code>, indica il nome del job:
          ogni job ha un indice separato.
        {/if}
      </p>
    {:else if filteredRuns.length === 0}
      <p class="mt-1 text-sm text-slate-500">Nessuna run corrisponde al filtro scelto.</p>
    {:else}
      <div class="card overflow-x-auto p-0">
        <table class="w-full table-fixed text-left text-xs">
          <!-- Explicit widths (Livello 1, punto 2, PIANO_GUI.md §10): the default table layout put
               nearly half the row into "Quando" while "Durata"/"Throughput" stayed cramped, with
               no relation to what either actually needs. -->
          <colgroup>
            <col class="w-[20%]" />
            <col class="w-[38%]" />
            <col class="w-[14%]" />
            <col class="w-[14%]" />
            <col class="w-[14%]" />
          </colgroup>
          <thead class="border-b border-slate-300 dark:border-slate-700">
            <tr>
              <th class="py-1.5 pr-3 pl-3 font-medium">Quando</th>
              <th class="py-1.5 pr-3 font-medium">Esito</th>
              <th class="py-1.5 pr-3 font-medium">File</th>
              <th class="py-1.5 pr-3 font-medium">Durata</th>
              <th class="py-1.5 pr-3 font-medium">Throughput</th>
            </tr>
          </thead>
          <tbody>
            {#each [...filteredRuns].reverse() as run}
              <tr class="border-b border-slate-200 last:border-0 dark:border-slate-800">
                <td class="py-1 pr-3 pl-3 font-mono">{new Date(run.timestamp).toLocaleString("it-IT")}</td>
                <td class="py-1 pr-3">
                  <span
                    class="inline-flex items-center gap-1 rounded px-1 text-[10px] font-semibold
                           {run.exit_code === 0
                             ? 'bg-emerald-100 text-emerald-900 dark:bg-emerald-950 dark:text-emerald-200'
                             : 'bg-red-100 text-red-900 dark:bg-red-950 dark:text-red-200'}"
                  >
                    {#if run.exit_code === 0}
                      <CircleCheck size={11} strokeWidth={2.25} aria-hidden="true" />
                    {:else}
                      <CircleX size={11} strokeWidth={2.25} aria-hidden="true" />
                    {/if}
                    {run.exit_code} — {EXIT_MEANING[run.exit_code] ?? "sconosciuto"}
                  </span>
                  {#if run.dry_run}
                    <span class="ml-1 text-[10px] text-slate-500">dry-run</span>
                  {/if}
                </td>
                <td class="py-1 pr-3">{run.files_copied} / {run.total_files}</td>
                <td class="py-1 pr-3">{duration(run.elapsed_seconds)}</td>
                <td class="py-1 pr-3">{run.throughput_mbps.toFixed(1)} MB/s</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  {:else if !error}
    <EmptyState
      icon={Clock}
      title="Scegli un report per vedere lo storico delle run"
      lines={[
        "L'indice delle run vive accanto al report, non nella destinazione del backup: scriverci dentro cambierebbe la data della destinazione e la run successiva ricopierebbe file immutati.",
        "Oltre alle run passate, questa scheda mostra l'analisi deterministica che la CLI stampa con --advise: nessun modello e nessuna rete, solo statistica sulle run precedenti con le sue evidenze numeriche.",
      ]}
    />
  {/if}
</section>
