import { useState } from "react";
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
  onChange: () => void;
  onOpenListing: (id: string) => void;
}) {
  const { agent, listing } = props;
  return <section className="page detail">
    <header className="page-head">
      <div><h1>Personal agent</h1><p className="muted">Your preferred assistant in OpenAgora.</p></div>
      <button className="btn ghost" onClick={props.onChange}>{agent ? "Change agent" : "Choose an agent"}</button>
    </header>
    {agent ? <div className="panel agent-setup">
      <span className="pill">Your choice</span>
      <h2>{agent.name}</h2>
      <p>{agent.summary}</p>
      <p className="muted">{agent.note}</p>
      <div className="actions">
        {listing?.available && <button className="btn primary" onClick={() => props.onOpenListing(listing.id)}>
          {listing.installed ? `Open ${agent.name}` : `Review ${agent.name} installation`}
        </button>}
        <ExternalLink url={agent.setup}>Setup guide</ExternalLink>
        <ExternalLink url={agent.source}>Source code</ExternalLink>
      </div>
      <p className="muted small">{agent.license} · {listing?.installed ? "Installed through OpenAgora" : agent.catalogId ? "Install separately from choosing" : "Setup outside OpenAgora; installation status isn’t tracked here"}</p>
    </div> : <p className="empty">You can use the Store without an agent. Choose one whenever you’re ready.</p>}
    <p className="muted">Choosing an agent saves your preference on this computer. It doesn’t connect it to your apps or grant access to them. Cross-app connections are coming later.</p>
  </section>;
}
