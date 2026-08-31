<script>
  import { invoke } from "@tauri-apps/api/core";

  // Read-only by design: this version has no write path at all, so it cannot damage a backup.
  // See PIANO_GUI_TAURI.md §5.2.
  let configPath = $state("");
  let jobs = $state([]);
  let error = $state(null);
  let loading = $state(false);

  async function loadJobs() {
    error = null;
    loading = true;
    try {
      // The frontend does not decide: it asks the library and renders what comes back (§4.1).
      // No judgement about mirroring, verification or outcomes is computed here.
      jobs = await invoke("list_jobs", { configPath });
    } catch (e) {
      error = String(e);
      jobs = [];
    } finally {
      loading = false;
    }
  }
</script>

<main class="min-h-screen bg-slate-50 text-slate-900 dark:bg-slate-950 dark:text-slate-100">
  <header class="border-b border-slate-200 px-4 py-3 dark:border-slate-800">
    <h1 class="text-sm font-semibold tracking-tight">rustcopy — console</h1>
    <p class="text-xs text-slate-500 dark:text-slate-400">
      Sola lettura: questa versione mostra la configurazione e lo storico, non esegue e non modifica nulla.
    </p>
  </header>

  <section class="p-4">
    <div class="flex gap-2">
      <!-- A placeholder is not a label: assistive technology needs a programmatic one. The visible
           text stays in the placeholder so the dense layout is unchanged. -->
      <label class="sr-only" for="config-path">Percorso del file di configurazione TOML</label>
      <input
        id="config-path"
        class="flex-1 rounded border border-slate-300 px-2 py-1 text-sm dark:border-slate-700 dark:bg-slate-900"
        placeholder="Percorso di un file di configurazione TOML"
        bind:value={configPath}
      />
      <button
        class="rounded bg-blue-600 px-3 py-1 text-sm text-white disabled:opacity-50"
        onclick={loadJobs}
        disabled={loading || configPath.length === 0}
      >
        {loading ? "Lettura…" : "Elenca job"}
      </button>
    </div>

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
      <table class="mt-4 w-full text-left text-xs">
        <thead class="border-b border-slate-300 dark:border-slate-700">
          <tr>
            <th class="py-1 pr-3 font-medium">Job</th>
            <th class="py-1 pr-3 font-medium">Sorgente</th>
            <th class="py-1 pr-3 font-medium">Destinazione</th>
            <th class="py-1 pr-3 font-medium">Tipo</th>
            <th class="py-1 pr-3 font-medium">Verifica</th>
          </tr>
        </thead>
        <tbody>
          {#each jobs as job (job.name)}
            <tr class="border-b border-slate-200 dark:border-slate-800">
              <td class="py-1 pr-3 font-mono">
                {job.name}
                <!-- `--mirror` deletes at the destination, so it must be visible as such and never
                     rendered like an ordinary copy. -->
                {#if job.mirror}
                  <span
                    class="ml-1 rounded bg-amber-200 px-1 text-[10px] font-semibold text-amber-900
                           dark:bg-amber-900 dark:text-amber-100"
                  >MIRROR — cancella in destinazione</span>
                {/if}
              </td>
              <td class="py-1 pr-3 font-mono text-slate-600 dark:text-slate-400">{job.source ?? "—"}</td>
              <td class="py-1 pr-3 font-mono text-slate-600 dark:text-slate-400">{job.dest ?? "—"}</td>
              <td class="py-1 pr-3">{job.backup_type ?? "copia"}</td>
              <td class="py-1 pr-3">
                <!-- fast_verify travels beside verify_integrity, never instead of it: it skips files
                     whose source is unchanged, so it is a weaker guarantee and saying only "yes"
                     would overstate it. -->
                {#if job.verify_integrity}
                  {job.fast_verify ? "sì (fast)" : "sì"}
                {:else}
                  no
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </section>
</main>
