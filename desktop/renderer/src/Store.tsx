// The store, the library and an app's page, and the question before an
// install: the app's source, the exact steps, and what Sunday may do with it.
import { useState } from "react";
import { bridge, osName, type App } from "./api";
import { Dialog, ExternalLink, Monogram } from "./ui";

const notAvailable = () => `Not available on ${osName(bridge?.platform)}`;

export function Store(props: { apps: App[]; busy: Record<string, string>; onOpen: (id: string) => void; onInstall: (app: App) => void }) {
  const [query, setQuery] = useState("");
  const q = query.trim().toLowerCase();
  const shown = props.apps.filter((a) => !q || [a.name, a.summary, a.category, a.id].some((f) => f.toLowerCase().includes(q)));
  return (
    <section className="page wide">
      <header className="page-head">
        <h1>Store</h1>
        <input className="search" type="search" placeholder="Search" aria-label="Search apps" value={query} onChange={(e) => setQuery(e.target.value)} autoFocus />
      </header>
      {shown.length === 0 ? <p className="empty">No apps match “{query}”.</p> : (
        <div className="apps">
          {shown.map((app) => (
            <article key={app.id} className={app.available ? "app-card" : "app-card unavailable"}>
              <Monogram app={app} />
              <div className="app-text">
                <button className="card-link" onClick={() => props.onOpen(app.id)}>{app.name}</button>
                <p className="muted clamp">{app.summary}</p>
              </div>
              {!app.available ? <span className="muted">{notAvailable()}</span>
                : props.busy[app.id] ? <span className="muted">{props.busy[app.id]}</span>
                : app.installed ? <button className="btn" onClick={() => props.onOpen(app.id)}>Open</button>
                : <button className="btn primary" onClick={() => props.onInstall(app)}>Install</button>}
            </article>
          ))}
        </div>
      )}
    </section>
  );
}

type RunProps = {
  busy?: string;
  onInstall: (app: App) => void;
  onStart: (app: App) => void;
  onStop: (app: App) => void;
  onOpenTab: (id: string) => void;
  onOpenWindow: (app: App) => void;
};

export function Library(props: RunProps & { apps: App[]; busyById: Record<string, string>; onOpenPage: (id: string) => void; onBrowse: () => void }) {
  return (
    <section className="page">
      <header className="page-head"><h1>Library</h1></header>
      {props.apps.length === 0 ? (
        <div className="empty">
          <p>No apps yet</p>
          <button className="btn primary" onClick={props.onBrowse}>Browse the Store</button>
        </div>
      ) : (
        <ul className="group" aria-label="Library">
          {props.apps.map((app) => (
            <li key={app.id} className="row">
              <button className="row-label row-main" onClick={() => props.onOpenPage(app.id)}>
                <Monogram app={app} /> <strong>{app.name}</strong>
                {app.status === "running" && <span className="dot on" title="Running"><span className="sr-only">Running</span></span>}
              </button>
              <RunButtons {...props} app={app} busy={props.busyById[app.id]} />
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

/** Open, start, stop or reinstall, as the app's state allows. */
function RunButtons(props: RunProps & { app: App }) {
  const { app } = props;
  if (props.busy) return <span className="muted">{props.busy}</span>;
  if (!app.present) return <button className="btn" onClick={() => props.onInstall(app)}>Reinstall…</button>;
  const running = app.status === "running";
  const open = app.can_open_window ? () => props.onOpenWindow(app)
    : app.ui === "web" ? () => (running ? props.onOpenTab(app.id) : props.onStart(app))
    : null;
  return (
    <span className="actions">
      {open ? <button className="btn primary" onClick={open}>Open</button>
        : !running && <button className="btn primary" onClick={() => props.onStart(app)}>Start</button>}
      {running && <button className="btn" onClick={() => props.onStop(app)}>Stop</button>}
    </span>
  );
}

export function Detail(props: RunProps & { app: App; progress: string[]; back: string; onBack: () => void; onUninstall: (app: App) => void }) {
  const { app } = props;
  const [removing, setRemoving] = useState(false);
  const agent = app.role === "agent";
  return (
    <section className="page">
      <button className="back" onClick={props.onBack}>‹ {props.back}</button>
      <header className="detail-head">
        <Monogram app={app} size="lg" />
        <div className="detail-title">
          <h1>{app.name}</h1>
          <p className="muted">
            <ExternalLink url={app.homepage}>Website</ExternalLink> · <ExternalLink url={app.source}>Source</ExternalLink> · {app.license}
          </p>
        </div>
        {!app.available ? <span className="muted">{notAvailable()}</span>
          : !app.installed ? <button className="btn primary big" disabled={!!props.busy || !bridge} onClick={() => props.onInstall(app)}>{props.busy ?? "Install"}</button>
          : <RunButtons {...props} />}
      </header>
      <p className="lead">{app.summary}</p>
      {app.setup_notes && <p className="notice">{app.setup_notes}</p>}
      {props.progress.length > 0 && <pre className="code log">{props.progress.join("\n")}</pre>}
      {app.installed && (
        <footer className="detail-foot">
          <button className="btn" disabled={!!props.busy} onClick={() => setRemoving(true)}>{agent ? "Remove Sunday…" : `Remove ${app.name}…`}</button>
        </footer>
      )}
      {removing && (
        <Dialog title={agent ? "Remove Sunday?" : app.adopted ? `Stop managing ${app.name}?` : `Remove ${app.name}?`} onCancel={() => setRemoving(false)} actions={<>
          <button className="btn" autoFocus onClick={() => setRemoving(false)}>Cancel</button>
          <button className="btn danger" onClick={() => { setRemoving(false); props.onUninstall(app); }}>Remove</button>
        </>}>
          <p>{agent ? `Deletes Hermes’s “${app.profile}” profile, which holds Sunday. ${app.name} stays.`
            : app.adopted ? `${app.name} stays on this computer.` : `Stops ${app.name} and deletes its folder.`}</p>
        </Dialog>
      )}
    </section>
  );
}

/** Before installing: where it comes from, the exact steps, and what Sunday may do with it. */
export function InstallDialog(props: { app: App; onCancel: () => void; onConfirm: () => void }) {
  const { app } = props;
  return (
    <Dialog title={`Install ${app.name}?`} onCancel={props.onCancel} actions={<>
      <button className="btn" onClick={props.onCancel}>Cancel</button>
      <button className="btn primary" autoFocus onClick={props.onConfirm}>Install</button>
    </>}>
      <p className="muted">{app.source.replace("https://", "")}</p>
      {app.setup_notes && <p className="notice">{app.setup_notes}</p>}
      {app.present ? <p>{app.name} is already on this computer. OpenAgora will use it as it is.</p>
        : !app.available ? <p>{notAvailable()}</p>
        : <ol className="steps">
          {app.steps.map((step, i) => (
            <li key={i}>
              {step.download && <span>Download {step.download.asset ?? "(no build for this CPU)"} from github.com/{step.download.github}</span>}
              <code>{step.run}</code>
            </li>
          ))}
        </ol>}
      {app.permissions.length > 0 && <>
        <h3>Sunday’s permissions</h3>
        <ul className="perms">
          {app.permissions.map((p) => (
            <li key={p.id}><span className={`perm ${p.default}`}>{{ granted: "Allowed", ask: "Asks first", denied: "Not allowed" }[p.default]}</span>{p.description}</li>
          ))}
        </ul>
      </>}
    </Dialog>
  );
}
