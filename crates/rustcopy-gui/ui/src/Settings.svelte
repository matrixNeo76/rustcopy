<script>
  import { invoke } from "@tauri-apps/api/core";
  import PathBar from "./PathBar.svelte";
  import EmptyState from "./EmptyState.svelte";
  import { session } from "./session.svelte.js";

  // The job-settings table below is read-only: it renders the TOML the CLI already reads and
  // changes nothing. F55's write surface (editing settings and scripts in place) is a separate,
  // still-undecided step, and this pane does not attempt it.
  //
  // The credential section further down is the one deliberate exception in this file: it writes,
  // but only to the Windows Credential Manager (F56, `crypto::write_credential`/
  // `delete_credential`) — never to any TOML, and not the scripts/settings F55 leaves undecided.
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

  // Independent of the config path above: a credential is not scoped to any one job or file.
  let credName = $state("");
  let credSecret = $state("");
  let credBusy = $state(false);
  let credMessage = $state(null);
  let credError = $state(null);

  async function saveCredential() {
    credError = null;
    credMessage = null;
    credBusy = true;
    try {
      await invoke("set_credential", { name: credName, secret: credSecret });
      credMessage = `Credenziale "${credName}" salvata. Usala come keyring:${credName}.`;
      // Cleared, not just hidden: nothing left in the page's own state once the secret has done
      // its one job of reaching the credential manager.
      credSecret = "";
    } catch (e) {
      credError = String(e);
    } finally {
      credBusy = false;
    }
  }

  async function removeCredential() {
    credError = null;
    credMessage = null;
    credBusy = true;
    try {
      await invoke("delete_credential", { name: credName });
      credMessage = `Credenziale "${credName}" rimossa.`;
    } catch (e) {
      credError = String(e);
    } finally {
      credBusy = false;
    }
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
                      <!-- Origin badge inline with the value it describes, not in its own column
                           at the far right — on a wide window that put it ~1300px from the value
                           it labels, forcing a full-width eye movement per row for no reason
                           (Livello 1, punto 3, PIANO_GUI.md §10). -->
                      <span class="font-mono">{entry.value}</span>
                      <span class="ml-1.5 rounded px-1 text-[10px] font-semibold {ORIGIN_CLASS[entry.origin]}">
                        {ORIGIN_LABEL[entry.origin]}
                      </span>
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

  <section class="mt-6 border-t border-slate-200 pt-4 dark:border-slate-800">
    <h2 class="text-xs font-semibold uppercase tracking-wide text-slate-500">Gestione credenziali</h2>
    <p class="mt-1 text-xs text-slate-600 dark:text-slate-400">
      Salva o rimuove un segreto in Gestione credenziali di Windows (F56) — mai nel file TOML, mai
      come argomento: il segreto passa solo per questo modulo. Usalo poi come
      <code>keyring:NOME</code> ovunque un campo accetti una chiave o una password, per esempio
      <code>--encrypt-aes256 keyring:NOME</code>.
    </p>
    <div class="mt-2 flex flex-wrap items-end gap-2">
      <label class="text-xs">
        Nome
        <input
          class="block rounded border border-slate-300 px-2 py-1 text-sm dark:border-slate-700 dark:bg-slate-900"
          bind:value={credName}
          placeholder="es. backup-nas"
          autocomplete="off"
        />
      </label>
      <label class="text-xs">
        Segreto
        <input
          type="password"
          class="block rounded border border-slate-300 px-2 py-1 text-sm dark:border-slate-700 dark:bg-slate-900"
          bind:value={credSecret}
          autocomplete="off"
        />
      </label>
      <button
        class="rounded bg-blue-600 px-3 py-1 text-sm text-white disabled:opacity-50"
        onclick={saveCredential}
        disabled={credBusy || credName.length === 0 || credSecret.length === 0}
      >Salva</button>
      <button
        class="rounded border border-slate-300 px-3 py-1 text-sm disabled:opacity-40 dark:border-slate-700"
        onclick={removeCredential}
        disabled={credBusy || credName.length === 0}
      >Elimina</button>
    </div>
    {#if credMessage}
      <p class="mt-2 rounded border border-emerald-300 bg-emerald-50 px-2 py-1 text-xs text-emerald-900
                dark:border-emerald-800 dark:bg-emerald-950 dark:text-emerald-200">
        {credMessage}
      </p>
    {/if}
    {#if credError}
      <p class="mt-2 rounded border border-red-300 bg-red-50 px-2 py-1 text-xs text-red-800
                dark:border-red-800 dark:bg-red-950 dark:text-red-200" role="alert">
        {credError}
      </p>
    {/if}
  </section>
</section>
