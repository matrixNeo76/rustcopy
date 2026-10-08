<script>
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { session } from "./session.svelte.js";
  import { invalidNameReason } from "./jobName.js";
  import { FolderPlus } from "@lucide/svelte";

  // F83: the gap this closes is not "the empty state has no text" -- it already did (F79's
  // example, F71's QuickSync) -- but that neither of those creates a *named, reusable* job for the
  // operator's own real folders. F79 writes fixed fake data; QuickSync (F71) writes a real file but
  // starts it immediately under a fixed name ("sync") with zero fields beyond source/dest --
  // deliberately minimal by its own design (see QuickSync.svelte's header comment), not meant to
  // be extended into this. This is the third, distinct entry point: name the job, pick real
  // folders, save it for later -- without running it and without a config to start from.
  //
  // Reuses the exact mechanism QuickSync already established: `job_editor::build_proposal`'s
  // `existing: None` branch (a single-job file, no [[jobs]]) via the same `write_proposal` command
  // Editor.svelte and QuickSync both already call. No core change, no new Tauri command.
  let name = $state("");
  let source = $state("");
  let dest = $state("");
  let verifyIntegrity = $state(false);
  let saving = $state(false);
  let error = $state(null);

  const nameProblem = $derived(name.trim() === "" ? null : invalidNameReason(name.trim()));
  const canSave = $derived(
    !saving && name.trim() !== "" && nameProblem === null && source.trim() !== "" && dest.trim() !== "",
  );

  async function browseFolder(field) {
    const picked = await open({ directory: true, multiple: false });
    if (typeof picked !== "string" || picked.length === 0) return;
    if (field === "source") source = picked;
    else dest = picked;
  }

  // Every field `JobDraft` requires (job_editor::JobDraft) beyond the four this form actually
  // exposes, spelled out explicitly rather than left to serde's Option-defaults-to-None behaviour
  // -- this is the only draft this pane ever builds, so there is no form bound to the rest and no
  // ambiguity worth hiding behind an implicit default. Mirrors QuickSync.svelte's own `buildDraft`.
  function buildDraft() {
    return {
      name: name.trim(),
      source,
      dest,
      pattern: null,
      threads: null,
      retries: null,
      retry_wait_seconds: null,
      verify_integrity: verifyIntegrity,
      fast_verify: false,
      ignore_transient_missing: false,
      exclude_junctions: false,
      compare_baseline: false,
      dry_run: false,
      long_paths: false,
      preserve_timestamps: false,
      preserve_acl: false,
      no_prescan: false,
      hash_algo: null,
      backup_type: null,
      exclude_files: [],
      exclude_dirs: [],
      min_age_days: null,
      max_age_days: null,
      bandwidth_limit_mbps: null,
      report_path: null,
      log_path: null,
      html_report_path: null,
      mirror: false,
      keep_generations: null,
      encrypt_aes256: null,
    };
  }

  async function createJob() {
    error = null;
    // Asked before anything is written, same as QuickSync: this is a real file the operator has to
    // be able to find again, not a scratch path chosen for them.
    const picked = await save({
      filters: [{ name: "Configurazione TOML", extensions: ["toml"] }],
    });
    if (typeof picked !== "string" || picked.length === 0) return;

    saving = true;
    try {
      await invoke("write_proposal", { configPath: null, drafts: [buildDraft()], outPath: picked });
      // Loading it straight away is the success signal, same pattern as F79's createExample():
      // the empty state this wizard lives in disappears the moment `jobs` is non-empty, replaced
      // by the table showing the job just created -- no separate "fatto!" banner needed.
      session.configPath = picked;
      onCreated();
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  let { onCreated } = $props();
</script>

<div class="mt-4 max-w-2xl rounded border border-slate-300 p-4 dark:border-slate-700">
  <div class="flex items-center gap-2">
    <FolderPlus size={16} strokeWidth={1.75} class="text-slate-400 dark:text-slate-600" aria-hidden="true" />
    <p class="text-sm font-semibold">Crea una configurazione per le tue cartelle</p>
  </div>
  <p class="mt-1 text-xs text-slate-600 dark:text-slate-400">
    Scrive un file TOML vero e riusabile — lo ritrovi in Job/Esegui/Modifica come qualunque altro,
    ma non lo avvia: decidi tu quando lanciarlo, da Esegui.
  </p>

  <div class="mt-3 grid grid-cols-[6rem_1fr] items-center gap-x-3 gap-y-2 text-xs">
    <label for="njw-name">Nome</label>
    <div>
      <input
        id="njw-name"
        class="w-64 rounded border border-slate-300 px-2 py-1 font-mono dark:border-slate-700 dark:bg-slate-900"
        placeholder="es. backup-documenti"
        bind:value={name}
      />
      {#if nameProblem}
        <p class="mt-0.5 text-[11px] font-medium text-red-700 dark:text-red-400">{nameProblem}</p>
      {/if}
    </div>

    <label for="njw-source">Sorgente</label>
    <div class="flex gap-2">
      <input
        id="njw-source"
        class="flex-1 rounded border border-slate-300 px-2 py-1 font-mono dark:border-slate-700 dark:bg-slate-900"
        bind:value={source}
      />
      <button
        type="button"
        class="shrink-0 rounded border border-slate-300 px-2 py-1 dark:border-slate-700"
        onclick={() => browseFolder("source")}
      >Sfoglia…</button>
    </div>

    <label for="njw-dest">Destinazione</label>
    <div class="flex gap-2">
      <input
        id="njw-dest"
        class="flex-1 rounded border border-slate-300 px-2 py-1 font-mono dark:border-slate-700 dark:bg-slate-900"
        bind:value={dest}
      />
      <button
        type="button"
        class="shrink-0 rounded border border-slate-300 px-2 py-1 dark:border-slate-700"
        onclick={() => browseFolder("dest")}
      >Sfoglia…</button>
    </div>
  </div>

  <label class="mt-3 flex items-center gap-1 text-xs">
    <input type="checkbox" bind:checked={verifyIntegrity} /> Verifica integrità
  </label>

  {#if error}
    <p
      class="mt-3 rounded border border-red-300 bg-red-50 px-2 py-1 text-xs text-red-800
             dark:border-red-800 dark:bg-red-950 dark:text-red-200"
      role="alert"
    >{error}</p>
  {/if}

  <button
    class="mt-3 rounded bg-blue-600 px-3 py-1 text-sm text-white disabled:opacity-50"
    onclick={createJob}
    disabled={!canSave}
  >
    {saving ? "Creazione…" : "Crea job"}
  </button>
  <p class="mt-1 text-[11px] text-slate-500">
    Per mirror, retention, tipo di backup o altre opzioni: crea qui il job, poi apri Modifica.
  </p>
</div>
