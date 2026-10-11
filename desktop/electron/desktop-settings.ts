// OpenAgora's own settings on this computer: Sunday's shortcut, and whether
// OpenAgora keeps running in the menu bar or tray after its window is closed.

import fs from "node:fs";
import path from "node:path";

export interface DesktopSettings {
  version: 1;
  /** An Electron accelerator, e.g. "CommandOrControl+Shift+Space". */
  shortcut: string;
  keepRunning: boolean;
}

export const DEFAULT_SHORTCUT = "CommandOrControl+Shift+Space";
export const DEFAULT_DESKTOP_SETTINGS: DesktopSettings = { version: 1, shortcut: DEFAULT_SHORTCUT, keepRunning: true };

const FILE = "desktop-settings.json";
const MODIFIERS = new Set(["Command", "Cmd", "Control", "Ctrl", "CommandOrControl", "CmdOrCtrl", "Alt", "Option", "AltGr", "Shift", "Super", "Meta"]);
const KEYS = /^(?:[A-Z0-9]|F(?:[1-9]|1[0-9]|2[0-4])|Space|Tab|Backspace|Delete|Insert|Return|Enter|Up|Down|Left|Right|Home|End|PageUp|PageDown|Escape|Esc|Plus|[`\-=[\];',./\\])$/;

/** A shortcut needs at least one modifier and exactly one key. */
export function validShortcut(value: unknown): value is string {
  if (typeof value !== "string" || value.length > 64) return false;
  const parts = value.split("+");
  const key = parts.pop() ?? "";
  return parts.length > 0 && parts.every((p) => MODIFIERS.has(p)) && new Set(parts).size === parts.length && KEYS.test(key);
}

/** Reads saved settings; anything unreadable falls back to the defaults. */
export function parseDesktopSettings(value: unknown): DesktopSettings {
  const saved = value && typeof value === "object" ? value as Record<string, unknown> : {};
  return {
    version: 1,
    shortcut: validShortcut(saved.shortcut) ? saved.shortcut : DEFAULT_SHORTCUT,
    keepRunning: typeof saved.keepRunning === "boolean" ? saved.keepRunning : true,
  };
}

export function readDesktopSettings(directory: string): DesktopSettings {
  try {
    return parseDesktopSettings(JSON.parse(fs.readFileSync(path.join(directory, FILE), "utf8")));
  } catch {
    return { ...DEFAULT_DESKTOP_SETTINGS };
  }
}

/** Saves settings after checking them. */
export function saveDesktopSettings(directory: string, value: unknown): DesktopSettings {
  const given = value && typeof value === "object" ? value as Record<string, unknown> : null;
  if (!given || given.version !== 1 || !validShortcut(given.shortcut) || typeof given.keepRunning !== "boolean") {
    throw new Error("Invalid settings");
  }
  const settings: DesktopSettings = { version: 1, shortcut: given.shortcut, keepRunning: given.keepRunning };
  fs.mkdirSync(directory, { recursive: true });
  const file = path.join(directory, FILE);
  const temporary = `${file}.tmp`;
  try {
    fs.writeFileSync(temporary, JSON.stringify(settings) + "\n", { mode: 0o600 });
    fs.renameSync(temporary, file);
  } finally {
    fs.rmSync(temporary, { force: true });
  }
  return settings;
}
