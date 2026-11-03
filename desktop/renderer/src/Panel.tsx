// Sunday's panel: a small window over any app, opened with Sunday's shortcut.
// It shows Sunday's chat (Hermes's own, from Sunday's dashboard) and what
// Sunday can see of the window the shortcut was pressed over. The user's
// choices here go to the engine, which keeps them and enforces them.
import { useCallback, useEffect, useRef, useState } from "react";
import { bridge, errorText, type ConfirmRequest, type ShortcutState, type SundayStatus } from "./api";
import { AgentConfirm } from "./Confirm";
import { dashboardUrl, PROVIDER_NOTICE } from "./Sunday";
import { useToast } from "./ui";

/** The most the desk takes in one file, and in one go (the engine checks too). */
const DESK_MAX_BYTES = 20 * 1024 * 1024;
const DESK_MAX_FILES = 10;

type CaptureOptions = { allow?: "once" | "always"; key?: string; pick?: boolean };

/** What Sunday can see of the window the shortcut was pressed over. */
function Seeing(props: { state: ShortcutState | null; busy: boolean; onCapture: (options: CaptureOptions) => void }) {
  const { state, busy } = props;
  if (!state) return null;
  const app = state.target?.app ?? "this window";
  // The panel says which app the user chose for, so a choice never applies
  // to a window the shortcut was pressed over later.
  const key = state.target?.key;
  if (state.phase === "looking") return <div className="seeing" role="status">Looking…</div>;
  if (state.phase === "offline") return <div className="seeing warn" role="status">OpenAgora’s engine isn’t running.</div>;
  if (state.phase === "capturing") return <div className="seeing" role="status">Capturing {app}…</div>;
  if (state.phase === "error") {
    return <div className="seeing warn" role="alert">
      <span className="seeing-text">{state.error}</span>
      {bridge?.platform === "darwin" && /Screen Recording/.test(state.error ?? "") &&
        <button className="btn" onClick={() => void bridge?.openScreenSettings()}>Open System Settings</button>}
    </div>;
  }
  if (state.item) {
    return <div className="seeing ok" role="status">
      {state.picture && <img className="seeing-picture" src={state.picture} alt="" />}
      <strong className="seeing-text">Sunday sees {app}.</strong>
    </div>;
  }
  switch (state.verdict) {
    case "not-allowed":
      return <div className="seeing" role="status">
        <span className="seeing-text"><strong>Sunday can’t see {app} yet.</strong><span className="muted">{PROVIDER_NOTICE}</span></span>
        <button className="btn" disabled={busy} onClick={() => props.onCapture({ allow: "once", key })}>Allow once</button>
        <button className="btn primary" disabled={busy} onClick={() => props.onCapture({ allow: "always", key })}>Always allow</button>
      </div>;
    case "never":
      return <div className="seeing" role="status"><strong className="seeing-text">OpenAgora never captures {app}.</strong></div>;
    case "unknown":
      if (!state.target && /only OpenAgora/.test(state.unknown ?? "")) {
        return <div className="seeing" role="status">No other window is on screen.</div>;
      }
      return <div className="seeing" role="status">
        <span className="seeing-text">Can’t tell which window you’re in{state.unknown ? ` (${state.unknown})` : ""}.</span>
        {state.pick && <button className="btn" disabled={busy} onClick={() => props.onCapture({ pick: true })}>Pick a window</button>}
      </div>;
    default:
      return null;
  }
}

