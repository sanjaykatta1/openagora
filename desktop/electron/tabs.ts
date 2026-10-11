// App tabs: each app's own web UI, in a view that OpenAgora's main process
// owns (Electron's WebContentsView). The main window's page only says where it
// shows a tab; it never gets the app's page. Because the view belongs to the
// main process, a tab can pop out into a window of its own and come back
// without reloading.
//
// - Local addresses only: a link that leaves this computer opens in the
//   default browser, and new windows are refused (as for every page).
// - A saved session per app (`persist:app-<id>`), so sign-ins survive restarts
//   and apps don't share cookies.
// - No preload, no Node, sandboxed, with context isolation. Like every page in
//   OpenAgora, no camera, microphone or screen (main.ts).

import { BaseWindow, shell, WebContentsView, type BrowserWindow, type Rectangle, type WebContents } from "electron";

const LOCAL_URL = /^http:\/\/(127\.0\.0\.1|localhost):\d+(\/|$)/;
const VALID_ID = /^[a-z][a-z0-9-]*$/;
const BACKGROUND = "#0f1115";

/** What the main window's page hears about its tabs. */
export interface TabEvent {
  id: string;
  kind: "loaded" | "failed" | "popped" | "returned";
  description?: string;
}

/** Where OpenAgora's main process keeps a tab. */
export interface TabHost {
  /** The main window, while it is open. */
  main(): BrowserWindow | null;
  /** Tells the main window's page about one of its tabs. */
  send(event: TabEvent): void;
  /** OpenAgora's windows changed (a tab popped out or came back, a window
   *  took or lost focus): tell the engine. */
  changed(): void;
}

interface Tab {
  id: string;
  name: string;
  /** The app's process: a new start of the app gets a new view. */
  pid: number;
  view: WebContentsView;
  /** Where the main window's page shows it, in the page's coordinates; null
   *  while the page shows something else. */
  bounds: Rectangle | null;
  /** The window the view is in now. */
  shownIn: BaseWindow | null;
  /** Its own window, while it is popped out. */
  popout: BaseWindow | null;
}

const tabs = new Map<string, Tab>();
/** A dialog of OpenAgora's is open over the main window's tabs. */
let covered = false;
let host: TabHost | null = null;

export function configureTabs(h: TabHost): void {
  host = h;
}

function alive(window: BaseWindow | null): window is BaseWindow {
  return !!window && !window.isDestroyed();
}

function moveTo(tab: Tab, window: BaseWindow | null): void {
  if (tab.shownIn === window) return;
  if (alive(tab.shownIn)) tab.shownIn.contentView.removeChildView(tab.view);
  tab.shownIn = null;
  if (alive(window)) {
    window.contentView.addChildView(tab.view);
    tab.shownIn = window;
  }
}

/** Shows or hides a tab in the main window, as its page asked. */
function apply(tab: Tab): void {
  if (tab.popout) return;
  const main = host?.main() ?? null;
  if (!alive(main)) {
    moveTo(tab, null);
    return;
  }
  const bounds = tab.bounds;
  if (bounds && !covered) {
    moveTo(tab, main);
    // The page measures in its own pixels; the window in device-independent
    // ones, which differ when the page is zoomed.
    const zoom = main.webContents.getZoomFactor();
    tab.view.setBounds({
      x: Math.round(bounds.x * zoom),
      y: Math.round(bounds.y * zoom),
      width: Math.round(bounds.width * zoom),
      height: Math.round(bounds.height * zoom),
    });
    tab.view.setVisible(true);
  } else {
    tab.view.setVisible(false);
  }
}

function fill(window: BaseWindow, view: WebContentsView): void {
  const [width, height] = window.getContentSize();
  view.setBounds({ x: 0, y: 0, width, height });
}

