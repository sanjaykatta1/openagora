// Pictures of windows, taken only when the engine asks (it checks first that
// the user allowed the app and that it isn't on the never-capture list).
//
// - An app in an OpenAgora tab, or popped out into a window of its own: its
//   page's own capturePage, which needs no OS permission.
// - Any other window: Chromium's window capture of that one window, through a
//   hidden page that calls getDisplayMedia while OpenAgora's handler hands it
//   exactly that window. No other window is captured, not even as a thumbnail.
//   macOS asks once for Screen Recording permission; Windows uses Windows
//   Graphics Capture; on Wayland the system's own dialog lets the user pick
//   the window.

import { app, BrowserWindow, desktopCapturer, nativeImage, session, systemPreferences, type NativeImage, type WebContents } from "electron";
import path from "node:path";
import { pathToFileURL } from "node:url";
import type { CaptureTarget, Picture } from "./engine-link";
import { tabContents } from "./tabs";

const CAPTURE_PARTITION = "openagora-capture";
const CAPTURE_PAGE = path.join(__dirname, "..", "renderer", "capture.html");
const CAPTURE_PAGE_URL = pathToFileURL(CAPTURE_PAGE).toString();
/** Larger pictures are sent as JPEG. */
const PNG_LIMIT = 4 * 1024 * 1024;
/** How long a capture may take (the engine waits 30 s); picking a window in
 *  the system's dialog takes the user longer (the engine waits 120 s). */
const CAPTURE_TIMEOUT_MS = 25_000;
const PICK_TIMEOUT_MS = 110_000;

export type ScreenAccess = "granted" | "denied" | "not-determined" | "restricted" | "unknown" | "not-needed";

/** Whether macOS lets OpenAgora capture other apps' windows. Other systems don't ask. */
export function screenAccess(): ScreenAccess {
  if (process.platform !== "darwin") return "not-needed";
  return systemPreferences.getMediaAccessStatus("screen");
}

/** Scales a picture down to `maxEdge` and encodes it. */
export function encode(image: NativeImage, maxEdge: number): Picture {
  let { width, height } = image.getSize();
  if (width === 0 || height === 0) throw new Error("the capture came back empty");
  if (Math.max(width, height) > maxEdge) {
    image = width >= height ? image.resize({ width: maxEdge, quality: "best" }) : image.resize({ height: maxEdge, quality: "best" });
    ({ width, height } = image.getSize());
  }
  let bytes = image.toPNG();
  let type = "image/png";
  if (bytes.length > PNG_LIMIT) {
    bytes = image.toJPEG(85);
    type = "image/jpeg";
  }
  return { type, data: bytes.toString("base64"), width, height };
}

/** The window currently being handed to the hidden capture page. */
let granting: { id: string; name: string } | null = null;
let grabber: BrowserWindow | null = null;

/** Whether a page may use the camera, the microphone or the screen: only
 *  OpenAgora's hidden capture page, while it captures what the engine asked
 *  for. Every other page in OpenAgora is refused (see main.ts). */
export function mayCapture(contents: WebContents | null | undefined, requestingUrl: string | undefined): boolean {
  return !!granting && !!contents && !!grabber && !grabber.isDestroyed()
    && contents.id === grabber.webContents.id && requestingUrl === CAPTURE_PAGE_URL;
}

/** Closes the capture page, after a capture that hung. */
function resetGrabber(): void {
  granting = null;
  const window = grabber;
  grabber = null;
  if (window && !window.isDestroyed()) window.destroy();
}

/** Whether OpenAgora runs on Wayland, where listing windows opens the
 *  system's own window picker. */
function waylandSession(): boolean {
  if (process.platform !== "linux" || app.commandLine.getSwitchValue("ozone-platform") === "x11") return false;
  return !!process.env.WAYLAND_DISPLAY || process.env.XDG_SESSION_TYPE === "wayland";
}

async function grabberWindow(): Promise<BrowserWindow> {
  if (grabber && !grabber.isDestroyed()) return grabber;
  const ses = session.fromPartition(CAPTURE_PARTITION);
  const window = new BrowserWindow({
    show: false,
    width: 320,
    height: 240,
    skipTaskbar: true,
    webPreferences: { partition: CAPTURE_PARTITION, sandbox: true, contextIsolation: true, nodeIntegration: false, backgroundThrottling: false },
  });
  ses.setDisplayMediaRequestHandler((request, callback) => {
    // Only OpenAgora's own capture page, only while OpenAgora is capturing.
    const own = request.frame && !window.isDestroyed()
      && request.frame.processId === window.webContents.mainFrame.processId
      && request.frame.routingId === window.webContents.mainFrame.routingId;
    if (!own || !granting) {
      callback({});
      return;
    }
    callback({ video: granting });
  });
  // getDisplayMedia asks for "media" first. Only the capture page may have
  // it, while capturing; nothing else is allowed in its session.
  ses.setPermissionRequestHandler((contents, permission, callback, details) => {
    callback((permission === "media" || permission === "display-capture") && mayCapture(contents, details.requestingUrl));
  });
  ses.setPermissionCheckHandler((contents, permission, _origin, details) =>
    (permission === "media" || permission === "display-capture") && mayCapture(contents, details.requestingUrl));
  // A page of OpenAgora's own (getDisplayMedia needs a secure page, which
  // about:blank isn't).
  await window.loadFile(CAPTURE_PAGE);
  window.on("closed", () => { if (grabber === window) grabber = null; });
  grabber = window;
  return window;
}

