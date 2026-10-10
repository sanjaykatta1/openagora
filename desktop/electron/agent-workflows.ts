// Launch the upstream interfaces unchanged. No provider or channel settings live here.
export interface AgentCommand { label: string; args: string[] }
export interface AgentWorkflow {
  executable?: string;
  note: string;
  commands: Record<string, AgentCommand>;
}

export const AGENT_WORKFLOWS: Record<string, AgentWorkflow> = {
  hermes: {
    executable: "hermes",
    note: "Hermes’s dashboard contains its own chat, model settings, channels, pairing, and gateway controls. Complete all account and phone setup there, or open Hermes’s own terminal setup wizard.",
    commands: {
      open: { label: "Open Hermes dashboard", args: ["dashboard"] },
      setup: { label: "Open Hermes setup", args: ["setup"] },
    },
  },
  openclaw: {
    executable: "openclaw",
    note: "Use OpenClaw’s own onboarding for models, accounts, channels, and pairing. Its dashboard opens in your browser; its gateway is managed by OpenClaw.",
    commands: {
      open: { label: "Open OpenClaw dashboard", args: ["dashboard"] },
      setup: { label: "Open OpenClaw onboarding", args: ["onboard"] },
    },
  },
  goose: {
    executable: "goose",
    note: "Use Goose Desktop directly, or launch its CLI here. Provider sign-in, model selection, extensions, and permissions stay in Goose. OpenAgora does not add messaging features to it.",
    commands: {
      open: { label: "Open Goose CLI", args: ["session"] },
      setup: { label: "Open Goose configuration", args: ["configure"] },
    },
  },
  nanobot: {
    executable: "nanobot",
    note: "The native nanobot WebUI opens in your browser and handles its own first-run setup, models, and channels.",
    commands: { open: { label: "Open nanobot WebUI", args: ["webui"] } },
  },
  nanoclaw: {
    note: "NanoClaw runs from its own project checkout and container runtime. Follow its official setup and use its own interface. OpenAgora does not run commands in a guessed checkout or manage its service.",
    commands: {},
  },
  picoclaw: {
    executable: "picoclaw",
    note: "Use PicoClaw’s own launcher or setup guide to configure providers and messaging. This shortcut opens its native CLI chat with the existing configuration.",
    commands: { open: { label: "Open PicoClaw CLI", args: ["agent"] } },
  },
  zeroclaw: {
    executable: "zeroclaw",
    note: "ZeroClaw Quickstart handles its own model, account, and agent setup. Use ZeroClaw’s interface and service instructions for chat and messaging.",
    commands: { setup: { label: "Open ZeroClaw Quickstart", args: ["quickstart"] } },
  },
};

export function agentWorkflow(id: unknown): AgentWorkflow {
  if (typeof id !== "string" || !Object.hasOwn(AGENT_WORKFLOWS, id)) throw new Error("Unknown personal agent");
  return AGENT_WORKFLOWS[id];
}

export function agentCommand(id: unknown, action: unknown): AgentCommand {
  const workflow = agentWorkflow(id);
  if (typeof action !== "string" || !Object.hasOwn(workflow.commands, action)) throw new Error("Unsupported agent action");
  return workflow.commands[action];
}
