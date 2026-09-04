<script>
  import Jobs from "./Jobs.svelte";
  import Settings from "./Settings.svelte";
  import Editor from "./Editor.svelte";
  import Run from "./Run.svelte";
  import Report from "./Report.svelte";
  import History from "./History.svelte";
  import Help from "./Help.svelte";
  import { session } from "./session.svelte.js";
  import {
    ListChecks,
    Settings as SettingsIcon,
    SquarePen,
    Play,
    FileText,
    Clock,
    CircleQuestionMark,
  } from "@lucide/svelte";

  // One list instead of five near-identical buttons: a sixth pane should not mean copying the
  // same class expression again and getting one of the states wrong.
  const TABS = [
    { id: "jobs", label: "Job", component: Jobs, icon: ListChecks },
    { id: "settings", label: "Impostazioni", component: Settings, icon: SettingsIcon },
    { id: "editor", label: "Modifica", component: Editor, icon: SquarePen },
    { id: "run", label: "Esegui", component: Run, icon: Play },
    { id: "report", label: "Report", component: Report, icon: FileText },
    { id: "history", label: "Storico", component: History, icon: Clock },
    { id: "help", label: "Aiuto", component: Help, icon: CircleQuestionMark },
  ];

  // Livello 3, punto 10 (PIANO_GUI.md §10): moving navigation into the sidebar frees the top
  // strip above each pane, previously occupied by the same static description on every tab.
  // Shown here instead: the file actually loaded (session.configPath is shared across every
  // pane), so the header answers "what am I looking at" rather than repeating "what is this
  // application" on every single click. Falls back to the static description when nothing is
  // loaded yet — an empty header would be a worse first impression than the sentence it replaces.
  const activeFileName = $derived(
    session.configPath ? session.configPath.replace(/^.*[\\/]/, "") : null,
  );
</script>

<div class="flex min-h-screen bg-slate-50 text-slate-900 dark:bg-slate-950 dark:text-slate-100">
  <aside
    class="flex w-52 shrink-0 flex-col border-r border-slate-200 dark:border-slate-800"
    aria-label="Sezioni"
  >
    <div class="border-b border-slate-200 px-3 py-3 dark:border-slate-800">
      <h1 class="text-sm font-semibold tracking-tight">rustcopy</h1>
      <p class="text-[11px] text-slate-500 dark:text-slate-400">console</p>
    </div>
    <nav class="flex flex-1 flex-col gap-0.5 p-2">
      {#each TABS as entry (entry.id)}
        {@const Icon = entry.icon}
        <button
          class="flex items-center gap-2 rounded px-2 py-1.5 text-left text-xs {session.activeTab ===
          entry.id
            ? 'bg-slate-200 font-semibold dark:bg-slate-800'
            : 'text-slate-600 hover:bg-slate-100 dark:text-slate-400 dark:hover:bg-slate-900'}"
          onclick={() => (session.activeTab = entry.id)}
          aria-current={session.activeTab === entry.id ? "page" : undefined}
        >
          <Icon size={15} strokeWidth={2} aria-hidden="true" />
          {entry.label}
        </button>
      {/each}
    </nav>
  </aside>

  <div class="min-w-0 flex-1">
    <header class="border-b border-slate-200 px-4 py-3 dark:border-slate-800">
      {#if activeFileName}
        <p
          class="truncate font-mono text-xs text-slate-600 dark:text-slate-400"
          title={session.configPath}
        >{activeFileName}</p>
      {:else}
        <p class="text-xs text-slate-500 dark:text-slate-400">
          Esegue i job di un file di configurazione avviando la CLI, e ne prepara le modifiche come
          proposte in file nuovi: quello in uso non viene mai toccato.
        </p>
      {/if}
    </header>

    <!-- Every pane stays mounted and inactive ones are hidden, rather than swapping in one
         component. Rendering only the active tab destroys the others: loading a configuration in
         Modifica, checking something under Aiuto and coming back lost every edit, silently. Hiding
         costs one wrapper element and keeps the work.

         Capped at max-w-6xl rather than left to fill the window: on a maximized 1620px-wide
         window the previous unconstrained layout left content pinned to the top-left corner with
         the rest of the window empty gray canvas (measured: ~700x350px of actual content in a
         1620x980 area on the Job tab) — reads as an application that failed to load, not as a
         dense operator dashboard (Livello 1, punto 1, PIANO_GUI.md §10). -->
    <div class="mx-auto max-w-6xl">
      {#each TABS as entry (entry.id)}
        {@const Pane = entry.component}
        <div class:hidden={session.activeTab !== entry.id}>
          <Pane />
        </div>
      {/each}
    </div>
  </div>
</div>
