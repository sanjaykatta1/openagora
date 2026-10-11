// Opening OpenAgora when the user logs in: off unless they turn it on, in
// Settings. macOS and Windows keep login items themselves (Electron's login
// item settings); on Linux it is an XDG autostart entry of OpenAgora's own,
// in ~/.config/autostart. Started that way, OpenAgora opens in the menu bar or
// tray, without its window, so Sunday's shortcut works from the start. The
// system's own list is the truth: a login item the user removed there stays
// removed.

import { app } from "electron";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

/** The argument a login start carries (Windows and Linux). */
export const HIDDEN = "--hidden";

/** OpenAgora's autostart entry on Linux. */
export function autostartFile(env: NodeJS.ProcessEnv = process.env): string {
  const config = env.XDG_CONFIG_HOME && path.isAbsolute(env.XDG_CONFIG_HOME) ? env.XDG_CONFIG_HOME : path.join(os.homedir(), ".config");
  return path.join(config, "autostart", "openagora.desktop");
}

/** What a login start runs: an AppImage itself (it is mounted somewhere new
 *  each time), else this program. */
function program(): string {
  return process.env.APPIMAGE || process.execPath;
}

/** A desktop entry's Exec value: the program quoted as the Desktop Entry
 *  specification says, then the argument. */
export function execLine(file: string): string {
  const quoted = file.replace(/[\\"`$]/g, (c) => `\\\\${c}`);
  return `"${quoted}" ${HIDDEN}`;
}

export function desktopEntry(file: string): string {
  return [
    "[Desktop Entry]",
    "Type=Application",
    "Name=OpenAgora",
    "Comment=Sunday, your personal agent, in the menu bar or tray",
    `Exec=${execLine(file)}`,
    "Terminal=false",
    "X-GNOME-Autostart-enabled=true",
    "",
  ].join("\n");
}

/** Whether OpenAgora opens when the user logs in, as the system has it. */
export function launchAtLogin(): boolean {
  if (process.platform === "linux") return fs.existsSync(autostartFile());
  return app.getLoginItemSettings({ args: [HIDDEN] }).openAtLogin;
}

export function setLaunchAtLogin(on: boolean): void {
  if (process.platform === "linux") {
    const file = autostartFile();
    if (!on) {
      fs.rmSync(file, { force: true });
      return;
    }
    fs.mkdirSync(path.dirname(file), { recursive: true });
    const partial = `${file}.partial`;
    fs.writeFileSync(partial, desktopEntry(program()), { mode: 0o644 });
    fs.renameSync(partial, file);
    return;
  }
  app.setLoginItemSettings({ openAtLogin: on, args: on ? [HIDDEN] : [] });
}

/** An AppImage moves: keep an existing entry pointing at it. */
export function refreshLaunchAtLogin(): void {
  if (process.platform !== "linux" || !process.env.APPIMAGE) return;
  try {
    const file = autostartFile();
    if (fs.existsSync(file) && !fs.readFileSync(file, "utf8").includes(execLine(program()))) setLaunchAtLogin(true);
  } catch {
    // Left as it is; Settings shows what the system has.
  }
}

/** Whether this start came from logging in. (macOS doesn't say, so there
 *  OpenAgora opens its window as usual.) */
export function startedAtLogin(argv: string[] = process.argv): boolean {
  return argv.includes(HIDDEN);
}
