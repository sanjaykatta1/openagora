// OpenAgora desktop: the window, and the bridge from its buttons to the
// `openagora` engine, which does the actual installing and running. Also
// Sunday's shortcut and panel, and the menu bar or tray icon that keeps
// OpenAgora running after its window closes.

import { app, BrowserWindow, clipboard, globalShortcut, ipcMain, Menu, nativeImage, screen, shell, Tray } from "electron";
import { spawn } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { readAgentPreferences, saveAgentPreferences } from "./preferences";
import { registerUpdater } from "./updater";
import { agentAvailability, commandDirectories, launchAgent } from "./agent-launcher";
import { EngineLink, type ConfirmAnswer, type ConfirmRequest, type Picture } from "./engine-link";
import { capture, screenAccess } from "./capture";
import { readDesktopSettings, saveDesktopSettings, type DesktopSettings } from "./desktop-settings";
import { TRAY_ICON } from "./tray-icon";

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
const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

let mainWin: BrowserWindow | null = null;
let panelWin: BrowserWindow | null = null;
let tray: Tray | null = null;
let link: EngineLink | null = null;
let settings: DesktopSettings;
let shortcutError: string | null = null;
let quitting = false;
/** The app in the main window's tab, while one shows. */
let currentTab: string | null = null;

/** The engine's questions waiting for the user's answer, and the window showing each. */
const pendingConfirms = new Map<string, { window: BrowserWindow; request: ConfirmRequest; finish: (answer: ConfirmAnswer) => void }>();
/** How long a question stays up; the engine itself gives up after 300 s. */
const CONFIRM_TIMEOUT_MS = 290_000;

function mainWindow(): Promise<BrowserWindow> {
  if (mainWin && !mainWin.isDestroyed()) return Promise.resolve(mainWin);
  const window = createWindow();
  return new Promise((resolve) => window.webContents.once("did-finish-load", () => resolve(window)));
}

function showMain(): void {
  void mainWindow().then((window) => {
    if (window.isMinimized()) window.restore();
    window.show();
    window.focus();
  });
}

/** Shows the engine's question in one of OpenAgora's own windows (the Sunday
 *  panel while it's open): the user's choice there is the permission. */
async function askInWindow(request: ConfirmRequest): Promise<ConfirmAnswer> {
  const window = panelWin && !panelWin.isDestroyed() && panelWin.isVisible() ? panelWin : await mainWindow();
  if (window.isMinimized()) window.restore();
  window.show();
  window.focus();
  return new Promise((resolve) => {
    const timer = setTimeout(() => finish({ allowed: false }), CONFIRM_TIMEOUT_MS);
    const finish = (answer: ConfirmAnswer) => {
      clearTimeout(timer);
      pendingConfirms.delete(request.id);
      if (!window.isDestroyed()) window.webContents.send("confirm-done", request.id);
      resolve(answer);
    };
    pendingConfirms.set(request.id, { window, request, finish });
    window.webContents.send("confirm-ask", request);
  });
}

/** What the engine needs to know about OpenAgora's own windows: which are
 *  ours, and which app the main window shows. */
function windowReport(): Record<string, unknown> {
  const id = (window: BrowserWindow | null) => (window && !window.isDestroyed() ? window.getMediaSourceId() : null);
  return { pid: process.pid, main: id(mainWin), panel: id(panelWin), tab: currentTab };
}

function reportWindows(): void {
  link?.notify("focus/openagora", windowReport());
}

// --- Sunday's panel -------------------------------------------------------

/** Where the shortcut stands, for the panel. */
interface ShortcutState {
  phase: "looking" | "pressed" | "capturing" | "offline" | "error";
  target?: { kind: string; app: string; key: string } | null;
  verdict?: string;
  rule?: string | null;
  unknown?: string | null;
  item?: { id: string; description: string } | null;
  error?: string | null;
  /** A small copy of the picture Sunday got. */
  picture?: string | null;
}

let shortcutState: ShortcutState | null = null;
let lastPicture: string | null = null;

function sendPanel(state: ShortcutState): void {
  shortcutState = state;
  if (panelWin && !panelWin.isDestroyed()) panelWin.webContents.send("panel-state", state);
}

