// Pictures of windows, taken only when the engine asks (it checks first that
// the user allowed the app and that it isn't on the never-capture list).
//
// - An app in an OpenAgora tab: the tab's own capturePage, which needs no OS
//   permission.
// - Any other window: Chromium's window capture of that one window, through a
//   hidden page that calls getDisplayMedia while OpenAgora's handler hands it
//   exactly that window. No other window is captured, not even as a thumbnail.
//   macOS asks once for Screen Recording permission; Windows uses Windows
//   Graphics Capture; on Wayland the system's own dialog lets the user pick
//   the window.

import { BrowserWindow, desktopCapturer, nativeImage, session, systemPreferences, webContents, type NativeImage, type WebContents } from "electron";
import path from "node:path";
import { pathToFileURL } from "node:url";
import type { CaptureTarget, Picture } from "./engine-link";

const CAPTURE_PARTITION = "openagora-capture";
const CAPTURE_PAGE = path.join(__dirname, "..", "renderer", "capture.html");
const CAPTURE_PAGE_URL = pathToFileURL(CAPTURE_PAGE).toString();
/** Larger pictures are sent as JPEG. */
const PNG_LIMIT = 4 * 1024 * 1024;

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

/** The web view showing an app's tab, if it is open. */
export function tabContents(appId: string): WebContents | undefined {
  const partition = session.fromPartition(`persist:app-${appId}`);
  return webContents.getAllWebContents().find((contents) =>
    !contents.isDestroyed() && contents.getType() === "webview"
    && (contents.session === partition || (!!partition.storagePath && contents.session.storagePath === partition.storagePath)));
}

/** The window currently being handed to the hidden capture page. */
let granting: { id: string; name: string } | null = null;
let grabber: BrowserWindow | null = null;

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
  // Nothing else may use the capture page's session.
  // getDisplayMedia asks for "media" first. Only the capture page may have it.
  ses.setPermissionRequestHandler((_contents, permission, callback, details) => {
    const ours = (details as { requestingUrl?: string }).requestingUrl === CAPTURE_PAGE_URL;
    callback(ours && (permission === "media" || permission === "display-capture"));
  });
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
  granting = source;
  try {
    const base64: string = await window.webContents.executeJavaScript(GRAB_FRAME, true);
    return nativeImage.createFromBuffer(Buffer.from(base64, "base64"));
  } finally {
    granting = null;
  }
}

function permissionHint(error: unknown): Error {
  const message = error instanceof Error ? error.message : String(error);
  const access = screenAccess();
  if (access === "denied" || access === "restricted") {
    return new Error("macOS hasn't given OpenAgora Screen Recording permission. Allow OpenAgora in System Settings → Privacy & Security → Screen Recording, then reopen OpenAgora.");
  }
  return new Error(`couldn't capture the window: ${message}`);
}

/** Captures what the engine asked for. */
export function capture(target: CaptureTarget, maxEdge: number): Promise<Picture> {
  return oneAtATime(async () => {
    if (target.kind === "tab") {
      const contents = tabContents(target.app);
      if (!contents) throw new Error("that app's tab isn't open in OpenAgora");
      return encode(await contents.capturePage(), maxEdge);
    }
    try {
      // Listing windows without thumbnails captures nothing; on macOS it
      // asks for Screen Recording permission the first time, and on Wayland
      // it opens the system's window picker.
      const sources = await desktopCapturer.getSources({ types: ["window"], thumbnailSize: { width: 0, height: 0 } });
      if (target.kind === "pick") {
        const picked = sources[0];
        if (!picked) throw new Error("no window was picked");
        return { ...encode(await captureSource({ id: picked.id, name: picked.name }), maxEdge), app: picked.name || undefined };
      }
      const prefix = `window:${target.id}:`;
      const found = sources.find((s) => s.id.startsWith(prefix));
      return encode(await captureSource({ id: found?.id ?? `${prefix}0`, name: found?.name ?? target.app ?? "window" }), maxEdge);
    } catch (error) {
      throw permissionHint(error);
    }
  });
}
