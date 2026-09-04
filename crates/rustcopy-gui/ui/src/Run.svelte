<script>
  import { invoke } from "@tauri-apps/api/core";
  import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
  import PathBar from "./PathBar.svelte";
  import EmptyState from "./EmptyState.svelte";
  import { session } from "./session.svelte.js";

  // The console does not run backups itself: it starts the same CLI a scheduled task would, so a
  // job behaves identically whether a person launched it or Task Scheduler did.
  let status = $state(null);
  let jobs = $state([]);
  let error = $state(null);
  let busy = $state(false);
  let timer = null;
  // Names of any Windows scheduled tasks whose command already references this exact file — read
  // once per "Esamina", advisory only. Never blocks starting a job, never offers to touch a
  // schedule from here (F61's prohibitions apply to this console as a whole).
  let existingSchedules = $state([]);
  // Onda 3: checkpoints found beside the config, read once per "Esamina" like the schedule badge
  // above. Independent of `jobs` on purpose — a checkpoint's own existence does not depend on the
  // config still parsing the same way it did when the interrupted run wrote it.
  let checkpoints = $state([]);

  // The batch position last seen in a live progress sample. Held apart from `status` itself
  // because `run_status`'s finished branch clears `progress` entirely — without this, the queue
  // view below would go blank at the exact moment a batch finishes, which is when an operator
  // most wants to see "all done" rather than nothing.
  //
  // Never reset on `inspect()` (an earlier version did, and lost the queue on re-examining the
  // very file that just finished — CodeRabbit review on #73). Instead `queue` below only renders
  // when `status?.config_path` matches `session.configPath`: `status` always reflects whichever
  // run this window is actually tracking (or last tracked), independent of what the operator has
  // typed into the path field, so the comparison alone keeps a stale retained position from a
  // *different* file from ever being shown, without needing to throw the value away.
  let lastBatchIndex = $state(null);
  let lastBatchTotal = $state(null);
  // Set only by `stop()`, this window's own only way to interrupt a run. Distinguishes "the batch
  // reached its natural end" from "it was cut short", which the `else` branch below needs: `run_
  // jobs` records a per-job failure and moves on regardless (only Ctrl+C/--cancel-file breaks out
  // early), so a batch that finished *without* ever being stopped from here is one where every
  // job's position was genuinely reached — even if the last job's own phases were too fast for a
  // single publisher tick to ever land, which the live samples alone cannot show (also #73).
  let wasStopped = $state(false);

  function rememberBatchPosition(s) {
    if (s?.progress?.batch_total > 1) {
      lastBatchIndex = s.progress.batch_index;
      lastBatchTotal = s.progress.batch_total;
    } else if (s && !s.running && !wasStopped && lastBatchTotal > 1) {
      lastBatchIndex = lastBatchTotal;
    }
  }

  // Onda 2, F49: a coarse queue, not a per-job outcome. `report_path` can carry `{timestamp}`
  // (P1), so which exact report file a given job wrote is not reliably knowable ahead of the run
  // — the queue only ever claims what `batch_index`/`batch_total` themselves can honestly say
  // (position), never a guessed riuscito/fallito. That distinction is what Report and Storico are
  // for, on the report the run actually wrote.
  const queue = $derived(
    jobs.length > 1 && lastBatchTotal > 1 && status?.config_path === session.configPath
      ? jobs.map((job, i) => {
          const position = i + 1;
          const state =
            position < lastBatchIndex
              ? "concluso"
              : position === lastBatchIndex
                ? status?.running
                  ? "in corso"
                  : "concluso"
                : "in attesa";
          return { name: job.name, state };
        })
      : [],
  );

  // A mirroring job cannot be authorised from here: the confirmation `check_mirror_safety` asks
  // for needs a terminal, and a child process launched from a window has none, so the run aborts
  // itself with exit 3. Saying so before the click is kinder than letting it fail.
  const mirrorJobs = $derived(jobs.filter((job) => job.mirror).map((job) => job.name));
  const unconfigured = $derived(jobs.filter((job) => job.unconfigured).map((job) => job.name));

  async function inspect() {
    error = null;
    busy = true;
    try {
      jobs = await invoke("list_jobs", { configPath: session.configPath });
      status = await invoke("run_status");
      rememberBatchPosition(status);
    } catch (e) {
      error = String(e);
      jobs = [];
    } finally {
      busy = false;
    }
    // Separate from the try above and never surfaced as `error`: a scheduler query failing (no
    // Task Scheduler access, a non-Windows dev build) must not block loading the job list itself
    // — this is a convenience note, not something the pane depends on.
    try {
      existingSchedules = await invoke("schedules_referencing", { configPath: session.configPath });
    } catch {
      existingSchedules = [];
    }
    // Same tolerance: a directory that cannot be scanned (permissions, a network share gone
    // missing) is "no checkpoints found", not a reason to fail the whole examination.
    try {
      checkpoints = await invoke("list_checkpoints", { configPath: session.configPath });
    } catch {
      checkpoints = [];
    }
  }

  async function resumeJob(checkpoint) {
    error = null;
    busy = true;
    wasStopped = false;
    try {
      status = await invoke("resume_job", { checkpointPath: checkpoint.path });
      rememberBatchPosition(status);
      poll();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function start() {
    error = null;
    busy = true;
    // A fresh run has not been stopped yet, whatever a previous one ended with.
    wasStopped = false;
    try {
      status = await invoke("start_job", { configPath: session.configPath });
      rememberBatchPosition(status);
      poll();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function stop() {
    error = null;
    try {
      status = await invoke("stop_job");
      wasStopped = true;
    } catch (e) {
      error = String(e);
    }
  }

  // Sampled on our own timer rather than pushed per event: a backup touches thousands of files a
  // second and an event per file is the shape D18 showed to be ruinous at this scale.
  //
  // Chained `setTimeout` rather than `setInterval`, and a generation counter on top. With an
  // interval, a slow reply and a fast one are in flight together and can settle out of order: an
  // earlier `running: true` landing after the final `running: false` leaves the pane insisting a
  // finished run is still going, with Avvia disabled and nothing that will ever re-enable it.
  let generation = 0;

  // Fired only from inside `tick`, on a genuine running→finished transition it observes itself —
  // never from `inspect()`, which can just as well load a run that already finished before this
  // window opened. Notifying for state the window did not watch happen would be a lie about what
  // just occurred, not a summary of it.
  async function notifyFinished(finished) {
    try {
      let granted = await isPermissionGranted();
      if (!granted) granted = (await requestPermission()) === "granted";
      if (!granted) return;
      sendNotification({
        title: finished.exit_code === 0 ? "Run riuscita" : "Run terminata con problemi",
        body: finished.meaning ?? `Codice di uscita ${finished.exit_code}`,
      });
    } catch {
      // A missed toast is not worth surfacing as an error: the pane already shows the same
      // outcome, this is a convenience on top of it, not the source of truth.
    }
  }

  function poll() {
    const mine = ++generation;
    clearTimeout(timer);

    const tick = async () => {
      try {
        const next = await invoke("run_status");
        // A reply from a superseded polling loop is discarded rather than rendered.
        if (mine !== generation) return;
        const wasRunning = status?.running === true;
        status = next;
        rememberBatchPosition(status);
        if (next.running) {
          timer = setTimeout(tick, 1000);
        } else if (wasRunning) {
          notifyFinished(next);
        }
      } catch (e) {
        if (mine !== generation) return;
        error = String(e);
      }
    };

    timer = setTimeout(tick, 1000);
  }

  $effect(() => () => {
    generation += 1;
    clearTimeout(timer);
  });
</script>

<section class="p-4">
  <PathBar
    bind:value={session.configPath}
    kind="config"
    label="Percorso del file di configurazione TOML"
    placeholder="Scegli un file di configurazione TOML"
    action="Esamina"
    busy={busy}
    onrun={inspect}
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

  {#if checkpoints.length > 0}
    <div class="mt-3 rounded border border-slate-300 p-2 dark:border-slate-700">
      <p class="text-xs font-semibold uppercase tracking-wide text-slate-500">
        {checkpoints.length === 1 ? "Ripresa disponibile" : "Riprese disponibili"}
      </p>
      <p class="mt-0.5 text-[11px] text-slate-500">
        Run interrotte che hanno scritto un checkpoint in questa cartella. Riprendere continua nella
        stessa direzione sorgente→destinazione, con pattern, thread, tentativi e verifica
        dell'interruzione — <strong>non</strong> il resto della configurazione originale (limite
        di quota, esclusioni, algoritmo di hash, mirror inclusi: D25 in ANALYSIS.md). Una ripresa
        può quindi girare più permissiva o più veloce dell'originale, mai più distruttiva.
      </p>
      <ul class="mt-2 space-y-1.5">
        {#each checkpoints as checkpoint}
          <li class="flex items-center justify-between gap-2 rounded border border-slate-200 px-2 py-1
                     text-xs dark:border-slate-800">
            <div class="min-w-0">
              <p class="font-mono truncate" title="{checkpoint.source} → {checkpoint.dest}">
                {checkpoint.source} → {checkpoint.dest}
              </p>
              <p class="text-[11px] text-slate-500">
                {new Date(checkpoint.timestamp).toLocaleString("it-IT")} — {checkpoint.reason}
              </p>
            </div>
            <button
              class="shrink-0 rounded border border-slate-300 px-2 py-1 text-[11px]
                     disabled:opacity-40 dark:border-slate-700"
              onclick={() => resumeJob(checkpoint)}
              disabled={busy || status?.running}
            >Riprendi</button>
          </li>
        {/each}
      </ul>
    </div>
  {/if}

  {#if jobs.length > 0}
    <p class="mt-3 text-xs text-slate-600 dark:text-slate-400">
      {jobs.length}
      {jobs.length === 1 ? "job" : "job"} in questo file: <span class="font-mono">{jobs.map((j) => j.name).join(", ")}</span>
    </p>

    {#if queue.length > 0}
      <ul class="mt-2 flex flex-wrap gap-1.5">
        {#each queue as entry}
          {@const cls =
            entry.state === "in corso"
              ? "bg-blue-100 text-blue-900 dark:bg-blue-950 dark:text-blue-200"
              : entry.state === "concluso"
                ? "bg-emerald-100 text-emerald-900 dark:bg-emerald-950 dark:text-emerald-200"
                : "bg-slate-100 text-slate-600 dark:bg-slate-800 dark:text-slate-400"}
          <li
            class="rounded px-1.5 py-0.5 text-[11px] font-mono {cls}"
            title={entry.state}
            aria-label="{entry.name}: {entry.state}"
          >
            {entry.name}
          </li>
        {/each}
      </ul>
      <p class="mt-1 text-[11px] text-slate-500">
        Solo la posizione nel batch: quale job è concluso, in corso o in attesa. L'esito di ciascuno
        — riuscito o fallito — resta nel Report o nello Storico di quella run.
      </p>
    {/if}

    {#if unconfigured.length > 0}
      <p class="mt-2 rounded border border-slate-300 bg-slate-50 px-2 py-1 text-xs
                dark:border-slate-700 dark:bg-slate-900">
        <strong>{unconfigured.join(", ")}</strong> {unconfigured.length === 1 ? "ha" : "hanno"}
        ancora percorsi segnaposto: è un file modello, non una configurazione. Compilalo prima di
        eseguirlo.
      </p>
    {/if}

    {#if mirrorJobs.length > 0}
      <p class="mt-2 rounded border border-amber-300 bg-amber-50 px-2 py-1 text-xs text-amber-900
                dark:border-amber-800 dark:bg-amber-950 dark:text-amber-200">
        <strong>{mirrorJobs.join(", ")}</strong> {mirrorJobs.length === 1 ? "cancella" : "cancellano"}
        in destinazione. Da qui non si può autorizzare: la conferma richiede un terminale, quindi la
        run si fermerà da sola con esito 3. Eseguila dalla CLI, dove la conferma mostra
        <em>quali</em> file verrebbero eliminati.
      </p>
    {/if}

    {#if existingSchedules.length > 0}
      <!-- Informational, not a warning: avviare qui non è pericoloso quanto un mirror, è solo
           potenzialmente ridondante con un'attività già pianificata. -->
      <p class="mt-2 rounded border border-blue-300 bg-blue-50 px-2 py-1 text-xs text-blue-900
                dark:border-blue-800 dark:bg-blue-950 dark:text-blue-200">
        {existingSchedules.length === 1 ? "Un'attività pianificata" : "Delle attività pianificate"}
        ({existingSchedules.join(", ")}) {existingSchedules.length === 1 ? "punta" : "puntano"} già
        a questo file: avviarlo da qui non lo sostituisce né lo disattiva, è un'esecuzione
        indipendente in più.
      </p>
    {/if}

    <div class="mt-3 flex items-center gap-2">
      <button
        class="rounded bg-blue-600 px-3 py-1 text-sm text-white disabled:opacity-50"
        onclick={start}
        disabled={busy || status?.running}
      >Avvia</button>
      <button
        class="rounded border border-slate-300 px-3 py-1 text-sm disabled:opacity-40
               dark:border-slate-700"
        onclick={stop}
        disabled={!status?.running || status?.stopping}
      >{status?.stopping ? "Arresto in corso…" : "Ferma"}</button>

      {#if status?.running}
        <span class="text-xs text-slate-600 dark:text-slate-400">
          {status.stopping
            ? "sto scrivendo il checkpoint, poi la run esce"
            : (status.phase_label ?? "in esecuzione")}
        </span>
      {:else if status?.exit_code !== null && status?.exit_code !== undefined}
        <span class="text-xs">
          <span
            class="rounded px-1 font-mono text-[10px] font-semibold {status.exit_code === 0
              ? 'bg-emerald-100 text-emerald-900 dark:bg-emerald-950 dark:text-emerald-200'
              : 'bg-amber-100 text-amber-900 dark:bg-amber-950 dark:text-amber-200'}"
          >{status.exit_code}</span>
          <!-- The meaning comes from the core: what an exit code means is a contract with
               schedulers, not a label this pane invents. -->
          {status.meaning}
        </span>
      {/if}
    </div>

    {#if status?.running && status?.progress}
      {@const p = status.progress}
      {@const fraction = p.bytes_total ? Math.min(1, p.bytes_done / p.bytes_total) : null}
      <div class="mt-3 max-w-2xl">
        <!-- A bar only where a percentage can honestly be computed. During the inventory there is
             no total, and a bar sitting at 0% for the twenty minutes a 1.34M-file prescan takes
             would be an invention presented as knowledge. -->
        {#if fraction !== null}
          <div class="h-1.5 w-full overflow-hidden rounded bg-slate-200 dark:bg-slate-800">
            <div class="h-full bg-blue-600" style="width: {(fraction * 100).toFixed(1)}%"></div>
          </div>
        {/if}
        <p class="mt-1 text-[11px] text-slate-600 dark:text-slate-400">
          {#if fraction !== null}
            {(fraction * 100).toFixed(0)}% —
          {/if}
          <!-- Against null, not truthiness: a known total of zero is a fact worth showing
               ("0 / 0 file"), while an absent one is the inventory not having finished. -->
          {#if p.files_total != null}
            {p.files_done} / {p.files_total} file —
          {/if}
          {Math.round(p.elapsed_seconds)}s
          {#if p.throughput_mbps > 0}
            — {p.throughput_mbps.toFixed(0)} MB/s
          {/if}
        </p>
      </div>
    {/if}

    {#if status?.output_tail}
      <!-- Shown whenever the run ended, not only on failure: a successful run's summary is worth
           reading too, and hiding it until something breaks means the operator only ever meets
           this panel in a bad moment. -->
      <details class="mt-3" open={status.exit_code !== 0}>
        <summary class="cursor-pointer text-xs text-slate-600 dark:text-slate-400">
          Output della run {status.exit_code === 0 ? "(riuscita)" : "— qui c'è il motivo"}
        </summary>
        <pre class="mt-1 max-h-64 overflow-auto rounded border border-slate-200 bg-slate-50 p-2
                    text-[11px] leading-snug whitespace-pre-wrap dark:border-slate-800
                    dark:bg-slate-900">{status.output_tail}</pre>
      </details>
    {/if}

    <p class="mt-2 text-[11px] text-slate-500">
      Fermare non uccide il processo: crea il file che la run sorveglia, così scrive il checkpoint
      e puoi riprendere con <code>--resume-from</code>. Terminarlo di forza salterebbe proprio
      quello.
    </p>
  {:else if !error}
    <EmptyState
      title="Scegli un file di configurazione per eseguirlo"
      lines={[
        "Questa scheda avvia la stessa CLI che eseguirebbe un'attività pianificata, come processo separato: un job si comporta allo stesso modo che lo lanci tu o Task Scheduler.",
        "Non può accendere il mirror, forzare un purge, installare servizi o pianificazioni: la lista degli argomenti è costruita nel core con una forma fissa, e un test verifica che quei flag non possano comparire.",
        "Se una run interrotta ha scritto un checkpoint in questa cartella, questa scheda lo trova e propone di riprenderla.",
        "A run conclusa, apri il report che ha prodotto nella scheda Report.",
      ]}
    />
  {/if}
</section>
