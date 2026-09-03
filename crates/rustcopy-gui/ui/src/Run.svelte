<script>
  import { invoke } from "@tauri-apps/api/core";
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
    } catch (e) {
      error = String(e);
      jobs = [];
    } finally {
      busy = false;
    }
  }

  async function start() {
    error = null;
    busy = true;
    try {
      status = await invoke("start_job", { configPath: session.configPath });
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
    } catch (e) {
      error = String(e);
    }
  }

  // Sampled on our own timer rather than pushed per event: a backup touches thousands of files a
  // second and an event per file is the shape D18 showed to be ruinous at this scale.
  function poll() {
    clearInterval(timer);
    timer = setInterval(async () => {
      try {
        status = await invoke("run_status");
        if (!status.running) clearInterval(timer);
      } catch (e) {
        error = String(e);
        clearInterval(timer);
      }
    }, 1000);
  }

  $effect(() => () => clearInterval(timer));
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

  {#if jobs.length > 0}
    <p class="mt-3 text-xs text-slate-600 dark:text-slate-400">
      {jobs.length}
      {jobs.length === 1 ? "job" : "job"} in questo file: <span class="font-mono">{jobs.map((j) => j.name).join(", ")}</span>
    </p>

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
            : "in esecuzione"}
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
        "A run conclusa, apri il report che ha prodotto nella scheda Report.",
      ]}
    />
  {/if}
</section>
