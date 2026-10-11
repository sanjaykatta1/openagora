// OpenAgora's window, connected to the engine service (`openagora serve`) over
// its local socket. The engine asks the user to confirm agents' actions here
// when the asking agent can't ask in its own chat, asks to show apps in tabs,
// and asks the window to capture windows (only the window may ask the OS for
// a picture) and read its own tabs' text, write the clipboard and open an app
// by its own URL scheme. The window tells the engine about its own windows
// and the shortcut, and puts things on the desk.

import net from "node:net";

export interface Choice {
  id: string;
  label: string;
}

export interface ConfirmRequest {
  id: string;
  client: string;
  title: string;
  message: string;
  /** Ways to say yes besides "Allow", e.g. "Allow once" and "Always allow". */
  choices?: Choice[];
}

export interface ConfirmAnswer {
  allowed: boolean;
  choice?: string;
}

/** What to capture: another app's window by its OS id, an app in an OpenAgora tab, or a window the user picks. */
export type CaptureTarget = { kind: "window"; id: number; app?: string } | { kind: "tab"; app: string } | { kind: "pick" };

export interface Picture {
  type: string;
  /** Base64. */
  data: string;
  width: number;
  height: number;
  /** The app's name, when the window was picked. */
  app?: string;
  /** The window's title as captured, when OpenAgora's window could see it. */
  title?: string;
}

/** Whom a capture is for: the shortcut's are shown in Sunday's panel. */
export type CapturePurpose = "shortcut" | "tool";

/** What the engine puts on the clipboard: a picture (base64) or text that
 *  Sunday handed over. */
export type ClipboardContent = { kind: "image"; type: "image/png" | "image/jpeg"; data: string } | { kind: "text"; text: string };

export interface EngineLinkEvents {
  /** The engine wants the user's yes or no; resolve allowed only for an explicit yes. */
  confirm(request: ConfirmRequest): Promise<ConfirmAnswer>;
  /** An agent asked to show an app. */
  openApp(id: string): void;
  /** The engine connects to an app that signs agents in with OAuth: show
   *  the app's own approval page. Resolves once it is shown. */
  openApproval(app: string, url: string): Promise<void>;
  /** The engine asks for a picture of a window. */
  capture(target: CaptureTarget, maxEdge: number, purpose: CapturePurpose): Promise<Picture>;
  /** The engine asks for the text an app's page in OpenAgora shows (other
   *  windows' text the engine reads itself, through the OS). */
  readText(app: string): Promise<{ title: string; text: string }>;
  /** Sunday handed something to the clipboard, for the user to paste. */
  writeClipboard(content: ClipboardContent): Promise<void>;
  /** Sunday handed a file to an app that opens files by its own URL scheme. */
  openUrl(url: string): Promise<void>;
  /** What the engine should know about this window when it (re)connects. */
  hello(): Record<string, unknown>;
  connected(connected: boolean): void;
}

interface Pending {
  resolve(value: unknown): void;
  reject(error: Error): void;
  timer: NodeJS.Timeout;
}

/** Keeps a connection to the engine, reconnecting when it drops. */
export class EngineLink {
  private socket: net.Socket | null = null;
  private stopped = false;
  private timer: NodeJS.Timeout | null = null;
  private pending = new Map<string, Pending>();
  private nextId = 1;
  private isConnected = false;

  constructor(private address: () => Promise<string>, private events: EngineLinkEvents, private retryMs = 3000) {}

  start(): void {
    void this.connect();
  }

  stop(): void {
    this.stopped = true;
    if (this.timer) clearTimeout(this.timer);
    this.socket?.destroy();
  }

  get connected(): boolean {
    return this.isConnected;
  }

