<script>
  import { invoke } from "@tauri-apps/api/core";
  import PathBar from "./PathBar.svelte";
  import EmptyState from "./EmptyState.svelte";
  import { session } from "./session.svelte.js";
  import { toCsv, downloadCsv } from "./csv.js";
  import { FileText, CircleCheck, CircleX } from "@lucide/svelte";

  // `read_report`/`read_report_page` existed in the core and on the IPC surface from F53 and no
  // pane ever called them: a complete report viewer with nothing attached to it. This is the pane.
  let report = $state(null);
  let error = $state(null);
  let loading = $state(false);
  let offset = $state(0);
  // Filters only the entries already on screen. Each list is server-paginated at PAGE (100) out of
  // up to MAX_REPORTED_ERRORS (10 000) — a filter that quietly searched only the loaded page while
  // *reading* as a search of the whole list would be exactly the kind of unlabelled truncation this
  // report otherwise goes out of its way to declare (see `truncated_at_source` below). The label on
  // the input says "in questa pagina" for that reason, not as decoration.
  let query = $state("");

  const PAGE = 100;

  // `report.integrity_status` is `format!("{:?}", IntegrityStatus)` from the core — a small,
  // stable, closed enum (`integrity.rs`: exactly Passed/Failed), so translating the label here is
  // a display choice, not a judgement the frontend is making about backup semantics. Unlike
  // `exit_code_meaning` below: that one is robocopy's own bitmask description assembled from up to
  // five composable English phrases ("files copied; extra files or directories detected; …",
  // `exit_code.rs::RobocopyStatus::describe`), not a fixed small vocabulary — a lookup table keyed
  // on the wrong shape of string here was caught before shipping (Livello 1, punto 4,
  // PIANO_GUI.md §10) and deliberately left in English rather than mistranslated.
  const INTEGRITY_LABEL = { Passed: "superata", Failed: "fallita" };

  async function load(from = 0) {
    error = null;
    loading = true;
    try {
      report = await invoke("read_report_page", {
        path: session.reportPath,
        offset: from,
        limit: PAGE,
      });
      offset = from;
      // A filter left over from a previous report, or from a page the operator just left, would
      // silently hide entries in the one just loaded.
      query = "";
    } catch (e) {
      error = String(e);
      report = null;
    } finally {
      loading = false;
    }
  }

  // "Apri il report di questa run" (Esegui, Livello 1 punto 5, PIANO_GUI.md §10) sets
  // `session.reportPath` and this flag together, then switches here. A one-shot signal consumed
  // immediately rather than a live binding on `reportPath` itself — this pane stays mounted even
  // while hidden (App.svelte), so watching the path directly would reload on every keystroke of
  // someone typing a path by hand in this very pane, not just on an actual cross-pane jump.
  $effect(() => {
    if (session.pendingReportLoad) {
      session.pendingReportLoad = false;
      load(0);
    }
  });

  function bytes(value) {
    if (value < 1024) return `${value} B`;
    const units = ["KB", "MB", "GB", "TB"];
    let n = value / 1024;
    let i = 0;
    while (n >= 1024 && i < units.length - 1) {
      n /= 1024;
      i += 1;
    }
    return `${n.toFixed(n < 10 ? 1 : 0)} ${units[i]}`;
  }

  function duration(seconds) {
    if (seconds < 10) return `${seconds.toFixed(2)}s`;
    const total = Math.round(seconds);
    const h = Math.floor(total / 3600);
    const m = Math.floor((total % 3600) / 60);
    return h > 0
      ? `${h}h ${String(m).padStart(2, "0")}m`
      : `${m}m ${String(total % 60).padStart(2, "0")}s`;
  }

  // The three per-file lists, rendered by one block rather than three near-identical ones.
  const LISTS = [
    ["mismatches", "Byte diversi fra sorgente e destinazione"],
    ["missing_in_dest", "Presenti nella sorgente, assenti in destinazione"],
    ["unreadable", "Non leggibili durante la verifica"],
  ];

  const anyErrors = $derived(
    report ? LISTS.some(([key]) => report[key].total > 0) : false,
  );

  // Shared by the template and the export below, so the two can never disagree about what "the
  // filter" currently matches.
  function filteredEntries(key) {
    const page = report[key];
    return query.trim() === ""
      ? page.entries
      : page.entries.filter((path) => path.toLowerCase().includes(query.trim().toLowerCase()));
  }

  const anyFilteredEntries = $derived(
    report ? LISTS.some(([key]) => filteredEntries(key).length > 0) : false,
  );

  // Exports exactly what is on screen: the current page, the current filter. Not the whole list —
  // that would need every page fetched first, and would misrepresent "in questa pagina" as
  // covering more than it does.
  function exportCsv() {
    const rows = [];
    for (const [key, title] of LISTS) {
      for (const path of filteredEntries(key)) rows.push([title, path]);
    }
    downloadCsv(`rustcopy-report-problemi-${Date.now()}.csv`, toCsv(["Categoria", "Percorso"], rows));
  }
