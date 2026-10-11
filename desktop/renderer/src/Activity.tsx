// Settings → Activity: the audit log. Every app action, capture, handoff,
// read of a coding agent's sessions and change of permission, with who asked
// and who said yes. It stays on this computer; the engine writes it.
import { useCallback, useEffect, useMemo, useState } from "react";
import { bridge, type AuditEntry } from "./api";

const WHO: Record<string, string> = {
  sunday: "Sunday", user: "You", openagora: "OpenAgora",
  "claude-code": "Claude Code", codex: "Codex", opencode: "OpenCode",
};

const WHAT: Record<string, string> = {
  app_install: "install an app", app_update: "update an app", app_uninstall: "remove an app",
  app_start: "start an app", app_stop: "stop an app", app_open: "open an app",
  capture: "capture a window", allow_app: "let Sunday see an app", grant: "allow", revoke: "take back",
  desk_add: "put on the desk", desk_get: "read from the desk", desk_clear: "clear the desk",
  never_add: "never capture", never_remove: "allow capturing again",
  handoff: "send", start_task: "start a task", allow_task: "allow starting tasks", allow_act: "allow acting in an app",
  app_tool: "use an app's tool", context_get: "read what Sunday sent", connect: "connect an app", disconnect: "disconnect an app",
  agent_setup: "set up a coding agent", agent_remove: "remove from a coding agent",
  sessions_read: "read coding sessions", allow_sessions: "let Sunday read sessions", revoke_sessions: "take back reading sessions",
  agent_hook: "add OpenAgora's hook to a coding agent",
  memory_provider: "change Sunday's memory", memory_digest: "memory digest", background: "background mode",
};

const DECISION: Record<string, string> = {
  allowed: "allowed", confirmed: "you said yes", declined: "you said no", refused: "refused",
};

const VIA: Record<string, string> = {
  window: "in OpenAgora's window", chat: "in the chat", "command line": "on the command line", "system dialog": "in the system's dialog",
};

const who = (client: string) => WHO[client] ?? client;
const via = (where: string) => VIA[where] ?? `in ${where}`;
const what = (action: string) => WHAT[action] ?? action.replace(/_/g, " ");

const errorText = (e: unknown) =>
  (e instanceof Error ? e.message : String(e)).replace(/^Error invoking remote method '[^']+': (Error: )?/, "");

export function ActivitySettings() {
  const [entries, setEntries] = useState<AuditEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [client, setClient] = useState("all");
  const [query, setQuery] = useState("");

  const load = useCallback(async () => {
    if (!bridge) return;
    try {
      setEntries(await bridge.auditLog());
      setError(null);
    } catch (e) {
      setError(errorText(e));
    }
  }, []);
  useEffect(() => { void load(); }, [load]);

  const clients = useMemo(() => [...new Set((entries ?? []).map((e) => e.client))].sort(), [entries]);
  const shown = useMemo(() => {
    const words = query.toLowerCase().split(/\s+/).filter(Boolean);
    return (entries ?? [])
      .filter((e) => client === "all" || e.client === client)
      .filter((e) => {
        const text = `${who(e.client)} ${what(e.action)} ${e.action} ${e.target} ${e.decision} ${e.via ?? ""} ${e.outcome}`.toLowerCase();
        return words.every((w) => text.includes(w));
      })
      .slice()
      .reverse()
      .slice(0, 300);
  }, [entries, client, query]);

  if (!bridge) return null;
  return (
    <div className="panel" aria-labelledby="activity-title">
      <h3 id="activity-title">Activity</h3>
      <p className="muted small">Everything Sunday, your coding agents and OpenAgora did for you, and every yes and no, newest first. It stays on this computer.</p>
      <div className="row">
        <label className="small">Who{" "}
          <select aria-label="Who" value={client} onChange={(e) => setClient(e.target.value)}>
            <option value="all">Everyone</option>
            {clients.map((c) => <option key={c} value={c}>{who(c)}</option>)}
          </select>
        </label>
        <input type="search" aria-label="Search the activity" placeholder="Search" value={query} onChange={(e) => setQuery(e.target.value)} />
        <button className="btn ghost" onClick={() => void load()}>Refresh</button>
      </div>
      {error && <p role="alert" className="agent-error">{error}</p>}
      {entries && shown.length === 0 && <p className="muted small">Nothing yet.</p>}
      <ul className="rows compact" aria-label="Audit log">
        {shown.map((e, n) => (
          <li key={`${e.at}-${n}`} className="row">
            <span>
              <strong>{who(e.client)}</strong>{" "}
              <span>{what(e.action)}{e.target ? `: ${e.target}` : ""}</span>{" "}
              <span className="muted small">— {DECISION[e.decision] ?? e.decision}{e.via ? ` ${via(e.via)}` : ""}{e.outcome && e.outcome !== "ok" ? `; ${e.outcome}` : ""}</span>
            </span>
            <time className="muted small" dateTime={new Date(e.at * 1000).toISOString()}>{new Date(e.at * 1000).toLocaleString()}</time>
          </li>
        ))}
      </ul>
    </div>
  );
}
