<script>
  // A pane that opens on a blank table teaches nothing. Each one says what it is for and what it
  // needs before anything has been chosen — which is the moment an operator is most lost, and the
  // moment the previous version said the least.
  let { title, lines = [] } = $props();
</script>

<div class="mt-6 max-w-2xl rounded border border-dashed border-slate-300 p-4 dark:border-slate-700">
  <p class="text-sm font-semibold">{title}</p>
  {#each lines as line}
    <p class="mt-1 text-xs text-slate-600 dark:text-slate-400">{line}</p>
  {/each}
</div>
