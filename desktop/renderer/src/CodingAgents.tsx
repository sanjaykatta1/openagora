// Settings → Coding agents: the coding agents on this computer, and the apps
// that take work. Setting an agent up runs the agent's own command, shown here
// first; connecting an app opens the app's own approval page. The engine does
// both, and writes them to the audit log.
import { useCallback, useEffect, useState } from "react";
import { bridge, type App, type CodingAgent, type CodingAgentsState, type EngineResult } from "./api";

interface Asking {
  title: string;
  lines: string[];
  commands: string[];
  confirm?: string;
  run?: () => Promise<void>;
}

const errorText = (e: unknown) =>
  (e instanceof Error ? e.message : String(e)).replace(/^Error invoking remote method '[^']+': (Error: )?/, "");

function agentState(a: CodingAgent): string {
  if (a.custom) return "added by you: it reads what Sunday sends it";
  if (!a.installed) return "not on this computer";
  if (a.set_up) return "set up: it reads what Sunday sends it";
  return `installed${a.version ? ` (${a.version})` : ""}, not set up`;
}

export function CodingAgentsSettings(props: { apps: App[] }) {
  const [state, setState] = useState<CodingAgentsState | null>(null);
  const [asking, setAsking] = useState<Asking | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ text: string; error: boolean } | null>(null);

  const load = useCallback(async () => {
    if (!bridge) return;
    try {
      setState(await bridge.codingAgents.state());
    } catch (e) {
      setMessage({ text: errorText(e), error: true });
    }
  }, []);
  useEffect(() => { void load(); }, [load]);

  const engine = (work: () => Promise<EngineResult>, done: string) => async () => {
    const result = await work();
    if (!result.ok) throw new Error(result.stderr.trim() || "That didn’t work.");
    setMessage({ text: done, error: false });
  };

  const answer = async () => {
    if (!asking?.run) { setAsking(null); return; }
    setBusy(true);
    setMessage(null);
    try {
      await asking.run();
      setAsking(null);
      await load();
    } catch (e) {
      setAsking(null);
      setMessage({ text: errorText(e), error: true });
    } finally {
      setBusy(false);
    }
  };

  if (!bridge) return null;
  const agents = state?.agents ?? [];
  const connections = state?.connections ?? [];
  // Apps that take work through their own MCP server, signed in with OAuth.
  const connectable = props.apps.filter((a) => a.installed && a.destination?.deliver === "mcp" && a.mcp_auth === "oauth");

  const askSetUp = (a: CodingAgent) => setAsking({
    title: `Set up ${a.name}?`,
    lines: [
      `OpenAgora adds an MCP server named “openagora” to ${a.name}’s own settings, so ${a.name} can read what Sunday sends it. It runs ${a.name}’s own command${a.id === "claude-code" ? "s" : ""}:`,
      ...(a.id === "claude-code" ? ["The plugin tells Claude Code about new items with your next prompt."] : []),
      "An agent only ever sees what Sunday sends it.",
    ],
    commands: [a.setup_command ?? "", ...(a.id === "claude-code" && a.hook_commands ? a.hook_commands : [])].filter(Boolean),
    confirm: "Run it",
    run: engine(() => bridge!.codingAgents.setUp(a.id), `${a.name} can now read what Sunday sends it. Ask it: “what did Sunday send me?”`),
  });

  const askRemove = (a: CodingAgent) => a.remove_command
    ? setAsking({
      title: `Remove OpenAgora from ${a.name}?`,
      lines: [`OpenAgora runs ${a.name}’s own command${a.id === "claude-code" ? "s" : ""}:`],
      commands: [a.remove_command, ...(a.id === "claude-code" && a.hook_remove_commands ? a.hook_remove_commands : [])],
      confirm: "Remove",
      run: engine(() => bridge!.codingAgents.remove(a.id), `${a.name} no longer has OpenAgora’s MCP server.`),
    })
    : setAsking({ title: `Remove OpenAgora from ${a.name}`, lines: [a.remove_by_hand ?? ""], commands: [] });

  const askConnect = (app: App) => setAsking({
    title: `Connect ${app.name}?`,
    lines: [
      `${app.name}’s own page opens in a window, and you approve OpenAgora there. Choose Supervised (or more) so Sunday can start threads; ${app.name} still asks you before each change it makes.`,
      `The sign-in lasts 30 days, and you can take it back in ${app.name}’s Settings → Connections.`,
    ],
    commands: [],
    confirm: `Open ${app.name}’s page`,
    run: async () => {
      const result = await bridge!.codingAgents.connect(app.id);
      setMessage({
        text: result.can_act
          ? `OpenAgora is connected to ${app.name}: Sunday can start threads there.`
          : `${app.name} gave OpenAgora read-only access: Sunday can read threads but not start them. Connect again and choose Supervised or more to let it.`,
        error: false,
      });
    },
  });

  return (
    <div className="panel" aria-labelledby="coding-title">
      <h3 id="coding-title">Coding agents</h3>
      <p className="muted small">Sunday can send what you’re working on to your coding agents. Each one reads what Sunday sent it with its own tools, through OpenAgora: ask it “what did Sunday send me?”. An agent sees only what was sent to it, never the whole desk.</p>
      <ul className="rows compact" aria-label="Coding agents">
        {agents.map((a) => (
          <li key={a.id} className="row">
            <span><strong>{a.name}</strong> <span className="muted small">{agentState(a)}</span></span>
            {!a.custom && a.installed && !a.set_up && <button className="btn" disabled={busy} onClick={() => askSetUp(a)}>Set up…</button>}
            {!a.custom && a.set_up && <button className="btn ghost" disabled={busy} onClick={() => askRemove(a)}>Remove…</button>}
          </li>
        ))}
      </ul>

      {connectable.length > 0 && <>
        <h4>Apps that take work</h4>
        <ul className="rows compact" aria-label="Apps that take work">
          {connectable.map((app) => {
            const c = connections.find((x) => x.app === app.id);
            const status = app.status !== "running" ? "not running: start it to connect"
              : !c ? "not connected"
              : !c.usable ? "signed out: connect again"
              : c.can_act ? "connected: Sunday can start threads" : "connected, read-only";
            return (
              <li key={app.id} className="row">
                <span><strong>{app.name}</strong> <span className="muted small">{status}</span></span>
                {app.status === "running" && (!c || !c.usable || !c.can_act) &&
                  <button className="btn" disabled={busy} onClick={() => askConnect(app)}>Connect…</button>}
                {c && <button className="btn ghost" disabled={busy}
                  onClick={() => void engine(() => bridge!.codingAgents.disconnect(app.id), `OpenAgora forgot its sign-in to ${app.name}.`)().then(load, (e) => setMessage({ text: errorText(e), error: true }))}>Disconnect</button>}
              </li>
            );
          })}
        </ul>
      </>}

      {asking && (
        <div className="modal-backdrop" onClick={() => !busy && setAsking(null)}>
          <div className="modal" role="dialog" aria-modal="true" aria-labelledby="coding-ask-title" onClick={(e) => e.stopPropagation()}>
            <h2 id="coding-ask-title">{asking.title}</h2>
            {asking.lines.slice(0, 1).map((line) => <p key={line}>{line}</p>)}
            {asking.commands.map((command) => <pre key={command} className="confirm-message"><code>{command}</code></pre>)}
            {asking.lines.slice(1).map((line) => <p key={line} className="muted small">{line}</p>)}
            {busy && <p role="status" className="muted small">Working…</p>}
            <footer className="modal-foot">
              <button className="btn ghost" disabled={busy} onClick={() => setAsking(null)}>{asking.run ? "Cancel" : "Close"}</button>
              {asking.run && <button className="btn primary" disabled={busy} onClick={() => void answer()} autoFocus>{asking.confirm}</button>}
            </footer>
          </div>
        </div>
      )}
      {message && <p role={message.error ? "alert" : "status"} className={message.error ? "agent-error" : undefined}>{message.text}</p>}
    </div>
  );
}