function destroy(tab: Tab): void {
  if (tabs.get(tab.id) === tab) tabs.delete(tab.id);
  const popout = tab.popout;
  tab.popout = null;
  moveTo(tab, null);
  if (alive(popout)) popout.destroy();
  if (!tab.view.webContents.isDestroyed()) tab.view.webContents.close();
}

/** Opens an app's tab, or keeps the one already open for this start of the
 *  app (a page that reloads asks again). */
export function openTab(id: string, name: string, url: string, pid: number): void {
  if (!VALID_ID.test(id) || !LOCAL_URL.test(url)) throw new Error("that isn't an app on this computer");
  const known = tabs.get(id);
  if (known && known.pid === pid && !known.view.webContents.isDestroyed()) {
    known.name = name;
    if (known.popout) host?.send({ id, kind: "popped" });
    return;
  }
  if (known) destroy(known);
  const view = new WebContentsView({
    webPreferences: { partition: `persist:app-${id}`, sandbox: true, contextIsolation: true, nodeIntegration: false, webviewTag: false },
  });
  view.setBackgroundColor(BACKGROUND);
  view.setVisible(false);
  const tab: Tab = { id, name, pid, view, bounds: null, shownIn: null, popout: null };
  tabs.set(id, tab);
  const contents = view.webContents;
  const leaving = (event: Electron.Event, next: string) => {
    if (LOCAL_URL.test(next)) return;
    event.preventDefault();
    if (next.startsWith("https://")) void shell.openExternal(next);
  };
  contents.on("will-navigate", leaving);
  contents.on("will-redirect", leaving);
  contents.on("did-fail-load", (_event, code, description, _url, isMainFrame) => {
    // -3 is an aborted load (a redirect or a new navigation), not a failure.
    if (isMainFrame && code !== -3) host?.send({ id, kind: "failed", description: description || `error ${code}` });
  });
  contents.on("did-finish-load", () => host?.send({ id, kind: "loaded" }));
  contents.on("page-title-updated", (_event, title) => {
    if (alive(tab.popout)) tab.popout.setTitle(title && title !== tab.name ? `${tab.name} — ${title}` : tab.name);
  });
  void contents.loadURL(url).catch(() => { /* did-fail-load says why */ });
}

/** Where the main window's page shows a tab, or null when it doesn't. */
export function placeTab(id: string, bounds: Rectangle | null): void {
  const tab = tabs.get(id);
  if (!tab) return;
  const before = shownInMain();
  tab.bounds = bounds;
  apply(tab);
  if (shownInMain() !== before) host?.changed();
}

/** A dialog of OpenAgora's opened or closed over the main window: the tabs
 *  would cover it, so they hide meanwhile. */
export function coverTabs(cover: boolean): void {
  covered = cover;
  for (const tab of tabs.values()) apply(tab);
}

/** Moves a tab into a window of its own, without reloading it. */
export function popOut(id: string): void {
  const tab = tabs.get(id);
  if (!tab) throw new Error("that app's tab isn't open");
  if (alive(tab.popout)) {
    showPopout(id);
    return;
  }
  const main = host?.main() ?? null;
  const size = tab.bounds ?? { width: 1000, height: 700 };
  const window = new BaseWindow({
    width: Math.max(480, Math.round(size.width)),
    height: Math.max(360, Math.round(size.height)),
    minWidth: 320,
    minHeight: 240,
    title: tab.name,
    backgroundColor: BACKGROUND,
    autoHideMenuBar: true,
    show: false,
  });
  if (alive(main)) {
    // Over the main window, a little down and to the right.
    const [x, y] = main.getPosition();
    window.setPosition(x + 48, y + 48);
  }
  tab.popout = window;
  moveTo(tab, window);
  fill(window, tab.view);
  tab.view.setVisible(true);
  window.on("resize", () => fill(window, tab.view));
  window.on("focus", () => {
    if (!tab.view.webContents.isDestroyed()) tab.view.webContents.focus();
    host?.changed();
  });
  window.on("blur", () => host?.changed());
  // Closing the window brings the app back to its tab.
  window.on("close", () => returned(tab, window));
  window.show();
  host?.send({ id, kind: "popped" });
  host?.changed();
}

