// Tracks whether the settings page has unsaved edits, so that navigation away
// (tab switches in TabBar) can prompt the user before discarding changes.

let dirty = $state(false);

export const settingsGuard = {
  get dirty() {
    return dirty;
  },
  set dirty(value: boolean) {
    dirty = value;
  },
  clear() {
    dirty = false;
  },
};
