// Settings → General: OpenAgora's own settings on this computer. Sunday's
// shortcut is recorded by pressing it.
import { useState } from "react";
import { bridge, type DesktopState } from "./api";
import { SwitchRow, Row, useAction, useEngine } from "./ui";

const MODIFIER_KEYS = new Set(["Meta", "Control", "Alt", "Shift"]);
const NAMED_KEYS: Record<string, string> = {
  Space: "Space", Tab: "Tab", Backspace: "Backspace", Delete: "Delete", Insert: "Insert", Enter: "Return",
  ArrowUp: "Up", ArrowDown: "Down", ArrowLeft: "Left", ArrowRight: "Right", Home: "Home", End: "End",
  PageUp: "PageUp", PageDown: "PageDown", Backquote: "`", Minus: "-", Equal: "=", BracketLeft: "[",
  BracketRight: "]", Semicolon: ";", Quote: "'", Comma: ",", Period: ".", Slash: "/", Backslash: "\\",
};
const MAC_SYMBOLS: Record<string, string> = { Control: "⌃", Alt: "⌥", Shift: "⇧", CommandOrControl: "⌘", Super: "⌘" };

/** The Electron accelerator for a key press, or null until it is one: a
 *  key, with at least one modifier. */
export function accelerator(e: { key: string; code: string; metaKey: boolean; ctrlKey: boolean; altKey: boolean; shiftKey: boolean }, mac: boolean): string | null {
  if (MODIFIER_KEYS.has(e.key)) return null;
  const key = /^Key[A-Z]$/.test(e.code) ? e.code.slice(3)
    : /^Digit\d$/.test(e.code) ? e.code.slice(5)
    : /^F([1-9]|1\d|2[0-4])$/.test(e.code) ? e.code
    : NAMED_KEYS[e.code];
  const modifiers = [
    (mac ? e.metaKey : e.ctrlKey) && "CommandOrControl",
    mac && e.ctrlKey && "Control",
    !mac && e.metaKey && "Super",
    e.altKey && "Alt",
    e.shiftKey && "Shift",
  ].filter((m): m is string => !!m);
  return key && modifiers.length > 0 ? [...modifiers, key].join("+") : null;
}

/** A shortcut as the system writes it: ⌥⌘K on a Mac, Ctrl+Alt+K elsewhere. */
export function shortcutLabel(value: string, mac: boolean): string {
  const parts = value.split("+").map((p) => (p === "CmdOrCtrl" || p === "Command" || p === "Cmd" ? "CommandOrControl" : p === "Ctrl" ? "Control" : p === "Option" ? "Alt" : p));
  const key = parts.pop() ?? "";
  if (!mac) return [...parts.map((p) => (p === "CommandOrControl" || p === "Control" ? "Ctrl" : p)), key].join("+");
  const order = ["Control", "Alt", "Shift", "CommandOrControl", "Super"];
  return parts.sort((a, b) => order.indexOf(a) - order.indexOf(b)).map((p) => MAC_SYMBOLS[p] ?? p).join("") + key;
}

/** Records a shortcut: focus it and press the keys. Escape stops recording. */
function ShortcutField(props: { value: string; mac: boolean; disabled: boolean; onChange: (value: string) => void }) {
  const [recording, setRecording] = useState(false);
  return (
    <input className="shortcut" readOnly aria-label="Sunday’s shortcut" disabled={props.disabled}
      value={recording ? "" : shortcutLabel(props.value, props.mac)} placeholder="Press keys"
      onFocus={() => setRecording(true)} onBlur={() => setRecording(false)}
      onKeyDown={(e) => {
        if (e.key === "Tab" && !e.metaKey && !e.ctrlKey && !e.altKey) return;
        e.preventDefault();
        if (e.key === "Escape") return e.currentTarget.blur();
        const value = accelerator(e, props.mac);
        if (!value) return;
        e.currentTarget.blur();
        if (value !== props.value) props.onChange(value);
      }} />
  );
}

export function GeneralSettings(props: { sunday: boolean }) {
  const [desktop, reload] = useEngine((b) => b.desktopSettings());
  const [shown, setShown] = useState<DesktopState | null>(null);
  const { run, busy } = useAction(reload);
  const state = shown ?? desktop;
  if (!bridge || !state) return null;
  const mac = state.platform === "darwin";
  /** Saves at once, showing the change while the main process applies it. A
   *  shortcut another app has shows as an error below it. */
  const save = (next: DesktopState, work: () => Promise<DesktopState>) => {
    setShown(next);
    void run(work).finally(() => setShown(null));
  };
  return (
    <ul className="group" aria-label="General">
      {props.sunday && <Row label="Sunday’s shortcut">
        <ShortcutField value={state.settings.shortcut} mac={mac} disabled={busy}
          onChange={(shortcut) => save({ ...state, settings: { ...state.settings, shortcut } }, () => bridge!.saveDesktopSettings({ ...state.settings, shortcut }))} />
      </Row>}
      {props.sunday && state.shortcutError && <li className="row error" role="alert">{state.shortcutError}</li>}
      <SwitchRow label={`Keep running in the ${mac ? "menu bar" : "tray"}`} checked={state.settings.keepRunning} disabled={busy}
        onChange={(keepRunning) => save({ ...state, settings: { ...state.settings, keepRunning } }, () => bridge!.saveDesktopSettings({ ...state.settings, keepRunning }))} />
      <SwitchRow label="Open at login" checked={state.launchAtLogin} disabled={busy}
        onChange={(on) => save({ ...state, launchAtLogin: on }, () => bridge!.setLaunchAtLogin(on))} />
    </ul>
  );
}