  /** Asks the engine something; rejects if it doesn't answer within `timeoutMs`. */
  request<T = unknown>(method: string, params: unknown, timeoutMs = 30_000): Promise<T> {
    const socket = this.socket;
    if (!socket || !this.isConnected) return Promise.reject(new Error("OpenAgora's engine isn't running"));
    const id = `d-${this.nextId++}`;
    return new Promise<T>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error("OpenAgora's engine didn't answer in time"));
      }, timeoutMs);
      this.pending.set(id, { resolve: resolve as (value: unknown) => void, reject, timer });
      socket.write(JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n");
    });
  }

  notify(method: string, params: unknown): void {
    if (this.socket && this.isConnected) this.socket.write(JSON.stringify({ jsonrpc: "2.0", method, params }) + "\n");
  }

  private retry(delay = this.retryMs): void {
    if (this.stopped || this.timer) return;
    this.timer = setTimeout(() => { this.timer = null; void this.connect(); }, delay);
    this.timer.unref?.();
  }

  private async connect(): Promise<void> {
    let address: string;
    try {
      address = await this.address();
    } catch {
      this.retry(this.retryMs * 10);
      return;
    }
    if (this.stopped) return;
    const socket = net.connect(address);
    this.socket = socket;
    let buffer = "";
    socket.setEncoding("utf8");
    socket.on("connect", () => {
      socket.write(JSON.stringify({ hello: { role: "desktop", client: "desktop", ...this.events.hello() } }) + "\n");
      this.isConnected = true;
      this.events.connected(true);
    });
    socket.on("data", (chunk: string) => {
      buffer += chunk;
      let newline: number;
      while ((newline = buffer.indexOf("\n")) >= 0) {
        const line = buffer.slice(0, newline).trim();
        buffer = buffer.slice(newline + 1);
        if (line) this.handle(socket, line);
      }
    });
    socket.on("error", () => { /* "close" follows */ });
    socket.on("close", () => {
      if (this.socket === socket) this.socket = null;
      this.isConnected = false;
      for (const [id, waiter] of this.pending) {
        clearTimeout(waiter.timer);
        waiter.reject(new Error("OpenAgora's engine stopped"));
        this.pending.delete(id);
      }
      this.events.connected(false);
      this.retry();
    });
  }

  private handle(socket: net.Socket, line: string): void {
    let message: { id?: unknown; method?: unknown; params?: Record<string, unknown>; result?: unknown; error?: { message?: unknown } };
    try { message = JSON.parse(line); } catch { return; }
    const params = message.params ?? {};
    // A reply to one of our requests.
    if (message.method === undefined && typeof message.id === "string" && this.pending.has(message.id)) {
      const waiter = this.pending.get(message.id)!;
      this.pending.delete(message.id);
      clearTimeout(waiter.timer);
      if (message.error) waiter.reject(new Error(String(message.error.message ?? "the engine said no")));
      else waiter.resolve(message.result);
      return;
    }
    const reply = (result: unknown) =>
      socket.writable && socket.write(JSON.stringify({ jsonrpc: "2.0", id: message.id, result }) + "\n");
    const fail = (error: unknown) =>
      socket.writable && socket.write(JSON.stringify({
        jsonrpc: "2.0", id: message.id,
        error: { code: -32000, message: error instanceof Error ? error.message : String(error) },
      }) + "\n");
    if (message.method === "confirm/ask" && message.id !== undefined) {
      const choices = Array.isArray(params.choices)
        ? params.choices
          .filter((c): c is { id: string; label: string } => typeof c?.id === "string" && typeof c?.label === "string")
          .slice(0, 3)
        : undefined;
      const request: ConfirmRequest = {
        id: String(message.id),
        client: String(params.client ?? "an agent"),
        title: String(params.title ?? "Allow this?"),
        message: String(params.message ?? ""),
        ...(choices && choices.length > 0 ? { choices } : {}),
      };
      this.events.confirm(request).then(
        (answer) => reply(answer.allowed ? { allowed: true, ...(answer.choice ? { choice: answer.choice } : {}) } : { allowed: false }),
        () => reply({ allowed: false }),
      );
    } else if (message.method === "capture/window" && message.id !== undefined) {
      const target = parseTarget(params.target);
      if (!target) { fail("unknown capture target"); return; }
      const max = typeof params.max === "number" && params.max >= 64 ? Math.min(params.max, 4096) : 1920;
      const purpose: CapturePurpose = params.purpose === "shortcut" ? "shortcut" : "tool";
      this.events.capture(target, max, purpose).then(reply, fail);
    } else if (message.method === "capture/text" && message.id !== undefined) {
      const target = parseTarget(params.target);
      if (target?.kind !== "tab") { fail("OpenAgora's window reads the text of its own tabs only"); return; }
      this.events.readText(target.app).then(reply, fail);
    } else if (message.method === "oauth/open" && message.id !== undefined) {
      if (typeof params.app !== "string" || typeof params.url !== "string") { fail("an approval page needs an app and an address"); return; }
      this.events.openApproval(params.app, params.url).then(() => reply({ opened: true }), fail);
    } else if (message.method === "clipboard/write" && message.id !== undefined) {
      const content = parseClipboard(params);
      if (!content) { fail("the clipboard takes a PNG or JPEG picture, or text"); return; }
      this.events.writeClipboard(content).then(() => reply({ written: true }), fail);
    } else if (message.method === "url/open" && message.id !== undefined) {
      const url = ownSchemeUrl(params.url);
      if (!url) { fail("OpenAgora opens only an app's own URL scheme"); return; }
      this.events.openUrl(url).then(() => reply({ opened: true }), fail);
    } else if (message.method === "app/open" && typeof params.id === "string") {
      this.events.openApp(params.id);
    } else if (message.id !== undefined && message.method !== undefined) {
      socket.write(JSON.stringify({ jsonrpc: "2.0", id: message.id, error: { code: -32601, message: "unknown method" } }) + "\n");
    }
  }
}

