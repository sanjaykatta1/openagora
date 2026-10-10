import fs from "node:fs";
import path from "node:path";
import { NEW_AGENT_PREFERENCES, parseAgentPreferences, type AgentPreferences } from "./agents";

export function readAgentPreferences(directory: string): AgentPreferences {
  try {
    return parseAgentPreferences(JSON.parse(fs.readFileSync(path.join(directory, "personal-agent.json"), "utf8")));
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return { ...NEW_AGENT_PREFERENCES };
    throw error;
  }
}

export function saveAgentPreferences(directory: string, value: unknown): AgentPreferences {
  const preferences = parseAgentPreferences(value);
  fs.mkdirSync(directory, { recursive: true });
  const file = path.join(directory, "personal-agent.json");
  const temporary = `${file}.tmp`;
  try {
    fs.writeFileSync(temporary, JSON.stringify(preferences) + "\n", { mode: 0o600 });
    fs.renameSync(temporary, file);
  } finally {
    fs.rmSync(temporary, { force: true });
  }
  return preferences;
}
