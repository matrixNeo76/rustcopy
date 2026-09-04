<script>
  import Jobs from "./Jobs.svelte";
  import Settings from "./Settings.svelte";
  import Editor from "./Editor.svelte";
  import Run from "./Run.svelte";
  import Report from "./Report.svelte";
  import History from "./History.svelte";
  import Help from "./Help.svelte";
  import { session } from "./session.svelte.js";

  // One list instead of five near-identical buttons: a sixth pane should not mean copying the
  // same class expression again and getting one of the states wrong.
  const TABS = [
    { id: "jobs", label: "Job", component: Jobs },
    { id: "settings", label: "Impostazioni", component: Settings },
    { id: "editor", label: "Modifica", component: Editor },
    { id: "run", label: "Esegui", component: Run },
    { id: "report", label: "Report", component: Report },
    { id: "history", label: "Storico", component: History },
    { id: "help", label: "Aiuto", component: Help },
  ];

</script>

<main class="min-h-screen bg-slate-50 text-slate-900 dark:bg-slate-950 dark:text-slate-100">
  <header class="border-b border-slate-200 dark:border-slate-800">
    <!-- The border above stays full-width (it anchors the whole window), but the content inside
         it is capped and centered like the panes below — otherwise the title/subtitle/nav would
         still hug the left edge on a wide window even after the panes themselves stopped doing
         it (Livello 1, punto 1, PIANO_GUI.md §10). -->
    <div class="mx-auto max-w-6xl px-4 py-3">
      <h1 class="text-sm font-semibold tracking-tight">rustcopy — console</h1>
      <p class="text-xs text-slate-500 dark:text-slate-400">
        Esegue i job di un file di configurazione avviando la CLI, e ne prepara le modifiche come
        proposte in file nuovi: quello in uso non viene mai toccato.
      </p>
      <nav class="mt-2 flex gap-1" aria-label="Sezioni">
        {#each TABS as entry (entry.id)}
          <button
            class="rounded px-2 py-0.5 text-xs {session.activeTab === entry.id
              ? 'bg-slate-200 font-semibold dark:bg-slate-800'
              : 'text-slate-500'}"
            onclick={() => (session.activeTab = entry.id)}
            aria-current={session.activeTab === entry.id ? "page" : undefined}
          >{entry.label}</button>
        {/each}
      </nav>
    </div>
  </header>

  <!-- Every pane stays mounted and inactive ones are hidden, rather than swapping in one
       component. Rendering only the active tab destroys the others: loading a configuration in
       Modifica, checking something under Aiuto and coming back lost every edit, silently. Hiding
       costs one wrapper element and keeps the work.

       Capped at the same max-w-6xl as the header rather than left to fill the window: on a
       maximized 1620px-wide window the previous unconstrained layout left content pinned to the
       top-left corner with the rest of the window empty gray canvas (measured: ~700x350px of
       actual content in a 1620x980 area on the Job tab) — reads as an application that failed to
       load, not as a dense operator dashboard. -->
  <div class="mx-auto max-w-6xl">
    {#each TABS as entry (entry.id)}
      {@const Pane = entry.component}
      <div class:hidden={session.activeTab !== entry.id}>
        <Pane />
      </div>
    {/each}
  </div>
</main>
