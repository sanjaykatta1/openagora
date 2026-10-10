// Updating OpenAgora itself from its GitHub releases: check in the background,
// download when the user clicks Update, and install on restart.

import { app, BrowserWindow, ipcMain } from "electron";
import { autoUpdater } from "electron-updater";

export type UpdateState =
  | { kind: "unsupported"; current: string }
  | { kind: "idle"; current: string }
  | { kind: "checking"; current: string }
  | { kind: "available"; current: string; version: string }
  | { kind: "downloading"; current: string; version: string; percent: number }
  | { kind: "ready"; current: string; version: string }
  | { kind: "error"; current: string; message: string };

const CHECK_EVERY_MS = 6 * 60 * 60 * 1000;

/** Call once, when the app is ready; every window hears about updates. */
export function registerUpdater(): void {
  const current = app.getVersion();
  // Development builds have no release to update from.
  let state: UpdateState = app.isPackaged ? { kind: "idle", current } : { kind: "unsupported", current };
  let version = "";
  const set = (next: UpdateState) => {
    state = next;
    for (const window of BrowserWindow.getAllWindows()) window.webContents.send("update-state", state);
  };

  ipcMain.handle("update-state", () => state);
  if (state.kind === "unsupported") return;

  autoUpdater.autoDownload = false;
  autoUpdater.autoInstallOnAppQuit = true;
  autoUpdater.on("checking-for-update", () => {
    // A background re-check shouldn't hide an update that's already found.
    if (state.kind === "idle" || state.kind === "error") set({ kind: "checking", current });
  });
  autoUpdater.on("update-not-available", () => {
    if (state.kind === "checking") set({ kind: "idle", current });
  });
  autoUpdater.on("update-available", (info) => {
    version = info.version;
    if (state.kind !== "downloading" && state.kind !== "ready") set({ kind: "available", current, version });
  });
  autoUpdater.on("download-progress", (p) =>
    set({ kind: "downloading", current, version, percent: Math.round(p.percent) }),
  );
  autoUpdater.on("update-downloaded", (info) => set({ kind: "ready", current, version: info.version }));
  autoUpdater.on("error", (e) => {
    // Only a download the user started is worth interrupting them for; a
    // failed background check (offline, say) just tries again later.
    if (state.kind === "downloading") set({ kind: "error", current, message: e?.message ?? String(e) });
    else if (state.kind === "checking") set({ kind: "idle", current });
  });

  const check = () => {
    if (state.kind === "downloading" || state.kind === "ready") return;
    // Failures arrive through the "error" event as well.
    autoUpdater.checkForUpdates().catch(() => {});
  };
  ipcMain.handle("update-check", check);
  ipcMain.handle("update-download", () => {
    if (state.kind !== "available") return;
    set({ kind: "downloading", current, version, percent: 0 });
    autoUpdater.downloadUpdate().catch(() => {});
  });
  ipcMain.handle("update-install", () => {
    if (state.kind === "ready") autoUpdater.quitAndInstall();
  });

  check();
  setInterval(check, CHECK_EVERY_MS).unref();
}
