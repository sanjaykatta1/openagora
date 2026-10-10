import { useCallback, useEffect, useState } from "react";
import { agentWorkflow } from "../../electron/agent-workflows";
import { PERSONAL_AGENTS, type PersonalAgent as Agent } from "../../electron/agents";
import { bridge, type App as Listing } from "./api";

function ExternalLink({ url, children }: { url: string; children: React.ReactNode }) {
  const [error, setError] = useState(false);
  return <>
    <a className="link" href={url} target="_blank" rel="noreferrer" onClick={(event) => {
      if (!bridge) return;
      event.preventDefault();
      setError(false);
      void bridge.openExternal(url).catch(() => setError(true));
    }}>{children} ↗</a>
    {error && <span role="alert">Couldn’t open the browser. Try again.</span>}
  </>;
}

export function AgentChooser(props: {
  firstLaunch: boolean;
  selected: string | null;
  saving: boolean;
  error: string | null;
  onSave: (id: string | null) => void;
  onCancel: () => void;
}) {
  const [selected, setSelected] = useState(props.selected);
  const agent = PERSONAL_AGENTS.find((a) => a.id === selected);
  return (
    <section className="agent-chooser" aria-labelledby="agent-heading">
      <header className="agent-intro">
        {props.firstLaunch && <div className="brand"><span className="brand-mark">◆</span> OpenAgora</div>}
        <p className="eyebrow">{props.firstLaunch ? "Make yourself at home" : "Your assistant, your choice"}</p>
        <h1 id="agent-heading">Choose your personal agent</h1>
        <p className="muted">Pick an open-source assistant that fits how you work. You can change this anytime.</p>
        <p className="muted small">This saves your preference. Installation and model setup come next; connecting agents across apps is still coming.</p>
      </header>
      <fieldset className="agent-options" disabled={props.saving}>
        <legend className="sr-only">Personal agent</legend>
        {PERSONAL_AGENTS.map((a) => (
          <div className={`agent-option${selected === a.id ? " selected" : ""}`} key={a.id}>
            <label>
              <input type="radio" name="personal-agent" value={a.id} checked={selected === a.id} onChange={() => setSelected(a.id)} />
              <span className="agent-option-text">
                <strong>{a.name}</strong>
                <span className="muted">{a.summary}</span>
              </span>
            </label>
            <div className="agent-option-foot small">
              <span className="muted">{a.license} · {a.catalogId ? "Install in OpenAgora" : "Set up separately"}</span>
              <ExternalLink url={a.source}>Source</ExternalLink>
            </div>
          </div>
        ))}
      </fieldset>
      <footer className="agent-choice-footer">
        {props.error && <p className="agent-error" role="alert">{props.error}</p>}
        <div className="agent-choice-actions">
          {props.firstLaunch ? (
            <button className="btn ghost" disabled={props.saving} onClick={() => props.onSave(null)}>Choose later</button>
          ) : (
            <>
              <button className="btn ghost" disabled={props.saving} onClick={props.onCancel}>Cancel</button>
              <button className="link" disabled={props.saving} onClick={() => props.onSave(null)}>Use without an agent</button>
            </>
          )}
          <button className="btn primary big" disabled={!agent || props.saving} onClick={() => props.onSave(selected)}>
            {props.saving ? "Saving…" : agent ? `Continue with ${agent.name}` : "Select an agent to continue"}
          </button>
        </div>
      </footer>
    </section>
  );
}

export function PersonalAgentPage(props: {
  agent: Agent | undefined;
  listing: Listing | undefined;
  busy?: string;
  onChange: () => void;
  onOpenListing: (id: string) => void;
  onOpenApp: (app: Listing) => void;
}) {
  const { agent, listing } = props;
  const [availability, setAvailability] = useState<{ detected: boolean; launchable: boolean } | null>(null);
  const [launching, setLaunching] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const refresh = useCallback(async () => {
    if (!agent || !bridge) return;
    setError(null);
    try { setAvailability(await bridge.agentAvailability(agent.id)); }
    catch { setError("Couldn’t check the local installation. Try Refresh."); }
  }, [agent?.id]);
  useEffect(() => { void refresh(); }, [refresh, listing?.installed]);
  const launch = async (action: string) => {
    if (!agent || !bridge) return;
    setLaunching(true);
    setError(null);
    setMessage(null);
    try {
      await bridge.launchAgent(agent.id, action);
      setMessage("Requested the agent’s own interface in Terminal. Complete any prompts there. Opening it does not mean setup is complete.");
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { setLaunching(false); }
  };
  const workflow = agent ? agentWorkflow(agent.id) : undefined;
  return <section className="page detail">
    <header className="page-head">
      <div><h1>Personal agent</h1><p className="muted">Your assistant, with its own interface and setup.</p></div>
      <button className="btn ghost" onClick={props.onChange}>{agent ? "Change agent" : "Choose an agent"}</button>
    </header>
    {agent && workflow ? <>
      <div className="panel agent-setup">
        <span className="pill">Your choice</span>
        <h2>{agent.name}</h2>
        <p>{agent.summary}</p>
        <p className="muted">{workflow.note}</p>
        <div className="actions">
          {listing?.available && (listing.installed ? (
            <button className="btn primary" disabled={!!props.busy} onClick={() => props.onOpenApp(listing)}>
              {props.busy ?? (listing.status === "running" ? `Open ${agent.name} in OpenAgora` : `Start and open ${agent.name}`)}
            </button>
          ) : (
            <button className="btn primary" disabled={!!props.busy} onClick={() => props.onOpenListing(listing.id)}>
              {props.busy ?? `Review ${agent.name} installation`}
            </button>
          ))}
          <ExternalLink url={agent.setup}>Official setup guide</ExternalLink>
          <ExternalLink url={agent.source}>Source code</ExternalLink>
        </div>
        <p className="muted small">{agent.license} · {listing?.installed ? "Installed through OpenAgora — finish setup in the agent" : agent.catalogId ? "Installation does not configure accounts or messaging" : "Install through the official project; OpenAgora does not manage this installation"}</p>
      </div>
      {workflow.executable && <div className="panel">
        <h3>Native agent shortcuts</h3>
        <p className="muted">These open {agent.name}’s own interactive commands in your terminal. Account sign-in, keys, QR codes, channels, and pairing stay in {agent.name}.</p>
        <p className="small muted">{!bridge ? "Available in the desktop app." : availability?.detected ? "CLI found. Configuration and connection status are shown by the agent itself." : "CLI not found yet. Desktop-only installations may not include it."}{" "}
          {bridge && <button className="link" onClick={() => void refresh()}>Refresh</button>}
        </p>
        <div className="native-agent-actions">
          {Object.entries(workflow.commands).map(([action, command]) => (
            <div key={action}>
              <button className="btn ghost" disabled={!availability?.detected || launching || !!props.busy} onClick={() => void launch(action)}>{command.label}</button>
              <code>{workflow.executable} {command.args.join(" ")}</code>
            </div>
          ))}
        </div>
        {message && <p role="status">{message}</p>}
      </div>}
      {error && <p role="alert" className="agent-error">{error}</p>}
      <p className="muted">Use the agent’s own settings for Telegram, WhatsApp, or any other channel it supports. Its gateway or service must remain running for phone messages; follow its background-service instructions if needed. OpenAgora does not add channels, collect credentials, or change the agent’s configuration.</p>
    </> : <p className="empty">You can use the Store without an agent. Choose one whenever you’re ready.</p>}
    <p className="muted small">Choosing or switching agents does not migrate data, stop another agent’s service, or grant access to other apps. Cross-app connections are not included in this release.</p>
  </section>;
}
