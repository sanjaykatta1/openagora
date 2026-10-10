// What the window can ask the engine for. In the desktop app this is the
// preload bridge; opened in a plain browser (`npm run preview`) it is null and
// the UI says so instead of pretending.
import type { AgentPreferences } from "../../electron/agents";

export type Status = "not-installed" | "stopped" | "running";

export interface Step {
  download: { github: string; asset: string | null } | null;
  run: string;
}

export interface Permission {
  id: string;
  description: string;
  default: "granted" | "ask" | "denied";
}

export interface App {
  id: string;
  name: string;
  summary: string;
  category: string;
  homepage: string;
  source: string;
  license: string;
  platforms: string[];
  available: boolean;
  role: "app" | "agent";
  ui: "web" | "terminal" | "window" | "background";
  steps: Step[];
  permissions: Permission[];
  installed: boolean;
  status: Status;
  pid: number | null;
  /** Where to open the app: may be a one-time sign-in link it printed at startup. */
  url: string | null;
  /** The app's plain address while it runs. */
  home_url: string | null;
}

export interface EngineResult {
  ok: boolean;
  stdout: string;
  stderr: string;
}

export interface Bridge {
  platform: string;
  agentPreferences(): Promise<AgentPreferences>;
  saveAgentPreferences(value: AgentPreferences): Promise<AgentPreferences>;
  catalog(): Promise<App[]>;
  install(id: string): Promise<EngineResult>;
  uninstall(id: string): Promise<EngineResult>;
  start(id: string): Promise<EngineResult>;
  stop(id: string): Promise<EngineResult>;
  logs(id: string): Promise<EngineResult>;
  openExternal(url: string): Promise<void>;
  onProgress(callback: (event: { id: string; line: string }) => void): () => void;
}

declare global {
  interface Window {
    openagora?: Bridge;
  }
}

export const bridge: Bridge | null = window.openagora ?? null;

export const osName = (platform: string | undefined): string =>
  ({ darwin: "macOS", win32: "Windows", linux: "Linux" })[platform ?? ""] ?? "this computer";
