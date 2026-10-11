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
  /** The agent's own profile (Sunday's, for Hermes). */
  profile: string | null;
  ui: "web" | "terminal" | "window" | "background";
  steps: Step[];
  permissions: Permission[];
  installed: boolean;
  /** Already on this computer when OpenAgora started using it. */
  adopted: boolean;
  /** What the app provides is on this computer, whoever installed it. */
  present: boolean;
  status: Status;
  pid: number | null;
  /** Where to open the app: may be a one-time sign-in link it printed at startup. */
  url: string | null;
  /** The app's plain address while it runs. */
  home_url: string | null;
}

/** Where Sunday stands; mirrors `openagora sunday status --json`. */
export interface SundayStatus {
  id: string;
  name: string;
  profile: string;
  /** Hermes's command on this computer, if it is installed. */
  hermes: string | null;
  installed: boolean;
  adopted: boolean;
  /** Fresh or cloned is the user's choice: there may be an earlier Hermes setup. */
  ask_how_to_start: boolean;
  version: string | null;
  min_version: string;
  version_ok: boolean | null;
  setup: "ready" | "missing" | "foreign" | "unknown";
  profile_dir: string | null;
  dashboard: "running" | "stopped";
  url: string | null;
  pid: number | null;
  /** Hermes's messaging gateway, as Hermes reports it (while the dashboard runs). */
  gateway: { running: boolean | null; state: string | null; platforms: Record<string, { state?: string } | undefined> | null } | null;
}

export type SundaySetupChoice = "default" | "fresh" | "clone" | "clone-channels" | "existing";

export interface EngineResult {
  ok: boolean;
  stdout: string;
  stderr: string;
}

/** OpenAgora's own updates; mirrors electron/updater.ts. */
export type UpdateState =
  | { kind: "unsupported"; current: string }
  | { kind: "idle"; current: string }
  | { kind: "checking"; current: string }
  | { kind: "available"; current: string; version: string }
  | { kind: "downloading"; current: string; version: string; percent: number }
  | { kind: "ready"; current: string; version: string }
  | { kind: "error"; current: string; message: string };

export interface Bridge {
  platform: string;
  agentAvailability(id: string): Promise<{ detected: boolean; launchable: boolean }>;
  launchAgent(id: string, action: string): Promise<void>;
  agentPreferences(): Promise<AgentPreferences>;
  saveAgentPreferences(value: AgentPreferences): Promise<AgentPreferences>;
  catalog(): Promise<App[]>;
  install(id: string): Promise<EngineResult>;
  uninstall(id: string): Promise<EngineResult>;
  start(id: string): Promise<EngineResult>;
  stop(id: string): Promise<EngineResult>;
  updateApp(id: string): Promise<EngineResult>;
  sundayStatus(): Promise<SundayStatus>;
  sundaySetup(choice: SundaySetupChoice): Promise<EngineResult>;
  logs(id: string): Promise<EngineResult>;
  openExternal(url: string): Promise<void>;
  update: {
    state(): Promise<UpdateState>;
    check(): Promise<void>;
    download(): Promise<void>;
    install(): Promise<void>;
    onState(callback: (state: UpdateState) => void): () => void;
  };
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
