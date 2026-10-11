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
  /** What Sunday may hand to the app, and how (`[destination]`). */
  destination?: { accepts: string[]; deliver: string; adapter: string | null } | null;
  /** How agents sign in to the app's MCP server. */
  mcp_auth?: "oauth" | "none" | null;
}

/** A coding agent on this computer; mirrors `openagora agents list --json`. */
export interface CodingAgent {
  id: string;
  name: string;
  homepage?: string;
  /** Added by the user (`openagora agents add`), set up by hand. */
  custom: boolean;
  installed: boolean;
  version?: string | null;
  /** Its config has OpenAgora's MCP server. */
  set_up: boolean;
  /** The agent's own command OpenAgora runs to set it up. */
  setup_command?: string;
  /** Claude Code: its own commands that add OpenAgora's prompt-hook plugin. */
  hook_commands?: string[];
  remove_command?: string | null;
  hook_remove_commands?: string[];
  /** How to remove it where the agent has no command for that. */
  remove_by_hand?: string | null;
  /** For an agent set up by hand: what its MCP config runs. */
  mcp_command?: string;
}

/** OpenAgora's sign-in to an app; mirrors `openagora connections --json`. */
export interface AppConnection {
  app: string;
  connected_at: number;
  expires_at: number;
  usable: boolean;
  /** More than reading (T3 Code: Supervised or more). */
  can_act: boolean;
  scope: string | null;
}

export interface CodingAgentsState {
  agents: CodingAgent[];
  connections: AppConnection[];
}

/** Sunday's memory; mirrors `openagora sunday memory status --json`. */
export interface MemoryState {
  /** The memory provider Hermes uses for Sunday, or null for built-in only. */
  provider: string | null;
  /** Providers OpenAgora sets up, with Hermes's commands for each, shown before they run. */
  providers: { id: string; name: string; needs: string; commands: string[] }[];
  digest: boolean;
  /** Seconds since the epoch. */
  digest_last_run?: number;
  digest_last_result?: string;
  /** Coding agents whose sessions OpenAgora can read, and whether Sunday may always read them. */
  sessions: { agent: string; name: string; always: boolean }[];
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
  codingAgents: {
    state(): Promise<CodingAgentsState>;
    setUp(id: string): Promise<EngineResult>;
    remove(id: string): Promise<EngineResult>;
    /** Opens the app's approval page; resolves when the user answered there. */
    connect(app: string): Promise<{ app: string; connected: boolean; can_act: boolean }>;
    disconnect(app: string): Promise<EngineResult>;
  };
  memory: {
    state(): Promise<MemoryState>;
    use(provider: string): Promise<EngineResult>;
    off(): Promise<EngineResult>;
    digest(on: boolean): Promise<EngineResult>;
    revokeSessions(agent: string): Promise<EngineResult>;
  };
  broker: {
    state(): Promise<BrokerState>;
    revoke(app: string, scope?: { client?: string; destination?: string }): Promise<EngineResult>;
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
