<script>
  // A single headline number with its label (F94, prototype of the visual system in
  // PIANO_GUI.md §22). The value is the thing an operator reads first, so it is set large; the
  // label stays small and the optional hint carries the nuance the number alone would overstate.
  let { label, value, hint = null, tone = "neutral" } = $props();

  const TONE = {
    neutral: "",
    ok: "text-emerald-700 dark:text-emerald-400",
    attention: "text-amber-700 dark:text-amber-400",
  };
</script>

<div class="card">
  <p class="text-xs font-medium uppercase tracking-wide text-slate-500">{label}</p>
  <p class="mt-1 text-2xl font-semibold tabular-nums {TONE[tone] ?? ''}">{value}</p>
  {#if hint}
    <p class="mt-0.5 text-xs text-slate-500">{hint}</p>
  {/if}
</div>
