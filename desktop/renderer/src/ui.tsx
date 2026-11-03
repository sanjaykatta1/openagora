// What every screen shares: the toast, dialogs, settings rows, app icons, and
// reading from and acting through the engine.
import { createContext, useCallback, useContext, useEffect, useId, useRef, useState, type ReactNode } from "react";
import { bridge, errorText, type Bridge, type EngineResult } from "./api";
import { CoversTabs } from "./Covers";

type Notify = (text: string, error?: boolean) => void;

const NotifyContext = createContext<Notify>(() => {});
export const NotifyProvider = NotifyContext.Provider;
/** Says something in the window's toast. Only for what the screen itself
 *  doesn't show: failures, and work that finished out of sight. */
export const useNotify = () => useContext(NotifyContext);

/** The window's toast, and the function that shows it. */
export function useToast(): [ReactNode, Notify] {
  const [toast, setToast] = useState<{ text: string; error: boolean } | null>(null);
  useEffect(() => {
    if (!toast) return;
    const timer = window.setTimeout(() => setToast(null), toast.error ? 9000 : 4000);
    return () => window.clearTimeout(timer);
  }, [toast]);
  const notify = useCallback<Notify>((text, error = false) => setToast({ text, error }), []);
  const element = toast && (
    <div className={toast.error ? "toast error" : "toast"} role={toast.error ? "alert" : "status"} onClick={() => setToast(null)}>
      {toast.text}
    </div>
  );
  return [element, notify];
}

/** Reads something from the engine when the screen shows, and again on
 *  `reload`. A failure goes to the toast. */
export function useEngine<T>(read: (bridge: Bridge) => Promise<T>): [T | null, () => Promise<void>] {
  const notify = useNotify();
  const [value, setValue] = useState<T | null>(null);
  const reader = useRef(read);
  reader.current = read;
  const reload = useCallback(async () => {
    if (!bridge) return;
    try {
      setValue(await reader.current(bridge));
    } catch (e) {
      notify(errorText(e), true);
    }
  }, [notify]);
  useEffect(() => { void reload(); }, [reload]);
  return [value, reload];
}

const failed = (result: unknown): result is EngineResult =>
  !!result && typeof result === "object" && (result as EngineResult).ok === false;

/** Runs the user's changes through the engine, then reads again (`after`).
 *  A failure goes to the toast; `done` says what finished, when it's worth
 *  saying. Resolves to whether it worked. */
export function useAction(after?: () => Promise<void>) {
  const notify = useNotify();
  const [busy, setBusy] = useState(false);
  const run = useCallback(async (work: () => Promise<unknown>, done?: string): Promise<boolean> => {
    setBusy(true);
    try {
      const result = await work();
      if (failed(result)) throw new Error(result.stderr.trim() || "That didn’t work.");
      if (done) notify(done);
      return true;
    } catch (e) {
      notify(errorText(e), true);
      return false;
    } finally {
      setBusy(false);
      await after?.();
    }
  }, [notify, after]);
  return { run, busy };
}

/** A dialog over the window, and over app tabs (they step aside). Escape, or
 *  a click outside, cancels; an alert closes only by its buttons or Escape. */
export function Dialog(props: { title: string; label?: string; alert?: boolean; onCancel?: () => void; actions: ReactNode; children?: ReactNode }) {
  const id = useId();
  const { onCancel } = props;
  useEffect(() => {
    if (!onCancel) return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onCancel();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onCancel]);
  return (
    <div className="modal-backdrop" onClick={props.alert ? undefined : onCancel}>
      <CoversTabs />
      <div className="modal" role={props.alert ? "alertdialog" : "dialog"} aria-modal="true" aria-labelledby={`${id}-title`}
        aria-describedby={props.children ? `${id}-body` : undefined} onClick={(e) => e.stopPropagation()}>
        {props.label && <p className="modal-label">{props.label}</p>}
        <h2 id={`${id}-title`}>{props.title}</h2>
        {props.children && <div id={`${id}-body`} className="modal-body">{props.children}</div>}
        <footer className="modal-foot">{props.actions}</footer>
      </div>
    </div>
  );
}

/** Commands, exactly as they will run. */
export const Commands = ({ lines }: { lines: string[] }) => <pre className="code">{lines.join("\n")}</pre>;

/** One line of a settings list: what it is, then its value and controls. */
export function Row(props: { label: ReactNode; children?: ReactNode }) {
  return <li className="row"><span className="row-label">{props.label}</span>{props.children}</li>;
}

/** A setting that is on or off. */
export function SwitchRow(props: { label: string; checked: boolean; disabled?: boolean; onChange: (on: boolean) => void; children?: ReactNode }) {
  const id = useId();
  return (
    <li className="row">
      <label className="row-label" htmlFor={id}>{props.label}</label>
      {props.children}
      <input id={id} type="checkbox" className="switch" checked={props.checked} disabled={props.disabled}
        onChange={(e) => props.onChange(e.target.checked)} />
    </li>
  );
}

/** An app's initial on a colour of its own, standing in for its icon. */
export function Monogram({ app, size = "md" }: { app: { id: string; name: string }; size?: "xs" | "md" | "lg" }) {
  const hue = [...app.id].reduce((h, c) => (h * 31 + c.charCodeAt(0)) % 360, 7);
  return <span className={`monogram ${size}`} style={{ background: `hsl(${hue} 55% 46%)` }} aria-hidden>{app.name.charAt(0)}</span>;
}

/** A link that opens in the browser. */
export function ExternalLink({ url, children }: { url: string; children: ReactNode }) {
  return (
    <a className="link" href={url} target="_blank" rel="noreferrer" onClick={(e) => {
      if (!bridge) return;
      e.preventDefault();
      void bridge.openExternal(url);
    }}>{children}</a>
  );
}
