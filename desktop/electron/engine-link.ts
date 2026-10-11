// OpenAgora's window, connected to the engine service (`openagora serve`) over
// its local socket: the engine asks the user to confirm app actions here when
// the asking agent can't ask in its own chat, and asks to show apps in tabs.

import net from "node:net";

export interface ConfirmRequest {
  id: string;
  client: string;
  title: string;
  message: string;
}

export interface EngineLinkEvents {
  /** The engine wants the user's yes or no; resolve true only for an explicit yes. */
  confirm(request: ConfirmRequest): Promise<boolean>;
  /** An agent asked to show an app. */
  openApp(id: string): void;
  connected(connected: boolean): void;
}

/** Keeps a connection to the engine, reconnecting when it drops. */
export class EngineLink {
  private socket: net.Socket | null = null;
  private stopped = false;
  private timer: NodeJS.Timeout | null = null;

  constructor(private address: () => Promise<string>, private events: EngineLinkEvents, private retryMs = 3000) {}

  start(): void {
    void this.connect();
  }

  stop(): void {
    this.stopped = true;
    if (this.timer) clearTimeout(this.timer);
    this.socket?.destroy();
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
      socket.write(JSON.stringify({ hello: { role: "desktop", client: "desktop" } }) + "\n");
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
      this.events.connected(false);
      this.retry();
    });
  }

  private handle(socket: net.Socket, line: string): void {
    let message: { id?: unknown; method?: unknown; params?: Record<string, unknown> };
    try { message = JSON.parse(line); } catch { return; }
    const params = message.params ?? {};
    const reply = (result: unknown) =>
      socket.writable && socket.write(JSON.stringify({ jsonrpc: "2.0", id: message.id, result }) + "\n");
    if (message.method === "confirm/ask" && message.id !== undefined) {
      const request: ConfirmRequest = {
        id: String(message.id),
        client: String(params.client ?? "an agent"),
        title: String(params.title ?? "Allow this?"),
        message: String(params.message ?? ""),
      };
      this.events.confirm(request).then((allowed) => reply({ allowed }), () => reply({ allowed: false }));
    } else if (message.method === "app/open" && typeof params.id === "string") {
      this.events.openApp(params.id);
    } else if (message.id !== undefined && message.method !== undefined) {
      socket.write(JSON.stringify({ jsonrpc: "2.0", id: message.id, error: { code: -32601, message: "unknown method" } }) + "\n");
    }
  }
}
