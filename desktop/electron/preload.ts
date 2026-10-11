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
  updateApp: (id: string) => ipcRenderer.invoke("update-app", id),
  sundayStatus: () => ipcRenderer.invoke("sunday-status"),
  sundaySetup: (choice: string) => ipcRenderer.invoke("sunday-setup", choice),
  sundayConnect: () => ipcRenderer.invoke("sunday-connect"),
  sundayDisconnect: () => ipcRenderer.invoke("sunday-disconnect"),
  answerConfirm: (id: string, allowed: boolean) => ipcRenderer.invoke("confirm-answer", id, allowed),
  onConfirm: (callback: (request: { id: string; client: string; title: string; message: string }) => void) => {
    const handler = (_e: IpcRendererEvent, request: { id: string; client: string; title: string; message: string }) => callback(request);
    ipcRenderer.on("confirm-ask", handler);
    return () => ipcRenderer.removeListener("confirm-ask", handler);
  },
  onConfirmDone: (callback: (id: string) => void) => {
    const handler = (_e: IpcRendererEvent, id: string) => callback(id);
    ipcRenderer.on("confirm-done", handler);
    return () => ipcRenderer.removeListener("confirm-done", handler);
  },
  onOpenApp: (callback: (id: string) => void) => {
    const handler = (_e: IpcRendererEvent, id: string) => callback(id);
    ipcRenderer.on("open-app", handler);
    return () => ipcRenderer.removeListener("open-app", handler);
  },
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
