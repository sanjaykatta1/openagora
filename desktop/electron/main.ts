// OpenAgora desktop: the window, and the bridge from its buttons to the
// `openagora` engine, which does the actual installing and running.

import { app, BrowserWindow, ipcMain, shell } from "electron";
import { spawn } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { registerUpdater } from "./updater";

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
  const extra =
    process.platform === "win32"
      ? []
      : ["/usr/local/bin", "/opt/homebrew/bin", path.join(os.homedir(), ".local", "bin")];
  const PATH = [...extra, process.env.PATH ?? ""].filter(Boolean).join(path.delimiter);
  return { ...process.env, PATH };
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

function registerIpc(window: BrowserWindow): void {
  const checkId = (id: unknown): string => {
    if (typeof id !== "string" || !VALID_ID.test(id)) throw new Error("invalid app id");
    return id;
  };
  ipcMain.handle("catalog", async () => {
    const r = await runEngine(["catalog", "--json"]);
    if (!r.ok) throw new Error(r.stderr || "could not read the catalog");
    return JSON.parse(r.stdout);
  });
  ipcMain.handle("install", (_e, id: unknown) => {
    const appId = checkId(id);
    return runEngine(["install", appId, "--yes"], (line) =>
      window.webContents.send("progress", { id: appId, line }),
    );
  });
  for (const action of ["uninstall", "start", "stop"] as const) {
    ipcMain.handle(action, (_e, id: unknown) => {
      const args = [action, checkId(id)];
      if (action === "uninstall") args.push("--yes");
      return runEngine(args);
    });
  }
  ipcMain.handle("logs", (_e, id: unknown) => runEngine(["logs", checkId(id), "-n", "200"]));
  ipcMain.handle("open-external", (_e, url: unknown) => {
    // Websites, and the user's own apps on this machine ("Open in browser").
    if (typeof url === "string" && (url.startsWith("https://") || LOCAL_URL.test(url))) {
      return shell.openExternal(url);
    }
  });
}

function createWindow(): void {
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
  registerIpc(window);
  window.loadFile(path.join(__dirname, "..", "renderer", "index.html"));
}

// Embedded app views may only show local apps, never get Node or our preload,
// and send any link that opens a new window to the default browser.
app.on("web-contents-created", (_event, contents) => {
  contents.on("will-attach-webview", (event, webPreferences, params) => {
    delete webPreferences.preload;
    webPreferences.nodeIntegration = false;
    webPreferences.contextIsolation = true;
    if (!LOCAL_URL.test(params.src)) event.preventDefault();
  });
  contents.setWindowOpenHandler(({ url }) => {
    if (url.startsWith("https://")) void shell.openExternal(url);
    return { action: "deny" };
  });
});

app.whenReady().then(() => {
  createWindow();
  registerUpdater();
});
app.on("window-all-closed", () => {
  if (process.platform !== "darwin") app.quit();
});
app.on("activate", () => {
  if (BrowserWindow.getAllWindows().length === 0) createWindow();
});
