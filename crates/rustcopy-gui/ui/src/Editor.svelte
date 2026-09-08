<script>
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import PathBar from "./PathBar.svelte";
  import EmptyState from "./EmptyState.svelte";
  import { session } from "./session.svelte.js";
  import { ChevronUp, ChevronDown } from "@lucide/svelte";

  // F76: an empty Report field is already correct -- it means "use the core's default", resolved
  // against the configuration file's directory once the job actually runs -- but a visually empty
  // field doesn't say so. Shown only as a placeholder, never written into the draft: writing it
  // literally would change the semantics from "inherit/use the default" to "this job pins this
  // path", a real difference `job_editor::pin` distinguishes on purpose. Mirrors
  // `gui_api::DEFAULT_REPORT_PATH` -- keep the two in sync if that default ever changes.
  const DEFAULT_REPORT_PATH_PLACEHOLDER = "./robocopy_ingest_report.json";

  // F75: Thread's placeholder needs the *actual* default this machine would use when the field
  // is left empty (`cli::default_threads`, clamped 1-128 via `MIN_THREADS`/`MAX_THREADS`).
  // Fetched from the core (`gui_api::default_threads`, one pure Tauri command) rather than reading
  // `navigator.hardwareConcurrency` in JS -- found by CodeRabbit that the latter is not a safe
  // substitute: Chromium (WebView2's engine) can clamp or mask it for fingerprinting protection,
  // so it is not guaranteed to equal what the core actually resolves an empty field to. `null`
  // until the one-time fetch resolves; the caption and placeholder both fall back to "…" for that
  // brief window, same pattern as F81's `meaningByCode`.
  let DEFAULT_THREADS = $state(null);
  invoke("default_threads").then((value) => {
    DEFAULT_THREADS = value;
  });

  // F74: verified empirically against real robocopy.exe before adding these -- `draft.pattern` is
  // a single string field (`CopyRequest::pattern`, `engine/robocopy.rs::build_args`) sent to
  // robocopy as ONE argv token, never split. Both a semicolon-joined form ("*.jpg;*.png;*.gif")
  // and a space-joined form inside one token ("*.jpg *.png *.gif") copy ZERO files with exit code
  // 0 -- no error, just a silently empty backup, because robocopy treats the whole string as one
  // literal filespec that matches no real filename. Multiple filespecs only work as separate argv
  // tokens (`robocopy src dst *.jpg *.png *.gif`, three arguments), which this single-string field
  // cannot produce. So the suggestions below stay single-pattern only -- do not add a
  // multi-extension example here without first changing `pattern` to a list, a core change out of
  // scope for a tooltip/caption feature.
  const PATTERN_SUGGESTIONS = ["*", "*.pdf", "*.jpg"];

  // F74: exclude_files/exclude_dirs are already a `Vec<String>` (`engine/robocopy.rs` pushes one
  // `/XF`/`/XD` flag per entry), so -- unlike Pattern above -- multiple entries genuinely work.
  // These add to `draft.exclude_files`, never replace what was typed by hand.
  const EXCLUDE_FILE_SHORTCUTS = ["*.tmp", "*.log", "Thumbs.db", "desktop.ini", "~$*"];

  // The only pane that writes. It never writes in place: it produces a proposal in a new file and
  // the operator decides whether it replaces the running configuration.
  let drafts = $state([]);
  let selected = $state(0);
  let outPath = $state("");
  let error = $state(null);
  let written = $state(null);
  let loading = $state(false);
  let saving = $state(false);
  // Which drafts were read from the file. Their names are their identity — `run_jobs` namespaces
  // report, cache and generation manifest by the job name (D12) — so renaming one here would
  // orphan its generation chain and start a fresh history under the new name. The editor offers
  // creation instead, which has no such consequence.
  let existingNames = $state(new Set());
  // The configuration these drafts actually came from. Not `session.configPath`: that is shared,
  // so opening a different file in another pane would silently retarget a write already in
  // progress here -- A's jobs applied over B's configuration. The shared path is what made the
  // console usable; this is the hole it opened, and the two have to be held apart.
  let loadedFrom = $state("");
  // F69: the floor a job's `keep_generations` may never go below, captured once at load time by
  // job name (existing jobs only -- `addJob` sets `keep_generations = null` on a new one, so it
  // never gets a floor and the raise-only control stays hidden for it, same as the core's own
  // "cannot introduce" rule). Read separately from `draft.keep_generations`, which the raise
  // control itself mutates as the operator types -- using the live draft value as its own floor
  // would let every keystroke redefine the minimum.
  let originalKeepGenerations = $state(new Map());
  // F73: result of the last "Verifica" click for Sorgente/Destinazione, keyed by which path it
  // was run against -- so editing the field after a check discards the now-stale answer instead
  // of showing it next to a path it no longer describes. `null` means "never checked" (or
  // discarded); never auto-run on every keystroke -- see `gui_api::inspect_path` for why.
  let sourceCheck = $state(null);
  let destCheck = $state(null);

  // The whole draft is loaded, edited in part, and sent back whole. That is deliberate: a field
  // this form does not render still round-trips untouched, so rendering a subset can never drop a
  // setting the file had.
  const draft = $derived(drafts[selected] ?? null);
  // `read_drafts` (core) already merges each job over the file's top-level defaults before handing
  // it to the form (`job_editor::draft_from(&job.merged_over(&config.defaults), ...)`), so this is
  // the *effective* starting value -- inheritance already accounted for, not just this job's own
  // field. `null` means "not set here": the raise control stays hidden and the read-only line below
  // explains why, exactly like `mirrorLocked` does for Mirror.
  const keepGenerationsFloor = $derived(draft ? (originalKeepGenerations.get(draft.name) ?? null) : null);

  // F80: `crypto::resolve_key` accepts four forms (`keyring:NAME`/`env:NAME`/`file:PATH`/
  // literal), but this form only ever writes `keyring:NAME` -- a literal key here would defeat
  // F56's entire point (a secret visible in a file on disk instead of the credential manager),
  // and env:/file: name something on the machine running the job, not something this form can
  // usefully offer a picker for. A value already set by hand in one of those other three forms is
  // shown read-only instead of forced into a text box that would silently rewrite it the moment
  // the operator touched an unrelated field.
  const encryptKeyringForm = $derived(
    !draft || draft.encrypt_aes256 == null || draft.encrypt_aes256.startsWith("keyring:"),
  );
  const encryptCredentialName = $derived(
    draft && encryptKeyringForm && draft.encrypt_aes256 ? draft.encrypt_aes256.slice("keyring:".length) : "",
  );

  function setEncryptCredential(name) {
    const trimmed = name.trim();
    draft.encrypt_aes256 = trimmed === "" ? null : `keyring:${trimmed}`;
  }

  // F73: a check is only shown while it still describes the field it was run against -- editing
  // the path afterward, switching job, or loading a different configuration file discards it
  // rather than displaying a now-stale answer next to a path it no longer matches (or, worse, one
  // resolved against a different file's anchor). `configPath` matters here even though `jobName`
  // and `path` are also compared: two different configuration files can each declare a job named
  // "job1" with the same relative `source`, and without this a check from one could appear valid
  // for the other, showing counts resolved against the wrong anchor -- caught by CodeRabbit on
  // this PR, not by the live verification that had only ever loaded one file at a time.
  const sourceCheckValid = $derived(
    sourceCheck &&
      draft &&
      sourceCheck.configPath === loadedFrom &&
      sourceCheck.jobName === draft.name &&
      sourceCheck.path === draft.source
      ? sourceCheck
      : null,
  );
  const destCheckValid = $derived(
    destCheck &&
      draft &&
      destCheck.configPath === loadedFrom &&
      destCheck.jobName === draft.name &&
      destCheck.path === draft.dest
      ? destCheck
      : null,
  );

  // Mirrors a rule the core owns and enforces (`job_editor`): the editor may narrow risk, never
  // widen it. Disabling the control here is an affordance, not the enforcement — `write_proposal`
  // refuses the same edit whatever this frontend sends.
  const mirrorLocked = $derived(draft ? !draft.mirror : true);
  const nameLocked = $derived(draft ? existingNames.has(draft.name) : true);

  // F72: mirrors `validate_job_name` (`lib.rs`) -- `namespaced_path` interpolates a job name
  // literally into a filename, so a Windows reserved character or reserved device name would
  // otherwise surface only as a cryptic I/O error hours later, at the job's first scheduled run.
  // `apply_draft` rejects the same thing; this is the immediate affordance, not the enforcement.
  const WINDOWS_RESERVED_FILENAME_CHARS = ["\\", "/", ":", "*", "?", '"', "<", ">", "|"];
  // Legacy superscript-digit forms (COM¹/COM²/COM³/LPT¹/LPT²/LPT³) are reserved identically to
  // the plain-digit ones -- confirmed against Microsoft's own docs, added after CodeRabbit found
  // the omission on the PR that introduced this list.
  const WINDOWS_RESERVED_DEVICE_NAMES = new Set([
    "CON", "PRN", "AUX", "NUL",
    "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
    "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    "COM¹", "COM²", "COM³", "LPT¹", "LPT²", "LPT³",
  ]);
  // Windows also forbids the C0 control range (U+0001-U+001F) in filenames -- same CodeRabbit
  // finding as the superscript variants above.
  const WINDOWS_CONTROL_CHAR_PATTERN = /[\u0001-\u001f]/;
  function invalidNameReason(name) {
    const bad = WINDOWS_RESERVED_FILENAME_CHARS.find((c) => name.includes(c));
    if (bad) return `Non può contenere '${bad}' (riservato in un nome di file Windows).`;
    if (WINDOWS_CONTROL_CHAR_PATTERN.test(name)) {
      return "Non può contenere caratteri di controllo (riservati in un nome di file Windows).";
    }
    if (WINDOWS_RESERVED_DEVICE_NAMES.has(name.toUpperCase())) {
      return "È un nome di dispositivo riservato da Windows.";
    }
    return null;
  }
  const nameInvalidReason = $derived(draft && !nameLocked ? invalidNameReason(draft.name) : null);

  const stale = $derived(loadedFrom !== "" && loadedFrom !== session.configPath);

  // F78: `IngestError::EditorCannotSplitSingleJobConfig` (job_editor.rs) is a deliberate rule, not
  // a bug (F54: a single-job file has no [[jobs]] to inherit from, so turning it into one changes
  // what every field means -- the core declines rather than doing it silently). Its raw message
  // ("add the [[jobs]] section by hand first") assumes the operator already knows what that means.
  // No core change: `build_proposal`'s match is exhaustive (empty / single-matching / reject) and
  // adding a real "Convert" capability is a separate decision (PIANO_GUI.md §18.1). This only
  // intercepts the one specific error to show a concrete TOML example instead of the raw string --
  // every other error still renders as-is below.
  // Anchored on the full literal suffix (`errors.rs`'s `#[error(...)]` text is fixed except for
  // the label), not just " into" -- found by CodeRabbit that a non-greedy match up to the first
  // " into" would stop early and extract the wrong (partial) name if an existing job's name
  // happened to contain that substring. `label_of` (job_editor.rs) reads a stored job's name
  // straight from the TOML with no validation on read (only the editor's own write path
  // restricts new names), so an externally-edited file's name is not something this can assume
  // is well-behaved.
  function splitJobErrorLabel(message) {
    const match = message?.match(
      /cannot split the single-job configuration holding (.+) into several jobs: add the \[\[jobs\]\] section by hand first/,
    );
    return match ? match[1] : null;
  }
  const splitJobLabel = $derived(error ? splitJobErrorLabel(error) : null);

  // The TOML example below embeds this label inside a quoted string literal -- since `label_of`
  // reads a stored job's name with no validation on read (see above), it could contain `"` or
  // `\` and produce an invalid TOML snippet if inserted raw. Escapes both TOML string-literal
  // metacharacters, same order real TOML escaping requires (backslash first, so escaping the
  // quote doesn't double-escape a backslash it just introduced).
  function tomlStringLiteral(value) {
    return value.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
  }

  async function load() {
    error = null;
    written = null;
    loading = true;
    try {
      const source = session.configPath;
      drafts = await invoke("read_job_drafts", { configPath: source });
      existingNames = new Set(drafts.map((entry) => entry.name));
      originalKeepGenerations = new Map(drafts.map((entry) => [entry.name, entry.keep_generations]));
      selected = 0;
      outPath = await invoke("suggest_proposal_path", { configPath: source });
      loadedFrom = source;
    } catch (e) {
      error = String(e);
      drafts = [];
    } finally {
      loading = false;
    }
  }

  function addJob() {
    if (drafts.length === 0) return;
    // Copied from the currently selected job so the new one starts from something valid, then
    // stripped of the settings the editor is not allowed to originate.
    const base = { ...drafts[selected] };
    base.name = "";
    base.mirror = false;
    base.keep_generations = null;
    drafts = [...drafts, base];
    selected = drafts.length - 1;
  }

  // Run order matters: `run_jobs` (CLI) executes `[[jobs]]` sequentially in file order, and that
  // order is fixed for the whole batch the moment the process starts — nothing outside can reorder
  // a batch already running (PIANO_GUI.md §14.4). This is therefore the only point where reordering
  // is actually cheap: before the file is even written. `write_proposal` already serializes
  // `drafts` in whatever order this array holds, so swapping two entries here is the entire
  // implementation — no new core surface, no new IPC command.
  function moveJob(delta) {
    const target = selected + delta;
    if (target < 0 || target >= drafts.length) return;
    const next = [...drafts];
    [next[selected], next[target]] = [next[target], next[selected]];
    drafts = next;
    selected = target;
  }

  // F68: `Sorgente`/`Destinazione` were the one place left where a path had to be typed instead
  // of chosen — every other path field in the console already goes through a native picker
  // (`PathBar.svelte`, this pane's own proposal-output field below). Not routed through `PathBar`
  // itself: its recent/favorite lists are for *files* (config/report, keyed by `kind`), and a
  // job's source/destination are folders with no comparable notion of "recently opened" here —
  // mixing the two would blur lists that answer different questions. Two direct calls to the same
  // plugin `PathBar` already depends on, nothing shared beyond the mechanism.
  async function browseFolder(field) {
    const picked = await open({ directory: true, multiple: false });
    if (typeof picked === "string" && picked.length > 0) {
      draft[field] = picked;
    }
  }

  // F73: manual "Verifica" button, never run automatically -- a real profile in this project
  // takes minutes to walk in full (`gui_api::inspect_path`'s own doc comment), so checking on
  // every keystroke would make the field feel broken rather than helpful.
  async function checkPath(field) {
    if (!draft) return;
    const configPath = loadedFrom;
    const jobName = draft.name;
    const path = draft[field];
    const set = field === "source" ? (v) => (sourceCheck = v) : (v) => (destCheck = v);
    set({ configPath, jobName, path, loading: true, result: null, error: null });
    try {
      const result = await invoke("inspect_path", { path, configPath });
      set({ configPath, jobName, path, loading: false, result, error: null });
    } catch (e) {
      set({ configPath, jobName, path, loading: false, result: null, error: String(e) });
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

  async function pickTarget() {
    // A save dialog offers to replace an existing file; the core refuses to. Rather than let the
    // operator pick a target that is then rejected, an existing choice is reported here as the
    // refusal it will be, in the same words.
    const picked = await save({
      defaultPath: outPath,
      filters: [{ name: "Configurazione TOML", extensions: ["toml"] }],
    });
    if (typeof picked === "string" && picked.length > 0) {
      outPath = picked;
      written = null;
      error = null;
    }
  }

  async function writeProposal() {
    error = null;
    written = null;
    saving = true;
    try {
      // `loadedFrom`, never the shared path: the proposal must be built over the same file the
      // drafts were read from.
      await invoke("write_proposal", { configPath: loadedFrom, drafts, outPath });
      written = outPath;
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  // Comma-separated in the form, a list in the draft. Empty entries are dropped so a trailing
  // comma does not become an exclusion pattern matching nothing.
  function toList(text) {
    return text
      .split(",")
      .map((item) => item.trim())
      .filter((item) => item.length > 0);
  }

  function numberOrNull(text) {
    const trimmed = text.trim();
    if (trimmed === "") return null;
    const value = Number(trimmed);
    return Number.isInteger(value) ? value : null;
  }

  // F74: a shortcut adds to the existing list; it never replaces what was typed by hand, and
  // clicking one already present is a no-op rather than a duplicate entry.
  function addExcludeFile(pattern) {
    if (!draft.exclude_files.includes(pattern)) {
      draft.exclude_files = [...draft.exclude_files, pattern];
    }
  }
</script>

<section class="p-4">
  <PathBar
    bind:value={session.configPath}
    kind="config"
    label="Percorso del file di configurazione TOML"
    placeholder="Scegli un file di configurazione TOML"
    action="Apri per modifica"
    busy={loading}
    onrun={load}
  />

  {#if error && splitJobLabel}
    <div
      class="mt-3 rounded border border-red-300 bg-red-50 px-3 py-2 text-sm text-red-900
             dark:border-red-800 dark:bg-red-950 dark:text-red-200"
      role="alert"
    >
      <p>
        Non puoi aggiungere un altro job a questo file da qui: <code>{splitJobLabel}</code> vive
        oggi nella parte superiore del TOML, senza una sezione <code>[[jobs]]</code> — trasformarlo
        cambierebbe il significato di ogni sua impostazione, quindi l'editor rifiuta piuttosto che
        farlo in silenzio.
      </p>
      <p class="mt-2">
        Per avere più job in questo file, riscrivine a mano l'inizio così, poi riapri qui per
        modificare:
      </p>
      <pre
        class="mt-1 overflow-x-auto rounded bg-red-100 px-2 py-1 font-mono text-xs
               dark:bg-red-900"
      >{`[[jobs]]
name = "${tomlStringLiteral(splitJobLabel)}"
source = "..."
dest = "..."
# ...il resto dei campi oggi in cima al file

[[jobs]]
name = "nuovo-job"
source = "..."
dest = "..."`}</pre>
    </div>
  {:else if error}
    <p
      class="mt-3 rounded border border-red-300 bg-red-50 px-2 py-1 text-sm text-red-800
             dark:border-red-800 dark:bg-red-950 dark:text-red-200"
      role="alert"
    >
      {error}
    </p>
  {/if}

  {#if written}
    <p
      class="mt-3 rounded border border-emerald-300 bg-emerald-50 px-2 py-1 text-sm text-emerald-900
             dark:border-emerald-800 dark:bg-emerald-950 dark:text-emerald-200"
      role="status"
    >
      Proposta scritta in <code>{written}</code>. Il file di configurazione in uso <strong>non è stato
      toccato</strong>: la sostituzione la decidi tu.
    </p>
  {/if}

  {#if stale}
    <p
      class="mt-3 rounded border border-amber-300 bg-amber-50 px-2 py-1 text-sm text-amber-900
             dark:border-amber-800 dark:bg-amber-950 dark:text-amber-200"
      role="status"
    >
      Le modifiche aperte qui vengono da <code>{loadedFrom}</code>, ma il file selezionato ora è un
      altro. Scriverle sopra la configurazione corrente vi mescolerebbe job di due file diversi:
      premi «Apri per modifica» per ricaricare, oppure rimetti il percorso precedente.
    </p>
  {/if}

  {#if draft}
    <div class="mt-4 flex gap-1">
      {#each drafts as entry, index (entry.name + index)}
        <button
          class="rounded px-2 py-0.5 font-mono text-xs {index === selected
            ? 'bg-slate-200 font-semibold dark:bg-slate-800'
            : 'text-slate-500'}"
          onclick={() => (selected = index)}
        >{entry.name}</button>
      {/each}
      <button
        class="rounded px-2 py-0.5 text-xs text-blue-700 dark:text-blue-300"
        onclick={addJob}
      >+ Nuovo job</button>

      {#if drafts.length > 1}
        <!-- Reorders the job selected above, not a drag target of its own — one pair of controls
             for the whole strip, not one per tab, so adding jobs never adds visual noise with them.
             Order here is the order `write_proposal` writes: this changes only the sequence
             `[[jobs]]` runs in, nothing else about any job. -->
        <span class="ml-1 flex items-center gap-0.5 border-l border-slate-300 pl-1 dark:border-slate-700">
          <button
            class="rounded p-0.5 text-slate-500 hover:bg-slate-100 disabled:opacity-30
                   dark:text-slate-400 dark:hover:bg-slate-800"
            onclick={() => moveJob(-1)}
            disabled={selected === 0}
            title="Sposta questo job prima nell'ordine di esecuzione"
          >
            <ChevronUp size={14} strokeWidth={2.25} aria-hidden="true" />
          </button>
          <button
            class="rounded p-0.5 text-slate-500 hover:bg-slate-100 disabled:opacity-30
                   dark:text-slate-400 dark:hover:bg-slate-800"
            onclick={() => moveJob(1)}
            disabled={selected === drafts.length - 1}
            title="Sposta questo job dopo nell'ordine di esecuzione"
          >
            <ChevronDown size={14} strokeWidth={2.25} aria-hidden="true" />
          </button>
        </span>
      {/if}
    </div>

    <div class="mt-3 grid grid-cols-[10rem_1fr] items-center gap-x-3 gap-y-2 text-xs">
      <label for="f-name">Nome</label>
      <div>
        <input
          id="f-name"
          class="w-64 rounded border border-slate-300 px-2 py-1 font-mono disabled:bg-slate-100
                 disabled:text-slate-500 dark:border-slate-700 dark:bg-slate-900
                 dark:disabled:bg-slate-800"
          bind:value={draft.name}
          disabled={nameLocked}
        />
        {#if nameLocked}
          <p class="mt-0.5 text-[11px] text-slate-500">
            Il nome è l'identità del job: report, cache e manifest delle generazioni sono
            namespacizzati su di esso. Rinominarlo orfanerebbe la catena delle generazioni, quindi
            l'editor non lo consente.
          </p>
        {:else if nameInvalidReason}
          <p class="mt-0.5 text-[11px] font-medium text-red-700 dark:text-red-400">
            {nameInvalidReason}
          </p>
        {/if}
      </div>

      <label for="f-source">Sorgente</label>
      <div>
        <div class="flex gap-2">
          <input id="f-source" class="flex-1 rounded border border-slate-300 px-2 py-1 font-mono dark:border-slate-700 dark:bg-slate-900" bind:value={draft.source} />
          <button
            type="button"
            class="shrink-0 rounded border border-slate-300 px-2 py-1 text-sm dark:border-slate-700"
            onclick={() => browseFolder("source")}
          >Sfoglia…</button>
          <button
            type="button"
            class="shrink-0 rounded border border-slate-300 px-2 py-1 text-sm disabled:opacity-50 dark:border-slate-700"
            disabled={!draft.source || sourceCheckValid?.loading}
            onclick={() => checkPath("source")}
          >Verifica</button>
        </div>
        {#if sourceCheckValid?.loading}
          <p class="mt-0.5 text-[11px] text-slate-500">
            Verifica in corso… su un albero molto grande può richiedere qualche minuto.
          </p>
        {:else if sourceCheckValid?.error}
          <p class="mt-0.5 text-[11px] text-red-700 dark:text-red-400">{sourceCheckValid.error}</p>
        {:else if sourceCheckValid?.result}
          {#if !sourceCheckValid.result.exists}
            <p class="mt-0.5 text-[11px] font-medium text-red-700 dark:text-red-400">
              Il percorso non esiste.
            </p>
          {:else if !sourceCheckValid.result.is_dir}
            <p class="mt-0.5 text-[11px] font-medium text-amber-700 dark:text-amber-400">
              Esiste, ma non è una cartella.
            </p>
          {:else}
            <p class="mt-0.5 text-[11px] text-slate-500">
              {sourceCheckValid.result.total_files} file, {sourceCheckValid.result.total_dirs} cartelle,
              {bytes(sourceCheckValid.result.total_bytes)}.
            </p>
          {/if}
        {/if}
      </div>

      <label for="f-dest">Destinazione</label>
      <div>
        <div class="flex gap-2">
          <input id="f-dest" class="flex-1 rounded border border-slate-300 px-2 py-1 font-mono dark:border-slate-700 dark:bg-slate-900" bind:value={draft.dest} />
          <button
            type="button"
            class="shrink-0 rounded border border-slate-300 px-2 py-1 text-sm dark:border-slate-700"
            onclick={() => browseFolder("dest")}
          >Sfoglia…</button>
          <button
            type="button"
            class="shrink-0 rounded border border-slate-300 px-2 py-1 text-sm disabled:opacity-50 dark:border-slate-700"
            disabled={!draft.dest || destCheckValid?.loading}
            onclick={() => checkPath("dest")}
          >Verifica</button>
        </div>
        {#if destCheckValid?.loading}
          <p class="mt-0.5 text-[11px] text-slate-500">
            Verifica in corso… su un albero molto grande può richiedere qualche minuto.
          </p>
        {:else if destCheckValid?.error}
          <p class="mt-0.5 text-[11px] text-red-700 dark:text-red-400">{destCheckValid.error}</p>
        {:else if destCheckValid?.result}
          {#if !destCheckValid.result.exists}
            <!-- Deliberately neutral, not a warning: an inexistent Destinazione is the ordinary
                 case for a first backup (F68), not a problem to flag. -->
            <p class="mt-0.5 text-[11px] text-slate-500">
              Non esiste ancora — normale per un primo backup.
            </p>
          {:else if !destCheckValid.result.is_dir}
            <p class="mt-0.5 text-[11px] font-medium text-amber-700 dark:text-amber-400">
              Esiste, ma non è una cartella.
            </p>
          {:else}
            <p class="mt-0.5 text-[11px] text-slate-500">
              {destCheckValid.result.total_files} file, {destCheckValid.result.total_dirs} cartelle,
              {bytes(destCheckValid.result.total_bytes)}.
            </p>
          {/if}
        {/if}
      </div>

      <label for="f-pattern">Pattern</label>
      <div>
        <input
          id="f-pattern"
          class="w-48 rounded border border-slate-300 px-2 py-1 font-mono dark:border-slate-700 dark:bg-slate-900"
          placeholder="*"
          title="Un pattern alla volta per robocopy (es. *.pdf). Una stringa con più pattern arriva a robocopy come un unico argomento e non corrisponde a nulla, verificato empiricamente."
          value={draft.pattern ?? ""}
          oninput={(e) => {
            const pattern = e.currentTarget.value.trim();
            draft.pattern = pattern === "" ? null : pattern;
          }}
        />
        <p class="mt-0.5 text-[11px] text-slate-500">
          Esempi: {#each PATTERN_SUGGESTIONS as example, index}{index > 0 ? ", " : ""}<code
            >{example}</code
          >{/each} — un pattern alla volta; per più estensioni serve un job per ciascuna.
        </p>
      </div>

      <label for="f-threads">Thread</label>
      <!-- 1..=128 is the range the CLI enforces (`IngestError::InvalidThreads`). The bounds here
           are the affordance; `apply_draft` refuses the same values whatever this form sends. -->
      <div>
        <input
          id="f-threads"
          type="number"
          min="1"
          max="128"
          step="1"
          class="w-32 rounded border border-slate-300 px-2 py-1 dark:border-slate-700 dark:bg-slate-900"
          placeholder={DEFAULT_THREADS === null ? "" : String(DEFAULT_THREADS)}
          title="Numero di thread di copia di robocopy (/MT). Vuoto = usa il default di questa macchina."
          value={draft.threads ?? ""}
          oninput={(e) => (draft.threads = numberOrNull(e.currentTarget.value))}
        />
        <p class="mt-0.5 text-[11px] text-slate-500">
          Vuoto = {DEFAULT_THREADS ?? "…"} (i core logici di questa macchina). Il valore migliore
          dipende dalla destinazione: una condivisione di rete spesso peggiora con più thread, un
          disco locale ne beneficia — verifica empiricamente piuttosto che indovinare.
          {#if draft.backup_type}
            <strong>Non ha effetto con Tipo di backup impostato</strong>: la pipeline a generazioni
            usa il motore di copia semplice, che non è multi-thread.
          {/if}
        </p>
      </div>

      <label for="f-retries">Tentativi</label>
      <div>
        <input
          id="f-retries"
          type="number"
          class="w-32 rounded border border-slate-300 px-2 py-1 dark:border-slate-700 dark:bg-slate-900"
          placeholder="3"
          title="Numero di tentativi per file non riuscito (robocopy /R)."
          value={draft.retries ?? ""}
          oninput={(e) => (draft.retries = numberOrNull(e.currentTarget.value))}
        />
        <p class="mt-0.5 text-[11px] text-slate-500">
          Vuoto = 3. Alza per destinazioni di rete instabili, abbassa per fallire più in fretta su
          un errore reale e non transitorio.
        </p>
      </div>

      <label for="f-excl-files">Escludi file</label>
      <div>
        <input
          id="f-excl-files"
          class="rounded border border-slate-300 px-2 py-1 font-mono dark:border-slate-700 dark:bg-slate-900"
          placeholder="*.tmp, *.log"
          value={draft.exclude_files.join(", ")}
          oninput={(e) => (draft.exclude_files = toList(e.currentTarget.value))}
        />
        <div class="mt-1 flex flex-wrap gap-1">
          {#each EXCLUDE_FILE_SHORTCUTS as pattern}
            <button
              type="button"
              class="rounded border border-slate-300 px-1.5 py-0.5 font-mono text-[11px]
                     text-slate-600 hover:bg-slate-100 dark:border-slate-700 dark:text-slate-400
                     dark:hover:bg-slate-800"
              onclick={() => addExcludeFile(pattern)}
            >+ {pattern}</button>
          {/each}
        </div>
      </div>

      <label for="f-excl-dirs">Escludi cartelle</label>
      <input
        id="f-excl-dirs"
        class="rounded border border-slate-300 px-2 py-1 font-mono dark:border-slate-700 dark:bg-slate-900"
        placeholder="node_modules, .git"
        value={draft.exclude_dirs.join(", ")}
        oninput={(e) => (draft.exclude_dirs = toList(e.currentTarget.value))}
      />

      <label for="f-report">Report</label>
      <div>
        <input
          id="f-report"
          class="w-full rounded border border-slate-300 px-2 py-1 font-mono dark:border-slate-700 dark:bg-slate-900"
          placeholder={DEFAULT_REPORT_PATH_PLACEHOLDER}
          value={draft.report_path ?? ""}
          oninput={(e) => (draft.report_path = e.currentTarget.value.trim() === "" ? null : e.currentTarget.value)}
        />
        <p class="mt-0.5 text-[11px] text-slate-500">
          Vuoto = usa questo default, risolto rispetto alla cartella del file di configurazione.
        </p>
      </div>

      <label for="f-backup-type">Tipo di backup</label>
      <div>
        <!-- F70: incompatibile con Mirror (`Args::validate()`, e ora anche `job_editor::apply_draft`
             -- il core rifiuterebbe comunque la combinazione, questa disabilitazione evita solo che
             l'editor la offra). Nessuna opzione qui non è "cancella backup_type": il valore
             `null`/stringa vuota è la stessa copia semplice pre-F34, la scelta di sempre. -->
        <select
          id="f-backup-type"
          class="w-48 rounded border border-slate-300 px-2 py-1 disabled:bg-slate-100
                 disabled:text-slate-500 dark:border-slate-700 dark:bg-slate-900
                 dark:disabled:bg-slate-800"
          title="Full copia tutto in una nuova generazione; Incremental copia solo ciò che è cambiato dall'ultima generazione; Differential copia ciò che è cambiato dall'ultimo Full. Nessuno = copia semplice, senza generazioni."
          value={draft.backup_type ?? ""}
          disabled={draft.mirror}
          onchange={(e) => (draft.backup_type = e.currentTarget.value === "" ? null : e.currentTarget.value)}
        >
          <option value="">Nessuno (copia semplice)</option>
          <option value="full">Full</option>
          <option value="incremental">Incremental</option>
          <option value="differential">Differential</option>
        </select>
        <p class="mt-0.5 text-[11px] text-slate-500">
          <strong>Full</strong> copia tutto in una nuova generazione; <strong>Incremental</strong>
          copia solo ciò che è cambiato dall'ultima generazione (di qualunque tipo);
          <strong>Differential</strong> copia ciò che è cambiato dall'ultimo Full, sempre rispetto
          allo stesso riferimento. <strong>Nessuno</strong> = copia semplice, senza generazioni né
          manifest.
        </p>
        {#if draft.mirror}
          <p class="mt-0.5 text-[11px] text-slate-500">
            Non selezionabile insieme a Mirror: le due destinazioni sono incompatibili (copia
            speculare 1:1 contro manifest e sottocartelle per generazione).
          </p>
        {/if}
      </div>

      <label for="f-encrypt">Cifratura</label>
      <div>
        <!-- F80: only the `keyring:NOME` form is editable here -- a literal key or an `env:`/`file:`
             reference is shown read-only instead. Accepting free text would let this form write a
             secret in clear text into the TOML, defeating the entire point of F56's keyring form. -->
        {#if encryptKeyringForm}
          <div class="flex items-center gap-1">
            <span class="text-xs text-slate-500">keyring:</span>
            <input
              id="f-encrypt"
              class="w-48 rounded border border-slate-300 px-2 py-1 font-mono disabled:bg-slate-100
                     disabled:text-slate-500 dark:border-slate-700 dark:bg-slate-900
                     dark:disabled:bg-slate-800"
              placeholder="nome credenziale"
              value={encryptCredentialName}
              disabled={!!draft.backup_type}
              oninput={(e) => setEncryptCredential(e.currentTarget.value)}
            />
          </div>
          {#if draft.backup_type}
            <p class="mt-0.5 text-[11px] text-slate-500">
              Non selezionabile insieme a Tipo di backup: la pipeline a generazioni non cifra
              ancora il proprio output (`Args::validate()` rifiuterebbe comunque la combinazione).
            </p>
          {:else}
            <p class="mt-0.5 text-[11px] text-slate-500">
              Nome di una credenziale salvata in Impostazioni → Gestione credenziali. Vuoto =
              nessuna cifratura per questo job.
            </p>
          {/if}
        {:else}
          <p
            id="f-encrypt"
            class="rounded border border-slate-300 bg-slate-50 px-2 py-1 font-mono text-xs text-slate-600 dark:border-slate-700 dark:bg-slate-900 dark:text-slate-400"
          >
            {draft.encrypt_aes256}
          </p>
          <p class="mt-0.5 text-[11px] text-slate-500">
            Valore non in forma <code>keyring:NOME</code> — non modificabile qui, resta invariato
            nella proposta. Modificalo a mano nel file di configurazione.
          </p>
        {/if}
      </div>
    </div>

    <!-- F77: tooltips reuse existing, already-verified text -- `Help.svelte`'s "verifica rapida"
         entry verbatim for fast_verify (same discipline F71 already established toward
         `Run.svelte`), and `cli.rs`'s own doc comments for the rest, which have no entry in Aiuto
         today. Neither is new copy invented here. -->
    <div class="mt-3 flex flex-wrap gap-x-5 gap-y-2 text-xs">
      <label
        class="flex items-center gap-1"
        title={draft.backup_type
          ? "Dopo il trasferimento, confronta i checksum di sorgente e destinazione. Non ha effetto con Tipo di backup impostato: la pipeline a generazioni non esegue ancora questa verifica."
          : "Dopo il trasferimento, confronta i checksum di sorgente e destinazione."}
      >
        <input type="checkbox" bind:checked={draft.verify_integrity} /> Verifica integrità
      </label>
      <label
        class="flex items-center gap-1"
        title="Salta i file la cui sorgente è immutata dall'ultima verifica riuscita. Si fida dell'identità della sorgente invece di rileggere i byte in destinazione: una corruzione nata in destinazione può sfuggire."
      >
        <input type="checkbox" bind:checked={draft.fast_verify} /> Verifica rapida
      </label>
      <label
        class="flex items-center gap-1"
        title="Mostra cosa succederebbe senza copiare nulla (robocopy /L)."
      >
        <input type="checkbox" bind:checked={draft.dry_run} /> Simulazione
      </label>
      <label
        class="flex items-center gap-1"
        title="Esclude giunzioni e cartelle collegate dalla copia (robocopy /XJ). Senza, robocopy le segue per default, che può duplicare dati o ciclare su una giunzione che punta a se stessa."
      >
        <input type="checkbox" bind:checked={draft.exclude_junctions} /> Escludi giunzioni
      </label>
      <label
        class="flex items-center gap-1"
        title="Conserva i permessi di sicurezza ACL NTFS (robocopy /COPYALL)."
      >
        <input type="checkbox" bind:checked={draft.preserve_acl} /> Conserva ACL
      </label>
    </div>
    {#if draft.backup_type && draft.verify_integrity}
      <!-- F77 fix (CodeRabbit, Major): execute_generation_backup never calls verify_integrity --
           an operator ticking this alongside a Tipo di backup could believe the generation backup
           was checksum-verified when it silently was not. The tooltip above states the limit;
           this is the same warning made impossible to miss, since a hover-only tooltip is not
           strong enough for a data-integrity claim that is not actually true. -->
      <p
        class="mt-1 rounded border border-amber-300 bg-amber-50 px-2 py-1 text-[11px]
               text-amber-900 dark:border-amber-800 dark:bg-amber-950 dark:text-amber-200"
        role="status"
      >
        Verifica integrità è impostata insieme a Tipo di backup, ma non ha effetto: la pipeline a
        generazioni non esegue ancora questa verifica. I dati vengono copiati, non verificati.
      </p>
    {/if}

    <h3 class="mt-4 text-xs font-semibold uppercase tracking-wide text-slate-500">
      Impostazioni distruttive
    </h3>
    <div class="mt-1 rounded border border-amber-300 p-2 text-xs dark:border-amber-800">
      <label class="flex items-center gap-2">
        <input type="checkbox" bind:checked={draft.mirror} disabled={mirrorLocked} />
        <span class={mirrorLocked ? "text-slate-400 dark:text-slate-600" : "font-semibold"}>
          Mirror — cancella in destinazione i file assenti nella sorgente
        </span>
      </label>
      <p class="mt-1 text-[11px] text-slate-600 dark:text-slate-400">
        {#if mirrorLocked}
          Un job che cancella non può nascere da qui: va scritto a mano nel file di configurazione.
          L'editor può solo spegnerlo, mai accenderlo.
        {:else}
          Questo job cancella già in destinazione. Puoi disattivarlo; l'editor non lo riaccenderebbe.
        {/if}
      </p>

      <!-- F69: the core already permits raising `keep_generations` on a job that has one (verified
           in `job_editor::apply_draft` -- only `(None, Some)` "introduce" and `(Some(from),
           Some(to)) if to < from` "lower" are rejected; keeping more deletes less). What it does
           not catch is emptying the field, which has the same effect as lowering it without ever
           reaching that check -- so this control offers no way to clear it, only to raise it. -->
      {#if keepGenerationsFloor === null}
        <p class="mt-2 text-[11px] text-slate-600 dark:text-slate-400">
          <code>keep_generations</code>: non impostato — la retention si introduce nel file di
          configurazione. L'editor non può introdurla, perché non tenerne alcuna significa
          cancellarle tutte.
        </p>
      {:else}
        <div class="mt-2 flex items-center gap-2 text-[11px] text-slate-600 dark:text-slate-400">
          <label for="f-keep-generations"><code>keep_generations</code></label>
          <input
            id="f-keep-generations"
            type="number"
            min={keepGenerationsFloor}
            step="1"
            class="w-20 rounded border border-slate-300 px-2 py-0.5 dark:border-slate-700 dark:bg-slate-900"
            value={draft.keep_generations}
            oninput={(e) => {
              const raw = e.currentTarget.value.trim();
              const value = Number(raw);
              if (raw !== "" && Number.isInteger(value) && value >= keepGenerationsFloor) {
                draft.keep_generations = value;
              } else {
                // Reject blank / non-numeric / below-floor input rather than accept it and let the
                // core reject the write later -- svuotare il campo avrebbe lo stesso effetto di
                // abbassarlo, che qui non deve mai essere raggiungibile.
                e.currentTarget.value = String(draft.keep_generations);
              }
            }}
          />
          <span>puoi solo alzarlo (minimo {keepGenerationsFloor}, il valore già in uso): tenere
            meno cicli significa cancellarne di più.</span>
        </div>
      {/if}

      <p class="mt-2 text-[11px] text-slate-600 dark:text-slate-400">
        Webhook e comandi pre/post non sono modificabili qui e restano invariati nella proposta.
      </p>
    </div>

    <div class="mt-4 flex items-center gap-2">
      <label class="sr-only" for="f-out">Percorso del file di proposta</label>
      <input
        id="f-out"
        class="flex-1 rounded border border-slate-300 px-2 py-1 text-sm font-mono dark:border-slate-700 dark:bg-slate-900"
        bind:value={outPath}
      />
      <button
        class="rounded border border-slate-300 px-2 py-1 text-sm dark:border-slate-700"
        onclick={pickTarget}
      >Sfoglia…</button>
      <button
        class="rounded bg-blue-600 px-3 py-1 text-sm text-white disabled:opacity-50"
        onclick={writeProposal}
        disabled={saving || stale || outPath.length === 0}
      >
        {saving ? "Scrittura…" : "Scrivi proposta"}
      </button>
    </div>
    <p class="mt-1 text-[11px] text-slate-500">
      Viene sempre creato un file nuovo. Se ne scegli uno esistente la scrittura viene rifiutata:
      la sostituzione della configurazione in uso resta una tua decisione, non un effetto
      collaterale di un salvataggio.
    </p>
  {:else if !error}
    <EmptyState
      title="Scegli un file di configurazione per modificarne i job"
      lines={[
        "Questa è l'unica scheda che scrive, e scrive sempre altrove: produce una proposta in un file nuovo e non tocca la configurazione in uso.",
        "Non può accendere il mirror né introdurre la retention: un job che cancella va scritto a mano nel file. Può spegnere il mirror e alzare (mai abbassare) una retention già impostata, perché ridurre una cancellazione non ha bisogno di cancelli.",
        "Webhook e comandi pre/post non sono modificabili qui e restano invariati nella proposta.",
      ]}
    />
  {/if}
</section>
