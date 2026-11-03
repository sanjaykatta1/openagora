// Sunday, OpenAgora's personal agent, is Hermes Agent under its own Hermes
// profile. Whether the user wants it is saved in personal-agent.json; this
// reads and checks that choice, for the window and the main process alike.

/** Sunday: Hermes, under its own profile. */
export const SUNDAY_ID = "hermes";

export interface AgentPreferences {
  version: 1;
  /** The user has answered: Sunday, or no personal agent. */
  completed: boolean;
  /** `hermes` for Sunday; null for no personal agent. */
  selectedAgent: string | null;
}

export const NEW_AGENT_PREFERENCES: AgentPreferences = { version: 1, completed: false, selectedAgent: null };

function checkShape(value: unknown): AgentPreferences {
  if (!value || typeof value !== "object") throw new Error("Invalid agent preferences");
  const p = value as Record<string, unknown>;
  if (p.version !== 1 || typeof p.completed !== "boolean" ||
      (p.selectedAgent !== null && typeof p.selectedAgent !== "string") ||
      (!p.completed && p.selectedAgent !== null)) {
    throw new Error("Invalid agent preferences");
  }
  return { version: 1, completed: p.completed, selectedAgent: p.selectedAgent as string | null };
}

/** Reads saved preferences. A choice of another agent, from versions that
 *  offered more than Sunday, is not an error: OpenAgora simply asks again. */
export function parseAgentPreferences(value: unknown): AgentPreferences {
  const preferences = checkShape(value);
  if (preferences.selectedAgent !== null && preferences.selectedAgent !== SUNDAY_ID) return { ...NEW_AGENT_PREFERENCES };
  return preferences;
}

/** Checks preferences before they are saved: Sunday, or no agent. */
export function validateAgentPreferences(value: unknown): AgentPreferences {
  const preferences = checkShape(value);
  if (preferences.selectedAgent !== null && preferences.selectedAgent !== SUNDAY_ID) throw new Error("Unknown personal agent");
  return preferences;
}
