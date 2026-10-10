// The only surface the window's web page gets: a few named calls to the engine.

import { contextBridge, ipcRenderer, type IpcRendererEvent } from "electron";

contextBridge.exposeInMainWorld("openagora", {
  platform: process.platform,
  catalog: () => ipcRenderer.invoke("catalog"),
  install: (id: string) => ipcRenderer.invoke("install", id),
  uninstall: (id: string) => ipcRenderer.invoke("uninstall", id),
  start: (id: string) => ipcRenderer.invoke("start", id),
  stop: (id: string) => ipcRenderer.invoke("stop", id),
  logs: (id: string) => ipcRenderer.invoke("logs", id),
  openExternal: (url: string) => ipcRenderer.invoke("open-external", url),
  update: {
    state: () => ipcRenderer.invoke("update-state"),
    check: () => ipcRenderer.invoke("update-check"),
    download: () => ipcRenderer.invoke("update-download"),
    install: () => ipcRenderer.invoke("update-install"),
    onState: (callback: (state: unknown) => void) => {
      const handler = (_e: IpcRendererEvent, state: unknown) => callback(state);
      ipcRenderer.on("update-state", handler);
      return () => ipcRenderer.removeListener("update-state", handler);
    },
  },
  onProgress: (callback: (event: { id: string; line: string }) => void) => {
    const handler = (_e: IpcRendererEvent, event: { id: string; line: string }) => callback(event);
    ipcRenderer.on("progress", handler);
    return () => ipcRenderer.removeListener("progress", handler);
  },
});
