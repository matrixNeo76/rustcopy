// Minimal CSV building and download: RFC 4180 quoting, nothing more. Two call sites (Report,
// Storico) don't justify a library.

function escapeField(value) {
  const s = String(value ?? "");
  return /[",\r\n]/.test(s) ? `"${s.replaceAll('"', '""')}"` : s;
}

export function toCsv(headers, rows) {
  // CRLF, per RFC 4180 — and what a spreadsheet expects without a BOM dance to also get right.
  return [headers, ...rows].map((row) => row.map(escapeField).join(",")).join("\r\n");
}

// A leading UTF-8 BOM so a spreadsheet opens accented content ("è") without mangling it. Written
// as the escape, not the literal character, so it survives a non-UTF-8 save of this source file
// intact instead of silently degrading to something else.
const UTF8_BOM = "\uFEFF";

export function downloadCsv(filename, csv) {
  const blob = new Blob([UTF8_BOM + csv], { type: "text/csv;charset=utf-8;" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}