/** Runs in the hidden page: one frame of the granted window, as base64 PNG. */
const GRAB_FRAME = `(async () => {
  let stream;
  try {
    stream = await navigator.mediaDevices.getDisplayMedia({ video: { frameRate: 5 }, audio: false });
  } catch (error) {
    throw new Error((error && error.name ? error.name + ": " : "") + (error && error.message || String(error)));
  }
  try {
    const [track] = stream.getVideoTracks();
    const capture = new ImageCapture(track);
    let frame = null;
    // The first frames of a new capture can still be blank.
    for (let attempt = 0; attempt < 6; attempt++) {
      frame = await capture.grabFrame();
      const probe = new OffscreenCanvas(16, 16).getContext("2d");
      probe.drawImage(frame, 0, 0, 16, 16);
      const pixels = probe.getImageData(0, 0, 16, 16).data;
      let lit = false;
      for (let i = 0; i < pixels.length; i += 4) if (pixels[i] || pixels[i + 1] || pixels[i + 2]) { lit = true; break; }
      if (lit) break;
      await new Promise((r) => setTimeout(r, 150));
    }
    const canvas = new OffscreenCanvas(frame.width, frame.height);
    canvas.getContext("2d").drawImage(frame, 0, 0);
    const bytes = new Uint8Array(await (await canvas.convertToBlob({ type: "image/png" })).arrayBuffer());
    let binary = "";
    for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
    return btoa(binary);
  } finally {
    stream.getTracks().forEach((track) => track.stop());
  }
})()`;

let queue: Promise<unknown> = Promise.resolve();

/** One capture at a time: the handler grants a single window. */
function oneAtATime<T>(work: () => Promise<T>): Promise<T> {
  const next = queue.then(work, work);
  queue = next.catch(() => {});
  return next;
}

async function captureSource(source: { id: string; name: string }): Promise<NativeImage> {
  const window = await grabberWindow();
  const mine = { ...source };
  granting = mine;
  try {
    const base64: string = await window.webContents.executeJavaScript(GRAB_FRAME, true);
    return nativeImage.createFromBuffer(Buffer.from(base64, "base64"));
  } finally {
    // A capture that hung and finishes late leaves the next one alone.
    if (granting === mine) granting = null;
  }
}

class TookTooLong extends Error {}

function within<T>(work: Promise<T>, ms: number): Promise<T> {
  let timer: NodeJS.Timeout | undefined;
  const late = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new TookTooLong("the capture took too long")), ms);
  });
  return Promise.race([work, late]).finally(() => clearTimeout(timer));
}

function permissionHint(error: unknown): Error {
  const message = error instanceof Error ? error.message : String(error);
  const access = screenAccess();
  if (access === "denied" || access === "restricted") {
    return new Error("macOS hasn't given OpenAgora Screen Recording permission. Allow OpenAgora in System Settings → Privacy & Security → Screen Recording, then reopen OpenAgora.");
  }
  return new Error(`couldn't capture the window: ${message}`);
}

/** Captures what the engine asked for. One capture at a time; one that
 *  hangs is given up on, and the capture page is made anew. */
export function capture(target: CaptureTarget, maxEdge: number): Promise<Picture> {
  const ms = target.kind === "pick" ? PICK_TIMEOUT_MS : CAPTURE_TIMEOUT_MS;
  return oneAtATime(() => within(captureNow(target, maxEdge), ms).catch((error: unknown) => {
    if (error instanceof TookTooLong) resetGrabber();
    throw error;
  }));
}

/** The picture, with the window's title as captured: the engine checks it
 *  against the never-capture list again. */
async function captureNow(target: CaptureTarget, maxEdge: number): Promise<Picture> {
  if (target.kind === "tab") {
    const contents = tabContents(target.app);
    if (!contents) throw new Error("that app's tab isn't open in OpenAgora");
    return { ...encode(await contents.capturePage(), maxEdge), title: contents.getTitle() };
  }
  // Only where the system's own dialog picks the window (Wayland):
  // anywhere else a "pick" would be a guess.
  if (target.kind === "pick" && !waylandSession()) throw new Error("this system doesn't let you pick a window");
  try {
    // Listing windows without thumbnails captures nothing; on macOS it
    // asks for Screen Recording permission the first time, and on Wayland
    // it opens the system's window picker.
    const sources = await desktopCapturer.getSources({ types: ["window"], thumbnailSize: { width: 0, height: 0 } });
    if (target.kind === "pick") {
      if (sources.length !== 1) throw new Error(sources.length === 0 ? "no window was picked" : "the system didn't ask which window to capture");
      const [picked] = sources;
      const name = picked.name || undefined;
      return { ...encode(await captureSource({ id: picked.id, name: picked.name }), maxEdge), app: name, title: name };
    }
    const prefix = `window:${target.id}:`;
    const found = sources.find((s) => s.id.startsWith(prefix));
    const picture = encode(await captureSource({ id: found?.id ?? `${prefix}0`, name: found?.name ?? target.app ?? "window" }), maxEdge);
    return found ? { ...picture, title: found.name } : picture;
  } catch (error) {
    throw permissionHint(error);
  }
}
