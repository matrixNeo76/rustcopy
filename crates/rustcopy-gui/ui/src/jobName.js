// F72: mirrors `validate_job_name` (`lib.rs`) -- `namespaced_path` interpolates a job name
// literally into a filename, so a Windows reserved character or reserved device name would
// otherwise surface only as a cryptic I/O error hours later, at the job's first scheduled run.
// The core (`apply_draft`) is the real enforcement; this is only the immediate affordance.
//
// Shared between Editor.svelte (F72, an existing job's rename-locked field still needs this for
// job) and any other pane that lets an operator type a job name -- keeping it in one module is
// the whole point after F72's own history: two independently hand-maintained copies (Rust, JS)
// already went out of sync once (CodeRabbit found the gap on that PR). A third copy pasted into a
// new file would reintroduce exactly that risk.
const WINDOWS_RESERVED_FILENAME_CHARS = ["\\", "/", ":", "*", "?", '"', "<", ">", "|"];

// Legacy superscript-digit forms (COM¹/COM²/COM³/LPT¹/LPT²/LPT³) are
// reserved identically to the plain-digit ones -- confirmed against Microsoft's own docs, added
// after CodeRabbit found the omission on the PR that introduced this list.
const WINDOWS_RESERVED_DEVICE_NAMES = new Set([
  "CON", "PRN", "AUX", "NUL",
  "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
  "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
  "COM¹", "COM²", "COM³", "LPT¹", "LPT²", "LPT³",
]);

// Windows also forbids the C0 control range (U+0001-U+001F) in filenames -- same CodeRabbit
// finding as the superscript variants above. Written as an escaped range, not literal control
// bytes, so the source file itself stays plain, readable text.
const WINDOWS_CONTROL_CHAR_PATTERN = /[\u0001-\u001f]/;

export function invalidNameReason(name) {
  const bad = WINDOWS_RESERVED_FILENAME_CHARS.find((c) => name.includes(c));
  if (bad) return `Non può contenere '${bad}' (riservato in un nome di file Windows).`;
  if (WINDOWS_CONTROL_CHAR_PATTERN.test(name)) {
    return "Non può contenere caratteri di controllo (riservati in un nome di file Windows).";
  }
  if (WINDOWS_RESERVED_DEVICE_NAMES.has(name.toUpperCase())) {
    return "È un nome di dispositivo riservato da Windows.";
  }
  // F83 (CodeRabbit finding, verified empirically against real NTFS before fixing): mirrors
  // `validate_job_name`'s own trailing '.'/' ' check -- Windows silently strips a single trailing
  // dot/space when the file is created, so two names differing only by one would land on the same
  // file. A form like "NUL.txt" (a device name *with* an extension) is deliberately NOT rejected
  // here -- see the Rust function's own comment for why: `namespaced_path` never places this name
  // as a filename's own leading segment, so the extra risk that check would guard against does
  // not exist in how this codebase actually uses a job name.
  if (name.endsWith(".") || name.endsWith(" ")) {
    return "Non può terminare con '.' o uno spazio (Windows lo elimina in silenzio, e due nomi diversi finirebbero sullo stesso file).";
  }
  return null;
}
