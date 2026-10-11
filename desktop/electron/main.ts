// OpenAgora desktop: the window, and the bridge from its buttons to the
// `openagora` engine, which does the actual installing and running. Also
// Sunday's shortcut and panel, and the menu bar or tray icon that keeps
// OpenAgora running after its window closes.

import { app, BaseWindow, BrowserWindow, clipboard, ClipboardItem, globalShortcut, ipcMain, Menu, nativeImage, screen, session, shell, systemPreferences, Tray } from "electron";
import { spawn } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { readAgentPreferences, saveAgentPreferences } from "./preferences";
import { registerUpdater } from "./updater";
import { agentAvailability, commandDirectories, launchAgent } from "./agent-launcher";
import { EngineLink, type ConfirmAnswer, type ConfirmRequest, type Picture } from "./engine-link";
import { capture, mayCapture, screenAccess } from "./capture";
import { readDesktopSettings, saveDesktopSettings, type DesktopSettings } from "./desktop-settings";
import { TRAY_ICON } from "./tray-icon";
import { bringBack, closeTab, configureTabs, coverTabs, describeTabs, focusedPopout, mainClosed, openTab, placeTab, poppedTabs, popouts, popOut, reloadTab, showPopout, shownInMain, tabText, tabUrl } from "./tabs";

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
 *  ours, which app the main window shows, the apps popped out into windows
 *  of their own, and which of OpenAgora's windows has focus (the engine
 *  can't always ask the OS: Wayland). */
function windowReport(): Record<string, unknown> {
  const id = (window: BrowserWindow | null) => (window && !window.isDestroyed() ? window.getMediaSourceId() : null);
  const front = BaseWindow.getFocusedWindow();
  const is = (window: BrowserWindow | null) => !!front && !!window && !window.isDestroyed() && window.id === front.id;
  const popped = focusedPopout();
  const focused = is(mainWin) ? { window: "main" } : is(panelWin) ? { window: "panel" } : popped ? { window: "popout", app: popped } : null;
  return { pid: process.pid, main: id(mainWin), panel: id(panelWin), tab: shownInMain(), popouts: popouts(), focused };
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
  pick?: boolean;
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
  window.on("focus", reportWindows);
  window.on("blur", reportWindows);
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
  if (state.phase === "pressed" && state.verdict === "allowed" && state.target) await capturePressed({ key: state.target.key });
}

/** Captures the window the shortcut was pressed over; `allow` records the
 *  user's choice from the panel first, for the app it showed (`key`), and
 *  `pick` asks the system to let the user pick a window where OpenAgora
 *  can't see which one is in front. */
