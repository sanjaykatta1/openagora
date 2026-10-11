// Sunday's panel: a small window over any app, opened with Sunday's shortcut.
// It shows Sunday's chat (Hermes's own, from Sunday's dashboard) and what
// Sunday can see of the window the shortcut was pressed over. The user's
// choices here go to the engine, which keeps them and enforces them.
import { useCallback, useEffect, useState } from "react";
import { bridge, type ConfirmRequest, type ShortcutState, type SundayStatus } from "./api";
import { AgentConfirm } from "./Confirm";
import { dashboardUrl } from "./Sunday";

export const PROVIDER_NOTICE = "Anything Sunday looks at is sent to the model provider you chose for Sunday.";

function SeeingLine(props: { state: ShortcutState | null; busy: boolean; onCapture: (options: { allow?: "once" | "always"; pick?: boolean }) => void }) {
  const { state } = props;
  if (!state) return null;
  const app = state.target?.app ?? "this window";
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
          <button className="btn" disabled={props.busy} onClick={() => props.onCapture({ allow: "once" })}>Allow once</button>
          <button className="btn primary" disabled={props.busy} onClick={() => props.onCapture({ allow: "always" })}>Always allow</button>
        </span>
      </div>;
    case "never":
      return <div className="seeing" role="status"><strong>OpenAgora never captures {app}.</strong> <span className="muted small block">It’s on the never-capture list ({state.rule}).</span></div>;
    case "unknown":
      if (!state.target && /only OpenAgora/.test(state.unknown ?? "")) {
        return <div className="seeing" role="status">Only OpenAgora is on screen, so there’s no other window for Sunday to see.</div>;
      }
      return <div className="seeing" role="status">
        <span>OpenAgora can’t see which window you’re in here{state.unknown ? ` (${state.unknown})` : ""}. <span className="muted small block">{PROVIDER_NOTICE}</span></span>
        <button className="btn" disabled={props.busy} onClick={() => props.onCapture({ pick: true })}>Pick a window</button>
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

  const captureNow = async (options: { allow?: "once" | "always"; pick?: boolean }) => {
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

  const onDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setDragging(false);
    if (!bridge) return;
    const paths = [...e.dataTransfer.files].map((f) => bridge!.desk.pathForFile(f)).filter(Boolean);
    if (paths.length === 0) return;
    void share(() => bridge!.desk.addFiles(paths), paths.length === 1 ? "Put the file on the desk for Sunday." : `Put ${paths.length} files on the desk for Sunday.`);
  };

  const running = status?.dashboard === "running" && !!status.url;
  return (
    <div className={dragging ? "panel-shell dragging" : "panel-shell"}
      onDragOver={(e) => { e.preventDefault(); setDragging(true); }}
      onDragLeave={() => setDragging(false)}
      onDrop={onDrop}>
      <header className="panel-head">
        <span className="sunday-title"><span aria-hidden>☀</span> Sunday</span>
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
