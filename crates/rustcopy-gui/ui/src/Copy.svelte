<script>
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { session } from "./session.svelte.js";
  import { bytes } from "./format.js";
  import { FolderPlus, FolderOpen, X, Copy as CopyIcon, ArrowRight } from "@lucide/svelte";

  // F95: the plain "copy these folders there" screen -- no configuration file to choose or save.
  // Everything that decides what is safe (a folder copied into itself, a whole drive, two folders
  // with the same name) lives in `runner::plan_copy`; this file only collects the choice, asks the
  // core to turn it into a throwaway configuration (`prepare_copy`) and starts it with the same
  // `start_job` every other tab uses. No mirror, purge or verification is reachable from here.
  let sources = $state([]);
  let dest = $state("");
  let error = $state(null);
  let starting = $state(false);

  // "Controlla": file/folder counts and size per chosen folder, on a button press only -- a real
  // profile in this project takes minutes to walk (`gui_api::inspect_path`), so never on every
  // change. `checks` is keyed by path; a result is shown only while its path is still chosen.
  let checks = $state({});
  let checking = $state(false);
  let destCheck = $state(null);
  let checkGeneration = 0;

  function addSources(paths) {
    const known = new Set(sources.map((path) => path.toLowerCase()));
    for (const path of paths) {
      if (!known.has(path.toLowerCase())) {
        sources.push(path);
        known.add(path.toLowerCase());
      }
    }
    // A different set of folders makes any earlier answer about the destination stale too, and
    // any refusal shown for the previous choice no longer describes this one.
    destCheck = null;
    error = null;
  }

  async function chooseSources() {
    const picked = await open({ directory: true, multiple: true });
    if (picked == null) return;
    addSources(Array.isArray(picked) ? picked : [picked]);
  }

  async function chooseDest() {
    const picked = await open({ directory: true, multiple: false });
    if (typeof picked === "string" && picked.length > 0) {
      dest = picked;
      destCheck = null;
      error = null;
    }
  }

  function removeSource(path) {
    sources = sources.filter((entry) => entry !== path);
    delete checks[path];
    error = null;
  }

  async function check() {
    const mine = ++checkGeneration;
    checking = true;
    error = null;
    try {
      const results = await Promise.all(
        sources.map(async (path) => [path, await invoke("inspect_path", { path, configPath: "" })]),
      );
      const destination = dest.trim()
        ? await invoke("inspect_path", { path: dest.trim(), configPath: "" })
        : null;
      if (mine !== checkGeneration) return;
      checks = Object.fromEntries(results);
      destCheck = destination;
    } catch (e) {
      if (mine === checkGeneration) error = String(e);
    } finally {
      if (mine === checkGeneration) checking = false;
    }
  }

  const totals = $derived.by(() => {
    let files = 0;
    let size = 0;
    let known = 0;
    for (const path of sources) {
      const result = checks[path];
      if (result?.exists && result.is_dir) {
        files += result.total_files;
        size += result.total_bytes;
        known += 1;
      }
    }
    return { files, size, known };
  });

  // Where each folder will land, shown before anything starts. Display only: the real planning
  // (and every refusal) is `plan_copy` in the core, run when "Copia" is pressed.
  function folderName(path) {
    return path.replace(/[\\/]+$/, "").split(/[\\/]/).pop();
  }
  function landing(path) {
    const root = dest.trim().replace(/[\\/]+$/, "");
    return root ? `${root}\\${folderName(path)}` : "";
  }

  async function copy() {
    error = null;
    starting = true;
    try {
      const configPath = await invoke("prepare_copy", { sources: $state.snapshot(sources), dest: dest.trim() });
      await invoke("start_job", { configPath });
      // Lands on Esegui already attached to the run (the one-shot flag F93 added for QuickSync).
      session.configPath = configPath;
      session.pendingRunAttach = true;
      session.activeTab = "run";
    } catch (e) {
      error = String(e);
    } finally {
      starting = false;
    }
  }

  const canCopy = $derived(sources.length > 0 && dest.trim().length > 0 && !starting);
</script>

