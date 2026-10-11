// "Pop out": an app's own interface in a separate window, signed in with the
// same saved session as its tab.

import { BrowserWindow, shell } from "electron";

const windows = new Map<string, BrowserWindow>();

export function popOut(id: string, name: string, url: string, localUrl: RegExp): void {
  const open = windows.get(id);
  if (open && !open.isDestroyed()) {
    open.show();
    open.focus();
    return;
  }
  const window = new BrowserWindow({
    width: 1100,
    height: 760,
    minWidth: 480,
    minHeight: 360,
    title: name,
    backgroundColor: "#0f1115",
    webPreferences: {
      partition: `persist:app-${id}`,
      contextIsolation: true,
      sandbox: true,
      nodeIntegration: false,
    },
  });
  windows.set(id, window);
  window.on("closed", () => windows.delete(id));
  // Keep the app's name as the title; many apps set an unhelpful page title.
  window.on("page-title-updated", (event) => event.preventDefault());
  // Like a tab, the window only shows the app; other sites open in the browser.
  window.webContents.on("will-navigate", (event, target) => {
    if (localUrl.test(target)) return;
    event.preventDefault();
    if (target.startsWith("https://")) void shell.openExternal(target);
  });
  void window.loadURL(url);
}

export function closePopOut(id: string): void {
  windows.get(id)?.close();
}
