<script>
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import PathBar from "./PathBar.svelte";
  import EmptyState from "./EmptyState.svelte";
  import { session } from "./session.svelte.js";
  import { ChevronUp, ChevronDown } from "@lucide/svelte";

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

  // Mirrors a rule the core owns and enforces (`job_editor`): the editor may narrow risk, never
  // widen it. Disabling the control here is an affordance, not the enforcement — `write_proposal`
  // refuses the same edit whatever this frontend sends.
  const mirrorLocked = $derived(draft ? !draft.mirror : true);
  const nameLocked = $derived(draft ? existingNames.has(draft.name) : true);
  const stale = $derived(loadedFrom !== "" && loadedFrom !== session.configPath);

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

  {#if error}
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
        {/if}
      </div>

      <label for="f-source">Sorgente</label>
      <div class="flex gap-2">
        <input id="f-source" class="flex-1 rounded border border-slate-300 px-2 py-1 font-mono dark:border-slate-700 dark:bg-slate-900" bind:value={draft.source} />
        <button
          type="button"
          class="shrink-0 rounded border border-slate-300 px-2 py-1 text-sm dark:border-slate-700"
          onclick={() => browseFolder("source")}
        >Sfoglia…</button>
      </div>

      <label for="f-dest">Destinazione</label>
      <div class="flex gap-2">
        <input id="f-dest" class="flex-1 rounded border border-slate-300 px-2 py-1 font-mono dark:border-slate-700 dark:bg-slate-900" bind:value={draft.dest} />
        <button
          type="button"
          class="shrink-0 rounded border border-slate-300 px-2 py-1 text-sm dark:border-slate-700"
          onclick={() => browseFolder("dest")}
        >Sfoglia…</button>
      </div>

      <label for="f-pattern">Pattern</label>
      <input
        id="f-pattern"
        class="w-48 rounded border border-slate-300 px-2 py-1 font-mono dark:border-slate-700 dark:bg-slate-900"
        placeholder="*"
        value={draft.pattern ?? ""}
        oninput={(e) => (draft.pattern = e.currentTarget.value.trim() === "" ? null : e.currentTarget.value)}
      />

      <label for="f-threads">Thread</label>
      <!-- 1..=128 is the range the CLI enforces (`IngestError::InvalidThreads`). The bounds here
           are the affordance; `apply_draft` refuses the same values whatever this form sends. -->
      <input
        id="f-threads"
        type="number"
        min="1"
        max="128"
        step="1"
        class="w-32 rounded border border-slate-300 px-2 py-1 dark:border-slate-700 dark:bg-slate-900"
        value={draft.threads ?? ""}
        oninput={(e) => (draft.threads = numberOrNull(e.currentTarget.value))}
      />

      <label for="f-retries">Tentativi</label>
      <input
        id="f-retries"
        type="number"
        class="w-32 rounded border border-slate-300 px-2 py-1 dark:border-slate-700 dark:bg-slate-900"
        value={draft.retries ?? ""}
        oninput={(e) => (draft.retries = numberOrNull(e.currentTarget.value))}
      />

      <label for="f-excl-files">Escludi file</label>
      <input
        id="f-excl-files"
        class="rounded border border-slate-300 px-2 py-1 font-mono dark:border-slate-700 dark:bg-slate-900"
        placeholder="*.tmp, *.log"
        value={draft.exclude_files.join(", ")}
        oninput={(e) => (draft.exclude_files = toList(e.currentTarget.value))}
      />

      <label for="f-excl-dirs">Escludi cartelle</label>
      <input
        id="f-excl-dirs"
        class="rounded border border-slate-300 px-2 py-1 font-mono dark:border-slate-700 dark:bg-slate-900"
        placeholder="node_modules, .git"
        value={draft.exclude_dirs.join(", ")}
        oninput={(e) => (draft.exclude_dirs = toList(e.currentTarget.value))}
      />

      <label for="f-report">Report</label>
      <input
        id="f-report"
        class="rounded border border-slate-300 px-2 py-1 font-mono dark:border-slate-700 dark:bg-slate-900"
        value={draft.report_path ?? ""}
        oninput={(e) => (draft.report_path = e.currentTarget.value.trim() === "" ? null : e.currentTarget.value)}
      />
    </div>

    <div class="mt-3 flex flex-wrap gap-x-5 gap-y-2 text-xs">
      <label class="flex items-center gap-1">
        <input type="checkbox" bind:checked={draft.verify_integrity} /> Verifica integrità
      </label>
      <label class="flex items-center gap-1">
        <input type="checkbox" bind:checked={draft.fast_verify} /> Verifica rapida
      </label>
      <label class="flex items-center gap-1">
        <input type="checkbox" bind:checked={draft.dry_run} /> Simulazione
      </label>
      <label class="flex items-center gap-1">
        <input type="checkbox" bind:checked={draft.exclude_junctions} /> Escludi giunzioni
      </label>
      <label class="flex items-center gap-1">
        <input type="checkbox" bind:checked={draft.preserve_acl} /> Conserva ACL
      </label>
    </div>

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