</script>

<section class="p-4">
  <PathBar
    bind:value={session.reportPath}
    kind="report"
    label="Percorso del report JSON"
    placeholder="Scegli il report JSON di una run"
    action="Apri report"
    busy={loading}
    onrun={() => load(0)}
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

  {#if report}
    <div class="card mt-4 grid grid-cols-2 gap-x-6 gap-y-2 text-xs md:grid-cols-4">
      <div>
        <p class="text-slate-500">Quando</p>
        <p class="font-mono text-sm">{new Date(report.timestamp).toLocaleString("it-IT")}</p>
      </div>
      <div>
        <p class="text-slate-500">Esito</p>
        <!-- No pass/fail icon here, deliberately: `exit_code_meaning` is robocopy's own open,
             composable bitmask description (`RobocopyStatus::describe`), not a closed enum like
             `integrity_status` below — `ReportView` does not even expose the numeric code to
             derive one from. Same reasoning that kept this field untranslated (Livello 1, punto
             4, PIANO_GUI.md §10): a fixed check/✕ here would claim a certainty the string itself
             does not have. -->
        <p class="font-mono text-sm">{report.exit_code_meaning ?? "—"}</p>
      </div>
      <div>
        <p class="text-slate-500">Durata</p>
        <p class="font-mono text-sm">{duration(report.elapsed_seconds)}</p>
      </div>
      <div>
        <p class="text-slate-500">Throughput</p>
        <p class="font-mono text-sm">{report.throughput_mbps.toFixed(1)} MB/s</p>
      </div>
      <div class="col-span-2">
        <p class="text-slate-500">Sorgente</p>
        <p class="truncate font-mono text-sm" title={report.source}>{report.source}</p>
      </div>
      <div class="col-span-2">
        <p class="text-slate-500">Destinazione</p>
        <p class="truncate font-mono text-sm" title={report.dest}>{report.dest}</p>
      </div>
      <div>
        <p class="text-slate-500">File copiati</p>
        <p class="font-mono text-sm">{report.files_copied} / {report.total_files}</p>
      </div>
      <div>
        <p class="text-slate-500">Byte copiati</p>
        <p class="font-mono text-sm">{bytes(report.bytes_copied)} / {bytes(report.total_bytes)}</p>
      </div>
      <div>
        <p class="text-slate-500">Cifrato</p>
        <p class="font-mono text-sm">{report.encrypted ? "sì" : "no"}</p>
      </div>
      <div>
        <p class="text-slate-500">Verifica</p>
        <!-- Absent is not the same as passed: a run without --verify-integrity compared nothing,
             and rendering that as a blank cell would read like a clean result. -->
        <p class="flex items-center gap-1 font-mono text-sm">
          {#if report.integrity_status === "Passed"}
            <CircleCheck size={14} strokeWidth={2} class="shrink-0 text-emerald-600 dark:text-emerald-400" aria-hidden="true" />
          {:else if report.integrity_status === "Failed"}
            <CircleX size={14} strokeWidth={2} class="shrink-0 text-amber-600 dark:text-amber-400" aria-hidden="true" />
          {/if}
          {INTEGRITY_LABEL[report.integrity_status] ?? report.integrity_status ?? "non eseguita"}
        </p>
      </div>
    </div>

    {#if report.copy_error}
      <p class="mt-3 rounded border border-red-300 bg-red-50 px-2 py-1 text-xs text-red-800
                dark:border-red-800 dark:bg-red-950 dark:text-red-200">
        La copia è fallita: {report.copy_error}
      </p>
    {/if}
    {#if report.webhook_error}
      <!-- Non-fatal by design: the backup succeeded and only the notification did not arrive.
           Shown as a warning rather than an error so the two are not confused. -->
      <p class="mt-2 rounded border border-amber-300 bg-amber-50 px-2 py-1 text-xs text-amber-900
                dark:border-amber-800 dark:bg-amber-950 dark:text-amber-200">
        Il backup è riuscito ma la notifica non è partita: {report.webhook_error}
      </p>
    {/if}
    {#if report.post_command_error}
      <p class="mt-2 rounded border border-amber-300 bg-amber-50 px-2 py-1 text-xs text-amber-900
                dark:border-amber-800 dark:bg-amber-950 dark:text-amber-200">
        Il comando post-job è fallito (non fa fallire il backup): {report.post_command_error}
      </p>
    {/if}

    {#if anyErrors}
      <h2 class="mt-5 text-xs font-semibold uppercase tracking-wide text-slate-500">
        Problemi rilevati dalla verifica ({report.integrity_error_count})
      </h2>

      <div class="mt-2 flex items-center gap-2">
        <input
          aria-label="Filtra i percorsi in questa pagina"
          class="w-full rounded border border-slate-300 px-2 py-1 text-xs dark:border-slate-700 dark:bg-slate-900"
          placeholder="Filtra per percorso, in questa pagina"
          bind:value={query}
        />
        <button
          class="shrink-0 rounded border border-slate-300 px-2 py-1 text-xs disabled:opacity-40 dark:border-slate-700"
          onclick={exportCsv}
          disabled={!anyFilteredEntries}
          title="Esporta la pagina corrente, con il filtro applicato"
        >Esporta CSV</button>
      </div>

      {#each LISTS as [key, title]}
        {@const page = report[key]}
        {@const filtered = filteredEntries(key)}
        <!-- On `entries`, not `total`: the three lists have different lengths and share one
             offset, so past the end of the shortest one a `total > 0` test still renders its
             header and prints a range like "mostrati 201-200". -->
        {#if page.entries.length > 0}
          <h3 class="mt-3 text-xs font-semibold">
            {title}
            <span class="ml-1 font-normal text-slate-500">
              <!-- "10 000" and "at least 10 000" are different claims: the source list is capped,
                   and rounding one into the other would be the report lying by omission. -->
              {page.truncated_at_source ? "almeno" : ""}
              {page.total}
              {#if page.total > page.entries.length}
                — mostrati {page.offset + 1}–{page.offset + page.entries.length}
              {/if}
              {#if query.trim() !== ""}
                — {filtered.length} corrispondono nella pagina
              {/if}
            </span>
          </h3>
          {#if filtered.length > 0}
            <ul class="mt-1 max-h-64 overflow-y-auto rounded border border-slate-200 dark:border-slate-800">
              {#each filtered as path}
                <li class="truncate px-2 py-0.5 font-mono text-[11px]" title={path}>{path}</li>
              {/each}
            </ul>
          {/if}
        {/if}
      {/each}

      <div class="mt-2 flex items-center gap-2 text-xs">
        <button
          class="rounded border border-slate-300 px-2 py-0.5 disabled:opacity-40 dark:border-slate-700"
          onclick={() => load(Math.max(0, offset - PAGE))}
          disabled={loading || offset === 0}
        >← Precedenti</button>
        <button
          class="rounded border border-slate-300 px-2 py-0.5 disabled:opacity-40 dark:border-slate-700"
          onclick={() => load(offset + PAGE)}
          disabled={loading || !LISTS.some(([k]) => report[k].total > offset + PAGE)}
        >Successivi →</button>
        <span class="text-slate-500">a blocchi di {PAGE}</span>
      </div>
    {:else if report.integrity_status}
      <p class="mt-4 text-xs text-slate-600 dark:text-slate-400">
        La verifica non ha rilevato differenze.
      </p>
    {/if}
  {:else if !error}
    <EmptyState
      icon={FileText}
      title="Scegli un report per vederne il dettaglio"
      lines={[
        "Ogni run conclusa scrive un report JSON (per impostazione predefinita ingest-report.json). Questa scheda ne mostra esito, volumi, durata e i file che la verifica ha segnalato.",
        "Gli elenchi per-file arrivano a blocchi di 100: un report può contenerne 10.000 per ciascuna delle tre liste, e mandarli tutti in un solo messaggio è la versione IPC dell'errore che D18 ha fatto con i log.",
      ]}
    />
  {/if}
</section>
