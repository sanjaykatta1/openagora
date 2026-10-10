// Shared, browser-safe directory. These are choices, not installed integrations.
// Sources and licenses checked against the upstream repositories, 2026-10-09.
export interface PersonalAgent {
  id: string;
  name: string;
  summary: string;
  license: string;
  source: string;
  setup: string;
  note: string;
  catalogId?: string;
}

export const PERSONAL_AGENTS: readonly PersonalAgent[] = [
  {
    id: "hermes", name: "Hermes", license: "MIT",
    summary: "A personal agent with persistent memory and reusable skills.",
    source: "https://github.com/NousResearch/hermes-agent",
    setup: "https://hermes-agent.nousresearch.com/docs/",
    note: "Install from the Store, then configure your model provider in Hermes.",
    catalogId: "hermes",
  },
  {
    id: "openclaw", name: "OpenClaw", license: "MIT",
    summary: "An assistant on your devices, connected to the chat apps you use.",
    source: "https://github.com/openclaw/openclaw",
    setup: "https://docs.openclaw.ai/start/getting-started",
    note: "Follow OpenClaw’s setup guide to install it, choose a model, and connect your channels.",
  },
  {
    id: "goose", name: "Goose", license: "Apache-2.0",
    summary: "A desktop and terminal agent for research, coding, and everyday workflows.",
    source: "https://github.com/aaif-goose/goose",
    setup: "https://github.com/aaif-goose/goose#Get-started",
    note: "Install Goose’s desktop app or CLI and configure a model provider in Goose.",
  },
  {
    id: "nanobot", name: "nanobot", license: "MIT",
    summary: "A lightweight Python assistant with memory, tools, and chat integrations.",
    source: "https://github.com/HKUDS/nanobot",
    setup: "https://github.com/HKUDS/nanobot#start-here",
    note: "Follow nanobot’s guide to set up its runtime, model provider, and optional chat channels.",
  },
  {
    id: "nanoclaw", name: "NanoClaw", license: "MIT",
    summary: "A customizable assistant that runs its agents in separate containers.",
    source: "https://github.com/nanocoai/nanoclaw",
    setup: "https://github.com/nanocoai/nanoclaw#readme",
    note: "Requires a container runtime. Follow NanoClaw’s setup for macOS, Linux, or Windows with WSL2.",
  },
  {
    id: "picoclaw", name: "PicoClaw", license: "MIT",
    summary: "A small Go-based assistant designed for lightweight devices and automation.",
    source: "https://github.com/sipeed/picoclaw",
    setup: "https://github.com/sipeed/picoclaw#readme",
    note: "Follow PicoClaw’s installation and onboarding instructions for your device.",
  },
  {
    id: "zeroclaw", name: "ZeroClaw", license: "MIT OR Apache-2.0",
    summary: "A Rust-based personal assistant with configurable models, tools, and channels.",
    source: "https://github.com/zeroclaw-labs/zeroclaw",
    setup: "https://github.com/zeroclaw-labs/zeroclaw#quick-start",
    note: "Install ZeroClaw and run its guided Quickstart to choose a provider and configure your assistant.",
  },
];

export interface AgentPreferences {
  version: 1;
  completed: boolean;
  selectedAgent: string | null;
}

export const NEW_AGENT_PREFERENCES: AgentPreferences = { version: 1, completed: false, selectedAgent: null };

export function parseAgentPreferences(value: unknown): AgentPreferences {
  if (!value || typeof value !== "object") throw new Error("Invalid agent preferences");
  const p = value as Record<string, unknown>;
  if (p.version !== 1 || typeof p.completed !== "boolean" ||
      (p.selectedAgent !== null && !PERSONAL_AGENTS.some((a) => a.id === p.selectedAgent)) ||
      (!p.completed && p.selectedAgent !== null)) {
    throw new Error("Invalid agent preferences");
  }
  return { version: 1, completed: p.completed, selectedAgent: p.selectedAgent as string | null };
}
