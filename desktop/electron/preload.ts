// The only surface the window's web page gets: a few named calls to the engine.

import { contextBridge, ipcRenderer, type IpcRendererEvent } from "electron";
import type { AgentPreferences } from "./agents";

contextBridge.exposeInMainWorld("openagora", {
  platform: process.platform,
  agentAvailability: (id: string) => ipcRenderer.invoke("agent-availability", id),
  launchAgent: (id: string, action: string) => ipcRenderer.invoke("launch-agent", id, action),
  agentPreferences: () => ipcRenderer.invoke("agent-preferences"),
  saveAgentPreferences: (value: AgentPreferences) => ipcRenderer.invoke("save-agent-preferences", value),
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
