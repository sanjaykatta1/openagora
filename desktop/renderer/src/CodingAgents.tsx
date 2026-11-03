// Settings → Coding agents: the coding agents on this computer, and the apps
// that take work. Connecting an agent runs the agent's own commands, shown
// first; connecting an app opens the app's own approval page. The engine does
// both, and writes them to the audit log.
import { useState } from "react";
import { bridge, type App, type CodingAgent } from "./api";
import { Commands, Dialog, Row, useAction, useEngine, useNotify } from "./ui";

/** A question before a change: the commands that will run, or what to do. */
interface Asking {
  title: string;
  commands?: string[];
  text?: string;
  confirm?: string;
  run?: () => Promise<unknown>;
}

export function CodingAgentsSettings(props: { apps: App[] }) {
  const [state, reload] = useEngine((b) => b.codingAgents.state());
  const { run, busy } = useAction(reload);
  const notify = useNotify();
  const [asking, setAsking] = useState<Asking | null>(null);
  if (!bridge || !state) return null;
  // Apps that take work through their own MCP server, signed in with OAuth.
  const connectable = props.apps.filter((a) => a.installed && a.destination?.deliver === "mcp" && a.mcp_auth === "oauth");
  // Claude Code also gets OpenAgora's prompt-hook plugin, with its own commands.
  const hooks = (a: CodingAgent, commands?: string[]) => (a.id === "claude-code" && commands ? commands : []);

  const askConnect = (a: CodingAgent) => setAsking({
    title: `Connect ${a.name}?`,
    commands: [a.setup_command ?? "", ...hooks(a, a.hook_commands)].filter(Boolean),
    confirm: "Connect",
    run: () => bridge!.codingAgents.setUp(a.id),
  });
  const askDisconnect = (a: CodingAgent) => setAsking(a.remove_command
    ? { title: `Disconnect ${a.name}?`, commands: [a.remove_command, ...hooks(a, a.hook_remove_commands)], confirm: "Disconnect", run: () => bridge!.codingAgents.remove(a.id) }
    // The agent has no command for it: say how, and change nothing.
    : { title: `Disconnect ${a.name}`, text: a.remove_by_hand ?? "" });
  const askApp = (app: App) => setAsking({
    title: `Connect ${app.name}?`,
    text: `Choose Supervised on ${app.name}’s page so Sunday can start threads.`,
    confirm: `Open ${app.name}`,
    run: async () => {
      const result = await bridge!.codingAgents.connect(app.id);
      if (!result.can_act) notify(`${app.name} allowed reading only. Connect again and choose Supervised so Sunday can start threads.`);
    },
  });
  const answer = (work: () => Promise<unknown>) => void run(work).then(() => setAsking(null));

  return (
    <>
      <ul className="group" aria-label="Coding agents">
        {state.agents.map((a) => (
          <Row key={a.id} label={a.name}>
            {a.custom ? <span className="value">Added by you</span>
              : !a.installed ? <span className="value">Not installed</span>
              : a.set_up ? <button className="btn" disabled={busy} onClick={() => askDisconnect(a)}>Disconnect…</button>
              : <button className="btn" disabled={busy} onClick={() => askConnect(a)}>Connect…</button>}
          </Row>
        ))}
      </ul>

      {connectable.length > 0 && <section>
        <h2>Apps</h2>
        <ul className="group" aria-label="Apps">
          {connectable.map((app) => {
            const c = state.connections.find((x) => x.app === app.id);
            const signedIn = !!c && c.usable;
            return (
              <Row key={app.id} label={app.name}>
                {signedIn && !c.can_act && <span className="value">Read-only</span>}
                {app.status !== "running" ? <span className="value">Not running</span>
                  : !(signedIn && c.can_act) && <button className="btn" disabled={busy} onClick={() => askApp(app)}>Connect…</button>}
                {c && <button className="btn" disabled={busy} onClick={() => void run(() => bridge!.codingAgents.disconnect(app.id))}>Disconnect</button>}
              </Row>
            );
          })}
        </ul>
      </section>}

      {asking && (
        <Dialog title={asking.title} onCancel={busy ? undefined : () => setAsking(null)} actions={<>
          <button className="btn" disabled={busy} onClick={() => setAsking(null)}>{asking.run ? "Cancel" : "Close"}</button>
          {asking.run && <button className="btn primary" autoFocus disabled={busy} onClick={() => answer(asking.run!)}>{busy ? "Working…" : asking.confirm}</button>}
        </>}>
          {asking.commands && <Commands lines={asking.commands} />}
          {asking.text && <p>{asking.text}</p>}
        </Dialog>
      )}
    </>
  );
}
