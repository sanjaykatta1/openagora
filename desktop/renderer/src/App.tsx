import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { bridge, osName, type App as Listing, type EngineResult, type UpdateState } from "./api";
import { NEW_AGENT_PREFERENCES, PERSONAL_AGENTS, parseAgentPreferences, type AgentPreferences } from "../../electron/agents";
import { AgentChooser, PersonalAgentPage } from "./PersonalAgent";

type View = { kind: "store" } | { kind: "library" } | { kind: "personal-agent" } | { kind: "app"; id: string } | { kind: "tab"; id: string };

const PREVIEW_PREFERENCES = "openagora-preview-personal-agent";

const CATEGORY_LABELS: Record<string, string> = {
  agents: "Agents",
  coding: "Coding",
  utilities: "Utilities",
};

const label = (category: string) =>
  CATEGORY_LABELS[category] ?? category.charAt(0).toUpperCase() + category.slice(1);

export function App() {
  const [apps, setApps] = useState<Listing[]>([]);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [view, setView] = useState<View>({ kind: "store" });
  const [tabs, setTabs] = useState<string[]>([]);
  const [busy, setBusy] = useState<Record<string, string>>({});
  const [progress, setProgress] = useState<Record<string, string[]>>({});
  const [confirming, setConfirming] = useState<Listing | null>(null);
  const [toast, setToast] = useState<{ text: string; error: boolean } | null>(null);
  const [preferences, setPreferences] = useState<AgentPreferences | null>(null);
  const [preferenceError, setPreferenceError] = useState<string | null>(null);
  const [savingPreferences, setSavingPreferences] = useState(false);
  const [choosingAgent, setChoosingAgent] = useState(false);

  const loadPreferences = useCallback(async () => {
    setPreferenceError(null);
    try {
      const saved = bridge ? await bridge.agentPreferences() : JSON.parse(localStorage.getItem(PREVIEW_PREFERENCES) ?? "null");
      setPreferences(parseAgentPreferences(saved ?? NEW_AGENT_PREFERENCES));
    } catch {
      setPreferenceError("Couldn’t read your saved agent choice. Retry, or choose again to replace it.");
    }
  }, []);

  useEffect(() => { void loadPreferences(); }, [loadPreferences]);

  const saveAgent = async (id: string | null) => {
    setSavingPreferences(true);
    setPreferenceError(null);
    try {
      const next = parseAgentPreferences({ version: 1, completed: true, selectedAgent: id });
      if (bridge) await bridge.saveAgentPreferences(next);
      else localStorage.setItem(PREVIEW_PREFERENCES, JSON.stringify(next));
      setPreferences(next);
      setChoosingAgent(false);
      setView({ kind: id ? "personal-agent" : "store" });
    } catch {
      setPreferenceError("Couldn’t save your choice. Please try again.");
    } finally {
      setSavingPreferences(false);
    }
  };

  const refresh = useCallback(async () => {
    if (!bridge) return;
    try {
      setApps(await bridge.catalog());
      setLoadError(null);
    } catch (e) {
      setLoadError(String(e instanceof Error ? e.message : e));
    }
  }, []);

  useEffect(() => {
    void refresh();
    if (!bridge) return;
    const off = bridge.onProgress(({ id, line }) =>
      setProgress((p) => ({ ...p, [id]: [...(p[id] ?? []), line].slice(-200) })),
    );
    const timer = window.setInterval(() => void refresh(), 5000);
    return () => {
      off();
      window.clearInterval(timer);
    };
  }, [refresh]);

  useEffect(() => {
    if (!toast) return;
    const t = window.setTimeout(() => setToast(null), toast.error ? 9000 : 4000);
    return () => window.clearTimeout(t);
  }, [toast]);

  const byId = useMemo(() => new Map(apps.map((a) => [a.id, a])), [apps]);

  const act = useCallback(
    async (app: Listing, action: "install" | "uninstall" | "start" | "stop", verb: string) => {
      if (!bridge) return;
      setBusy((b) => ({ ...b, [app.id]: verb }));
      if (action === "install") setProgress((p) => ({ ...p, [app.id]: [] }));
      let result: EngineResult;
      try {
        result = await bridge[action](app.id);
      } catch (e) {
        result = { ok: false, stdout: "", stderr: String(e) };
      }
      setBusy(({ [app.id]: _done, ...rest }) => rest);
      await refresh();
      if (result.ok) {
        const done = { install: "installed", uninstall: "removed", start: "started", stop: "stopped" }[action];
        setToast({ text: `${app.name} ${done}.`, error: false });
        if (action === "install" && app.id === preferences?.selectedAgent) setView({ kind: "personal-agent" });
        if (action === "stop" || action === "uninstall") setTabs((t) => t.filter((id) => id !== app.id));
        if (action === "start" && app.ui === "web") openTab(app.id);
      } else {
        setToast({ text: result.stderr.trim() || `${app.name}: ${action} failed.`, error: true });
      }
    },
    // openTab only calls state setters, so it never goes stale.
    [refresh, preferences?.selectedAgent],
  );

  const openTab = (id: string) => {
    setTabs((t) => (t.includes(id) ? t : [...t, id]));
    setView({ kind: "tab", id });
  };

  const closeTab = (id: string) => {
    setTabs((t) => t.filter((x) => x !== id));
    if (view.kind === "tab" && view.id === id) setView({ kind: "library" });
  };

  const installedCount = apps.filter((a) => a.installed).length;
  const personalAgent = PERSONAL_AGENTS.find((a) => a.id === preferences?.selectedAgent);

  if (!preferences) return (
    <main className="onboarding loading" aria-live="polite">
      {preferenceError ? <div>
        <p role="alert">{preferenceError}</p>
        <div className="actions">
          <button className="btn primary" onClick={() => void loadPreferences()}>Retry</button>
          <button className="btn ghost" onClick={() => {
            setPreferenceError(null);
            setPreferences({ ...NEW_AGENT_PREFERENCES });
          }}>Choose again</button>
        </div>
      </div> : <p className="muted">Opening OpenAgora…</p>}
    </main>
  );

  if (!preferences.completed) return <main className="onboarding">
    {!bridge && <div className="notice">Browser preview — your choice here is separate from the desktop app.</div>}
    <AgentChooser firstLaunch selected={preferences.selectedAgent} saving={savingPreferences} error={preferenceError}
      onSave={(id) => void saveAgent(id)} onCancel={() => {}} />
  </main>;

  return (
    <div className="shell">
      <aside className="sidebar">
        <div className="brand">
          <span className="brand-mark">◆</span> OpenAgora
        </div>
        <nav>
          <button className={view.kind === "store" || view.kind === "app" ? "nav active" : "nav"} onClick={() => setView({ kind: "store" })}>
            <span className="nav-icon">⌂</span> Store
          </button>
          <button className={view.kind === "library" ? "nav active" : "nav"} onClick={() => setView({ kind: "library" })}>
            <span className="nav-icon">▤</span> Library
            {installedCount > 0 && <span className="count">{installedCount}</span>}
          </button>
          <button className={view.kind === "personal-agent" ? "nav active" : "nav"} onClick={() => {
            setChoosingAgent(false);
            setPreferenceError(null);
            setView({ kind: "personal-agent" });
          }}>
            <span className="nav-icon">◇</span>
            <span>Personal agent<span className="muted small block">{personalAgent?.name ?? "Choose anytime"}</span></span>
          </button>
        </nav>
        {tabs.length > 0 && (
          <>
            <div className="nav-heading">Open apps</div>
            <nav>
              {tabs.map((id) => {
                const app = byId.get(id);
                if (!app) return null;
                return (
                  <div key={id} className={view.kind === "tab" && view.id === id ? "nav tab active" : "nav tab"}>
                    <button className="tab-open" onClick={() => setView({ kind: "tab", id })}>
                      <Monogram app={app} size="xs" /> {app.name}
                    </button>
                    <button className="tab-close" title={`Close ${app.name} tab`} onClick={() => closeTab(id)}>
                      ×
                    </button>
                  </div>
                );
              })}
            </nav>
          </>
        )}
        <UpdateCard />
        <div className="sidebar-foot">{bridge ? `Running on ${osName(bridge.platform)}` : "Preview"}</div>
      </aside>

      <main className="content">
        {!bridge && (
          <div className="notice">
            This is the OpenAgora window opened in a browser. Install, start and stop only work in the desktop app.
          </div>
        )}
        {loadError && <div className="notice error">Couldn't read the catalog: {loadError}</div>}

        {view.kind === "personal-agent" && (choosingAgent ? (
          <AgentChooser firstLaunch={false} selected={preferences.selectedAgent} saving={savingPreferences} error={preferenceError}
            onSave={(id) => void saveAgent(id)} onCancel={() => { setChoosingAgent(false); setPreferenceError(null); }} />
        ) : (
          <PersonalAgentPage key={personalAgent?.id ?? "none"} agent={personalAgent} listing={personalAgent?.catalogId ? byId.get(personalAgent.catalogId) : undefined}
            busy={personalAgent?.catalogId ? busy[personalAgent.catalogId] : undefined}
            onChange={() => setChoosingAgent(true)} onOpenListing={(id) => setView({ kind: "app", id })}
            onOpenApp={(app) => app.status === "running" ? openTab(app.id) : void act(app, "start", "Starting…")} />
        ))}

        {view.kind === "store" && <Store apps={apps} busy={busy} onOpen={(id) => setView({ kind: "app", id })} onInstall={setConfirming} />}

        {view.kind === "library" && (
          <Library
            apps={apps.filter((a) => a.installed)}
            busy={busy}
            onOpenPage={(id) => setView({ kind: "app", id })}
            onStart={(a) => act(a, "start", "Starting…")}
            onStop={(a) => act(a, "stop", "Stopping…")}
            onOpenTab={openTab}
            onBrowse={() => setView({ kind: "store" })}
          />
        )}

        {view.kind === "app" && byId.get(view.id) && (
          <Detail
            app={byId.get(view.id)!}
            busy={busy[view.id]}
            progress={progress[view.id] ?? []}
            onBack={() => setView({ kind: "store" })}
            onInstall={setConfirming}
            onUninstall={(a) => act(a, "uninstall", "Removing…")}
            onStart={(a) => act(a, "start", "Starting…")}
            onStop={(a) => act(a, "stop", "Stopping…")}
            onOpenTab={openTab}
          />
        )}

        {tabs.map((id) => {
          const app = byId.get(id);
          if (!app) return null;
          const visible = view.kind === "tab" && view.id === id;
          // An app can stop on its own (crash, quit elsewhere). Say so in its
          // tab rather than leaving the tab blank.
          if (app.status !== "running" || app.pid === null) {
            return (
              <StoppedTab
                key={id}
                app={app}
                visible={visible}
                busy={busy[id]}
                onStart={() => act(app, "start", "Starting…")}
              />
            );
          }
          return <AppTab key={`${id}:${app.pid}`} app={app} visible={visible} />;
        })}
      </main>

      {confirming && (
        <ConfirmInstall
          app={confirming}
          onCancel={() => setConfirming(null)}
          onConfirm={() => {
            const app = confirming;
            setConfirming(null);
            setView({ kind: "app", id: app.id });
            void act(app, "install", "Installing…");
          }}
        />
      )}

      {toast && (
        <div className={toast.error ? "toast error" : "toast"} role="status" onClick={() => setToast(null)}>
          {toast.text}
        </div>
      )}
    </div>
  );
}

function Monogram({ app, size = "md" }: { app: Listing; size?: "xs" | "md" | "lg" }) {
  const hue = [...app.id].reduce((h, c) => (h * 31 + c.charCodeAt(0)) % 360, 7);
  return (
    <span className={`monogram ${size}`} style={{ background: `hsl(${hue} 55% 46%)` }} aria-hidden>
      {app.name.charAt(0)}
    </span>
  );
}

function StatusPill({ app }: { app: Listing }) {
  if (app.status === "running") return <span className="pill running">Running</span>;
  if (app.installed) return <span className="pill">Installed</span>;
  return null;
}

function Store(props: {
  apps: Listing[];
  busy: Record<string, string>;
  onOpen: (id: string) => void;
  onInstall: (app: Listing) => void;
}) {
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState("all");
  const categories = useMemo(() => [...new Set(props.apps.map((a) => a.category))].sort(), [props.apps]);
  const q = query.trim().toLowerCase();
  const shown = props.apps.filter(
    (a) =>
      (category === "all" || a.category === category) &&
      (!q || [a.name, a.summary, a.category, a.id].some((f) => f.toLowerCase().includes(q))),
  );

  return (
    <section className="page">
      <header className="page-head">
        <div>
          <h1>Store</h1>
          <p className="muted">Open-source apps, installed from their official source.</p>
        </div>
        <input
          className="search"
          type="search"
          placeholder="Search apps"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          autoFocus
        />
      </header>
      <div className="chips">
        {["all", ...categories].map((c) => (
          <button key={c} className={c === category ? "chip active" : "chip"} onClick={() => setCategory(c)}>
            {c === "all" ? "All" : label(c)}
          </button>
        ))}
      </div>
      {shown.length === 0 ? (
        <p className="empty">No apps match “{query}”.</p>
      ) : (
        <div className="grid">
          {shown.map((app) => (
            <article key={app.id} className={app.available ? "card" : "card unavailable"} onClick={() => props.onOpen(app.id)}>
              <div className="card-top">
                <Monogram app={app} />
                <div className="card-title">
                  <h2>{app.name}</h2>
                  <span className="muted small">{label(app.category)}</span>
                </div>
                <StatusPill app={app} />
              </div>
              <p className="summary">{app.summary}</p>
              <div className="card-foot">
                <span className="muted small">{app.license}</span>
                {!app.available ? (
                  <span className="muted small">Not available on {osName(window.openagora?.platform)}</span>
                ) : props.busy[app.id] ? (
                  <span className="small">{props.busy[app.id]}</span>
                ) : app.installed ? (
                  <button className="btn ghost" onClick={(e) => (e.stopPropagation(), props.onOpen(app.id))}>
                    Open
                  </button>
                ) : (
                  <button className="btn primary" onClick={(e) => (e.stopPropagation(), props.onInstall(app))}>
                    Install
                  </button>
                )}
              </div>
            </article>
          ))}
        </div>
      )}
    </section>
  );
}

function Library(props: {
  apps: Listing[];
  busy: Record<string, string>;
  onOpenPage: (id: string) => void;
  onStart: (app: Listing) => void;
  onStop: (app: Listing) => void;
  onOpenTab: (id: string) => void;
  onBrowse: () => void;
}) {
  return (
    <section className="page">
      <header className="page-head">
        <div>
          <h1>Library</h1>
          <p className="muted">Apps you've installed.</p>
        </div>
      </header>
      {props.apps.length === 0 ? (
        <div className="empty">
          <p>Nothing installed yet.</p>
          <button className="btn primary" onClick={props.onBrowse}>
            Browse the Store
          </button>
        </div>
      ) : (
        <ul className="rows">
          {props.apps.map((app) => (
            <li key={app.id} className="row">
              <button className="row-main" onClick={() => props.onOpenPage(app.id)}>
                <Monogram app={app} />
                <span>
                  <strong>{app.name}</strong>
                  <span className="muted small block">
                    {app.status === "running"
                      ? app.ui === "background"
                        ? "Running in the background"
                        : "Running"
                      : "Stopped"}
                  </span>
                </span>
              </button>
              <span className={app.status === "running" ? "dot on" : "dot"} />
              <RunButtons app={app} busy={props.busy[app.id]} onStart={props.onStart} onStop={props.onStop} onOpenTab={props.onOpenTab} />
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function RunButtons(props: {
  app: Listing;
  busy?: string;
  onStart: (app: Listing) => void;
  onStop: (app: Listing) => void;
  onOpenTab: (id: string) => void;
}) {
  const { app } = props;
  if (props.busy) return <span className="small">{props.busy}</span>;
  if (app.status === "running")
    return (
      <span className="actions">
        {app.ui === "web" && (
          <button className="btn primary" onClick={() => props.onOpenTab(app.id)}>
            Open
          </button>
        )}
        <button className="btn ghost" onClick={() => props.onStop(app)}>
          Stop
        </button>
      </span>
    );
  return (
    <button className="btn primary" onClick={() => props.onStart(app)}>
      {app.ui === "web" ? "Start and open" : "Start"}
    </button>
  );
}

function Detail(props: {
  app: Listing;
  busy?: string;
  progress: string[];
  onBack: () => void;
  onInstall: (app: Listing) => void;
  onUninstall: (app: Listing) => void;
  onStart: (app: Listing) => void;
  onStop: (app: Listing) => void;
  onOpenTab: (id: string) => void;
}) {
  const { app } = props;
  const [confirmRemove, setConfirmRemove] = useState(false);
  const external = (url: string) => () => void window.openagora?.openExternal(url);

  return (
    <section className="page detail">
      <button className="back" onClick={props.onBack}>
        ← Store
      </button>
      <header className="detail-head">
        <Monogram app={app} size="lg" />
        <div className="detail-title">
          <h1>{app.name}</h1>
          <p className="muted">
            {label(app.category)} · {app.license} · {app.platforms.map((p) => osName({ macos: "darwin", windows: "win32", linux: "linux" }[p])).join(", ")}
          </p>
        </div>
        <div className="detail-actions">
          {!app.available ? (
            <span className="muted">Not available on {osName(window.openagora?.platform)}</span>
          ) : !app.installed ? (
            <button className="btn primary big" disabled={!!props.busy || !window.openagora} onClick={() => props.onInstall(app)}>
              {props.busy ?? "Install"}
            </button>
          ) : (
            <RunButtons app={app} busy={props.busy} onStart={props.onStart} onStop={props.onStop} onOpenTab={props.onOpenTab} />
          )}
        </div>
      </header>

      <p className="lead">{app.summary}</p>

      <div className="links">
        <button className="link" onClick={external(app.homepage)}>
          Website ↗
        </button>
        <button className="link" onClick={external(app.source)}>
          Source code ↗
        </button>
      </div>

      {props.progress.length > 0 && (
        <div className="panel">
          <h3>Install log</h3>
          <pre className="log">{props.progress.join("\n")}</pre>
        </div>
      )}

      <div className="panel">
        <h3>What installing does</h3>
        <InstallSteps app={app} />
      </div>

      {app.permissions.length > 0 && (
        <div className="panel">
          <h3>Available agent permissions for {app.name}</h3>
          <Permissions app={app} />
        </div>
      )}

      {app.installed && app.role !== "agent" && (
        <div className="panel danger">
          <h3>Remove</h3>
          {confirmRemove ? (
            <p>
              Stop {app.name} and delete its folder?{" "}
              <button className="btn danger" onClick={() => (setConfirmRemove(false), props.onUninstall(app))}>
                Remove
              </button>{" "}
              <button className="btn ghost" onClick={() => setConfirmRemove(false)}>
                Cancel
              </button>
            </p>
          ) : (
            <button className="btn ghost" disabled={!!props.busy} onClick={() => setConfirmRemove(true)}>
              Remove {app.name}…
            </button>
          )}
        </div>
      )}
    </section>
  );
}

function InstallSteps({ app }: { app: Listing }) {
  if (!app.available) return <p className="muted">Not available on this computer.</p>;
  return (
    <ol className="steps">
      {app.steps.map((step, i) => (
        <li key={i}>
          {step.download && (
            <div>
              Download <code>{step.download.asset ?? "(no build for this CPU)"}</code> from the latest release of{" "}
              <code>github.com/{step.download.github}</code>, checking its checksum
            </div>
          )}
          <div className="muted small">
            {step.download ? "Then run" : "Run"}: <code>{step.run}</code>
          </div>
        </li>
      ))}
    </ol>
  );
}

const RELEASES = "https://github.com/sanjaykatta1/openagora/releases/latest";

/** OpenAgora's own update: shown only when there's something to do. */
function UpdateCard() {
  const [state, setState] = useState<UpdateState | null>(null);
  useEffect(() => {
    if (!bridge) return;
    const stop = bridge.update.onState(setState);
    void bridge.update.state().then(setState);
    return stop;
  }, []);
  if (!bridge || !state) return null;
  const update = bridge.update;
  switch (state.kind) {
    case "available":
      return (
        <div className="update-card">
          <div>OpenAgora {state.version} is available.</div>
          <button className="btn primary" onClick={() => void update.download()}>
            Update
          </button>
        </div>
      );
    case "downloading":
      return (
        <div className="update-card">
          <div>Downloading {state.version}…</div>
          <div className="update-bar">
            <div style={{ width: `${state.percent}%` }} />
          </div>
        </div>
      );
    case "ready":
      return (
        <div className="update-card">
          <div>OpenAgora {state.version} is ready to install.</div>
          <button className="btn primary" onClick={() => void update.install()}>
            Restart to update
          </button>
        </div>
      );
    case "error":
      return (
        <div className="update-card">
          <div>Couldn't update OpenAgora.</div>
          <div className="update-error" title={state.message}>
            {state.message}
          </div>
          <div className="actions">
            <button className="btn" onClick={() => void update.check()}>
              Try again
            </button>
            <button className="btn ghost" onClick={() => void bridge?.openExternal(RELEASES)}>
              Download ↗
            </button>
          </div>
        </div>
      );
    default:
      return null;
  }
}

function Permissions({ app }: { app: Listing }) {
  return (
    <ul className="perms">
      {app.permissions.map((p) => (
        <li key={p.id}>
          <span className={`perm ${p.default}`}>{{ granted: "Allowed", ask: "Asks first", denied: "Not allowed" }[p.default]}</span>
          {p.description}
        </li>
      ))}
    </ul>
  );
}

function ConfirmInstall(props: { app: Listing; onCancel: () => void; onConfirm: () => void }) {
  const { app } = props;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && props.onCancel();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [props]);
  return (
    <div className="modal-backdrop" onClick={props.onCancel}>
      <div className="modal" role="dialog" aria-modal="true" aria-labelledby="confirm-title" onClick={(e) => e.stopPropagation()}>
        <header className="modal-head">
          <Monogram app={app} />
          <div>
            <h2 id="confirm-title">Install {app.name}?</h2>
            <p className="muted small">
              From <code>{app.source.replace("https://", "")}</code> · {app.license}
            </p>
          </div>
        </header>
        <h3>This will</h3>
        <InstallSteps app={app} />
        {app.permissions.length > 0 && (
          <>
            <h3>Available agent permissions</h3>
            <Permissions app={app} />
          </>
        )}
        <p className="muted small">Open-source apps run with your user's permissions. Only install apps you trust.</p>
        <footer className="modal-foot">
          <button className="btn ghost" onClick={props.onCancel}>
            Cancel
          </button>
          <button className="btn primary" onClick={props.onConfirm} autoFocus>
            Install
          </button>
        </footer>
      </div>
    </div>
  );
}

function StoppedTab(props: { app: Listing; visible: boolean; busy?: string; onStart: () => void }) {
  const [log, setLog] = useState<string | null>(null);
  const showLog = async () => {
    const result = await window.openagora?.logs(props.app.id);
    setLog(result ? (result.ok ? result.stdout : result.stderr) || "(the log is empty)" : null);
  };
  return (
    <div className="webview-wrap stopped-tab" style={{ display: props.visible ? "flex" : "none" }}>
      <div className="stopped-card">
        <Monogram app={props.app} size="lg" />
        <h2>{props.app.name} isn't running</h2>
        <p className="muted">It may have quit or crashed. Its log usually says why.</p>
        <div className="actions">
          <button className="btn primary" disabled={!!props.busy} onClick={props.onStart}>
            {props.busy ?? "Start"}
          </button>
          <button className="btn ghost" onClick={() => void showLog()}>
            Show log
          </button>
        </div>
        {log !== null && <pre className="log stopped-log">{log}</pre>}
      </div>
    </div>
  );
}

/** The subset of Electron's <webview> element this page uses. */
interface WebviewElement extends HTMLElement {
  reload(): void;
  loadURL(url: string): Promise<void>;
}

/** Each start of an app may print a one-time sign-in link (`url`). Use it for
 *  the first visit only; after that the tab's saved session is signed in, so
 *  later visits go to the app's plain address (`home_url`). */
function entryUrl(app: Listing): string {
  const key = `openagora.entered.${app.id}`;
  const visit = `${app.pid}`;
  let entered: string | null = null;
  try {
    entered = window.localStorage.getItem(key);
    window.localStorage.setItem(key, visit);
  } catch {
    // Storage unavailable: fall back to the plain address after the first visit.
  }
  if (entered === visit && app.home_url) return app.home_url;
  return app.url ?? app.home_url ?? "about:blank";
}

function AppTab({ app, visible }: { app: Listing; visible: boolean }) {
  const ref = useRef<WebviewElement | null>(null);
  const [src] = useState(() => entryUrl(app));
  const [failure, setFailure] = useState<string | null>(null);
  const [log, setLog] = useState<string | null>(null);

  useEffect(() => {
    const view = ref.current;
    if (!view) return;
    const failed = (event: Event) => {
      const { errorCode, errorDescription, isMainFrame } = event as Event & {
        errorCode: number;
        errorDescription: string;
        isMainFrame: boolean;
      };
      // -3 is an aborted load (a redirect or a new navigation), not a failure.
      if (isMainFrame && errorCode !== -3) setFailure(errorDescription || `error ${errorCode}`);
    };
    const loaded = () => setFailure(null);
    view.addEventListener("did-fail-load", failed);
    view.addEventListener("did-finish-load", loaded);
    return () => {
      view.removeEventListener("did-fail-load", failed);
      view.removeEventListener("did-finish-load", loaded);
    };
  }, []);

  const shownAddress = app.home_url ?? src.split("#")[0];
  const openInBrowser = () => void window.openagora?.openExternal(app.home_url ?? src);
  const showLog = async () => {
    const result = await window.openagora?.logs(app.id);
    setLog(result ? (result.ok ? result.stdout : result.stderr) || "(the log is empty)" : null);
  };

  return (
    <div className="webview-wrap" style={{ display: visible ? "flex" : "none" }}>
      <div className="webview-bar">
        <span>{app.name}</span>
        <span className="muted">{shownAddress}</span>
        <span className="bar-actions">
          <button className="link" onClick={() => (setFailure(null), ref.current?.reload())}>
            Reload
          </button>
          <button className="link" onClick={openInBrowser}>
            Open in browser ↗
          </button>
          <button className="link" onClick={() => void showLog()}>
            Log
          </button>
        </span>
      </div>
      {failure && (
        <div className="notice error tab-notice">
          {app.name} didn't load: {failure}.{" "}
          <button className="link" onClick={() => (setFailure(null), ref.current?.reload())}>
            Retry
          </button>
        </div>
      )}
      {log !== null && (
        <div className="panel tab-log">
          <div className="tab-log-head">
            <h3>{app.name} log</h3>
            <button className="link" onClick={() => setLog(null)}>
              Close
            </button>
          </div>
          <pre className="log">{log}</pre>
        </div>
      )}
      {/* A saved session per app, so sign-ins survive restarts and apps don't share cookies. */}
      <webview ref={ref} src={src} partition={`persist:app-${app.id}`} className="webview" />
    </div>
  );
}
