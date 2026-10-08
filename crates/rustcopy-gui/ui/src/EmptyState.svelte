<script>
  import { Inbox } from "@lucide/svelte";

  // A pane that opens on a blank table teaches nothing. Each one says what it is for and what it
  // needs before anything has been chosen — which is the moment an operator is most lost, and the
  // moment the previous version said the least.
  //
  // `icon` (Livello 3, punto 12, PIANO_GUI.md §10): an optional visual anchor beside the title,
  // so the first thing a pane shows is not three lines of prose with nothing to land the eye on.
  // Defaults to a generic inbox rather than requiring every call site to pick one — most empty
  // states really do mean "nothing loaded here yet", and only a few need something more specific.
  let { title, lines = [], icon: Icon = Inbox } = $props();
</script>

<div class="mt-6 max-w-2xl rounded border border-dashed border-slate-300 p-4 dark:border-slate-700">
  <div class="flex items-center gap-2">
    <Icon size={18} strokeWidth={1.75} class="shrink-0 text-slate-400 dark:text-slate-600" aria-hidden="true" />
    <p class="text-sm font-semibold">{title}</p>
  </div>
  {#each lines as line}
    <p class="mt-1 text-xs text-slate-600 dark:text-slate-400">{line}</p>
  {/each}
</div>