const APP_ID = /^[a-z][a-z0-9-]*$/;

export function parseTarget(value: unknown): CaptureTarget | null {
  if (typeof value !== "object" || value === null) return null;
  const target = value as Record<string, unknown>;
  if (target.kind === "window" && typeof target.id === "number" && Number.isSafeInteger(target.id) && target.id > 0) {
    return { kind: "window", id: target.id, ...(typeof target.app === "string" ? { app: target.app } : {}) };
  }
  if (target.kind === "tab" && typeof target.app === "string" && APP_ID.test(target.app)) return { kind: "tab", app: target.app };
  if (target.kind === "pick") return { kind: "pick" };
  return null;
}

/** The most the engine puts on the clipboard at once: a 20 MB desk item, as base64. */
const CLIPBOARD_MAX = 28 * 1024 * 1024;

export function parseClipboard(params: Record<string, unknown>): ClipboardContent | null {
  if (params.kind === "text" && typeof params.text === "string" && params.text.length <= CLIPBOARD_MAX) {
    return { kind: "text", text: params.text };
  }
  if (params.kind === "image" && (params.type === "image/png" || params.type === "image/jpeg")
    && typeof params.data === "string" && params.data.length > 0 && params.data.length <= CLIPBOARD_MAX
    && /^[A-Za-z0-9+/]+={0,2}$/.test(params.data)) {
    return { kind: "image", type: params.type, data: params.data };
  }
  return null;
}

/** Schemes that open web pages, files, mail or scripts, or that Windows
 *  runs tools with: never an app's own. */
const NOT_AN_APPS_OWN = new Set([
  "http", "https", "file", "ftp", "javascript", "vbscript", "data", "blob", "about", "mailto",
  "ms-msdt", "search-ms", "search", "ms-officecmd", "ms-settings",
]);

/** `value`, when it is a URL in an app's own scheme. */
export function ownSchemeUrl(value: unknown): string | null {
  if (typeof value !== "string" || value.length > 8192 || /[\s\0]/.test(value)) return null;
  const match = /^([a-z][a-z0-9+.-]*):/i.exec(value);
  if (!match || NOT_AN_APPS_OWN.has(match[1].toLowerCase())) return null;
  return value;
}
