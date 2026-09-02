<script>
  import Jobs from "./Jobs.svelte";
  import Settings from "./Settings.svelte";
  import Editor from "./Editor.svelte";
  import Report from "./Report.svelte";
  import History from "./History.svelte";
  import Help from "./Help.svelte";

  let tab = $state("jobs");

  // One list instead of five near-identical buttons: a sixth pane should not mean copying the
  // same class expression again and getting one of the states wrong.
  const TABS = [
    { id: "jobs", label: "Job", component: Jobs },
    { id: "settings", label: "Impostazioni", component: Settings },
    { id: "editor", label: "Modifica", component: Editor },
    { id: "report", label: "Report", component: Report },
    { id: "history", label: "Storico", component: History },
    { id: "help", label: "Aiuto", component: Help },
  ];

</script>

<main class="min-h-screen bg-slate-50 text-slate-900 dark:bg-slate-950 dark:text-slate-100">
  <header class="border-b border-slate-200 px-4 py-3 dark:border-slate-800">
    <h1 class="text-sm font-semibold tracking-tight">rustcopy — console</h1>
    <p class="text-xs text-slate-500 dark:text-slate-400">
      Non esegue backup. L'unica scrittura è una proposta di configurazione in un file nuovo: il
      file in uso non viene mai toccato.
    </p>
    <nav class="mt-2 flex gap-1" aria-label="Sezioni">
      {#each TABS as entry (entry.id)}
        <button
          class="rounded px-2 py-0.5 text-xs {tab === entry.id
            ? 'bg-slate-200 font-semibold dark:bg-slate-800'
            : 'text-slate-500'}"
          onclick={() => (tab = entry.id)}
          aria-current={tab === entry.id ? "page" : undefined}
        >{entry.label}</button>
      {/each}
    </nav>
  </header>

  <!-- Every pane stays mounted and inactive ones are hidden, rather than swapping in one
       component. Rendering only the active tab destroys the others: loading a configuration in
       Modifica, checking something under Aiuto and coming back lost every edit, silently. Hiding
       costs one wrapper element and keeps the work. -->
  {#each TABS as entry (entry.id)}
    {@const Pane = entry.component}
    <div class:hidden={tab !== entry.id}>
      <Pane />
    </div>
  {/each}
</main>
