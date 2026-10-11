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
  answerConfirm: (id: string, allowed: boolean, choice?: string) => ipcRenderer.invoke("confirm-answer", id, allowed, choice),
  onConfirm: (callback: (request: unknown) => void) => {
    const handler = (_e: IpcRendererEvent, request: unknown) => callback(request);
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
  /** App tabs: the main process owns each app's page; this page only says
   *  where it shows one. */
  tabs: {
    open: (id: string, name: string, url: string, pid: number) => ipcRenderer.invoke("tab-open", id, name, url, pid),
    place: (id: string, bounds: { x: number; y: number; width: number; height: number } | null) => ipcRenderer.send("tab-place", id, bounds),
    cover: (covered: boolean) => ipcRenderer.send("tab-cover", covered),
    popOut: (id: string) => ipcRenderer.invoke("tab-pop-out", id),
    bringBack: (id: string) => ipcRenderer.invoke("tab-bring-back", id),
    showWindow: (id: string) => ipcRenderer.invoke("tab-show-window", id),
    reload: (id: string) => ipcRenderer.invoke("tab-reload", id),
    close: (id: string) => ipcRenderer.invoke("tab-close", id),
    url: (id: string) => ipcRenderer.invoke("tab-url", id),
    popped: () => ipcRenderer.invoke("tabs-popped"),
    onEvent: (callback: (event: unknown) => void) => {
      const handler = (_e: IpcRendererEvent, event: unknown) => callback(event);
      ipcRenderer.on("tab-event", handler);
      return () => ipcRenderer.removeListener("tab-event", handler);
    },
  },
  showMain: () => ipcRenderer.invoke("show-main"),
  panel: {
    state: () => ipcRenderer.invoke("panel-state"),
    capture: (options: { allow?: "once" | "always"; key?: string; pick?: boolean }) => ipcRenderer.invoke("panel-capture", options),
    hide: () => ipcRenderer.invoke("panel-hide"),
    onState: (callback: (state: unknown) => void) => {
      const handler = (_e: IpcRendererEvent, state: unknown) => callback(state);
      ipcRenderer.on("panel-state", handler);
      return () => ipcRenderer.removeListener("panel-state", handler);
    },
  },
  desk: {
    addClipboard: () => ipcRenderer.invoke("desk-add-clipboard"),
    /** Files the user dropped on the panel, as their contents: OpenAgora
     *  never reads a file by a path a page gives it. */
    addFiles: (files: { name: string; data: Uint8Array }[]) => ipcRenderer.invoke("desk-add-files", files),
  },
  desktopSettings: () => ipcRenderer.invoke("desktop-settings"),
  saveDesktopSettings: (value: unknown) => ipcRenderer.invoke("save-desktop-settings", value),
  openScreenSettings: () => ipcRenderer.invoke("open-screen-settings"),
  openAccessibilitySettings: () => ipcRenderer.invoke("open-accessibility-settings"),
  codingAgents: {
    state: () => ipcRenderer.invoke("coding-agents"),
    setUp: (id: string) => ipcRenderer.invoke("agent-setup", id),
    remove: (id: string) => ipcRenderer.invoke("agent-remove", id),
    connect: (app: string) => ipcRenderer.invoke("app-connect", app),
    disconnect: (app: string) => ipcRenderer.invoke("app-disconnect", app),
  },
  auditLog: () => ipcRenderer.invoke("audit-log"),
  background: {
    indicator: () => ipcRenderer.invoke("background-indicator"),
    status: () => ipcRenderer.invoke("background-status"),
    turn: (on: boolean) => ipcRenderer.invoke("background-turn", on),
    pause: (minutes: number | null) => ipcRenderer.invoke("background-pause", minutes),
    resume: () => ipcRenderer.invoke("background-resume"),
    clear: () => ipcRenderer.invoke("background-clear"),
    keep: (days: number) => ipcRenderer.invoke("background-keep", days),
    onState: (callback: (state: unknown) => void) => {
      const handler = (_e: IpcRendererEvent, state: unknown) => callback(state);
      ipcRenderer.on("background-state", handler);
      return () => ipcRenderer.removeListener("background-state", handler);
    },
  },
  memory: {
    state: () => ipcRenderer.invoke("memory-state"),
    use: (provider: string) => ipcRenderer.invoke("memory-use", provider),
    off: () => ipcRenderer.invoke("memory-off"),
    digest: (on: boolean) => ipcRenderer.invoke("memory-digest", on),
    revokeSessions: (agent: string) => ipcRenderer.invoke("sessions-revoke", agent),
  },
  broker: {
    state: () => ipcRenderer.invoke("broker-state"),
    revoke: (app: string, scope?: { client?: string; destination?: string }) => ipcRenderer.invoke("broker-revoke", app, scope),
    neverAdd: (rule: { name: string; app?: string; title?: string }) => ipcRenderer.invoke("never-add", rule),
    neverRemove: (name: string) => ipcRenderer.invoke("never-remove", name),
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