/** A popped-out tab's window is closing: the app goes back to its tab in the
 *  main window. Without a main window to go back to, the app's page closes
 *  (the app keeps running). */
function returned(tab: Tab, window: BaseWindow): void {
  if (tab.popout !== window) return;
  tab.popout = null;
  if (tabs.get(tab.id) !== tab) return;
  const main = host?.main() ?? null;
  if (!alive(main)) {
    destroy(tab);
  } else {
    moveTo(tab, main);
    tab.view.setVisible(false);
    apply(tab);
    host?.send({ id: tab.id, kind: "returned" });
  }
  host?.changed();
}

/** Brings a popped-out tab back into the main window. */
export function bringBack(id: string): void {
  const window = tabs.get(id)?.popout ?? null;
  if (alive(window)) window.close();
}

/** Shows a popped-out tab's window. */
export function showPopout(id: string): void {
  const window = tabs.get(id)?.popout ?? null;
  if (!alive(window)) return;
  if (window.isMinimized()) window.restore();
  window.show();
  window.focus();
}

export function closeTab(id: string): void {
  const tab = tabs.get(id);
  if (!tab) return;
  destroy(tab);
  host?.changed();
}

export function reloadTab(id: string): void {
  const contents = tabs.get(id)?.view.webContents;
  if (contents && !contents.isDestroyed()) contents.reload();
}

/** The address a tab shows now. */
export function tabUrl(id: string): string | null {
  const contents = tabs.get(id)?.view.webContents;
  return contents && !contents.isDestroyed() ? contents.getURL() : null;
}

/** The apps whose tabs are popped out, for a main window that opens again. */
export function poppedTabs(): string[] {
  return [...tabs.values()].filter((t) => alive(t.popout)).map((t) => t.id);
}

/** The page of an app's tab, wherever it is: for captures. */
export function tabContents(appId: string): WebContents | undefined {
  const contents = tabs.get(appId)?.view.webContents;
  return contents && !contents.isDestroyed() ? contents : undefined;
}

/** The app the main window shows, if it shows one (and not in its own window). */
export function shownInMain(): string | null {
  for (const tab of tabs.values()) if (tab.bounds && !tab.popout) return tab.id;
  return null;
}

/** OpenAgora's windows of popped-out apps, by the OS's id for each window
 *  (`window:<id>:0`, as for the main window). */
export function popouts(): { window: string; app: string }[] {
  return [...tabs.values()]
    .filter((t): t is Tab & { popout: BaseWindow } => alive(t.popout))
    .map((t) => ({ window: t.popout.getMediaSourceId(), app: t.id }));
}

/** The popped-out app whose window has focus, if one has. */
export function focusedPopout(): string | null {
  const focused = BaseWindow.getFocusedWindow();
  if (!focused) return null;
  for (const tab of tabs.values()) if (alive(tab.popout) && tab.popout.id === focused.id) return tab.id;
  return null;
}

/** The main window closed: its tabs close with it, and popped-out apps stay
 *  in their own windows. */
export function mainClosed(): void {
  for (const tab of [...tabs.values()]) if (!tab.popout) destroy(tab);
}

/** What the tabs look like now, for UI tests. */
export function describeTabs(): { id: string; visible: boolean; popped: boolean; inMain: boolean; bounds: Rectangle; url: string }[] {
  const main = host?.main() ?? null;
  return [...tabs.values()].map((t) => ({
    id: t.id,
    visible: t.view.getVisible(),
    popped: alive(t.popout),
    inMain: alive(main) && alive(t.shownIn) && t.shownIn.id === main.id,
    bounds: t.view.getBounds(),
    url: t.view.webContents.isDestroyed() ? "" : t.view.webContents.getURL(),
  }));
}
