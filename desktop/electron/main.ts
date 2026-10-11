// OpenAgora desktop: the window, and the bridge from its buttons to the
// `openagora` engine, which does the actual installing and running.

import { app, BrowserWindow, ipcMain, shell } from "electron";
import { spawn } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { readAgentPreferences, saveAgentPreferences } from "./preferences";
import { registerUpdater } from "./updater";
import { agentAvailability, commandDirectories, launchAgent } from "./agent-launcher";
import { EngineLink, type ConfirmRequest } from "./engine-link";

const EXE = process.platform === "win32" ? "openagora.exe" : "openagora";
const LOCAL_URL = /^http:\/\/(127\.0\.0\.1|localhost):\d+(\/|$)/;

/** The engine: bundled with the app, or a dev build, or one on PATH. */
function enginePath(): string {
  const candidates = [
    process.env.OPENAGORA_ENGINE,
    path.join(process.resourcesPath ?? "", "bin", EXE),
    path.join(app.getAppPath(), "..", "engine", "target", "release", EXE),
    path.join(app.getAppPath(), "..", "engine", "target", "debug", EXE),
  ];
  return candidates.find((p): p is string => !!p && fs.existsSync(p)) ?? EXE;
}

/** Apps launched from the Dock or Start menu get a minimal PATH; add the
 *  usual places installers put their commands. */
function engineEnv(): NodeJS.ProcessEnv {
  return { ...process.env, PATH: commandDirectories().join(path.delimiter) };
}

interface EngineResult {
  ok: boolean;
  stdout: string;
  stderr: string;
}

/** OPENAGORA_CATALOG points the app at a local catalog folder (development). */
function catalogArgs(): string[] {
  const dir = process.env.OPENAGORA_CATALOG;
  return dir ? ["--catalog", dir] : [];
}

function runEngine(args: string[], onLine?: (line: string) => void): Promise<EngineResult> {
  return new Promise((resolve) => {
    const child = spawn(enginePath(), [...catalogArgs(), ...args], { env: engineEnv(), stdio: ["ignore", "pipe", "pipe"] });
    let stdout = "";
    let stderr = "";
    const feed = (chunk: Buffer, sink: "out" | "err") => {
      const text = chunk.toString();
      if (sink === "out") stdout += text;
      else stderr += text;
      if (onLine) text.split(/\r?\n/).filter(Boolean).forEach(onLine);
    };
    child.stdout.on("data", (c: Buffer) => feed(c, "out"));
    child.stderr.on("data", (c: Buffer) => feed(c, "err"));
    child.on("error", (e) => resolve({ ok: false, stdout, stderr: `could not run the engine: ${e.message}` }));
    child.on("close", (code) => resolve({ ok: code === 0, stdout, stderr: stderr.replace(/^openagora: /gm, "") }));
  });
}

const VALID_ID = /^[a-z][a-z0-9-]*$/;

/** The engine's questions waiting for the user's answer in the window. */
const pendingConfirms = new Map<string, (allowed: boolean) => void>();
/** How long a question stays up; the engine itself gives up after 300 s. */
const CONFIRM_TIMEOUT_MS = 290_000;

function mainWindow(): Promise<BrowserWindow> {
  const existing = BrowserWindow.getAllWindows()[0];
  if (existing) return Promise.resolve(existing);
  const window = createWindow();
  return new Promise((resolve) => window.webContents.once("did-finish-load", () => resolve(window)));
}

/** Shows the engine's question in OpenAgora's own window: the user's choice there is the permission. */
async function askInWindow(request: ConfirmRequest): Promise<boolean> {
  const window = await mainWindow();
  if (window.isMinimized()) window.restore();
  window.show();
  window.focus();
  return new Promise((resolve) => {
    const timer = setTimeout(() => finish(false), CONFIRM_TIMEOUT_MS);
    const finish = (allowed: boolean) => {
      clearTimeout(timer);
      pendingConfirms.delete(request.id);
      if (!window.isDestroyed()) window.webContents.send("confirm-done", request.id);
      resolve(allowed);
    };
    pendingConfirms.set(request.id, finish);
    window.webContents.send("confirm-ask", request);
  });
}

function startEngineLink(): EngineLink {
  const link = new EngineLink(async () => {
    // Tests point the window at a stand-in for the engine service.
    if (process.env.OPENAGORA_DAEMON_ADDRESS) return process.env.OPENAGORA_DAEMON_ADDRESS;
    const r = await runEngine(["daemon", "start", "--json"]);
    if (!r.ok) throw new Error(r.stderr || "the engine service did not start");
    return JSON.parse(r.stdout).address as string;
  }, {
    confirm: askInWindow,
    openApp: (id) => {
      if (!VALID_ID.test(id)) return;
      void mainWindow().then((window) => { window.show(); window.webContents.send("open-app", id); });
    },
    connected: (connected) => {
      for (const window of BrowserWindow.getAllWindows()) window.webContents.send("engine-connected", connected);
    },
  });
  link.start();
  return link;
}