function createPanel(): BrowserWindow {
  const window = new BrowserWindow({
    width: 440,
    height: 680,
    minWidth: 360,
    minHeight: 420,
    show: false,
    title: "Sunday",
    alwaysOnTop: true,
    skipTaskbar: true,
    minimizable: false,
    maximizable: false,
    fullscreenable: false,
    backgroundColor: "#0f1115",
    webPreferences: {
      preload: path.join(__dirname, "preload.js"),
      contextIsolation: true,
      sandbox: true,
      nodeIntegration: false,
      webviewTag: true,
    },
  });
  if (process.platform === "darwin") {
    window.setAlwaysOnTop(true, "floating");
    window.setVisibleOnAllWorkspaces(true, { visibleOnFullScreen: true });
  }
  // Closing only hides it, so the chat stays where it was.
  window.on("close", (event) => {
    if (quitting) return;
    event.preventDefault();
    window.hide();
  });
  window.on("closed", () => { if (panelWin === window) panelWin = null; });
  window.on("show", reportWindows);
  window.on("hide", reportWindows);
  window.loadFile(path.join(__dirname, "..", "renderer", "index.html"), { hash: "panel" });
  return window;
}

function showPanel(state: ShortcutState): void {
  if (!panelWin || panelWin.isDestroyed()) panelWin = createPanel();
  const window = panelWin;
  // Top right of the screen the pointer is on.
  const area = screen.getDisplayNearestPoint(screen.getCursorScreenPoint()).workArea;
  const [width, height] = window.getSize();
  const fitted = Math.min(height, area.height - 48);
  window.setBounds({ x: area.x + area.width - width - 24, y: area.y + 24, width, height: fitted });
  sendPanel(state);
  window.show();
  window.focus();
}

function hidePanel(): void {
  if (panelWin && !panelWin.isDestroyed()) panelWin.hide();
}

/** Sunday's shortcut: find the window the user is in (before the panel opens
 *  over it), then open the panel and, if the app is allowed, capture it. */
async function onShortcut(): Promise<void> {
  if (panelWin && !panelWin.isDestroyed() && panelWin.isVisible() && panelWin.isFocused()) {
    hidePanel();
    return;
  }
  reportWindows();
  let state: ShortcutState;
  if (!link?.connected) {
    state = { phase: "offline" };
  } else {
    try {
      state = { phase: "pressed", ...(await link.request<Omit<ShortcutState, "phase">>("shortcut/pressed", {}, 15_000)) };
    } catch (e) {
      state = { phase: "error", error: message(e) };
    }
  }
  showPanel(state);
  if (state.phase === "pressed" && state.verdict === "allowed") await capturePressed({});
}

/** Captures the window the shortcut was pressed over; `allow` records the
 *  user's choice from the panel first, `pick` asks the system to let the
 *  user pick a window where OpenAgora can't see which one is in front. */
async function capturePressed(params: { allow?: "once" | "always"; pick?: boolean }): Promise<ShortcutState> {
  const before = shortcutState ?? { phase: "pressed" as const };
  sendPanel({ ...before, phase: "capturing", error: null });
  let state: ShortcutState;
  try {
    if (!link?.connected) throw new Error("OpenAgora's engine isn't running");
    lastPicture = null;
    const result = await link.request<Omit<ShortcutState, "phase">>("shortcut/capture", params, 150_000);
    state = { phase: "pressed", ...result, picture: result.item ? lastPicture : null };
  } catch (e) {
    state = { ...before, phase: "error", error: message(e) };
  }
  sendPanel(state);
  return state;
}

/** A small copy of a capture, for the panel to show what Sunday got. */
function thumbnail(picture: Picture): string | null {
  try {
    const image = nativeImage.createFromBuffer(Buffer.from(picture.data, "base64"));
    return image.resize({ width: Math.min(360, picture.width) }).toDataURL();
  } catch {
    return null;
  }
}

// --- The global shortcut and the tray ------------------------------------

function registerShortcut(accelerator: string): void {
  globalShortcut.unregisterAll();
  let registered = false;
  try {
    registered = globalShortcut.register(accelerator, () => void onShortcut());
  } catch {
    registered = false;
  }
  shortcutError = registered ? null : `${accelerator.replace("CommandOrControl", process.platform === "darwin" ? "Command" : "Ctrl")} is taken by another app or by the system. Choose another shortcut in Settings.`;
}

function trayIcon(): Electron.NativeImage {
  const mac = process.platform === "darwin";
  const icon = nativeImage.createFromDataURL(`data:image/png;base64,${mac ? TRAY_ICON.template16 : TRAY_ICON.color16}`);
  icon.addRepresentation({ scaleFactor: 2, dataURL: `data:image/png;base64,${mac ? TRAY_ICON.template32 : TRAY_ICON.color32}` });
  if (mac) icon.setTemplateImage(true);
  return icon;
}

