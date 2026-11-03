// Settings → Privacy: the apps the user allowed Sunday and coding agents to
// see, what Sunday may hand over without asking, and the never-capture list.
// The engine keeps and enforces all of it; this page shows it and passes the
// user's changes on.
import { useState } from "react";
import { bridge, type Grant } from "./api";
import { clientName } from "./Confirm";
import { Row, useAction, useEngine } from "./ui";

/** What a handoff grant lets Sunday do without asking. */
const handoff = (g: Grant) => g.type === "tasks/*" ? `Start tasks in ${clientName(g.destination)}`
  : g.type === "act/*" ? `Use ${clientName(g.destination)}’s tools`
  : `${g.source_name} → ${clientName(g.destination)}`;

export function PrivacySettings() {
  const [broker, reload] = useEngine((b) => b.broker.state());
  const [desktop] = useEngine((b) => b.desktopSettings());
  const { run, busy } = useAction(reload);
  const [neverApp, setNeverApp] = useState("");
  const [neverTitle, setNeverTitle] = useState("");
  if (!bridge || !broker) return null;

  const grants = broker.grants;
  // Reading coding sessions is in Settings → Memory.
  const seeing = grants.filter((g) => g.client === "sunday" && g.destination === "sunday" && g.type !== "sessions/*");
  const sending = grants.filter((g) => g.client === "sunday" && g.destination !== "sunday");
  const agentsSee = grants.filter((g) => g.client !== "sunday");
  const revoke = (g: Grant) => void run(() => bridge!.broker.revoke(g.source, { client: g.client, destination: g.destination }));
  const remove = (g: Grant) => <button className="btn" disabled={busy} onClick={() => revoke(g)}>Remove</button>;
  const once = (g: Grant) => g.scope === "once" && <span className="value">Once</span>;
  const permissions = desktop?.platform === "darwin" && [
    desktop.screen !== "granted" && <Row key="screen" label="Screen Recording">
      <button className="btn" onClick={() => void bridge!.openScreenSettings()}>Allow in System Settings</button>
    </Row>,
    desktop.accessibility === false && <Row key="text" label="Accessibility">
      <button className="btn" onClick={() => void bridge!.openAccessibilitySettings()}>Allow in System Settings</button>
    </Row>,
  ].filter(Boolean);

  return (
    <>
      {permissions && permissions.length > 0 && <ul className="group" aria-label="macOS permissions">{permissions}</ul>}

      <section>
        <h2>Apps Sunday can see</h2>
        <ul className="group" aria-label="Apps Sunday can see">
          {seeing.length === 0 && <li className="row muted">None</li>}
          {seeing.map((g) => <Row key={`${g.source}:${g.scope}`} label={g.source_name}>{once(g)}{remove(g)}</Row>)}
        </ul>
      </section>

      {sending.length > 0 && <section>
        <h2>Allowed without asking</h2>
        <ul className="group" aria-label="Allowed without asking">
          {sending.map((g) => <Row key={`${g.source}:${g.type}:${g.destination}:${g.scope}`} label={handoff(g)}>{once(g)}{remove(g)}</Row>)}
        </ul>
      </section>}

      {agentsSee.length > 0 && <section>
        <h2>Coding agents can see</h2>
        <ul className="group" aria-label="Coding agents can see">
          {agentsSee.map((g) => <Row key={`${g.client}:${g.source}:${g.type}`} label={`${g.source_name} → ${clientName(g.client)}`}>{once(g)}{remove(g)}</Row>)}
        </ul>
      </section>}

      <section>
        <h2>Never captured</h2>
        <ul className="group" aria-label="Never captured">
          <Row label="Built in"><span className="value">{broker.never.built_in.map((r) => r.name).join(", ")}</span></Row>
          {broker.never.user.map((r) => (
            <Row key={r.name} label={r.name}>
              <span className="value">{[...(r.apps ?? []), ...(r.titles ?? []).map((t) => `“${t}”`)].filter((v) => v !== r.name).join(", ")}</span>
              <button className="btn" disabled={busy} onClick={() => void run(() => bridge!.broker.neverRemove(r.name))}>Remove</button>
            </Row>
          ))}
          <li className="row">
            <form className="inline-form" onSubmit={(e) => {
              e.preventDefault();
              const app = neverApp.trim();
              const title = neverTitle.trim();
              if (!app && !title) return;
              void run(() => bridge!.broker.neverAdd({ name: app || title, ...(app ? { app } : {}), ...(title ? { title } : {}) }))
                .then((ok) => { if (ok) { setNeverApp(""); setNeverTitle(""); } });
            }}>
              <input aria-label="App" placeholder="App" value={neverApp} onChange={(e) => setNeverApp(e.target.value)} />
              <input aria-label="Window title" placeholder="Window title" value={neverTitle} onChange={(e) => setNeverTitle(e.target.value)} />
              <button className="btn" type="submit" disabled={busy || (!neverApp.trim() && !neverTitle.trim())}>Add</button>
            </form>
          </li>
        </ul>
      </section>
    </>
  );
}
