<script>
  import { invoke } from "@tauri-apps/api/core";
  import PathBar from "./PathBar.svelte";
  import EmptyState from "./EmptyState.svelte";
  import { session } from "./session.svelte.js";

  // Read-only, like every other pane in this version: it renders the TOML the CLI already reads
  // and changes nothing. F55's write surface is a separate, still-undecided step.
  let jobs = $state([]);
  let error = $state(null);
  let loading = $state(false);
  let loaded = $state(false);
  // Most settings sit at their default and say nothing interesting. Hiding them by default keeps
  // the pane readable; the toggle is there because "what is this job actually set to" is a
  // legitimate question too.
  let showDefaults = $state(false);

  // The origin comes from the library as an enum. Rendering it is the frontend's job; deciding it
  // is not — `merged_over` resolves the value and only the library knows which layer supplied it.
  const ORIGIN_LABEL = {
    Job: "job",
    Inherited: "ereditato",
    Default: "default",
  };
  const ORIGIN_CLASS = {
    Job: "bg-blue-100 text-blue-900 dark:bg-blue-950 dark:text-blue-200",
    Inherited: "bg-slate-200 text-slate-700 dark:bg-slate-800 dark:text-slate-300",
    Default: "bg-transparent text-slate-400 dark:text-slate-600",
  };

  async function load() {
    error = null;
    loading = true;
    try {
      jobs = await invoke("read_settings", { configPath: session.configPath });
      loaded = true;
    } catch (e) {
      error = String(e);
      jobs = [];
    } finally {
      loading = false;
    }
  }

  function visible(entries) {
    // A caution is never hidden, whatever its origin: a job that mirrors because nobody set
    // anything is still a job that mirrors.
    return showDefaults
      ? entries
      : entries.filter((entry) => entry.origin !== "Default" || entry.caution);
  }
</script>

<section class="p-4">
  <PathBar
    bind:value={session.configPath}
    kind="config"
    label="Percorso del file di configurazione TOML"
    placeholder="Scegli un file di configurazione TOML"
    action="Apri impostazioni"
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
    <label class="mt-3 flex items-center gap-2 text-xs text-slate-600 dark:text-slate-400">
      <input type="checkbox" bind:checked={showDefaults} />
      Mostra anche le impostazioni lasciate al valore predefinito
    </label>

    {#each jobs as job (job.name)}
      <article class="mt-4">
        <h2 class="font-mono text-sm font-semibold">{job.name}</h2>

        {#each job.groups as group}
          {@const entries = visible(group.entries)}
          {#if entries.length > 0}
            <h3 class="mt-3 text-xs font-semibold uppercase tracking-wide text-slate-500">
              {group.title}
            </h3>
            <table class="mt-1 w-full text-left text-xs">
              <tbody>
                {#each entries as entry}
                  <tr class="border-b border-slate-200 align-top dark:border-slate-800">
                    <td class="w-56 py-1 pr-3 font-mono text-slate-600 dark:text-slate-400">
                      {entry.key}
                    </td>
                    <td class="py-1 pr-3">
                      <span class="font-mono">{entry.value}</span>
                      {#if entry.redacted}
                        <!-- The value shown is not the stored one. Saying so is the difference
                             between a redaction and a wrong reading of the file. -->
                        <span class="ml-1 text-[10px] text-slate-500">(troncato)</span>
                      {/if}
                      {#if entry.caution}
                        <p class="mt-0.5 text-[11px] text-amber-700 dark:text-amber-300">
                          {entry.caution}
                        </p>
                      {/if}
                    </td>
                    <td class="w-24 py-1 text-right">
                      <span class="rounded px-1 text-[10px] font-semibold {ORIGIN_CLASS[entry.origin]}">
                        {ORIGIN_LABEL[entry.origin]}
                      </span>
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          {/if}
        {/each}
      </article>
    {/each}
  {:else if loaded && !error}
    <!-- `read_settings` returns at least one entry for any file it can parse, so this branch is
         currently unreachable. Kept because a pane that renders nothing gives an operator no way
         to tell an empty result from an application that broke, and the guard costs three lines. -->
    <EmptyState
      title="Il file non descrive nessuna impostazione"
      lines={["Un file senza [[jobs]] descrive comunque un job singolo nei campi di primo livello."]}
    />
  {:else if !error}
    <EmptyState
      title="Scegli un file di configurazione per vederne le impostazioni"
      lines={[
        "Questa scheda mostra le due cose che il TOML non dice: da quale strato viene il valore che vince per ciascun job, e quali impostazioni portano una conseguenza — cancellano, saltano controlli, eliminano generazioni.",
        "L'URL di un webhook viene troncato a schema e host di proposito: vale come credenziale e questa finestra finisce negli screenshot.",
      ]}
    />
  {/if}
</section>