function updateTray(): void {
  if (!settings.keepRunning) {
    tray?.destroy();
    tray = null;
    return;
  }
  try {
    if (!tray) {
      tray = new Tray(trayIcon());
      tray.setToolTip("OpenAgora");
      tray.on("click", () => { if (process.platform !== "darwin") showMain(); });
    }
    tray.setContextMenu(Menu.buildFromTemplate([
      { label: "Open Sunday", accelerator: settings.shortcut, registerAccelerator: false, click: () => void onShortcut() },
      { label: "Show OpenAgora", click: showMain },
      { type: "separator" },
      { label: "Quit OpenAgora", click: () => { quitting = true; app.quit(); } },
    ]));
  } catch (e) {
    // Some Linux desktops have no tray; OpenAgora still runs, and opening it
    // again shows the window.
    console.error(`openagora: no tray icon: ${message(e)}`);
  }
}

// --- The engine service ---------------------------------------------------

function startEngineLink(): EngineLink {
  const engine = new EngineLink(async () => {
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
    capture: async (target, maxEdge) => {
      const picture = await capture(target, maxEdge);
      lastPicture = thumbnail(picture);
      return picture;
    },
    hello: windowReport,
    connected: (connected) => {
      for (const window of [mainWin, panelWin]) {
        if (window && !window.isDestroyed()) window.webContents.send("engine-connected", connected);
      }
    },
  });
  engine.start();
  return engine;
}

/** The largest file the desk takes (the engine checks too). */
const DESK_MAX_BYTES = 20 * 1024 * 1024;
const PICTURE_TYPES: Record<string, string> = { ".png": "image/png", ".jpg": "image/jpeg", ".jpeg": "image/jpeg", ".gif": "image/gif", ".webp": "image/webp" };
const TEXT_TYPES: Record<string, string> = { ".txt": "text/plain", ".md": "text/markdown", ".json": "application/json", ".csv": "text/csv", ".log": "text/plain", ".pdf": "application/pdf" };

async function addToDesk(params: Record<string, unknown>): Promise<unknown> {
  if (!link?.connected) throw new Error("OpenAgora's engine isn't running");
  return link.request("desk/add", params, 30_000);
}

function fromPanelOrMain(event: Electron.IpcMainInvokeEvent): boolean {
  const window = BrowserWindow.fromWebContents(event.sender);
  return !!window && (window === mainWin || window === panelWin) && event.senderFrame === event.sender.mainFrame;
}