function registerIpc(): void {
  const checkId = (id: unknown): string => {
    if (typeof id !== "string" || !VALID_ID.test(id)) throw new Error("invalid app id");
    return id;
  };
  ipcMain.handle("catalog", async () => {
    const r = await runEngine(["catalog", "--json"]);
    if (!r.ok) throw new Error(r.stderr || "could not read the catalog");
    return JSON.parse(r.stdout);
  });
  ipcMain.handle("agent-preferences", () => readAgentPreferences(app.getPath("userData")));
  ipcMain.handle("save-agent-preferences", (_e, value: unknown) => saveAgentPreferences(app.getPath("userData"), value));
  ipcMain.handle("agent-availability", (_e, id: unknown) => agentAvailability(id));
  ipcMain.handle("launch-agent", (event, id: unknown, action: unknown) => {
    if (!BrowserWindow.fromWebContents(event.sender) || event.senderFrame !== event.sender.mainFrame) {
      throw new Error("Agent actions are only available in the OpenAgora window");
    }
    return launchAgent(id, action, app.getPath("userData"));
  });
  ipcMain.handle("install", (event, id: unknown) => {
    const appId = checkId(id);
    return runEngine(["install", appId, "--yes"], (line) =>
      !event.sender.isDestroyed() && event.sender.send("progress", { id: appId, line }),
    );
  });
  for (const action of ["uninstall", "start", "stop"] as const) {
    ipcMain.handle(action, (_e, id: unknown) => {
      const args = [action, checkId(id)];
      if (action === "uninstall") args.push("--yes");
      return runEngine(args);
    });
  }
  // An app's own update command; the window asks first.
  ipcMain.handle("update-app", (event, id: unknown) => {
    const appId = checkId(id);
    return runEngine(["update", appId, "--yes"], (line) =>
      !event.sender.isDestroyed() && event.sender.send("progress", { id: appId, line }),
    );
  });
  ipcMain.handle("sunday-status", async () => {
    const r = await runEngine(["sunday", "status", "--json"]);
    if (!r.ok) throw new Error(r.stderr || "could not read Sunday's status");
    return JSON.parse(r.stdout);
  });
  ipcMain.handle("confirm-answer", (event, id: unknown, allowed: unknown) => {
    if (!BrowserWindow.fromWebContents(event.sender) || event.senderFrame !== event.sender.mainFrame) return;
    if (typeof id === "string") pendingConfirms.get(id)?.(allowed === true);
  });
  // The window's own consent card is the confirmation for these.
  ipcMain.handle("sunday-connect", () => runEngine(["sunday", "connect", "--yes"]));
  ipcMain.handle("sunday-disconnect", () => runEngine(["sunday", "disconnect", "--yes"]));
  ipcMain.handle("sunday-setup", (_e, choice: unknown) => {
    // Only these fixed choices cross into the engine.
    const flags: Record<string, string[]> = {
      default: [],
      fresh: ["--fresh"],
      clone: ["--clone"],
      "clone-channels": ["--clone", "--clone-channels"],
      existing: ["--use-existing"],
    };
    if (typeof choice !== "string" || !Object.hasOwn(flags, choice)) throw new Error("invalid setup choice");
    return runEngine(["sunday", "setup", ...flags[choice]]);
  });
  ipcMain.handle("logs", (_e, id: unknown) => runEngine(["logs", checkId(id), "-n", "200"]));
  ipcMain.handle("open-external", (_e, url: unknown) => {
    // Websites, and the user's own apps on this machine ("Open in browser").
    if (typeof url === "string" && (url.startsWith("https://") || LOCAL_URL.test(url))) {
      return shell.openExternal(url);
    }
  });
}

function createWindow(): BrowserWindow {
  const window = new BrowserWindow({
    width: 1240,
    height: 820,
    minWidth: 900,
    minHeight: 600,
    title: "OpenAgora",
    backgroundColor: "#0f1115",
    titleBarStyle: process.platform === "darwin" ? "hiddenInset" : "default",
    webPreferences: {
      preload: path.join(__dirname, "preload.js"),
      contextIsolation: true,
      sandbox: true,
      nodeIntegration: false,
      webviewTag: true,
    },
  });
  window.loadFile(path.join(__dirname, "..", "renderer", "index.html"));
  return window;
}

// Embedded app views may only show local apps, never get Node or our preload,
// and send any link that opens a new window, or leaves this computer, to the
// default browser.
app.on("web-contents-created", (_event, contents) => {
  contents.on("will-attach-webview", (event, webPreferences, params) => {
    delete webPreferences.preload;
    webPreferences.nodeIntegration = false;
    webPreferences.contextIsolation = true;
    if (!LOCAL_URL.test(params.src)) event.preventDefault();
  });
  if (contents.getType() === "webview") {
    contents.on("will-navigate", (event, url) => {
      if (LOCAL_URL.test(url)) return;
      event.preventDefault();
      if (url.startsWith("https://")) void shell.openExternal(url);
    });
  }
  contents.setWindowOpenHandler(({ url }) => {
    if (url.startsWith("https://")) void shell.openExternal(url);
    return { action: "deny" };
  });
});

app.whenReady().then(() => {
  registerIpc();
  registerUpdater();
  createWindow();
  startEngineLink();
});
app.on("window-all-closed", () => {
  if (process.platform !== "darwin") app.quit();
});
app.on("activate", () => {
  if (BrowserWindow.getAllWindows().length === 0) createWindow();
});
