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
  // Owned here rather than as App.svelte's own local state so a pane can navigate to another one
  // — "Apri il report di questa run" in Esegui needs to switch to Report with reportPath already
  // set, and App.svelte is the only place that otherwise ever reads or writes which tab is active
  // (Livello 1, punto 5, PIANO_GUI.md §10).
  activeTab: "jobs",
  // One-shot: set together with reportPath by "Apri il report di questa run", consumed by
  // Report.svelte's own effect the moment it fires. Never left `true` — a signal that could stay
  // set would re-trigger a load the next time something unrelated touched reportPath.
  pendingReportLoad: false,
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

// F66: named favorites, a superset of "Recenti" and not its replacement — an unlabeled MRU of 8
// is uncomfortable once an operator manages more than two or three recurring destinations, which
// is exactly the case scripts/profiles.json exists to work around at the PowerShell layer
// (PIANO_GUI.md §12.1). This stays a label on a path already accepted by Job/Impostazioni/Report —
// never a second configuration format: no fields beyond `label`/`path` are stored here, and
// nothing here is read by the core or by `[[jobs]]` TOML.
const FAVORITE_LIMIT = 20;
const FAVORITE_KEYS = {
  config: "rustcopy.favorites.config",
  report: "rustcopy.favorites.report",
};

function readFavorites(key) {
  try {
    const raw = localStorage.getItem(key);
    const parsed = raw ? JSON.parse(raw) : [];
    return Array.isArray(parsed)
      ? parsed.filter(
          (entry) =>
            entry && typeof entry.path === "string" && typeof entry.label === "string",
        )
      : [];
  } catch {
    return [];
  }
}

function writeFavorites(key, values) {
  try {
    localStorage.setItem(key, JSON.stringify(values.slice(0, FAVORITE_LIMIT)));
  } catch {
    // Same tolerance as `write()` above: a lost favorite is a convenience gone, not work lost.
  }
}

const favoriteLists = $state({
  config: readFavorites(FAVORITE_KEYS.config),
  report: readFavorites(FAVORITE_KEYS.report),
});

export function favorites(kind) {
  return favoriteLists[kind] ?? [];
}

export function isFavorite(kind, path) {
  const key = kind in FAVORITE_KEYS ? kind : "config";
  return (favoriteLists[key] ?? []).some((entry) => entry.path === path);
}

// Adding an already-favorited path updates its label in place rather than duplicating the entry —
// re-labeling should never leave the old label behind as a second row for the same path.
export function addFavorite(kind, path, label) {
  if (!path) return;
  const key = kind in FAVORITE_KEYS ? kind : "config";
  const trimmed = label.trim() || path;
  const kept = (favoriteLists[key] ?? []).filter((entry) => entry.path !== path);
  favoriteLists[key] = [{ label: trimmed, path }, ...kept].slice(0, FAVORITE_LIMIT);
  writeFavorites(FAVORITE_KEYS[key], favoriteLists[key]);
}

export function removeFavorite(kind, path) {
  const key = kind in FAVORITE_KEYS ? kind : "config";
  favoriteLists[key] = (favoriteLists[key] ?? []).filter((entry) => entry.path !== path);
  writeFavorites(FAVORITE_KEYS[key], favoriteLists[key]);
}
