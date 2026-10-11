// Sunday's panel: a small window over any app, opened with Sunday's shortcut.
// It shows Sunday's chat (Hermes's own, from Sunday's dashboard) and what
// Sunday can see of the window the shortcut was pressed over. The user's
// choices here go to the engine, which keeps them and enforces them.
import { useCallback, useEffect, useState } from "react";
import { bridge, type ConfirmRequest, type ShortcutState, type SundayStatus } from "./api";
import { AgentConfirm } from "./Confirm";
import { dashboardUrl, HermesCredit } from "./Sunday";

export const PROVIDER_NOTICE = "Anything Sunday looks at is sent to the model provider you chose for Sunday.";
/** The most the desk takes in one file, and in one drop (the engine checks too). */
const DESK_MAX_BYTES = 20 * 1024 * 1024;
const DESK_MAX_FILES = 10;

type CaptureOptions = { allow?: "once" | "always"; key?: string; pick?: boolean };

function SeeingLine(props: { state: ShortcutState | null; busy: boolean; onCapture: (options: CaptureOptions) => void }) {
  const { state } = props;
  if (!state) return null;
  const app = state.target?.app ?? "this window";
  // The panel says which app the user chose for, so a choice never applies
  // to a window the shortcut was pressed over later.
  const key = state.target?.key;
  if (state.phase === "looking") return <div className="seeing" role="status">Looking at the window you’re in…</div>;
  if (state.phase === "offline") return <div className="seeing warn" role="status">OpenAgora’s engine isn’t running, so Sunday can’t see your windows right now.</div>;
  if (state.phase === "capturing") return <div className="seeing" role="status">Capturing {app}…</div>;
  if (state.phase === "error") {
    return <div className="seeing warn" role="alert">
      {state.error}
      {window.openagora?.platform === "darwin" && /Screen Recording/.test(state.error ?? "") &&
        <> <button className="link" onClick={() => void bridge?.openScreenSettings()}>Open System Settings</button></>}
    </div>;
  }
  if (state.item) {
    return <div className="seeing ok" role="status">
      {state.picture && <img className="seeing-picture" src={state.picture} alt={`What Sunday got from ${app}`} />}
      <span><strong>Sunday sees {app}.</strong> <span className="muted small block">On the desk as {state.item.id}. {PROVIDER_NOTICE}</span></span>
    </div>;
  }
  switch (state.verdict) {
    case "not-allowed":
      return <div className="seeing" role="status">
        <span><strong>Sunday can’t see {app} yet.</strong> <span className="muted small block">Allow it to let Sunday look at its window when you press the shortcut. {PROVIDER_NOTICE}</span></span>
        <span className="actions">
          <button className="btn" disabled={props.busy} onClick={() => props.onCapture({ allow: "once", key })}>Allow once</button>
          <button className="btn primary" disabled={props.busy} onClick={() => props.onCapture({ allow: "always", key })}>Always allow</button>
        </span>
      </div>;
    case "never":
      return <div className="seeing" role="status"><strong>OpenAgora never captures {app}.</strong> <span className="muted small block">It’s on the never-capture list ({state.rule}).</span></div>;
    case "unknown":
      if (!state.target && /only OpenAgora/.test(state.unknown ?? "")) {
        return <div className="seeing" role="status">Only OpenAgora is on screen, so there’s no other window for Sunday to see.</div>;
      }
      return <div className="seeing" role="status">
        <span>OpenAgora can’t see which window you’re in here{state.unknown ? ` (${state.unknown})` : ""}.
          {state.pick && <span className="muted small block">Pick the window for Sunday in your system’s dialog. {PROVIDER_NOTICE}</span>}</span>
        {state.pick && <button className="btn" disabled={props.busy} onClick={() => props.onCapture({ pick: true })}>Pick a window</button>}
      </div>;
    default:
      return null;
  }
}

export function Panel() {
  const [status, setStatus] = useState<SundayStatus | null>(null);
  const [state, setState] = useState<ShortcutState | null>(null);
  const [asks, setAsks] = useState<ConfirmRequest[]>([]);
  const [note, setNote] = useState<{ text: string; error: boolean } | null>(null);
  const [busy, setBusy] = useState(false);
  const [dragging, setDragging] = useState(false);

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

  useEffect(() => {
    if (!note) return;
    const t = window.setTimeout(() => setNote(null), note.error ? 9000 : 4000);
    return () => window.clearTimeout(t);
  }, [note]);

  const captureNow = async (options: CaptureOptions) => {
    if (!bridge) return;
    setBusy(true);
    try { setState(await bridge.panel.capture(options)); } finally { setBusy(false); }
  };

  const share = async (work: () => Promise<unknown>, done: string) => {
    try {
      await work();
      setNote({ text: done, error: false });
    } catch (e) {
      setNote({ text: (e instanceof Error ? e.message : String(e)).replace(/^Error invoking remote method '[^']+': (Error: )?/, ""), error: true });
    }
  };

  // Dropped files are read here and sent as their contents.
  const onDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setDragging(false);
    if (!bridge) return;
    const files = [...e.dataTransfer.files];
    if (files.length === 0) return;
    void share(async () => {
      if (files.length > DESK_MAX_FILES) throw new Error(`Drop up to ${DESK_MAX_FILES} files at a time.`);
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
      onDrop={onDrop}>
      <header className="panel-head">
        <span className="panel-title">
          <span className="sunday-title"><span aria-hidden>☀</span> Sunday</span>
          <HermesCredit />
        </span>
        <span className="bar-actions">
          <button className="link" onClick={() => void bridge?.showMain()}>Open OpenAgora</button>
          <button className="link" aria-label="Close" title="Close (Esc)" onClick={() => void bridge?.panel.hide()}>×</button>
        </span>
      </header>
      <SeeingLine state={state} busy={busy} onCapture={(options) => void captureNow(options)} />
      <div className="panel-body">
        {!bridge ? <p className="empty">Sunday’s panel runs in the desktop app.</p>
          : running ? <webview key={status!.pid ?? 0} src={dashboardUrl(status!.url!, "/chat", status!.profile)} partition="persist:sunday" className="webview" />
          : status?.setup === "ready" ? <div className="empty">
            <p>Sunday isn’t running.</p>
            <button className="btn primary" onClick={() => void share(async () => {
              const result = await bridge!.start(status.id);
              if (!result.ok) throw new Error(result.stderr.trim() || "Couldn’t start Sunday.");
              await refresh();
            }, "Sunday started.")}>Start Sunday</button>
          </div>
          : <div className="empty">
            <p>{status ? "Set Sunday up in OpenAgora first." : "Checking Sunday…"}</p>
            {status && <button className="btn primary" onClick={() => void bridge?.showMain()}>Open OpenAgora</button>}
          </div>}
      </div>
      <footer className="panel-foot">
        <button className="btn ghost" onClick={() => void share(() => bridge!.desk.addClipboard(), "Put the clipboard on the desk for Sunday.")} disabled={!bridge}>Add clipboard</button>
        <span className="muted small">or drop files here. {PROVIDER_NOTICE}</span>
      </footer>
      {note && <div className={note.error ? "toast error" : "toast"} role="status" onClick={() => setNote(null)}>{note.text}</div>}
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
