import { useState } from "react";
import { PERSONAL_AGENTS } from "../../electron/agents";
import { bridge } from "./api";

export function ExternalLink({ url, children }: { url: string; children: React.ReactNode }) {
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
  const agent = PERSONAL_AGENTS.find((a) => a.id === selected && !a.comingSoon);
  return (
    <section className="agent-chooser" aria-labelledby="agent-heading">
      <header className="agent-intro">
        {props.firstLaunch && <div className="brand"><span className="brand-mark">◆</span> OpenAgora</div>}
        <p className="eyebrow">{props.firstLaunch ? "Make yourself at home" : "Your assistant"}</p>
        <h1 id="agent-heading">Choose your personal agent</h1>
        <p className="muted">Sunday is OpenAgora’s personal agent: it remembers across your apps and carries your work to the coding agents you use. More agents are coming soon.</p>
        <p className="muted small">This saves your choice. Installing Hermes and choosing a model come next, in Hermes’s own setup.</p>
      </header>
      <fieldset className="agent-options" disabled={props.saving}>
        <legend className="sr-only">Personal agent</legend>
        {PERSONAL_AGENTS.map((a) => (
          <div className={`agent-option${selected === a.id ? " selected" : ""}${a.comingSoon ? " coming-soon" : ""}`} key={a.id}>
            <label>
              <input type="radio" name="personal-agent" value={a.id} checked={selected === a.id} disabled={a.comingSoon}
                onChange={() => setSelected(a.id)} />
              <span className="agent-option-text">
                <strong>{a.name}{a.comingSoon && <span className="pill soon">Coming soon</span>}</strong>
                <span className="muted">{a.summary}</span>
              </span>
            </label>
            <div className="agent-option-foot small">
              <span className="muted">{a.license} · {a.poweredBy ? `Powered by ${a.poweredBy}` : "Not available yet"}</span>
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
