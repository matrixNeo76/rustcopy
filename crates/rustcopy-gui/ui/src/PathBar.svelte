<script>
  import { open } from "@tauri-apps/plugin-dialog";
  import { recent, remember } from "./session.svelte.js";

  // One row: a native picker, the chosen path, the recent ones, and the action. Every pane used to
  // hand-roll this and every pane got it slightly differently.
  let {
    value = $bindable(""),
    kind = "config",
    label = "Percorso",
    placeholder = "",
    action = "Apri",
    busy = false,
    onrun = () => {},
  } = $props();

  let showRecent = $state(false);
  // This instance's own element. Every pane stays mounted, so several PathBars carry
  // `kind="config"` at once and a shared attribute selector would let a click inside one
  // pane's dropdown count as "inside" for all of them.
  let root;
  const recents = $derived(showRecent ? recent(kind) : []);

  const FILTERS = {
    config: [{ name: "Configurazione TOML", extensions: ["toml"] }],
    report: [{ name: "Report JSON", extensions: ["json"] }],
  };

  async function browse() {
    // A picker returns a path the operator selected; it reads no file and writes none. Choosing is
    // strictly safer than typing, because a mistyped path is indistinguishable from a missing one.
    const picked = await open({
      multiple: false,
      directory: false,
      filters: FILTERS[kind] ?? FILTERS.config,
    });
    if (typeof picked === "string" && picked.length > 0) {
      value = picked;
      // Closed here too: picking through the file dialog is choosing a path, and leaving the
      // recents list hanging open over the result was a panel with no obvious way to dismiss it.
      showRecent = false;
      remember(kind, picked);
      onrun();
    }
  }

  function run() {
    // `busy` too: the button is disabled while a read is in flight, but the input's Enter
    // handler is not, and neither caller guards re-entry.
    if (busy || value.length === 0) return;
    remember(kind, value);
    onrun();
  }

  function choose(path) {
    value = path;
    showRecent = false;
    remember(kind, path);
    onrun();
  }
</script>

<svelte:window
  onclick={(e) => {
    // A dropdown that only closes by re-pressing the button it came from looks stuck. Any click
    // outside dismisses it, which is what every other list on the system does — measured against
    // this instance's own element, not a shared attribute.
    if (showRecent && root && !root.contains(e.target)) showRecent = false;
  }}
  onkeydown={(e) => {
    // At window level so it works wherever focus is: the input, the button that opened the list,
    // or an entry inside it.
    if (e.key === "Escape") showRecent = false;
  }}
/>

<div class="relative" bind:this={root}>
  <div class="flex gap-2">
    <label class="sr-only" for="pathbar-{kind}">{label}</label>
    <!-- `autocomplete="off"`: the WebView's own saved-values popup opened over this field and
         stayed there, a second dropdown nobody asked for on top of the recents list below. The
         recents list is the feature; the browser's guess at what we typed is not. -->
    <input
      id="pathbar-{kind}"
      class="flex-1 rounded border border-slate-300 px-2 py-1 text-sm dark:border-slate-700 dark:bg-slate-900"
      autocomplete="off"
      spellcheck="false"
      {placeholder}
      bind:value
      onfocus={() => (showRecent = false)}
      onkeydown={(e) => e.key === "Enter" && run()}
    />
    <button
      class="rounded border border-slate-300 px-2 py-1 text-sm dark:border-slate-700"
      onclick={browse}
      title="Scegli il file"
    >Sfoglia…</button>
    <button
      class="rounded border border-slate-300 px-2 py-1 text-sm disabled:opacity-40 dark:border-slate-700"
      onclick={() => (showRecent = !showRecent)}
      disabled={recent(kind).length === 0}
      title="File aperti di recente"
      aria-expanded={showRecent}
    >Recenti</button>
    <button
      class="rounded bg-blue-600 px-3 py-1 text-sm text-white disabled:opacity-50"
      onclick={run}
      disabled={busy || value.length === 0}
    >
      {busy ? "Lettura…" : action}
    </button>
  </div>

  {#if showRecent && recents.length > 0}
    <ul
      class="absolute right-0 z-10 mt-1 w-full max-w-3xl rounded border border-slate-300 bg-white
             p-1 shadow-lg dark:border-slate-700 dark:bg-slate-900"
    >
      {#each recents as path}
        <li>
          <button
            class="w-full truncate rounded px-2 py-1 text-left font-mono text-xs
                   hover:bg-slate-100 dark:hover:bg-slate-800"
            onclick={() => choose(path)}
          >{path}</button>
        </li>
      {/each}
    </ul>
  {/if}
</div>
