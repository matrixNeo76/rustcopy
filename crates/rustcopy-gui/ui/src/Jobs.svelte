<script>
  import { invoke } from "@tauri-apps/api/core";
  import PathBar from "./PathBar.svelte";
  import EmptyState from "./EmptyState.svelte";
  import { session } from "./session.svelte.js";

  // The frontend does not decide: it asks the library and renders what comes back
  // (PIANO_GUI_TAURI.md 4.1). No judgement about mirroring, verification or outcomes here.
  let jobs = $state([]);
  let error = $state(null);
  let loading = $state(false);
  let loaded = $state(false);

  async function load() {
    error = null;
    loading = true;
    try {
      jobs = await invoke("list_jobs", { configPath: session.configPath });
      loaded = true;
    } catch (e) {
      error = String(e);
      jobs = [];
    } finally {
      loading = false;
    }
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
              {#if job.unconfigured}
                <!-- A template read as if it were a configured job is how a first look at the
                     product ends in confusion: the row looks complete and points nowhere. -->
                <span
                  class="ml-1 rounded bg-slate-200 px-1 text-[10px] font-semibold text-slate-700
                         dark:bg-slate-700 dark:text-slate-200"
                >MODELLO — percorsi da compilare</span>
              {/if}
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
  {:else if loaded && !error}
    <EmptyState
      title="Il file non descrive nessun job"
      lines={["Un file senza [[jobs]] descrive comunque un job singolo nei campi di primo livello: se anche quelli sono vuoti, non c'è niente da elencare."]}
    />
  {:else if !error}
    <EmptyState
      title="Scegli un file di configurazione per cominciare"
      lines={[
        "Questa scheda elenca i job che un file TOML descrive: sorgente, destinazione, tipo di backup e se la verifica è attiva.",
        "Un job che cancella in destinazione (mirror) viene segnalato in modo distinto, perché è l'impostazione più distruttiva che possa avere.",
        "Non hai un file? Prova examples/demo-locale.toml: copia qualche file finto del repository in una cartella accanto, quindi non può toccare nulla di tuo.",
      ]}
    />
  {/if}
</section>
