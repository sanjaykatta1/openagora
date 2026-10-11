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
  /** Sunday's profile uses OpenAgora's MCP gateway, per-reply hook and skills. */
  connected: boolean;
  dashboard: "running" | "stopped";
  url: string | null;
  pid: number | null;
  /** Hermes's messaging gateway, as Hermes reports it (while the dashboard runs). */
  gateway: { running: boolean | null; state: string | null; platforms: Record<string, { state?: string } | undefined> | null } | null;
}

export type SundaySetupChoice = "default" | "fresh" | "clone" | "clone-channels" | "existing";

/** The engine asking the user to confirm something an agent wants to do. */
export interface ConfirmRequest {
  id: string;
  client: string;
  title: string;
  message: string;
  /** Ways to say yes, e.g. "Allow once" and "Always allow". */
  choices?: { id: string; label: string }[];
}

/** Where Sunday's shortcut stands; shown in the Sunday panel. */
export interface ShortcutState {
  phase: "looking" | "pressed" | "capturing" | "offline" | "error";
  /** The window the shortcut was pressed over. */
  target?: { kind: "window" | "tab"; app: string; key: string } | null;
  verdict?: "allowed" | "not-allowed" | "never" | "unknown";
  /** The never-capture rule that covers it. */
  rule?: string | null;
  /** Why OpenAgora couldn't tell which window was in front. */
  unknown?: string | null;
  /** Whether the system lets the user pick the window instead (Wayland). */
  pick?: boolean;
  item?: { id: string; description: string } | null;
  error?: string | null;
  /** A small copy of what Sunday got. */
  picture?: string | null;
}

/** A file the user dropped on the panel, read by the panel itself. */
export interface DeskFile {
  name: string;
  data: Uint8Array;
}

export type ScreenAccess = "granted" | "denied" | "not-determined" | "restricted" | "unknown" | "not-needed";

export interface DesktopSettings {
  version: 1;
  shortcut: string;
  keepRunning: boolean;
}

/** OpenAgora's own settings on this computer, and what the system allows. */
export interface DesktopState {
  settings: DesktopSettings;
  /** Why the shortcut couldn't be set, if it couldn't. */
  shortcutError: string | null;
  screen: ScreenAccess;
  platform: string;
}

/** What the user allowed Sunday to see; mirrors `openagora grants list --json`. */
export interface Grant {
  client: string;
  source: string;
  source_name: string;
  destination: string;
  type: string;
  scope: "once" | "always";
  granted_at: number;
}

export interface NeverRule {
  name: string;
  apps?: string[];
  titles?: string[];
}

export interface BrokerState {
  grants: Grant[];
  never: { built_in: NeverRule[]; user: NeverRule[] };
}

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
  sundayConnect(): Promise<EngineResult>;
  sundayDisconnect(): Promise<EngineResult>;
  answerConfirm(id: string, allowed: boolean, choice?: string): Promise<void>;
  onConfirm(callback: (request: ConfirmRequest) => void): () => void;
  onConfirmDone(callback: (id: string) => void): () => void;
  onOpenApp(callback: (id: string) => void): () => void;
  /** Tells OpenAgora what the main window shows (for Sunday's shortcut). */
  viewChanged(view: { kind: string; app?: string }): void;
  showMain(): Promise<void>;
  panel: {
    state(): Promise<ShortcutState | null>;
    capture(options: { allow?: "once" | "always"; key?: string; pick?: boolean }): Promise<ShortcutState>;
    hide(): Promise<void>;
    onState(callback: (state: ShortcutState) => void): () => void;
  };
  desk: {
    addClipboard(): Promise<{ id: string }>;
    addFiles(files: DeskFile[]): Promise<{ id: string }[]>;
  };
  desktopSettings(): Promise<DesktopState>;
  saveDesktopSettings(value: DesktopSettings): Promise<DesktopState>;
  openScreenSettings(): Promise<void>;
  broker: {
    state(): Promise<BrokerState>;
    revoke(app: string): Promise<EngineResult>;
    neverAdd(rule: { name: string; app?: string; title?: string }): Promise<EngineResult>;
    neverRemove(name: string): Promise<EngineResult>;
  };
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