async function capturePressed(params: { allow?: "once" | "always"; key?: string; pick?: boolean }): Promise<ShortcutState> {
  const before = shortcutState ?? { phase: "pressed" as const };
  sendPanel({ ...before, phase: "capturing", error: null });
  let state: ShortcutState;
  try {
    if (!link?.connected) throw new Error("OpenAgora's engine isn't running");
    lastPicture = null;
    const result = await link.request<Omit<ShortcutState, "phase">>("shortcut/capture", params, 150_000);
    state = { phase: result.error ? "error" : "pressed", ...result, picture: result.item ? lastPicture : null };
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

// --- Apps' approval pages --------------------------------------------------

/** Approval pages open now, by app. */
const approvals = new Map<string, BrowserWindow>();

/** An app's own page for approving OpenAgora (its OAuth sign-in), in a window
 *  of OpenAgora's that uses the app's tab session: an app the user already
 *  signed in to there (T3 Code's paired browser) approves without a code.
 *  Only pages on this computer; the window closes once the app sends the
 *  answer back to the engine. */
async function openApproval(app: string, url: string): Promise<void> {
  if (!VALID_ID.test(app) || !LOCAL_URL.test(url)) throw new Error("that isn't an approval page on this computer");
  approvals.get(app)?.close();
  const window = new BrowserWindow({
    width: 560,
    height: 760,
    title: "Approve OpenAgora",
    autoHideMenuBar: true,
    backgroundColor: "#0f1115",
    webPreferences: { partition: `persist:app-${app}`, sandbox: true, contextIsolation: true, nodeIntegration: false },
  });
  approvals.set(app, window);
  window.on("closed", () => { if (approvals.get(app) === window) approvals.delete(app); });
  window.webContents.on("will-navigate", (event, next) => { if (!LOCAL_URL.test(next)) event.preventDefault(); });
  window.webContents.on("will-redirect", (event, next) => { if (!LOCAL_URL.test(next)) event.preventDefault(); });
  // The app sends the user back to the engine, which answers with a short
  // page: leave it up for a moment.
  window.webContents.on("did-navigate", (_event, next) => {
    if (/^http:\/\/(127\.0\.0\.1|localhost):\d+\/callback\?/.test(next)) {
      setTimeout(() => { if (!window.isDestroyed()) window.close(); }, 1500);
    }
  });
  void window.loadURL(url).catch(() => { /* The page shows its own error. */ });
  window.show();
  window.focus();
}

/** Opens an app with a URL in its own scheme, for a handoff: only a scheme
 *  that an installed app's listing names for taking files. */
async function openOwnScheme(url: string): Promise<void> {
  const scheme = url.slice(0, url.indexOf(":") + 1).toLowerCase();
  const r = await runEngine(["catalog", "--json"]);
  if (!r.ok) throw new Error(r.stderr || "could not read the catalog");
  const apps = JSON.parse(r.stdout) as { installed?: boolean; destination?: { deliver?: string; url?: string | null } | null }[];
  const declared = apps.some((a) => a.installed && a.destination?.deliver === "url-scheme"
    && typeof a.destination.url === "string" && a.destination.url.toLowerCase().startsWith(scheme));
  if (!declared) throw new Error("no installed app takes files by that URL scheme");
  await shell.openExternal(url);
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
      tray.setToolTip("OpenAgora · Sunday, powered by Hermes Agent from Nous Research");
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
    openApproval,
    capture: async (target, maxEdge, purpose) => {
      const picture = await capture(target, maxEdge);
      // The panel shows what the shortcut captured, not what Sunday did.
      if (purpose === "shortcut") lastPicture = thumbnail(picture);
      return picture;
    },
    readText: tabText,
    writeClipboard: async (content) => {
      if (content.kind === "text") return clipboard.writeText(content.text);
      // The clipboard takes pictures as PNG: a JPEG goes on as one.
      const image = nativeImage.createFromBuffer(Buffer.from(content.data, "base64"));
      if (image.isEmpty()) throw new Error("that picture can't be read");
      const png = content.type === "image/png" ? Buffer.from(content.data, "base64") : image.toPNG();
      await clipboard.write([new ClipboardItem({ "image/png": new Blob([png], { type: "image/png" }) })]);
    },
    openUrl: openOwnScheme,
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

/** Whether a message comes from OpenAgora's own pages: the main window or
 *  the Sunday panel, never an app's tab or a frame inside them. */
function fromPanelOrMain(event: Electron.IpcMainInvokeEvent | Electron.IpcMainEvent): boolean {
  const ours = [mainWin, panelWin].some((window) => !!window && !window.isDestroyed() && event.sender === window.webContents);
  return ours && event.senderFrame === event.sender.mainFrame;
}

/** Answers OpenAgora's own pages only. */
function handle(channel: string, listener: (event: Electron.IpcMainInvokeEvent, ...args: unknown[]) => unknown): void {
  ipcMain.handle(channel, (event, ...args: unknown[]) => {
    if (!fromPanelOrMain(event)) throw new Error("only OpenAgora's own windows can do this");
    return listener(event, ...args);
  });
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
  handle("catalog", async () => {
    const r = await runEngine(["catalog", "--json"]);
    if (!r.ok) throw new Error(r.stderr || "could not read the catalog");
    return JSON.parse(r.stdout);
  });
  handle("agent-preferences", () => readAgentPreferences(app.getPath("userData")));
  handle("save-agent-preferences", (_e, value: unknown) => saveAgentPreferences(app.getPath("userData"), value));
  handle("agent-availability", (_e, id: unknown) => agentAvailability(id));
  handle("launch-agent", (_e, id: unknown, action: unknown) => launchAgent(id, action, app.getPath("userData")));
  handle("install", (event, id: unknown) => {
    const appId = checkId(id);
    return runEngine(["install", appId, "--yes"], (line) =>
      !event.sender.isDestroyed() && event.sender.send("progress", { id: appId, line }),
    );
  });
  for (const action of ["uninstall", "start", "stop"] as const) {
    handle(action, (_e, id: unknown) => {
      const args = [action, checkId(id)];
      if (action === "uninstall") args.push("--yes");
      return runEngine(args);
    });
  }
  // An app's own update command; the window asks first.
  handle("update-app", (event, id: unknown) => {
    const appId = checkId(id);
    return runEngine(["update", appId, "--yes"], (line) =>
      !event.sender.isDestroyed() && event.sender.send("progress", { id: appId, line }),
    );
  });
  handle("sunday-status", async () => {
    const r = await runEngine(["sunday", "status", "--json"]);
    if (!r.ok) throw new Error(r.stderr || "could not read Sunday's status");
    return JSON.parse(r.stdout);
  });
  handle("confirm-answer", (event, id: unknown, allowed: unknown, choice: unknown) => {
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
  handle("sunday-connect", () => runEngine(["sunday", "connect", "--yes"]));
  handle("sunday-disconnect", () => runEngine(["sunday", "disconnect", "--yes"]));
  handle("sunday-setup", (_e, choice: unknown) => {
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
  handle("logs", (_e, id: unknown) => runEngine(["logs", checkId(id), "-n", "200"]));
  handle("open-external", (_e, url: unknown) => {
    // Websites, and the user's own apps on this machine ("Open in browser").
    if (typeof url === "string" && (url.startsWith("https://") || LOCAL_URL.test(url))) {
      return shell.openExternal(url);
    }
  });

  // App tabs: only the main window's own page says where it shows them.
  const fromMain = (event: Electron.IpcMainInvokeEvent | Electron.IpcMainEvent) =>
    !!mainWin && !mainWin.isDestroyed() && event.sender === mainWin.webContents && event.senderFrame === event.sender.mainFrame;
  const mainOnly = (channel: string, listener: (...args: unknown[]) => unknown) =>
    ipcMain.handle(channel, (event, ...args: unknown[]) => {
      if (!fromMain(event)) throw new Error("only OpenAgora's main window can do this");
      return listener(...args);
    });
  mainOnly("tab-open", (id, name, url, pid) => {
    if (typeof name !== "string" || typeof url !== "string" || typeof pid !== "number") throw new Error("invalid tab");
    openTab(checkId(id), name.slice(0, 200), url, pid);
  });
  mainOnly("tab-pop-out", (id) => popOut(checkId(id)));
  mainOnly("tab-bring-back", (id) => bringBack(checkId(id)));
  mainOnly("tab-show-window", (id) => showPopout(checkId(id)));
  mainOnly("tab-reload", (id) => reloadTab(checkId(id)));
  mainOnly("tab-close", (id) => closeTab(checkId(id)));
  mainOnly("tab-url", (id) => tabUrl(checkId(id)));
  mainOnly("tabs-popped", () => poppedTabs());
  ipcMain.on("tab-place", (event, id: unknown, bounds: unknown) => {
    if (!fromMain(event) || typeof id !== "string" || !VALID_ID.test(id)) return;
    const b = bounds && typeof bounds === "object" ? bounds as Record<string, unknown> : null;
    const n = (v: unknown) => (typeof v === "number" && Number.isFinite(v) ? Math.max(0, Math.min(v, 100_000)) : 0);
    placeTab(id, b ? { x: n(b.x), y: n(b.y), width: n(b.width), height: n(b.height) } : null);
  });
  ipcMain.on("tab-cover", (event, covered: unknown) => {
    if (fromMain(event)) coverTabs(covered === true);
  });

  // Sunday's panel.
  handle("panel-state", () => shortcutState);
  handle("panel-capture", (_e, options: unknown) => {
    const o = options && typeof options === "object" ? options as Record<string, unknown> : {};
    const allow = o.allow === "once" || o.allow === "always" ? o.allow : undefined;
    const key = typeof o.key === "string" && o.key.length <= 512 ? o.key : undefined;
    return capturePressed({ ...(allow ? { allow } : {}), ...(key ? { key } : {}), ...(o.pick === true ? { pick: true } : {}) });
  });
  handle("panel-hide", () => hidePanel());
  handle("show-main", () => showMain());
  handle("desk-add-clipboard", async () => {
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
  // Dropped files come as their contents, read by the panel: a page never
  // has OpenAgora read a file by its path.
  handle("desk-add-files", async (_e, files: unknown) => {
    if (!Array.isArray(files) || files.length === 0 || files.length > 10) throw new Error("drop up to 10 files");
    const added = [];
    for (const file of files) {
      const f = file && typeof file === "object" ? file as Record<string, unknown> : {};
      if (typeof f.name !== "string" || !(f.data instanceof Uint8Array)) throw new Error("invalid file");
      const name = path.basename(f.name.replace(/[\0-\x1f\\]/g, "/")).slice(0, 200) || "file";
      if (f.data.byteLength > DESK_MAX_BYTES) throw new Error(`${name} is too big for the desk (at most 20 MB)`);
      const extension = path.extname(name).toLowerCase();
      const picture = PICTURE_TYPES[extension];
      added.push(await addToDesk({
        kind: picture ? "image" : "file",
        type: picture ?? TEXT_TYPES[extension] ?? "application/octet-stream",
        data: Buffer.from(f.data).toString("base64"),
        name,
        source: { app: "Files" },
      }));
    }
    return added;
  });

  // Coding agents: set up with each agent's own command (the window shows the
  // exact command first), and apps OpenAgora connects to.
  const engineJson = async (args: string[], what: string) => {
    const r = await runEngine(args);
    if (!r.ok) throw new Error(r.stderr || `could not read ${what}`);
    return JSON.parse(r.stdout);
  };
  handle("coding-agents", async () => {
    const [agents, connections] = await Promise.all([
      engineJson(["agents", "list", "--json"], "the coding agents"),
      engineJson(["connections", "--json"], "the connections"),
    ]);
    return { agents, connections };
  });
  handle("agent-setup", (_e, id: unknown) => runEngine(["agents", "setup", checkId(id), "--yes"]));
  handle("agent-remove", (_e, id: unknown) => runEngine(["agents", "remove", checkId(id), "--yes"]));
  handle("app-connect", async (_e, id: unknown) => {
    const app = checkId(id);
    if (!link?.connected) throw new Error("OpenAgora's engine isn't running");
    try {
      // The user has five minutes to approve on the app's page.
      return await link.request("connections/connect", { app }, 330_000);
    } finally {
      approvals.get(app)?.close();
    }
  });
  handle("app-disconnect", (_e, id: unknown) => runEngine(["disconnect", checkId(id)]));

  // Sunday's memory: Hermes's own commands for the sunday profile, run by
  // the engine after the user agreed in the window.
  const PROVIDERS = new Set(["holographic"]);
  handle("memory-state", () => engineJson(["sunday", "memory", "status", "--json"], "Sunday's memory"));
  handle("memory-use", (_e, provider: unknown) => {
    if (typeof provider !== "string" || !PROVIDERS.has(provider)) throw new Error("unknown memory provider");
    return runEngine(["sunday", "memory", "use", provider, "--yes"]);
  });
  handle("memory-off", () => runEngine(["sunday", "memory", "off", "--yes"]));
  handle("memory-digest", (_e, on: unknown) => {
    if (typeof on !== "boolean") throw new Error("invalid choice");
    return runEngine(["sunday", "memory", "digest", on ? "on" : "off", "--yes"]);
  });
  handle("sessions-revoke", (_e, agent: unknown) => runEngine(["sessions", "revoke", checkId(agent)]));

  // What Sunday can see: OpenAgora's own settings and the engine's broker.
  // macOS: reading other apps' text needs Accessibility permission (null
  // elsewhere, where nothing asks).
  const accessibility = () => (process.platform === "darwin" ? systemPreferences.isTrustedAccessibilityClient(false) : null);
  const desktopState = () => ({ settings, shortcutError, screen: screenAccess(), accessibility: accessibility(), platform: process.platform });
  handle("desktop-settings", desktopState);
  handle("save-desktop-settings", (_e, value: unknown) => {
    settings = saveDesktopSettings(app.getPath("userData"), value);
    registerShortcut(settings.shortcut);
    updateTray();
    return desktopState();
  });
  handle("open-screen-settings", () => {
    if (process.platform === "darwin") return shell.openExternal("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture");
  });
  handle("open-accessibility-settings", () => {
    if (process.platform !== "darwin") return;
    // Lists OpenAgora under Accessibility (macOS asks the user itself), then
    // shows that page.
    systemPreferences.isTrustedAccessibilityClient(true);
    return shell.openExternal("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility");
  });
  handle("audit-log", async () => {
    const r = await runEngine(["audit", "-n", "1000", "--json"]);
    if (!r.ok) throw new Error(r.stderr || "could not read the audit log");
    return JSON.parse(r.stdout);
  });
  handle("broker-state", async () => {
    const [grants, never] = await Promise.all([runEngine(["grants", "list", "--json"]), runEngine(["never", "list", "--json"])]);
    if (!grants.ok || !never.ok) throw new Error(grants.stderr || never.stderr || "could not read what Sunday may see");
    return { grants: JSON.parse(grants.stdout), never: JSON.parse(never.stdout) };
  });
  handle("broker-revoke", (_e, key: unknown, scope: unknown) => {
    // Optionally only one client's permission, or only sending to one place.
    const s = scope && typeof scope === "object" ? scope as Record<string, unknown> : {};
    const args = ["grants", "revoke"];
    if (s.client !== undefined) args.push(`--for=${checkId(s.client)}`);
    if (s.destination !== undefined) args.push(`--to=${checkId(s.destination)}`);
    return runEngine([...args, "--", argument(key, "app")]);
  });
  handle("never-add", (_e, rule: unknown) => {
    const r = rule && typeof rule === "object" ? rule as Record<string, unknown> : {};
    const name = argument(r.name, "name");
    const args = ["never", "add"];
    if (r.app !== undefined) args.push(`--app=${argument(r.app, "app")}`);
    if (r.title !== undefined) args.push(`--title=${argument(r.title, "title")}`);
    if (args.length === 2) throw new Error("give an app or words in a window title");
    return runEngine([...args, "--", name]);
  });
  handle("never-remove", (_e, name: unknown) => runEngine(["never", "remove", "--", argument(name, "name")]));
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
  // The page's tabs go with the page: when it reloads, and when the window
  // closes (apps popped out into their own windows stay).
  window.webContents.on("did-navigate", () => { if (mainWin === window) mainClosed(); });
  window.on("focus", reportWindows);
  window.on("blur", reportWindows);
  window.on("closed", () => {
    if (mainWin === window) mainWin = null;
    mainClosed();
    reportWindows();
    // Without the menu bar or tray icon, closing the window ends OpenAgora
    // (on macOS, apps stay in the Dock as usual).
    if (!settings.keepRunning && process.platform !== "darwin") app.quit();
  });
  window.once("ready-to-show", reportWindows);
  return window;
}

// No page in OpenAgora (its own, apps' tabs, Sunday's chat) gets the camera,
// the microphone or the screen: OpenAgora captures windows only for the
// engine, after the user allowed it, through its own hidden capture page.
// Everything else stays as Electron has it.
const MEDIA_PERMISSIONS = new Set(["media", "display-capture"]);

function lockDown(ses: Electron.Session): void {
  ses.setPermissionRequestHandler((contents, permission, callback, details) =>
    callback(!MEDIA_PERMISSIONS.has(permission) || mayCapture(contents, details.requestingUrl)));
  ses.setPermissionCheckHandler((contents, permission, _origin, details) =>
    !MEDIA_PERMISSIONS.has(permission) || mayCapture(contents, details.requestingUrl));
}
app.on("session-created", lockDown);

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
    // Sunday is Hermes Agent; OpenAgora's About box says so.
    app.setAboutPanelOptions({
      applicationName: "OpenAgora",
      applicationVersion: app.getVersion(),
      credits: "Sunday, OpenAgora’s personal agent, is Hermes Agent by Nous Research, used under the MIT license.",
      authors: ["OpenAgora contributors", "Sunday: Hermes Agent by Nous Research (MIT license)"],
      website: "https://github.com/sanjaykatta1/openagora",
    });
    lockDown(session.defaultSession);
    settings = readDesktopSettings(app.getPath("userData"));
    configureTabs({
      main: () => (mainWin && !mainWin.isDestroyed() ? mainWin : null),
      send: (event) => { if (mainWin && !mainWin.isDestroyed()) mainWin.webContents.send("tab-event", event); },
      changed: reportWindows,
    });
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
          main: () => mainWin,
          showMain,
          tabs: describeTabs,
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
