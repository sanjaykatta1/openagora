// OpenAgora's window: the sidebar, and the page it shows. App state lives
// here; each page is its own file.
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { NEW_AGENT_PREFERENCES, SUNDAY_ID, parseAgentPreferences, type AgentPreferences } from "../../electron/agents";
import { bridge, errorText, type App as Listing, type BackgroundState, type ConfirmRequest, type EngineResult, type SundaySetupChoice, type SundayStatus, type UpdateState } from "./api";
import { AppTab, StoppedTab } from "./AppTab";
import { backgroundWords } from "./Background";
import { AgentConfirm } from "./Confirm";
import { Settings, type SettingsTab } from "./Settings";
import { Detail, InstallDialog, Library, Store } from "./Store";
import { ConnectDialog, SundayDashboard, SundayHome, sundayStage } from "./Sunday";
import { Monogram, NotifyProvider, useToast } from "./ui";

type View =
  | { kind: "sunday" } | { kind: "store" } | { kind: "library" } | { kind: "settings" } | { kind: "tab"; id: string }
  /** An app's page, opened from the store or the library. */
  | { kind: "detail"; id: string; from: "store" | "library" };

type Action = "install" | "uninstall" | "start" | "stop" | "updateApp" | "open";

/** Where the browser preview (`npm run preview`) keeps the user's choice. */
const PREVIEW_PREFERENCES = "openagora-preview-personal-agent";
const RELEASES = "https://github.com/sanjaykatta1/openagora/releases/latest";