/** Plain text for the engine's command line: one line, not an option. */
function argument(value: unknown, what: string): string {
  if (typeof value !== "string" || !value.trim() || value.length > 200 || /[\r\n\0]/.test(value)) throw new Error(`invalid ${what}`);
  return value.trim();
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
  ipcMain.handle("confirm-answer", (event, id: unknown, allowed: unknown, choice: unknown) => {
    if (typeof id !== "string") return;
    const pending = pendingConfirms.get(id);
    // Only the window that showed the question can answer it.
    if (!pending || pending.window.isDestroyed() || event.sender !== pending.window.webContents || event.senderFrame !== event.sender.mainFrame) return;
    const choices = pending.request.choices;
    if (allowed !== true) pending.finish({ allowed: false });
    else if (!choices) pending.finish({ allowed: true });
    else if (typeof choice === "string" && choices.some((c) => c.id === choice)) pending.finish({ allowed: true, choice });
    else pending.finish({ allowed: false });
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

  // What the main window shows: an app's tab, or OpenAgora's own pages.
  ipcMain.on("view-changed", (event, view: unknown) => {
    if (!mainWin || event.sender !== mainWin.webContents) return;
    const v = view && typeof view === "object" ? view as Record<string, unknown> : {};
    currentTab = v.kind === "tab" && typeof v.app === "string" && VALID_ID.test(v.app) ? v.app : null;
    reportWindows();
  });

  // Sunday's panel.
  ipcMain.handle("panel-state", () => shortcutState);
  ipcMain.handle("panel-capture", (event, options: unknown) => {
    if (!fromPanelOrMain(event)) throw new Error("only OpenAgora's windows can capture");
    const o = options && typeof options === "object" ? options as Record<string, unknown> : {};
    const allow = o.allow === "once" || o.allow === "always" ? o.allow : undefined;
    return capturePressed({ ...(allow ? { allow } : {}), ...(o.pick === true ? { pick: true } : {}) });
  });
  ipcMain.handle("panel-hide", () => hidePanel());
  ipcMain.handle("show-main", () => showMain());
  ipcMain.handle("desk-add-clipboard", async (event) => {
    if (!fromPanelOrMain(event)) throw new Error("only OpenAgora's windows can share");
    for (const item of await clipboard.read()) {
      const type = item.types.find((t) => Object.values(PICTURE_TYPES).includes(t));
      if (!type) continue;
      const blob = await item.getType(type);
      if (!(blob instanceof Blob)) continue;
      return addToDesk({ kind: "image", data: Buffer.from(await blob.arrayBuffer()).toString("base64"), source: { app: "Clipboard" } });
    }
    const text = await clipboard.readText();
    if (!text.trim()) throw new Error("The clipboard is empty.");
    const link = /^https?:\/\/\S+$/.test(text.trim());
    return addToDesk({ kind: link ? "link" : "text", text: link ? text.trim() : text, source: { app: "Clipboard" } });
  });
  ipcMain.handle("desk-add-files", async (event, paths: unknown) => {
    if (!fromPanelOrMain(event)) throw new Error("only OpenAgora's windows can share");
    if (!Array.isArray(paths) || paths.length === 0 || paths.length > 10) throw new Error("drop up to 10 files");
    const added = [];
    for (const file of paths) {
      if (typeof file !== "string" || !path.isAbsolute(file)) throw new Error("invalid file");
      const stat = fs.statSync(file);
      if (!stat.isFile()) throw new Error(`${path.basename(file)} isn't a file`);
      if (stat.size > DESK_MAX_BYTES) throw new Error(`${path.basename(file)} is too big for the desk (at most 20 MB)`);
      const extension = path.extname(file).toLowerCase();
      const picture = PICTURE_TYPES[extension];
      added.push(await addToDesk({
        kind: picture ? "image" : "file",
        type: picture ?? TEXT_TYPES[extension] ?? "application/octet-stream",
        data: fs.readFileSync(file).toString("base64"),
        name: path.basename(file),
        source: { app: "Files" },
      }));
    }
    return added;
  });

  // What Sunday can see: OpenAgora's own settings and the engine's broker.
  const desktopState = () => ({ settings, shortcutError, screen: screenAccess(), platform: process.platform });
  ipcMain.handle("desktop-settings", desktopState);
  ipcMain.handle("save-desktop-settings", (_e, value: unknown) => {
    settings = saveDesktopSettings(app.getPath("userData"), value);
    registerShortcut(settings.shortcut);
    updateTray();
    return desktopState();
  });
  ipcMain.handle("open-screen-settings", () => {
    if (process.platform === "darwin") return shell.openExternal("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture");
  });
  ipcMain.handle("broker-state", async () => {
    const [grants, never] = await Promise.all([runEngine(["grants", "list", "--json"]), runEngine(["never", "list", "--json"])]);
    if (!grants.ok || !never.ok) throw new Error(grants.stderr || never.stderr || "could not read what Sunday may see");
    return { grants: JSON.parse(grants.stdout), never: JSON.parse(never.stdout) };
  });
  ipcMain.handle("broker-revoke", (_e, key: unknown) => runEngine(["grants", "revoke", "--", argument(key, "app")]));
  ipcMain.handle("never-add", (_e, rule: unknown) => {
    const r = rule && typeof rule === "object" ? rule as Record<string, unknown> : {};
    const name = argument(r.name, "name");
    const args = ["never", "add"];
    if (r.app !== undefined) args.push(`--app=${argument(r.app, "app")}`);
    if (r.title !== undefined) args.push(`--title=${argument(r.title, "title")}`);
    if (args.length === 2) throw new Error("give an app or words in a window title");
    return runEngine([...args, "--", name]);
  });
  ipcMain.handle("never-remove", (_e, name: unknown) => runEngine(["never", "remove", "--", argument(name, "name")]));
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
  mainWin = window;
  window.on("closed", () => {
    if (mainWin === window) mainWin = null;
    currentTab = null;
    reportWindows();
    // Without the menu bar or tray icon, closing the window ends OpenAgora
    // (on macOS, apps stay in the Dock as usual).
    if (!settings.keepRunning && process.platform !== "darwin") app.quit();
  });
  window.once("ready-to-show", reportWindows);
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

// One OpenAgora at a time: a second launch shows the first one's window.
if (!app.requestSingleInstanceLock()) {
  app.quit();
} else {
  app.on("second-instance", showMain);
  app.whenReady().then(() => {
    settings = readDesktopSettings(app.getPath("userData"));
    registerIpc();
    registerUpdater();
    createWindow();
    link = startEngineLink();
    registerShortcut(settings.shortcut);
    updateTray();
    if (process.env.OPENAGORA_TEST === "1") {
      // UI tests drive what the OS would: the shortcut, and a look at the windows.
      Object.assign(globalThis, {
        openagoraTest: {
          pressShortcut: () => onShortcut(),
          panel: () => panelWin,
          tray: () => tray,
          shortcutRegistered: () => shortcutError === null,
        },
      });
    }
  });
}
app.on("window-all-closed", () => {
  if (!settings?.keepRunning && process.platform !== "darwin") app.quit();
});
app.on("before-quit", () => { quitting = true; });
app.on("will-quit", () => {
  globalShortcut.unregisterAll();
  link?.stop();
});
app.on("activate", showMain);
