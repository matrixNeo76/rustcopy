// Minimal CSV building and download: RFC 4180 quoting, nothing more. Two call sites (Report,
// Storico) don't justify a library.

// A path in a report is not free text a person typed — it comes from whatever the source or
// destination tree actually contains, which for a backup tool can be a NAS or a shared folder
// neither side fully controls. A file named `=HYPERLINK(...)` would carry that name through the
// scan, into the JSON report, and into this CSV verbatim; RFC 4180 quoting alone does not stop a
// spreadsheet from reading a quoted field that still starts with `=` as a formula.
//
// Best-effort, not a guarantee (OWASP's own CSV-injection page says the same): prefixing with an
// apostrophe is what every major spreadsheet honours for keeping a leading trigger character as
// literal text, but it is not a promise that no application anywhere behaves differently.
const FORMULA_TRIGGERS = ["=", "+", "-", "@", "\t", "\r"];

function neutralizeFormula(s) {
  return FORMULA_TRIGGERS.some((prefix) => s.startsWith(prefix)) ? `'${s}` : s;
}

function escapeField(value) {
  const s = neutralizeFormula(String(value ?? ""));
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