<section class="p-4">
  <div class="max-w-3xl">
    <h2 class="text-lg font-semibold">Copia cartelle</h2>
    <p class="mt-1 text-sm text-slate-600 dark:text-slate-400">
      Scegli una o più cartelle e dove metterle: ciascuna viene copiata dentro la destinazione, con il
      suo nome. Copia solo i file nuovi o cambiati e <strong>non cancella nulla</strong>. Non serve
      alcun file di configurazione.
    </p>

    <h3 class="mt-5 text-xs font-semibold uppercase tracking-wide text-slate-500">Cosa copiare</h3>
    <div class="card mt-1">
      {#if sources.length === 0}
        <p class="text-sm text-slate-500">Nessuna cartella scelta.</p>
      {:else}
        <ul class="divide-y divide-slate-200 dark:divide-slate-800">
          {#each sources as path (path)}
            {@const result = checks[path]}
            <li class="flex items-start justify-between gap-3 py-2">
              <div class="min-w-0">
                <p class="truncate font-mono text-sm" title={path}>{path}</p>
                {#if result}
                  <p class="mt-0.5 text-xs {result.exists && result.is_dir ? 'text-slate-500' : 'text-amber-700 dark:text-amber-400'}">
                    {#if !result.exists}
                      non esiste
                    {:else if !result.is_dir}
                      non è una cartella
                    {:else}
                      {result.total_files} file · {result.total_dirs} cartelle · {bytes(result.total_bytes)}
                    {/if}
                  </p>
                {/if}
              </div>
              <button
                class="shrink-0 rounded p-1 text-slate-500 hover:bg-slate-100 hover:text-slate-800 dark:hover:bg-slate-800 dark:hover:text-slate-200"
                title="Togli questa cartella dall'elenco"
                aria-label="Togli {path} dall'elenco"
                onclick={() => removeSource(path)}
              ><X size={16} strokeWidth={2} aria-hidden="true" /></button>
            </li>
          {/each}
        </ul>
      {/if}
      <button
        class="mt-2 inline-flex items-center gap-1.5 rounded border border-slate-300 px-3 py-1.5 text-sm dark:border-slate-700"
        onclick={chooseSources}
      ><FolderPlus size={16} strokeWidth={2} aria-hidden="true" />Aggiungi cartelle…</button>
    </div>

    <h3 class="mt-5 text-xs font-semibold uppercase tracking-wide text-slate-500">Dove metterle</h3>
    <div class="card mt-1">
      <div class="flex gap-2">
        <input
          class="min-w-0 flex-1 rounded border border-slate-300 px-2 py-1.5 font-mono text-sm dark:border-slate-700 dark:bg-slate-900"
          bind:value={dest}
          oninput={() => {
            destCheck = null;
            error = null;
          }}
          placeholder="Cartella di destinazione, anche di rete (\\server\condivisione)"
          aria-label="Cartella di destinazione"
        />
        <button
          class="inline-flex shrink-0 items-center gap-1.5 rounded border border-slate-300 px-3 py-1.5 text-sm dark:border-slate-700"
          onclick={chooseDest}
        ><FolderOpen size={16} strokeWidth={2} aria-hidden="true" />Sfoglia…</button>
      </div>
      {#if destCheck}
        <p class="mt-1.5 text-xs text-slate-500">
          {#if !destCheck.exists}
            La destinazione non esiste ancora: verrà creata.
          {:else if !destCheck.is_dir}
            <span class="text-amber-700 dark:text-amber-400">Questo percorso è un file, non una cartella.</span>
          {:else}
            La destinazione esiste e contiene già {destCheck.total_files} file.
          {/if}
        </p>
      {/if}
    </div>

    {#if sources.length > 0 && dest.trim()}
      <h3 class="mt-5 text-xs font-semibold uppercase tracking-wide text-slate-500">Cosa succederà</h3>
      <div class="card mt-1 text-sm">
        <ul class="space-y-1">
          {#each sources as path (path)}
            <li class="flex items-center gap-2">
              <span class="min-w-0 truncate font-mono" title={path}>{folderName(path)}</span>
              <ArrowRight size={14} strokeWidth={2} class="shrink-0 text-slate-400" aria-hidden="true" />
              <span class="min-w-0 truncate font-mono text-slate-600 dark:text-slate-400" title={landing(path)}>{landing(path)}</span>
            </li>
          {/each}
        </ul>
        {#if totals.known > 0}
          <p class="mt-2 text-xs text-slate-500">
            In tutto {totals.files} file, {bytes(totals.size)}.
          </p>
        {/if}
      </div>
    {/if}

    {#if error}
      <p
        class="mt-4 rounded border border-amber-300 bg-amber-50 px-3 py-2 text-sm text-amber-900
               dark:border-amber-800 dark:bg-amber-950 dark:text-amber-200"
        role="alert"
      >{error}</p>
    {/if}

    <div class="mt-5 flex items-center gap-3">
      <button
        class="inline-flex items-center gap-1.5 rounded bg-blue-600 px-4 py-2 text-sm font-semibold text-white disabled:opacity-50"
        onclick={copy}
        disabled={!canCopy}
      ><CopyIcon size={16} strokeWidth={2} aria-hidden="true" />{starting ? "Avvio…" : "Copia"}</button>
      <button
        class="rounded border border-slate-300 px-3 py-2 text-sm disabled:opacity-40 dark:border-slate-700"
        onclick={check}
        disabled={checking || (sources.length === 0 && !dest.trim())}
      >{checking ? "Controllo…" : "Controlla prima"}</button>
    </div>
    <p class="mt-2 text-xs text-slate-500">
      "Controlla prima" conta i file e lo spazio senza copiare niente (su cartelle molto grandi può
      richiedere un po'). Dopo "Copia" vedi l'avanzamento nella scheda Esegui.
    </p>
  </div>
</section>