export function App() {
  const [toast, notify] = useToast();
  const [preferences, setPreferences] = useState<AgentPreferences | null>(null);
  const [view, setView] = useState<View>({ kind: "sunday" });
  const [settingsTab, setSettingsTab] = useState<SettingsTab>("general");
  const [apps, setApps] = useState<Listing[]>([]);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [tabs, setTabs] = useState<string[]>([]);
  const [busy, setBusy] = useState<Record<string, string>>({});
  const [progress, setProgress] = useState<Record<string, string[]>>({});
  const [installing, setInstalling] = useState<Listing | null>(null);
  const [askingConnect, setAskingConnect] = useState(false);
  const [sunday, setSunday] = useState<SundayStatus | null>(null);
  const [sundayError, setSundayError] = useState<string | null>(null);
  const [dashboardRoute, setDashboardRoute] = useState("/");
  const [asks, setAsks] = useState<ConfirmRequest[]>([]);
  const [background, setBackground] = useState<BackgroundState | null>(null);

  // Sunday is home for everyone who uses it, or hasn't answered yet.
  useEffect(() => {
    void (async () => {
      let saved = NEW_AGENT_PREFERENCES;
      try {
        const raw = bridge ? await bridge.agentPreferences() : JSON.parse(localStorage.getItem(PREVIEW_PREFERENCES) ?? "null");
        saved = parseAgentPreferences(raw ?? NEW_AGENT_PREFERENCES);
      } catch {
        // The file stays as it is until the user answers again.
        notify("Couldn’t read your saved choice of personal agent.", true);
      }
      setPreferences(saved);
      if (saved.completed && saved.selectedAgent === null) setView({ kind: "store" });
    })();
  }, [notify]);

  const saveAgent = async (id: string | null) => {
    const next: AgentPreferences = { version: 1, completed: true, selectedAgent: id };
    try {
      if (bridge) await bridge.saveAgentPreferences(next);
      else localStorage.setItem(PREVIEW_PREFERENCES, JSON.stringify(next));
      setPreferences(next);
      setView({ kind: id ? "sunday" : "store" });
    } catch {
      notify("Couldn’t save your choice.", true);
    }
  };

  const refreshSunday = useCallback(async () => {
    if (!bridge) return;
    try {
      setSunday(await bridge.sundayStatus());
    } catch (e) {
      setSundayError(errorText(e));
    }
  }, []);

  const refresh = useCallback(async () => {
    if (!bridge) return;
    try {
      setApps(await bridge.catalog());
      setLoadError(null);
    } catch (e) {
      setLoadError(errorText(e));
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

  const byId = useMemo(() => new Map(apps.map((a) => [a.id, a])), [apps]);
  const agentListing = byId.get(SUNDAY_ID);

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

  const closeTab = (id: string) => {
    setTabs((t) => t.filter((x) => x !== id));
    if (view.kind === "tab" && view.id === id) setView({ kind: "library" });
  };

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

  // Background mode shows in the sidebar while it's on (and in the tray).
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

  const act = useCallback(async (app: Listing, action: Action, verb: string) => {
    if (!bridge) return;
    const agent = app.role === "agent";
    setBusy((b) => ({ ...b, [app.id]: verb }));
    if (action === "install") setProgress((p) => ({ ...p, [app.id]: [] }));
    if (agent) setSundayError(null);
    let result: EngineResult;
    try {
      result = await bridge[action](app.id);
    } catch (e) {
      result = { ok: false, stdout: "", stderr: errorText(e) };
    }
    await refresh();
    if (agent) await refreshSunday();
    setBusy(({ [app.id]: _done, ...rest }) => rest);
    if (!result.ok) {
      const message = result.stderr.trim() || `${app.name}: ${action} failed.`;
      if (agent) setSundayError(message);
      notify(message, true);
      return;
    }
    // Long work may finish out of sight, so it says so; the rest shows on screen.
    const done: Partial<Record<Action, string>> = { install: "installed", uninstall: "removed", updateApp: "updated" };
    if (done[action]) notify(agent && action === "uninstall" ? "Sunday removed." : `${app.name} ${done[action]}.`);
    if (agent && (action === "install" || action === "start")) setView({ kind: "sunday" });
    if (action === "stop" || action === "uninstall") setTabs((t) => t.filter((id) => id !== app.id));
    if (action === "start" && app.ui === "web" && !agent) openTabRef.current(app.id);
  }, [refresh, refreshSunday, notify]);

  const setupSunday = async (choice: SundaySetupChoice) => {
    if (!bridge) return;
    setBusy((b) => ({ ...b, [SUNDAY_ID]: "Setting up Sunday…" }));
    setSundayError(null);
    let result: EngineResult;
    try {
      result = await bridge.sundaySetup(choice);
    } catch (e) {
      result = { ok: false, stdout: "", stderr: errorText(e) };
    }
    await refresh();
    await refreshSunday();
    setBusy(({ [SUNDAY_ID]: _done, ...rest }) => rest);
    if (!result.ok) {
      setSundayError(result.stderr.trim() || "Couldn’t set Sunday up.");
      return;
    }
    // A new profile has no model yet: Hermes's own setup comes first.
    setDashboardRoute(choice === "default" || choice === "fresh" ? "/models" : "/");
    if (agentListing) await act(agentListing, "start", "Starting Sunday…");
  };

  const connectSunday = async (connect: boolean) => {
    if (!bridge) return;
    setAskingConnect(false);
    setBusy((b) => ({ ...b, [SUNDAY_ID]: connect ? "Connecting…" : "Disconnecting…" }));
    let result: EngineResult;
    try {
      result = await (connect ? bridge.sundayConnect() : bridge.sundayDisconnect());
    } catch (e) {
      result = { ok: false, stdout: "", stderr: errorText(e) };
    }
    setBusy(({ [SUNDAY_ID]: _done, ...rest }) => rest);
    await refreshSunday();
    if (!result.ok) notify(result.stderr.trim() || "That didn’t work.", true);
  };

  /** One of Hermes's own pages, in Sunday's dashboard. */
  const openDashboardPage = async (route: string) => {
    setDashboardRoute(route);
    setView({ kind: "sunday" });
    if (stage === "start" && agentListing) await act(agentListing, "start", "Starting Sunday…");
  };

  if (!preferences) return null;

  const chosen = preferences.selectedAgent === SUNDAY_ID;
  const tabApp = view.kind === "tab" ? byId.get(view.id) : undefined;
  const detailApp = view.kind === "detail" ? byId.get(view.id) : undefined;
  const showingDashboard = chosen && view.kind === "sunday" && stage === "running";
  const showingAppTab = (tabApp?.status === "running" && tabApp.pid !== null) || showingDashboard;
  const run = {
    onInstall: setInstalling,
    onStart: (a: Listing) => void act(a, "start", "Starting…"),
    onStop: (a: Listing) => void act(a, "stop", "Stopping…"),
    onOpenTab: openTab,
    onOpenWindow: (a: Listing) => void act(a, "open", "Opening…"),
  };

  return (
    <NotifyProvider value={notify}>
      <div className="shell">
        <aside className="sidebar">
          <div className="brand"><span className="brand-mark" aria-hidden>◆</span>OpenAgora</div>
          <nav className="nav-list" aria-label="OpenAgora">
            <NavButton active={view.kind === "sunday"} onClick={() => setView({ kind: "sunday" })}>Sunday</NavButton>
            <NavButton active={view.kind === "store" || (view.kind === "detail" && view.from === "store")} onClick={() => setView({ kind: "store" })}>Store</NavButton>
            <NavButton active={view.kind === "library" || (view.kind === "detail" && view.from === "library")} onClick={() => setView({ kind: "library" })}>Library</NavButton>
          </nav>
          {tabs.length > 0 && (
            <nav className="nav-list open-apps" aria-label="Open apps">
              {tabs.map((id) => {
                const app = byId.get(id);
                if (!app) return null;
                return (
                  <div key={id} className={view.kind === "tab" && view.id === id ? "nav tab active" : "nav tab"}>
                    <button className="tab-open" onClick={() => setView({ kind: "tab", id })}><Monogram app={app} size="xs" />{app.name}</button>
                    <button className="tab-close" title={`Close ${app.name} tab`} aria-label={`Close ${app.name} tab`} onClick={() => closeTab(id)}>×</button>
                  </div>
                );
              })}
            </nav>
          )}
          <div className="sidebar-foot">
            {background?.on && (
              <button className={background.paused ? "background-indicator paused" : "background-indicator"}
                onClick={() => { setSettingsTab("memory"); setView({ kind: "settings" }); }}>
                <span className="dot" aria-hidden />{backgroundWords(background)}
              </button>
            )}
            <UpdateButton />
            <NavButton active={view.kind === "settings"} onClick={() => setView({ kind: "settings" })}>Settings</NavButton>
          </div>
        </aside>

        <main className="content">
          {!bridge && <p className="notice">Preview: the engine runs in the desktop app.</p>}
          {loadError && <p className="notice error">Couldn’t read the store: {loadError}</p>}

          {view.kind === "sunday" && (
            <SundayHome status={sunday} error={sundayError} listing={agentListing} chosen={chosen} decided={preferences.completed}
              busy={busy[SUNDAY_ID]}
              onChoose={() => void saveAgent(SUNDAY_ID)}
              onDecline={() => void saveAgent(null)}
              onInstall={() => agentListing && setInstalling(agentListing)}
              onSetup={(choice) => void setupSunday(choice)}
              onUpdate={() => agentListing && void act(agentListing, "updateApp", "Updating Hermes…")}
              onStart={() => agentListing && void act(agentListing, "start", "Starting Sunday…")}
              onRetry={() => { setSundayError(null); void refreshSunday(); }} />
          )}

          {view.kind === "settings" && (
            <Settings tab={settingsTab} onTab={setSettingsTab} sunday={chosen} status={sunday} busy={busy[SUNDAY_ID]} apps={apps}
              onConnect={(connect) => (connect ? setAskingConnect(true) : void connectSunday(false))}
              onOpenPage={(route) => void openDashboardPage(route)}
              onStopUsing={() => void saveAgent(null)}
              onRemove={() => agentListing && void act(agentListing, "uninstall", "Removing Sunday…")}
              onBackground={setBackground} />
          )}

          {view.kind === "store" && <Store apps={apps} busy={busy} onOpen={(id) => setView({ kind: "detail", id, from: "store" })} onInstall={setInstalling} />}

          {view.kind === "library" && (
            <Library {...run} apps={apps.filter((a) => a.installed)} busyById={busy}
              onOpenPage={(id) => setView({ kind: "detail", id, from: "library" })} onBrowse={() => setView({ kind: "store" })} />
          )}

          {view.kind === "detail" && detailApp && (
            <Detail {...run} app={detailApp} busy={busy[detailApp.id]} progress={progress[detailApp.id] ?? []}
              back={view.from === "library" ? "Library" : "Store"} onBack={() => setView({ kind: view.from })} onUninstall={(a) => void act(a, "uninstall", a.role === "agent" ? "Removing Sunday…" : "Removing…")} />
          )}

          {/* Sunday's dashboard stays loaded while you look at other pages. */}
          {sunday && stage === "running" && (
            <SundayDashboard key={`${sunday.pid}`} status={sunday} visible={showingDashboard} route={dashboardRoute}
              busy={busy[SUNDAY_ID]} onConnect={() => setAskingConnect(true)} />
          )}

          {tabs.map((id) => {
            const app = byId.get(id);
            if (!app) return null;
            const visible = view.kind === "tab" && view.id === id;
            // An app can stop on its own (crash, quit elsewhere): its tab says so.
            if (app.status !== "running" || app.pid === null) {
              return <StoppedTab key={id} app={app} visible={visible} busy={busy[id]} onStart={() => void act(app, "start", "Starting…")} />;
            }
            return <AppTab key={`${id}:${app.pid}`} app={app} visible={visible} />;
          })}
        </main>

        {/* The macOS title bar is hidden, so the top of the content area is a
            handle for moving the window. A running app tab's bar is its own
            handle, so the strip would only sit over its buttons there. */}
        {!showingAppTab && <div className="titlebar-drag" aria-hidden />}

        {installing && (
          <InstallDialog app={installing} onCancel={() => setInstalling(null)} onConfirm={() => {
            const app = installing;
            setInstalling(null);
            if (app.role === "agent") setView({ kind: "sunday" });
            else if (view.kind !== "detail") setView({ kind: "detail", id: app.id, from: view.kind === "library" ? "library" : "store" });
            void act(app, "install", app.role === "agent" ? "Installing Hermes…" : "Installing…");
          }} />
        )}
        {askingConnect && <ConnectDialog onCancel={() => setAskingConnect(false)} onConnect={() => void connectSunday(true)} />}
        {asks.length > 0 && (
          <AgentConfirm key={asks[0].id} request={asks[0]} waiting={asks.length - 1} onAnswer={(allowed, choice) => {
            const request = asks[0];
            setAsks((a) => a.filter((r) => r.id !== request.id));
            void bridge?.answerConfirm(request.id, allowed, choice);
          }} />
        )}
        {toast}
      </div>
    </NotifyProvider>
  );
}

function NavButton(props: { active: boolean; onClick: () => void; children: ReactNode }) {
  return <button className={props.active ? "nav active" : "nav"} aria-current={props.active ? "page" : undefined} onClick={props.onClick}>{props.children}</button>;
}

/** OpenAgora's own update: shown only when there's something to do. */
function UpdateButton() {
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
      return <button className="btn primary update" onClick={() => void update.download()}>Update to {state.version}</button>;
    case "downloading":
      return <div className="update-bar" role="progressbar" aria-label={`Downloading OpenAgora ${state.version}`} aria-valuenow={Math.round(state.percent)}>
        <div style={{ width: `${state.percent}%` }} />
      </div>;
    case "ready":
      return <button className="btn primary update" onClick={() => void update.install()}>Restart to update</button>;
    case "error":
      return <div className="actions update" title={state.message}>
        <button className="btn" onClick={() => void update.check()}>Retry update</button>
        <button className="btn ghost" onClick={() => void bridge?.openExternal(RELEASES)}>Download</button>
      </div>;
    default:
      return null;
  }
}
