// Plain-language outcome of a run (F93). Derived only from fields the core already computed --
// the frontend decides how to *word* an outcome, never whether a backup succeeded: that stays
// `exit_code_is_success` (robocopy's own bitwise rule) and `integrity_status`.
import { bytes, duration } from "./format.js";

/** Variant for a CLI exit code from the run history (AGENTS.md rule 12): only `0` is clean. */
export function cliOutcomeVariant(code) {
  return code === 0 ? "ok" : "attention";
}

/**
 * Headline for a loaded non-dry-run `ReportView` (a dry run keeps its own banner in
 * `Report.svelte`, which also explains why its numbers are not a real transfer).
 * Returns `{ variant, failed, title, reasons }` -- `failed` picks the icon (the hue is amber for
 * anything that is not clean), `reasons` are short sentences shown under the title.
 */
export function reportOutcome(report) {
  const reasons = [];
  const failedFiles = report.copy_detail?.files_failed ?? 0;
  if (failedFiles > 0) reasons.push(`${failedFiles} file non copiati per errore`);
  if (report.integrity_status === "Failed") {
    reasons.push(`la verifica ha trovato ${report.integrity_error_count} differenze`);
  }
  if (report.webhook_error) reasons.push("la notifica webhook non è partita");
  if (report.post_command_error) reasons.push("il comando finale è fallito");

  if (report.copy_error || report.exit_code_is_success === false) {
    return {
      variant: "attention",
      failed: true,
      title: "Backup non riuscito",
      reasons: report.copy_error ? [report.copy_error, ...reasons] : reasons,
    };
  }
  if (report.exit_code_is_success == null) {
    return { variant: "neutral", failed: false, title: "Esito non disponibile", reasons };
  }

  const detail =
    report.files_copied === 0 && report.total_files > 0
      ? "tutto era già aggiornato"
      : `${report.files_copied} file (${bytes(report.bytes_copied)}) in ${duration(report.elapsed_seconds)}`;

  if (reasons.length > 0) {
    return { variant: "attention", failed: false, title: `Backup riuscito con avvisi: ${detail}`, reasons };
  }
  return { variant: "ok", failed: false, title: `Backup riuscito: ${detail}`, reasons };
}
