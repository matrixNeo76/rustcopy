<script>
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { session } from "./session.svelte.js";
  import { FolderSync } from "@lucide/svelte";

  // F71: the simplest case Modifica already covers -- one job, no mirror, no retention, no
  // backup_type -- reachable without first having a TOML to open. "Copy only new/changed files"
  // needs no feature of its own: it is robocopy's own default whenever --mirror is absent,
  // verified empirically several times this session (D26/D27). The real gap was only that Modifica
  // requires an existing file to start from (`read_drafts` -> `IngestConfig::load_from` fails on a
  // path that doesn't exist yet) -- this panel is the one entry point that doesn't need one.
  //
  // Deliberately minimal: only source/destination, no mirror/backup_type/retention offered here.
  // Whoever needs those already has Modifica -- this stays the simple, safe path, not a shortcut
  // around it (PIANO_GUI.md §17.3/§17.4).
  let source = $state("");
  let dest = $state("");
  let starting = $state(false);
  let error = $state(null);

  async function browseFolder(field) {
    const picked = await open({ directory: true, multiple: false });
    if (typeof picked !== "string" || picked.length === 0) return;
    if (field === "source") source = picked;
    else dest = picked;
  }

  // Every field `JobDraft` requires (job_editor::JobDraft) beyond name/source/dest, spelled out
  // explicitly rather than left to serde's Option-defaults-to-None behavior -- this is the only
  // draft this pane ever builds, so there is no form bound to the rest and no ambiguity worth
  // hiding behind an implicit default.
  function buildDraft() {
    return {
      name: "sync",
      source,
      dest,
      pattern: null,
      threads: null,
      retries: null,
      retry_wait_seconds: null,
      verify_integrity: false,
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
    };
  }

  async function sync() {
    error = null;
    // Asked before anything is written: the job is real, not a throwaway scratch file (unlike
    // F64's restore-preview report), so the operator has to be able to find and reopen it later.
    const picked = await save({
      filters: [{ name: "Configurazione TOML", extensions: ["toml"] }],
    });
    if (typeof picked !== "string" || picked.length === 0) return;

    starting = true;
    try {
      // No existing config: `job_editor::build_proposal`'s `existing: None` branch, a single-job
      // file with no `[[jobs]]`. F54's guards apply unchanged -- this draft's `mirror`/
      // `backup_type` are already false/null, so there is nothing for `apply_draft` to widen.
      await invoke("write_proposal", { configPath: null, drafts: [buildDraft()], outPath: picked });
      // The same `start_job` Esegui's own "Avvia" calls -- this panel writes, then hands off to
      // the run the CLI performs identically either way.
      await invoke("start_job", { configPath: picked });
      // Lands on Esegui with the new file already loaded, reusing its own tracking (`run_status`,
      // its poll loop, "Apri il report di questa run") rather than a second copy of that logic
      // here -- one "Esamina" click there, the same as starting any other job from Esegui itself.
      session.configPath = picked;
      session.activeTab = "run";
    } catch (e) {
      error = String(e);
    } finally {
      starting = false;
    }
  }
</script>

<div class="mt-4 max-w-2xl rounded border border-slate-300 p-4 dark:border-slate-700">
  <div class="flex items-center gap-2">
    <FolderSync size={16} strokeWidth={1.75} class="text-slate-400 dark:text-slate-600" aria-hidden="true" />
    <p class="text-sm font-semibold">Sincronizza due cartelle adesso</p>
  </div>
  <p class="mt-1 text-xs text-slate-600 dark:text-slate-400">
    Copia in destinazione solo i file nuovi o cambiati — il comportamento di default di robocopy
    quando non usa mirror. Nessuna opzione distruttiva qui: per mirror, retention o tipo di backup
    serve Modifica.
  </p>

  <div class="mt-3 grid grid-cols-[6rem_1fr] items-center gap-x-3 gap-y-2 text-xs">
    <label for="qs-source">Sorgente</label>
    <div class="flex gap-2">
      <input
        id="qs-source"
        class="flex-1 rounded border border-slate-300 px-2 py-1 font-mono dark:border-slate-700 dark:bg-slate-900"
        bind:value={source}
      />
      <button
        type="button"
        class="shrink-0 rounded border border-slate-300 px-2 py-1 dark:border-slate-700"
        onclick={() => browseFolder("source")}
      >Sfoglia…</button>
    </div>

    <label for="qs-dest">Destinazione</label>
    <div class="flex gap-2">
      <input
        id="qs-dest"
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

  {#if error}
    <p
      class="mt-3 rounded border border-red-300 bg-red-50 px-2 py-1 text-xs text-red-800
             dark:border-red-800 dark:bg-red-950 dark:text-red-200"
      role="alert"
    >{error}</p>
  {/if}

  <button
    class="mt-3 rounded bg-blue-600 px-3 py-1 text-sm text-white disabled:opacity-50"
    onclick={sync}
    disabled={starting || source.trim().length === 0 || dest.trim().length === 0}
  >
    {starting ? "Avvio…" : "Sincronizza"}
  </button>
  <p class="mt-1 text-[11px] text-slate-500">
    Chiede dove salvare il job (un file TOML vero, non usa e getta: si riapre e si rilancia da
    Job/Esegui/Modifica come qualunque altro), poi lo avvia subito.
  </p>
</div>
