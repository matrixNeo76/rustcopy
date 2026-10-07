// Display formatters shared by the panes that show sizes and durations. `Report.svelte` and
// `History.svelte` each carried their own copy; this is the one `outcome.js` needs as well.

export function bytes(value) {
  if (value < 1024) return `${value} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let n = value / 1024;
  let i = 0;
  while (n >= 1024 && i < units.length - 1) {
    n /= 1024;
    i += 1;
  }
  return `${n.toFixed(n < 10 ? 1 : 0)} ${units[i]}`;
}

export function duration(seconds) {
  if (seconds < 10) return `${seconds.toFixed(2)}s`;
  const total = Math.round(seconds);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  return h > 0
    ? `${h}h ${String(m).padStart(2, "0")}m`
    : `${m}m ${String(total % 60).padStart(2, "0")}s`;
}
