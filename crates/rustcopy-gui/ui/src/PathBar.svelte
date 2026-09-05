<script>
  import { open } from "@tauri-apps/plugin-dialog";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { Star, X } from "@lucide/svelte";
  import {
    recent,
    remember,
    favorites,
    addFavorite,
    removeFavorite,
    isFavorite,
  } from "./session.svelte.js";

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
  // F66: a separate dropdown from "Recenti" — a named, curated list the operator builds on
  // purpose, not an MRU that evicts itself. Kept as its own panel rather than merged into one
  // list so the two questions ("what did I open recently" vs. "what do I always come back to")
  // stay visually distinct.
  let showFavorites = $state(false);
  // This instance's own element. Every pane stays mounted, so several PathBars carry
  // `kind="config"` at once and a shared attribute selector would let a click inside one
  // pane's dropdown count as "inside" for all of them.
  let root;
  const recents = $derived(showRecent ? recent(kind) : []);
  const favs = $derived(showFavorites ? favorites(kind) : []);
  const alreadyFavorite = $derived(value.length > 0 && isFavorite(kind, value));

  function basename(path) {
    const parts = path.split(/[\\/]/);
    return parts[parts.length - 1] || path;
  }

  function toggleFavorite() {
    if (value.length === 0) return;
    if (alreadyFavorite) {
      removeFavorite(kind, value);
    } else {
      addFavorite(kind, value, basename(value));
      showRecent = false;
    }
  }

  function renameFavorite(path, newLabel) {
    addFavorite(kind, path, newLabel);
  }

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
      showFavorites = false;
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
    showFavorites = false;
    remember(kind, path);
    onrun();
  }

  // Tauri's own window drag-and-drop, not the browser's File API: a dropped File object never
  // carries a real filesystem path (a deliberate browser security restriction) — only this event
  // does. One listener per mounted instance, but a drop only acts on the instance the cursor was
  // actually over: every hidden pane's PathBar has a zero-size rect, which can never contain a
  // drop position, so nothing extra has to ask which tab is active.
  $effect(() => {
    let unlisten;
    let cancelled = false;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type !== "drop" || !root) return;
        const rect = root.getBoundingClientRect();
        if (rect.width === 0 || rect.height === 0) return;
        // Tauri reports the drop in physical pixels; the DOM rect is in CSS pixels.
        const ratio = window.devicePixelRatio || 1;
        const x = event.payload.position.x / ratio;
        const y = event.payload.position.y / ratio;
        if (x < rect.left || x > rect.right || y < rect.top || y > rect.bottom) return;
        const path = event.payload.paths[0];
        const expectedExt = FILTERS[kind]?.[0]?.extensions?.[0];
        // Silently ignored rather than loaded anyway: dropping the wrong kind of file should not
        // populate the field with something that will only fail once "Apri"/"Esamina" is pressed.
        if (!path || (expectedExt && !path.toLowerCase().endsWith(`.${expectedExt}`))) return;
        choose(path);
      })
      .then((fn) => {
        // The effect can be torn down (pane unmounted, though this app never unmounts panes; a
        // future change might) before this promise settles — unlisten immediately rather than
        // leaking a listener nothing will ever clean up.
        if (cancelled) fn();
        else unlisten = fn;
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  });
</script>

<svelte:window
  onclick={(e) => {
    // A dropdown that only closes by re-pressing the button it came from looks stuck. Any click
    // outside dismisses it, which is what every other list on the system does — measured against
    // this instance's own element, not a shared attribute.
    if (showRecent && root && !root.contains(e.target)) showRecent = false;
    if (showFavorites && root && !root.contains(e.target)) showFavorites = false;
  }}
  onkeydown={(e) => {
    // At window level so it works wherever focus is: the input, the button that opened the list,
    // or an entry inside it.
    if (e.key === "Escape") {
      showRecent = false;
      showFavorites = false;
    }
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
      onfocus={() => {
        showRecent = false;
        showFavorites = false;
      }}
      onkeydown={(e) => e.key === "Enter" && run()}
    />
    <button
      class="rounded border border-slate-300 px-1.5 py-1 disabled:opacity-40 dark:border-slate-700"
      onclick={toggleFavorite}
      disabled={value.length === 0}
      title={alreadyFavorite ? "Rimuovi dai preferiti" : "Aggiungi ai preferiti"}
      aria-pressed={alreadyFavorite}
    >
      <Star
        size={15}
        strokeWidth={2}
        fill={alreadyFavorite ? "currentColor" : "none"}
        class={alreadyFavorite ? "text-amber-500" : "text-slate-400"}
        aria-hidden="true"
      />
    </button>
    <button
      class="rounded border border-slate-300 px-2 py-1 text-sm dark:border-slate-700"
      onclick={browse}
      title="Scegli il file"
    >Sfoglia…</button>
    <button
      class="rounded border border-slate-300 px-2 py-1 text-sm disabled:opacity-40 dark:border-slate-700"
      onclick={() => {
        showFavorites = false;
        showRecent = !showRecent;
      }}
      disabled={recent(kind).length === 0}
      title="File aperti di recente"
      aria-expanded={showRecent}
    >Recenti</button>
    <button
      class="rounded border border-slate-300 px-2 py-1 text-sm disabled:opacity-40 dark:border-slate-700"
      onclick={() => {
        showRecent = false;
        showFavorites = !showFavorites;
      }}
      disabled={favorites(kind).length === 0}
      title="Percorsi preferiti"
      aria-expanded={showFavorites}
    >Preferiti</button>
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

  {#if showFavorites && favs.length > 0}
    <ul
      class="absolute right-0 z-10 mt-1 w-full max-w-3xl rounded border border-slate-300 bg-white
             p-1 shadow-lg dark:border-slate-700 dark:bg-slate-900"
    >
      {#each favs as fav (fav.path)}
        <li class="flex items-center gap-1 rounded px-1 py-0.5 hover:bg-slate-100 dark:hover:bg-slate-800">
          <!-- The label is the one editable bit of a favorite — rename by typing here, no
               separate dialog. `addFavorite` already updates a matching path in place rather
               than duplicating it, so this and "aggiungi ai preferiti" share one code path. -->
          <input
            class="w-32 shrink-0 rounded border border-transparent bg-transparent px-1 py-0.5 text-xs
                   font-medium hover:border-slate-300 focus:border-slate-300 focus:bg-white
                   dark:hover:border-slate-700 dark:focus:border-slate-700 dark:focus:bg-slate-950"
            value={fav.label}
            onclick={(e) => e.stopPropagation()}
            onchange={(e) => renameFavorite(fav.path, e.currentTarget.value)}
          />
          <button
            class="min-w-0 flex-1 truncate rounded px-1 py-0.5 text-left font-mono text-[11px] text-slate-600
                   hover:bg-slate-200 dark:text-slate-400 dark:hover:bg-slate-700"
            title={fav.path}
            onclick={() => choose(fav.path)}
          >{fav.path}</button>
          <button
            class="shrink-0 rounded p-0.5 text-slate-400 hover:bg-slate-200 hover:text-slate-700
                   dark:hover:bg-slate-700 dark:hover:text-slate-200"
            title="Rimuovi dai preferiti"
            onclick={() => removeFavorite(kind, fav.path)}
          >
            <X size={13} strokeWidth={2} aria-hidden="true" />
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>
