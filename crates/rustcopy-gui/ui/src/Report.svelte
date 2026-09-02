<script>
  import { invoke } from "@tauri-apps/api/core";
  import PathBar from "./PathBar.svelte";
  import EmptyState from "./EmptyState.svelte";
  import { session } from "./session.svelte.js";

  // `read_report`/`read_report_page` existed in the core and on the IPC surface from F53 and no
  // pane ever called them: a complete report viewer with nothing attached to it. This is the pane.
  let report = $state(null);
  let error = $state(null);
  let loading = $state(false);
  let offset = $state(0);

  const PAGE = 100;

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
    } catch (e) {
      error = String(e);
      report = null;
    } finally {
      loading = false;
    }
  }

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
    <div class="mt-4 grid grid-cols-2 gap-x-6 gap-y-1 text-xs md:grid-cols-4">
      <div>
        <p class="text-slate-500">Quando</p>
        <p class="font-mono">{new Date(report.timestamp).toLocaleString("it-IT")}</p>
      </div>
      <div>
        <p class="text-slate-500">Esito</p>
        <p class="font-mono">{report.exit_code_meaning ?? "—"}</p>
      </div>
      <div>
        <p class="text-slate-500">Durata</p>
        <p class="font-mono">{duration(report.elapsed_seconds)}</p>
      </div>
      <div>
        <p class="text-slate-500">Throughput</p>
        <p class="font-mono">{report.throughput_mbps.toFixed(1)} MB/s</p>
      </div>
      <div class="col-span-2">
        <p class="text-slate-500">Sorgente</p>
        <p class="truncate font-mono" title={report.source}>{report.source}</p>
      </div>
      <div class="col-span-2">
        <p class="text-slate-500">Destinazione</p>
        <p class="truncate font-mono" title={report.dest}>{report.dest}</p>
      </div>
      <div>
        <p class="text-slate-500">File copiati</p>
        <p class="font-mono">{report.files_copied} / {report.total_files}</p>
      </div>
      <div>
        <p class="text-slate-500">Byte copiati</p>
        <p class="font-mono">{bytes(report.bytes_copied)} / {bytes(report.total_bytes)}</p>
      </div>
      <div>
        <p class="text-slate-500">Cifrato</p>
        <p class="font-mono">{report.encrypted ? "sì" : "no"}</p>
      </div>
      <div>
        <p class="text-slate-500">Verifica</p>
        <!-- Absent is not the same as passed: a run without --verify-integrity compared nothing,
             and rendering that as a blank cell would read like a clean result. -->
        <p class="font-mono">{report.integrity_status ?? "non eseguita"}</p>
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

      {#each LISTS as [key, title]}
        {@const page = report[key]}
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
            </span>
          </h3>
          <ul class="mt-1 max-h-64 overflow-y-auto rounded border border-slate-200 dark:border-slate-800">
            {#each page.entries as path}
              <li class="truncate px-2 py-0.5 font-mono text-[11px]" title={path}>{path}</li>
            {/each}
          </ul>
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
      title="Scegli un report per vederne il dettaglio"
      lines={[
        "Ogni run conclusa scrive un report JSON (per impostazione predefinita ingest-report.json). Questa scheda ne mostra esito, volumi, durata e i file che la verifica ha segnalato.",
        "Gli elenchi per-file arrivano a blocchi di 100: un report può contenerne 10.000 per ciascuna delle tre liste, e mandarli tutti in un solo messaggio è la versione IPC dell'errore che D18 ha fatto con i log.",
      ]}
    />
  {/if}
</section>