export function Panel() {
  const [toast, notify] = useToast();
  const [status, setStatus] = useState<SundayStatus | null>(null);
  const [state, setState] = useState<ShortcutState | null>(null);
  const [asks, setAsks] = useState<ConfirmRequest[]>([]);
  const [busy, setBusy] = useState(false);
  const [dragging, setDragging] = useState(false);
  const picker = useRef<HTMLInputElement | null>(null);

  const refresh = useCallback(async () => {
    if (!bridge) return;
    try { setStatus(await bridge.sundayStatus()); } catch { setStatus(null); }
  }, []);

  useEffect(() => {
    if (!bridge) return;
    void refresh();
    void bridge.panel.state().then((s) => s && setState(s));
    const offState = bridge.panel.onState((s) => { setState(s); void refresh(); });
    const offAsk = bridge.onConfirm((request) => setAsks((a) => [...a, request]));
    const offDone = bridge.onConfirmDone((id) => setAsks((a) => a.filter((r) => r.id !== id)));
    return () => { offState(); offAsk(); offDone(); };
  }, [refresh]);

  useEffect(() => {
    if (asks.length > 0) return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && void bridge?.panel.hide();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [asks.length]);

  const captureNow = async (options: CaptureOptions) => {
    if (!bridge) return;
    setBusy(true);
    try { setState(await bridge.panel.capture(options)); } finally { setBusy(false); }
  };

  const share = async (work: () => Promise<unknown>, done?: string) => {
    try {
      await work();
      if (done) notify(done);
    } catch (e) {
      notify(errorText(e), true);
    }
  };

  // Dropped or picked files are read here and sent as their contents.
  const addFiles = (files: File[]) => {
    if (!bridge || files.length === 0) return;
    void share(async () => {
      if (files.length > DESK_MAX_FILES) throw new Error(`Add up to ${DESK_MAX_FILES} files at a time.`);
      const big = files.find((f) => f.size > DESK_MAX_BYTES);
      if (big) throw new Error(`${big.name} is too big for the desk (at most 20 MB).`);
      const read = await Promise.all(files.map(async (f) => ({ name: f.name, data: new Uint8Array(await f.arrayBuffer()) })));
      await bridge!.desk.addFiles(read);
    }, files.length === 1 ? "Put the file on the desk for Sunday." : `Put ${files.length} files on the desk for Sunday.`);
  };

  const running = status?.dashboard === "running" && !!status.url;
  return (
    <div className={dragging ? "panel-shell dragging" : "panel-shell"}
      onDragOver={(e) => { e.preventDefault(); setDragging(true); }}
      onDragLeave={() => setDragging(false)}
      onDrop={(e) => { e.preventDefault(); setDragging(false); addFiles([...e.dataTransfer.files]); }}>
      <header className="bar">
        <span>Sunday</span>
        <span className="bar-actions">
          <button className="btn ghost" onClick={() => void bridge?.showMain()}>Open OpenAgora</button>
          <button className="btn ghost" aria-label="Close" title="Close (Esc)" onClick={() => void bridge?.panel.hide()}>×</button>
        </span>
      </header>
      <Seeing state={state} busy={busy} onCapture={(options) => void captureNow(options)} />
      <div className="panel-body">
        {!bridge ? <p className="empty">Sunday’s panel runs in the desktop app.</p>
          : running ? <webview key={status!.pid ?? 0} src={dashboardUrl(status!.url!, "/chat", status!.profile)} partition="persist:sunday" className="webview" />
          : status?.setup === "ready" ? <div className="empty">
            <button className="btn primary" onClick={() => void share(async () => {
              const result = await bridge!.start(status.id);
              if (!result.ok) throw new Error(result.stderr.trim() || "Couldn’t start Sunday.");
              await refresh();
            })}>Start Sunday</button>
          </div>
          : status && <div className="empty">
            <button className="btn primary" onClick={() => void bridge?.showMain()}>Set up Sunday</button>
          </div>}
      </div>
      <footer className="panel-foot">
        <button className="btn" disabled={!bridge} onClick={() => void share(() => bridge!.desk.addClipboard(), "Put the clipboard on the desk for Sunday.")}>Add clipboard</button>
        <button className="btn" disabled={!bridge} onClick={() => picker.current?.click()}>Add files…</button>
        <input ref={picker} type="file" multiple hidden onChange={(e) => { addFiles([...(e.target.files ?? [])]); e.target.value = ""; }} />
      </footer>
      {toast}
      {asks.length > 0 && (
        <AgentConfirm key={asks[0].id} request={asks[0]} waiting={asks.length - 1} onAnswer={(allowed, choice) => {
          const request = asks[0];
          setAsks((a) => a.filter((r) => r.id !== request.id));
          void bridge?.answerConfirm(request.id, allowed, choice);
        }} />
      )}
    </div>
  );
}
