// Settings → What Sunday can see: Sunday's shortcut, the apps the user allowed,
// and the never-capture list. The engine keeps and enforces all of it; this
// page only shows it and passes the user's changes on.
import { useCallback, useEffect, useState } from "react";
import { bridge, type BrokerState, type DesktopState, type Grant } from "./api";
import { clientName } from "./Confirm";
import { PROVIDER_NOTICE } from "./Panel";

const shortcutLabel = (accelerator: string, platform: string) =>
  accelerator.replace("CommandOrControl", platform === "darwin" ? "Command" : "Ctrl").replace(/\+/g, " + ");

export function SeeingSettings() {
  const [desktop, setDesktop] = useState<DesktopState | null>(null);
  const [broker, setBroker] = useState<BrokerState | null>(null);
  const [shortcut, setShortcut] = useState("");
  const [neverApp, setNeverApp] = useState("");
  const [neverTitle, setNeverTitle] = useState("");
  const [message, setMessage] = useState<{ text: string; error: boolean } | null>(null);

  const load = useCallback(async () => {
    if (!bridge) return;
    try {
      const state = await bridge.desktopSettings();
      setDesktop(state);
      setShortcut(state.settings.shortcut);
      setBroker(await bridge.broker.state());
    } catch (e) {
      setMessage({ text: e instanceof Error ? e.message : String(e), error: true });
    }
  }, []);
  useEffect(() => { void load(); }, [load]);

  const run = async (work: () => Promise<{ ok: boolean; stdout: string; stderr: string } | DesktopState>, done: string) => {
    setMessage(null);
    try {
      const result = await work();
      if ("ok" in result && !result.ok) throw new Error(result.stderr.trim() || "That didn’t work.");
      if ("settings" in result) {
        setDesktop(result);
        if (result.shortcutError) { setMessage({ text: result.shortcutError, error: true }); return; }
      }
      setMessage({ text: done, error: false });
      await load();
    } catch (e) {
      setMessage({ text: (e instanceof Error ? e.message : String(e)).replace(/^Error invoking remote method '[^']+': (Error: )?/, ""), error: true });
    }
  };

  if (!bridge) return null;
  const platform = desktop?.platform ?? bridge.platform;
  const trayName = platform === "darwin" ? "menu bar" : "tray";
  const grants = broker?.grants ?? [];
  // Sunday seeing apps' windows; Sunday sending to coding agents or starting
  // work in apps; coding agents seeing apps. Reading coding sessions is in
  // Settings → Memory.
  const seeing = grants.filter((g) => g.client === "sunday" && g.destination === "sunday" && g.type !== "sessions/*");
  const sending = grants.filter((g) => g.client === "sunday" && g.destination !== "sunday");
  const agentsSee = grants.filter((g) => g.client !== "sunday");
  const scopeText = (g: Grant) => g.scope === "once" ? "once" : "always";
  const sendText = (g: Grant) => g.type === "tasks/*" ? `Start tasks in ${clientName(g.destination)}`
    : g.type === "act/*" ? `Act in ${clientName(g.destination)} with its own tools`
    : `Send from ${g.source_name} to ${clientName(g.destination)}`;
  const revoke = (g: Grant, done: string) =>
    void run(() => bridge!.broker.revoke(g.source, { client: g.client, destination: g.destination }), done);
  const own = broker?.never.user ?? [];
  return (
    <div className="panel" aria-labelledby="seeing-title">
      <h3 id="seeing-title">What Sunday can see</h3>
      <p className="muted small">Sunday sees a window, or reads its text, only when you press its shortcut or ask it to look, and only for apps you allow. Captures stay on this computer, on the desk, for 24 hours. {PROVIDER_NOTICE}</p>

      {desktop && <>
        <div className="settings-row">
          <label className="field">Sunday’s shortcut
            <input value={shortcut} onChange={(e) => setShortcut(e.target.value)} spellCheck={false} aria-describedby="shortcut-help" />
          </label>
          <button className="btn ghost" disabled={shortcut === desktop.settings.shortcut}
            onClick={() => void run(() => bridge!.saveDesktopSettings({ ...desktop.settings, shortcut: shortcut.trim() }), `Sunday’s shortcut is now ${shortcutLabel(shortcut.trim(), platform)}.`)}>
            Save shortcut
          </button>
        </div>
        <p id="shortcut-help" className="muted small">Now {shortcutLabel(desktop.settings.shortcut, platform)}. Write it like CommandOrControl+Shift+Space: modifiers, then one key.</p>
        {desktop.shortcutError && <p role="alert" className="agent-error">{desktop.shortcutError}</p>}
        <label className="choice-detail">
          <input type="checkbox" checked={desktop.settings.keepRunning}
            onChange={(e) => {
              const keepRunning = e.target.checked;
              const next = { ...desktop.settings, keepRunning };
              setDesktop({ ...desktop, settings: next });
              void run(() => bridge!.saveDesktopSettings(next),
                keepRunning ? `OpenAgora keeps running in the ${trayName}.` : "OpenAgora closes with its window.");
            }} />
          <span>Keep OpenAgora running in the {trayName} when its window is closed
            <span className="muted block small">Sunday’s shortcut works only while OpenAgora runs.</span>
          </span>
        </label>
        {platform === "darwin" && desktop.screen !== "granted" && (
          <p className="notice">macOS needs Screen Recording permission before OpenAgora can capture other apps’ windows{desktop.screen === "not-determined" ? "; it asks the first time" : ""}.{" "}
            <button className="link" onClick={() => void bridge!.openScreenSettings()}>Open System Settings</button>
          </p>
        )}
        {desktop.accessibility === false && (
          <p className="notice">Sunday can read the text on other apps’ windows (only apps you allow) once macOS gives OpenAgora Accessibility permission.{" "}
            <button className="link" onClick={() => void bridge!.openAccessibilitySettings()}>Open System Settings</button>
          </p>
        )}
      </>}

      <h4>Apps Sunday may see</h4>
      {seeing.length === 0
        ? <p className="muted">None yet. Press Sunday’s shortcut over an app to allow it.</p>
        : <ul className="rows compact">
          {seeing.map((g) => (
            <li key={`${g.source}:${g.scope}:${g.destination}`} className="row">
              <span><strong>{g.source_name}</strong> <span className="muted small">{g.scope === "once" ? "next capture only" : "always"}</span></span>
              <button className="btn ghost" onClick={() => revoke(g, `Sunday may no longer see ${g.source_name}.`)}>Remove</button>
            </li>
          ))}
        </ul>}
      {sending.length > 0 && <>
        <h4>What Sunday may hand over without asking</h4>
        <ul className="rows compact">
          {sending.map((g) => (
            <li key={`${g.source}:${g.type}:${g.destination}:${g.scope}`} className="row">
              <span><strong>{sendText(g)}</strong> <span className="muted small">{scopeText(g)}</span></span>
              <button className="btn ghost" onClick={() => revoke(g, `Sunday asks again before: ${sendText(g).toLowerCase()}.`)}>Remove</button>
            </li>
          ))}
        </ul>
      </>}
      {agentsSee.length > 0 && <>
        <h4>Apps coding agents may see</h4>
        <ul className="rows compact">
          {agentsSee.map((g) => (
            <li key={`${g.client}:${g.source}:${g.type}`} className="row">
              <span><strong>{clientName(g.client)} may see {g.source_name}</strong> <span className="muted small">{scopeText(g)}</span></span>
              <button className="btn ghost" onClick={() => revoke(g, `${clientName(g.client)} may no longer see ${g.source_name}.`)}>Remove</button>
            </li>
          ))}
        </ul>
      </>}

      <h4>Never captured</h4>
      <p className="muted small">Built in, whatever else you allow: {(broker?.never.built_in ?? []).map((r) => r.name).join(", ")}.</p>
      {own.length > 0 && <ul className="rows compact">
        {own.map((r) => (
          <li key={r.name} className="row">
            <span><strong>{r.name}</strong> <span className="muted small">{[...(r.apps ?? []), ...(r.titles ?? []).map((t) => `windows titled “${t}”`)].join(", ")}</span></span>
            <button className="btn ghost" onClick={() => void run(() => bridge!.broker.neverRemove(r.name), `Removed ${r.name} from your never-capture list.`)}>Remove</button>
          </li>
        ))}
      </ul>}
      <form className="settings-row" onSubmit={(e) => {
        e.preventDefault();
        const app = neverApp.trim();
        const title = neverTitle.trim();
        if (!app && !title) return;
        void run(() => bridge!.broker.neverAdd({ name: app || title, ...(app ? { app } : {}), ...(title ? { title } : {}) }), `OpenAgora will never capture ${app || `windows titled “${title}”`}.`)
          .then(() => { setNeverApp(""); setNeverTitle(""); });
      }}>
        <label className="field">An app
          <input value={neverApp} onChange={(e) => setNeverApp(e.target.value)} placeholder="e.g. Signal" />
        </label>
        <label className="field">or words in a window title
          <input value={neverTitle} onChange={(e) => setNeverTitle(e.target.value)} placeholder="e.g. My Bank" />
        </label>
        <button className="btn ghost" type="submit" disabled={!neverApp.trim() && !neverTitle.trim()}>Never capture</button>
      </form>
      {message && <p role={message.error ? "alert" : "status"} className={message.error ? "agent-error" : undefined}>{message.text}</p>}
    </div>
  );
}
