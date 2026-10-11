import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { bridge, osName, type App as Listing, type BackgroundState, type ConfirmRequest, type EngineResult, type SundaySetupChoice, type SundayStatus, type UpdateState } from "./api";
import { AgentConfirm } from "./Confirm";
import { CoversTabs } from "./Covers";
import { NEW_AGENT_PREFERENCES, PERSONAL_AGENTS, SUNDAY_ID, parseAgentPreferences, type AgentPreferences } from "../../electron/agents";
import { AgentChooser } from "./PersonalAgent";
import { SeeingSettings } from "./Seeing";
import { CodingAgentsSettings } from "./CodingAgents";
import { MemorySettings } from "./Memory";
import { ActivitySettings } from "./Activity";
import { BackgroundSettings, backgroundWords } from "./Background";
import { SundayDashboard, SundayHome, SundaySettings, sundayStage } from "./Sunday";

type View = { kind: "sunday" } | { kind: "store" } | { kind: "library" } | { kind: "settings" } | { kind: "app"; id: string } | { kind: "tab"; id: string };

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
  const [sunday, setSunday] = useState<SundayStatus | null>(null);
  const [sundayError, setSundayError] = useState<string | null>(null);
  const [dashboardRoute, setDashboardRoute] = useState("/");
  const [asks, setAsks] = useState<ConfirmRequest[]>([]);
  const landed = useRef(false);

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

  // Sunday is the home screen for everyone who chose it.
  useEffect(() => {
    if (!preferences?.completed || landed.current) return;
    landed.current = true;
    if (preferences.selectedAgent === SUNDAY_ID) setView({ kind: "sunday" });
  }, [preferences]);

  const refreshSunday = useCallback(async () => {
    if (!bridge) return;
    try {
      setSunday(await bridge.sundayStatus());
    } catch (e) {
      setSundayError(String(e instanceof Error ? e.message : e));
    }
  }, []);

  const saveAgent = async (id: string | null) => {
    setSavingPreferences(true);
    setPreferenceError(null);
    try {
      const next = parseAgentPreferences({ version: 1, completed: true, selectedAgent: id });
      if (bridge) await bridge.saveAgentPreferences(next);
      else localStorage.setItem(PREVIEW_PREFERENCES, JSON.stringify(next));
      setPreferences(next);
      setChoosingAgent(false);
      landed.current = true;
      setView({ kind: id ? "sunday" : "store" });
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
    void refreshSunday();
    if (!bridge) return;
    const off = bridge.onProgress(({ id, line }) =>
      setProgress((p) => ({ ...p, [id]: [...(p[id] ?? []), line].slice(-200) })),
    );
    const timer = window.setInterval(() => void refresh(), 5000);
    return () => {
      off();
      window.clearInterval(timer);
    };
  }, [refresh, refreshSunday]);

  // While Sunday runs, keep its status (and Hermes's gateway) fresh.
  const stage = sundayStage(sunday);
  useEffect(() => {
    if (stage !== "running") return;
    const timer = window.setInterval(() => void refreshSunday(), 10000);
    return () => window.clearInterval(timer);
  }, [stage, refreshSunday]);

  // Look again whenever Sunday or its settings come into view.
  useEffect(() => {
    if (view.kind === "sunday" || view.kind === "settings") void refreshSunday();
  }, [view.kind, refreshSunday]);

  useEffect(() => {
    if (!toast) return;
    const t = window.setTimeout(() => setToast(null), toast.error ? 9000 : 4000);
    return () => window.clearTimeout(t);
  }, [toast]);

  // The engine's questions, one at a time, and agents asking to show an app.
  useEffect(() => {
    if (!bridge) return;
    const offAsk = bridge.onConfirm((request) => setAsks((a) => [...a, request]));
    const offDone = bridge.onConfirmDone((id) => setAsks((a) => a.filter((r) => r.id !== id)));
    const offOpen = bridge.onOpenApp(async (id) => {
      await refresh();
      openTabRef.current(id);
    });
    return () => { offAsk(); offDone(); offOpen(); };
  }, [refresh]);

  const answer = (request: ConfirmRequest, allowed: boolean, choice?: string) => {
    setAsks((a) => a.filter((r) => r.id !== request.id));
    void bridge?.answerConfirm(request.id, allowed, choice);
  };

  // Background mode, shown in the sidebar while it's on (and in the tray).
  const [background, setBackground] = useState<BackgroundState | null>(null);
  useEffect(() => {
    if (!bridge) return;
    void bridge.background.indicator().then(setBackground, () => {});
    return bridge.background.onState(setBackground);
  }, []);

  // Apps popped out into their own windows outlive the main window: a main
  // window that opens again lists them as tabs.
  useEffect(() => {
    void bridge?.tabs.popped().then((ids) => {
      if (ids.length > 0) setTabs((t) => [...t, ...ids.filter((id) => !t.includes(id))]);
    }, () => {});
  }, []);

  const byId = useMemo(() => new Map(apps.map((a) => [a.id, a])), [apps]);
  const agentListing = byId.get(SUNDAY_ID);

  const act = useCallback(
    async (app: Listing, action: "install" | "uninstall" | "start" | "stop" | "updateApp", verb: string) => {
      if (!bridge) return;
      const agent = app.role === "agent";
      setBusy((b) => ({ ...b, [app.id]: verb }));
      if (action === "install") setProgress((p) => ({ ...p, [app.id]: [] }));
      if (agent) setSundayError(null);
      let result: EngineResult;
      try {
        result = await bridge[action](app.id);
      } catch (e) {
        result = { ok: false, stdout: "", stderr: String(e) };
      }
      setBusy(({ [app.id]: _done, ...rest }) => rest);
      await refresh();
      if (agent) await refreshSunday();
      if (result.ok) {
        const done = { install: "installed", uninstall: "removed", start: "started", stop: "stopped", updateApp: "updated" }[action];
        setToast({ text: agent && action === "uninstall" ? `Sunday removed. ${app.name} is still installed.` : `${agent && action !== "install" && action !== "updateApp" ? "Sunday" : app.name} ${done}.`, error: false });
        if (agent && (action === "install" || action === "start")) setView({ kind: "sunday" });
        if (action === "stop" || action === "uninstall") setTabs((t) => t.filter((id) => id !== app.id));
        if (action === "start" && app.ui === "web" && !agent) openTab(app.id);
      } else {
        const message = result.stderr.trim() || `${app.name}: ${action} failed.`;
        if (agent) setSundayError(message);
        setToast({ text: message, error: true });
      }
      return result.ok;
    },
    // openTab only calls state setters, so it never goes stale.
    [refresh, refreshSunday],
  );

  const setupSunday = async (choice: SundaySetupChoice) => {
    if (!bridge) return;
    setBusy((b) => ({ ...b, [SUNDAY_ID]: "Setting up Sunday…" }));
    setSundayError(null);
    let result: EngineResult;
    try {
      result = await bridge.sundaySetup(choice);
    } catch (e) {
      result = { ok: false, stdout: "", stderr: String(e) };
    }
    setBusy(({ [SUNDAY_ID]: _done, ...rest }) => rest);
    await refresh();
    await refreshSunday();
    if (!result.ok) {
      setSundayError(result.stderr.trim() || "Couldn’t set Sunday up.");
      return;
    }
    // A new profile has no model yet: Hermes's own setup comes first.
    setDashboardRoute(choice === "default" || choice === "fresh" ? "/models" : "/");
    if (agentListing) await act(agentListing, "start", "Starting Sunday…");
  };

  const openTab = (id: string) => {
    if (byId.get(id)?.role === "agent") {
      setView({ kind: "sunday" });
      return;
    }
    setTabs((t) => (t.includes(id) ? t : [...t, id]));
    setView({ kind: "tab", id });
  };
  const openTabRef = useRef(openTab);
  openTabRef.current = openTab;

  const connectSunday = async (connect: boolean) => {
    if (!bridge) return;
    setBusy((b) => ({ ...b, [SUNDAY_ID]: connect ? "Connecting…" : "Disconnecting…" }));
    let result: EngineResult;
    try {
      result = await (connect ? bridge.sundayConnect() : bridge.sundayDisconnect());
    } catch (e) {
      result = { ok: false, stdout: "", stderr: String(e) };
    }
    setBusy(({ [SUNDAY_ID]: _done, ...rest }) => rest);
    await refreshSunday();
    setToast(result.ok
      ? { text: result.stdout.trim() || (connect ? "Sunday now uses OpenAgora. New Sunday chats pick it up." : "Sunday no longer uses OpenAgora."), error: false }
      : { text: result.stderr.trim() || "That didn’t work.", error: true });
  };

  const closeTab = (id: string) => {
    setTabs((t) => t.filter((x) => x !== id));
    if (view.kind === "tab" && view.id === id) setView({ kind: "library" });
  };

  /** Settings → Sunday: one of Hermes's own pages, in Sunday's dashboard. */
  const openDashboardPage = async (route: string) => {
    setDashboardRoute(route);
    setView({ kind: "sunday" });
    if (stage === "start" && agentListing) await act(agentListing, "start", "Starting Sunday…");
  };

  const installedCount = apps.filter((a) => a.installed).length;
  const sundayChosen = preferences?.selectedAgent === SUNDAY_ID;

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

  const tabApp = view.kind === "tab" ? byId.get(view.id) : undefined;
  const showingDashboard = view.kind === "sunday" && stage === "running";
  const showingAppTab = (tabApp?.status === "running" && tabApp.pid !== null) || showingDashboard;
  const agentName = PERSONAL_AGENTS.find((a) => a.id === preferences.selectedAgent)?.name;

  return (
    <div className="shell">
      <aside className="sidebar">
        <div className="brand">
          <span className="brand-mark">◆</span> OpenAgora
        </div>
        <nav>
          <button className={view.kind === "sunday" ? "nav active" : "nav"} onClick={() => setView({ kind: "sunday" })}>
            <span className="nav-icon" aria-hidden>☀</span>
            <span>Sunday<span className="muted small block">{sundayChosen ? (stage === "running" ? "Running" : "Your personal agent") : "Personal agent"}</span></span>
          </button>
          <button className={view.kind === "store" || view.kind === "app" ? "nav active" : "nav"} onClick={() => setView({ kind: "store" })}>
            <span className="nav-icon" aria-hidden>⌂</span> Store
          </button>
          <button className={view.kind === "library" ? "nav active" : "nav"} onClick={() => setView({ kind: "library" })}>
            <span className="nav-icon" aria-hidden>▤</span> Library
            {installedCount > 0 && <span className="count">{installedCount}</span>}
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
                    <button className="tab-close" title={`Close ${app.name} tab`} aria-label={`Close ${app.name} tab`} onClick={() => closeTab(id)}>
                      ×
                    </button>
                  </div>
                );
              })}
            </nav>
          </>
        )}
        <nav className="nav-bottom">
          <button className={view.kind === "settings" ? "nav active" : "nav"} onClick={() => {
            setChoosingAgent(false);
            setPreferenceError(null);
            setView({ kind: "settings" });
          }}>
            <span className="nav-icon" aria-hidden>⚙</span>
            <span>Settings<span className="muted small block">{agentName ? `Personal agent: ${agentName}` : "No personal agent"}</span></span>
          </button>
        </nav>
        {background?.on && (
          <button className={background.paused ? "background-indicator paused" : "background-indicator"} onClick={() => setView({ kind: "settings" })}>
            <span aria-hidden>●</span> {backgroundWords(background)}
          </button>
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

        {view.kind === "sunday" && (
          <SundayHome status={sunday} error={sundayError} listing={agentListing} chosen={sundayChosen}
            busy={busy[SUNDAY_ID]}
            onChoose={() => void saveAgent(SUNDAY_ID)}
            onInstall={() => agentListing && setConfirming(agentListing)}
            onSetup={(choice) => void setupSunday(choice)}
            onUpdate={() => agentListing && void act(agentListing, "updateApp", "Updating Hermes…")}
            onStart={() => agentListing && void act(agentListing, "start", "Starting Sunday…")}
            onConnect={() => void connectSunday(true)}
            onRetry={() => { setSundayError(null); void refreshSunday(); }} />
        )}

        {view.kind === "settings" && (choosingAgent ? (
          <AgentChooser firstLaunch={false} selected={preferences.selectedAgent} saving={savingPreferences} error={preferenceError}
            onSave={(id) => void saveAgent(id)} onCancel={() => { setChoosingAgent(false); setPreferenceError(null); }} />
        ) : (
          <SundaySettings status={sunday} busy={busy[SUNDAY_ID]}
            onConnect={(connect) => void connectSunday(connect)}
            onOpenPage={(route) => void openDashboardPage(route)}
            onChangeAgent={() => setChoosingAgent(true)}
            onRemove={() => agentListing && void act(agentListing, "uninstall", "Removing Sunday…")}>
            {sundayChosen && <SeeingSettings />}
            {sundayChosen && <CodingAgentsSettings apps={apps} />}
            {sundayChosen && <MemorySettings />}
            {sundayChosen && <BackgroundSettings onChange={setBackground} />}
            {sundayChosen && <ActivitySettings />}
          </SundaySettings>
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

        {/* Sunday's dashboard stays loaded while you look at other pages. */}
        {sunday && stage === "running" && (
          <SundayDashboard key={`${sunday.pid}`} status={sunday} visible={showingDashboard} route={dashboardRoute}
            busy={busy[SUNDAY_ID]} onConnect={() => void connectSunday(true)} />
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

      {/* The macOS title bar is hidden, so give the top of the content area a
          handle for moving the window. A running app tab's address bar is its
          own handle, so the strip would only sit over its buttons there. */}
      {!showingAppTab && <div className="titlebar-drag" aria-hidden />}

      {confirming && (
        <ConfirmInstall
          app={confirming}
          onCancel={() => setConfirming(null)}
          onConfirm={() => {
            const app = confirming;
            setConfirming(null);
            setView(app.role === "agent" ? { kind: "sunday" } : { kind: "app", id: app.id });
            void act(app, "install", app.role === "agent" ? "Installing Hermes…" : "Installing…");
          }}
        />
      )}

      {asks.length > 0 && (
        <AgentConfirm key={asks[0].id} request={asks[0]} waiting={asks.length - 1} onAnswer={(allowed, choice) => answer(asks[0], allowed, choice)} />
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
            {app.role === "agent" ? "Open Sunday" : "Open"}
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

      {app.installed && (
        <div className="panel danger">
          <h3>{app.role === "agent" ? "Remove Sunday" : "Remove"}</h3>
          {app.role === "agent" && (
            <p className="muted">Deletes only Hermes’s “{app.profile}” profile, which holds Sunday. {app.name} itself and your other profiles stay.</p>
          )}
          {confirmRemove ? (
            <p>
              {app.role === "agent" ? "Remove Sunday?" : app.adopted ? `Stop managing ${app.name}? It stays installed on this computer.` : `Stop ${app.name} and delete its folder?`}{" "}
              <button className="btn danger" onClick={() => (setConfirmRemove(false), props.onUninstall(app))}>
                Remove
              </button>{" "}
              <button className="btn ghost" onClick={() => setConfirmRemove(false)}>
                Cancel
              </button>
            </p>
          ) : (
            <button className="btn ghost" disabled={!!props.busy} onClick={() => setConfirmRemove(true)}>
              {app.role === "agent" ? "Remove Sunday…" : `Remove ${app.name}…`}
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
      <CoversTabs />
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
        {app.role === "agent" && (
          <p>Sunday, OpenAgora’s personal agent, runs on {app.name}. After installing, OpenAgora creates {app.name}’s “{app.profile}” profile for Sunday.</p>
        )}
        {app.present ? (
          <p>{app.name} is already on this computer, so OpenAgora uses it as it is instead of installing it again.</p>
        ) : (
          <>
            <h3>This will</h3>
            <InstallSteps app={app} />
          </>
        )}
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
  const host = useRef<HTMLDivElement | null>(null);
  const [src] = useState(() => entryUrl(app));
  const [failure, setFailure] = useState<string | null>(null);
  const [popped, setPopped] = useState(false);
  const [log, setLog] = useState<string | null>(null);
  const tabs = bridge?.tabs;

  // The app's page lives in OpenAgora's main process, one per start of the
  // app, with a saved session per app (sign-ins survive restarts, and apps
  // don't share cookies). Opened before it is placed.
  useLayoutEffect(() => {
    if (!tabs) return;
    const off = tabs.onEvent((event) => {
      if (event.id !== app.id) return;
      if (event.kind === "failed") setFailure(event.description ?? "it didn't answer");
      else if (event.kind === "loaded") setFailure(null);
      else setPopped(event.kind === "popped");
    });
    tabs.open(app.id, app.name, src, app.pid ?? 0).catch((e: unknown) => setFailure(e instanceof Error ? e.message : String(e)));
    return () => {
      off();
      void tabs.close(app.id);
    };
  }, [tabs, app.id, app.pid, src]);

  // Where this page shows it: the main process draws it there.
  useLayoutEffect(() => {
    const element = host.current;
    if (!tabs) return;
    if (!element || !visible || popped) {
      tabs.place(app.id, null);
      return;
    }
    const place = () => {
      const r = element.getBoundingClientRect();
      tabs.place(app.id, { x: r.left, y: r.top, width: r.width, height: r.height });
    };
    place();
    const observer = new ResizeObserver(place);
    observer.observe(element);
    window.addEventListener("resize", place);
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", place);
      tabs.place(app.id, null);
    };
  }, [tabs, app.id, visible, popped, failure, log]);

  const shownAddress = app.home_url ?? src.split("#")[0];
  const openInBrowser = () => void window.openagora?.openExternal(app.home_url ?? src);
  const reload = () => {
    setFailure(null);
    void tabs?.reload(app.id);
  };
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
          <button className="link" onClick={reload}>
            Reload
          </button>
          {popped
            ? <button className="link" onClick={() => void tabs?.bringBack(app.id)}>Bring back</button>
            : <button className="link" onClick={() => void tabs?.popOut(app.id)}>Pop out</button>}
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
          <button className="link" onClick={reload}>
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
      {popped ? (
        <div className="popped-tab">
          <p>{app.name} is open in its own window.</p>
          <div className="actions">
            <button className="btn primary" onClick={() => void tabs?.showWindow(app.id)}>
              Show its window
            </button>
            <button className="btn ghost" onClick={() => void tabs?.bringBack(app.id)}>
              Bring it back here
            </button>
          </div>
        </div>
      ) : (
        <div ref={host} className="webview" aria-label={`${app.name}'s page`} role="region" />
      )}
    </div>
  );
}
