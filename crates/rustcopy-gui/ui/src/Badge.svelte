<script>
  // One place for "what a status chip looks like" (F93). Before this, Jobs, Storico and Report
  // each assembled the same emerald/amber classes by hand at 10px. The colour convention is
  // unchanged from F81: emerald only for a clean outcome, amber for anything that needs a look,
  // never red -- a non-zero exit code is not necessarily a failure (4 = copied, verification
  // mismatch), and the icon, not the hue, carries "failed".
  let { variant = "neutral", icon: Icon = null, children } = $props();

  const CLASS = {
    ok: "bg-emerald-100 text-emerald-900 dark:bg-emerald-950 dark:text-emerald-200",
    attention: "bg-amber-100 text-amber-900 dark:bg-amber-950 dark:text-amber-200",
    info: "bg-blue-100 text-blue-900 dark:bg-blue-950 dark:text-blue-200",
    neutral: "bg-slate-100 text-slate-700 dark:bg-slate-800 dark:text-slate-300",
  };
</script>

<span
  class="inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-xs font-semibold {CLASS[variant] ?? CLASS.neutral}"
>
  {#if Icon}
    <Icon size={12} strokeWidth={2.25} aria-hidden="true" />
  {/if}
  {@render children?.()}
</span>
