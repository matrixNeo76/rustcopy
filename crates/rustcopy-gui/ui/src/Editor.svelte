<script>
  import { invoke } from "@tauri-apps/api/core";

  // The only pane that writes. It never writes in place: it produces a proposal in a new file and
  // the operator decides whether it replaces the running configuration.
  let configPath = $state("");
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

  // The whole draft is loaded, edited in part, and sent back whole. That is deliberate: a field
  // this form does not render still round-trips untouched, so rendering a subset can never drop a
  // setting the file had.
  const draft = $derived(drafts[selected] ?? null);

  // Mirrors a rule the core owns and enforces (`job_editor`): the editor may narrow risk, never
  // widen it. Disabling the control here is an affordance, not the enforcement — `write_proposal`
  // refuses the same edit whatever this frontend sends.
  const mirrorLocked = $derived(draft ? !draft.mirror : true);
  const nameLocked = $derived(draft ? existingNames.has(draft.name) : true);

  async function load() {
    error = null;
    written = null;
    loading = true;
    try {
      drafts = await invoke("read_job_drafts", { configPath });
      existingNames = new Set(drafts.map((entry) => entry.name));
      selected = 0;
      outPath = await invoke("suggest_proposal_path", { configPath });
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

  async function save() {
    error = null;
    written = null;
    saving = true;
    try {
      await invoke("write_proposal", { configPath, drafts, outPath });
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
  <div class="flex gap-2">
    <label class="sr-only" for="editor-config-path">Percorso del file di configurazione TOML</label>
    <input
      id="editor-config-path"
      class="flex-1 rounded border border-slate-300 px-2 py-1 text-sm dark:border-slate-700 dark:bg-slate-900"
      placeholder="Percorso di un file di configurazione TOML"
      bind:value={configPath}
    />
    <button
      class="rounded bg-blue-600 px-3 py-1 text-sm text-white disabled:opacity-50"
      onclick={load}
      disabled={loading || configPath.length === 0}
    >
      {loading ? "Lettura…" : "Apri per modifica"}
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
    </div>

    <div class="mt-3 grid grid-cols-[10rem_1fr] items-center gap-x-3 gap-y-2 text-xs">
      <label for="f-name">Nome</label>
      <div>
        <input
          id="f-name"
          class="w-full rounded border border-slate-300 px-2 py-1 font-mono disabled:bg-slate-100
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
      <input id="f-source" class="rounded border border-slate-300 px-2 py-1 font-mono dark:border-slate-700 dark:bg-slate-900" bind:value={draft.source} />

      <label for="f-dest">Destinazione</label>
      <input id="f-dest" class="rounded border border-slate-300 px-2 py-1 font-mono dark:border-slate-700 dark:bg-slate-900" bind:value={draft.dest} />

      <label for="f-pattern">Pattern</label>
      <input
        id="f-pattern"
        class="rounded border border-slate-300 px-2 py-1 font-mono dark:border-slate-700 dark:bg-slate-900"
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

      <p class="mt-2 text-[11px] text-slate-600 dark:text-slate-400">
        <code>keep_generations</code>: {draft.keep_generations ?? "non impostato"} — la retention si
        modifica nel file di configurazione. L'editor non può introdurla né abbassarla, perché tenere
        meno cicli significa cancellarne di più.
      </p>

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
        class="rounded bg-blue-600 px-3 py-1 text-sm text-white disabled:opacity-50"
        onclick={save}
        disabled={saving || outPath.length === 0}
      >
        {saving ? "Scrittura…" : "Scrivi proposta"}
      </button>
    </div>
    <p class="mt-1 text-[11px] text-slate-500">
      Viene sempre creato un file nuovo. Se esiste già, la scrittura viene rifiutata.
    </p>
  {/if}
</section>
