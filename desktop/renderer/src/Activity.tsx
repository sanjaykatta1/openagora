// Settings → Activity: the audit log. Every app action, capture, handoff,
// read of a coding agent's sessions and change of permission, with who asked
// and who said yes. It stays on this computer; the engine writes it.
import { useMemo, useState } from "react";
import { bridge } from "./api";
import { useEngine } from "./ui";

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

export function ActivitySettings() {
  const [entries, reload] = useEngine((b) => b.auditLog());
  const [client, setClient] = useState("all");
  const [query, setQuery] = useState("");

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
    <>
      <div className="toolbar">
        <select aria-label="Who" value={client} onChange={(e) => setClient(e.target.value)}>
          <option value="all">Everyone</option>
          {clients.map((c) => <option key={c} value={c}>{who(c)}</option>)}
        </select>
        <input type="search" aria-label="Search the activity" placeholder="Search" value={query} onChange={(e) => setQuery(e.target.value)} />
        <button className="btn" onClick={() => void reload()}>Refresh</button>
      </div>
      <ul className="group" aria-label="Audit log">
        {entries && shown.length === 0 && <li className="row muted">Nothing yet</li>}
        {shown.map((e, n) => (
          <li key={`${e.at}-${n}`} className="row">
            <span className="row-label">
              <strong>{who(e.client)}</strong> {what(e.action)}{e.target ? `: ${e.target}` : ""}{" "}
              <span className="muted">— {DECISION[e.decision] ?? e.decision}{e.via ? ` ${via(e.via)}` : ""}{e.outcome && e.outcome !== "ok" ? `; ${e.outcome}` : ""}</span>
            </span>
            <time className="value" dateTime={new Date(e.at * 1000).toISOString()}>{new Date(e.at * 1000).toLocaleString([], { dateStyle: "medium", timeStyle: "short" })}</time>
          </li>
        ))}
      </ul>
    </>
  );
}
