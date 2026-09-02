// What the operator has chosen in this session, shared by every pane.
//
// Each pane used to own its own path box, so opening the same configuration in Job, Impostazioni
// and Modifica meant pasting the same absolute path three times. That was not a small annoyance:
// it made the application unusable to anyone who did not already know the exact path by heart,
// which is nobody.
//
// A mutated object rather than reassigned exports, because Svelte 5 module state is shared by
// reference: `session.configPath = x` reaches every pane, `session = {...}` would not.
export const session = $state({
  configPath: "",
  reportPath: "",
  jobName: "",
});

const RECENT_LIMIT = 8;
const KEYS = { config: "rustcopy.recent.config", report: "rustcopy.recent.report" };

// localStorage is per-viewer convenience and nothing more. Every read of it is wrapped: a webview
// with site data blocked throws on access rather than returning empty, and a console that cannot
// open because it could not read a list of recent files would be a worse failure than having no
// list at all.
function read(key) {
  try {
    const raw = localStorage.getItem(key);
    const parsed = raw ? JSON.parse(raw) : [];
    return Array.isArray(parsed) ? parsed.filter((entry) => typeof entry === "string") : [];
  } catch {
    return [];
  }
}

function write(key, values) {
  try {
    localStorage.setItem(key, JSON.stringify(values.slice(0, RECENT_LIMIT)));
  } catch {
    // Not worth surfacing: the operator loses a convenience, not any work.
  }
}

// Reactive, not read from localStorage at render time: a plain function call in a template is not
// a dependency, so the "Recenti" button stayed disabled after the first file was opened and only
// woke up when something else re-rendered the pane. Measured on the running application.
const lists = $state({ config: read(KEYS.config), report: read(KEYS.report) });

export function recent(kind) {
  return lists[kind] ?? [];
}

export function remember(kind, path) {
  if (!path) return;
  const key = kind in KEYS ? kind : "config";
  const kept = (lists[key] ?? []).filter((entry) => entry !== path);
  lists[key] = [path, ...kept].slice(0, RECENT_LIMIT);
  write(KEYS[key], lists[key]);
}
