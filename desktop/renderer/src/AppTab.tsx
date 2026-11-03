// An app's tab: its own page, which OpenAgora's main process owns and draws
// where this page shows the tab, so it can pop out into a window of its own
// and come back without reloading.
import { useLayoutEffect, useRef, useState } from "react";
import { bridge, type App } from "./api";
import { Monogram } from "./ui";

/** Each start of an app may print a one-time sign-in link (`url`). Use it for
 *  the first visit only; after that the tab's saved session is signed in, so
 *  later visits go to the app's plain address (`home_url`). */
function entryUrl(app: App): string {
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

/** The end of an app's log, or why there's none. */
async function readLog(id: string): Promise<string> {
  const result = await bridge?.logs(id);
  return (result ? (result.ok ? result.stdout : result.stderr) : "") || "The log is empty.";
}

export function AppTab({ app, visible }: { app: App; visible: boolean }) {
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

  const reload = () => {
    setFailure(null);
    void tabs?.reload(app.id);
  };

  return (
    <div className="webview-wrap" style={{ display: visible ? "flex" : "none" }}>
      <div className="bar">
        <span>{app.name}</span>
        <span className="bar-actions">
          <button className="btn ghost" onClick={reload}>Reload</button>
          {popped
            ? <button className="btn ghost" onClick={() => void tabs?.bringBack(app.id)}>Bring back</button>
            : <button className="btn ghost" onClick={() => void tabs?.popOut(app.id)}>Pop out</button>}
          <button className="btn ghost" onClick={() => void bridge?.openExternal(app.home_url ?? src)}>Open in browser</button>
          <button className="btn ghost" onClick={async () => setLog(log === null ? await readLog(app.id) : null)}>Log</button>
        </span>
      </div>
      {failure && <p className="notice error tab-notice">{app.name} didn’t load: {failure}. <button className="link" onClick={reload}>Retry</button></p>}
      {log !== null && <pre className="code log tab-log">{log}</pre>}
      {popped ? (
        <div className="empty">
          <p>{app.name} is open in its own window.</p>
          <button className="btn" onClick={() => void tabs?.showWindow(app.id)}>Show window</button>
        </div>
      ) : (
        <div ref={host} className="webview" aria-label={app.name} role="region" />
      )}
    </div>
  );
}

/** An app that stopped on its own (it crashed, or quit elsewhere) says so in
 *  its tab, rather than leaving the tab blank. */
export function StoppedTab(props: { app: App; visible: boolean; busy?: string; onStart: () => void }) {
  const [log, setLog] = useState<string | null>(null);
  return (
    <div className="webview-wrap" style={{ display: props.visible ? "flex" : "none" }}>
      <div className="empty">
        <Monogram app={props.app} size="lg" />
        <h2>{props.app.name} isn’t running</h2>
        <span className="actions">
          <button className="btn primary" disabled={!!props.busy} onClick={props.onStart}>{props.busy ?? "Start"}</button>
          <button className="btn" onClick={async () => setLog(await readLog(props.app.id))}>Show log</button>
        </span>
        {log !== null && <pre className="code log">{log}</pre>}
      </div>
    </div>
  );
}
