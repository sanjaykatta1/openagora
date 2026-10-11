// Settings → Background mode: off unless the user turns it on. While on,
// OpenAgora keeps what apps the user always allowed show, on this computer,
// for Sunday to search; it can be paused, kept for fewer or more days, and
// cleared. The engine keeps and enforces all of it; this page shows it and
// passes the user's choices on.
import { useCallback, useEffect, useState } from "react";
import { bridge, type BackgroundState, type BackgroundStatus, type EngineResult } from "./api";
import { CoversTabs } from "./Covers";
import { PROVIDER_NOTICE } from "./Panel";

const KEEP_CHOICES = [1, 3, 7, 14, 30];

const errorText = (e: unknown) =>
  (e instanceof Error ? e.message : String(e)).replace(/^Error invoking remote method '[^']+': (Error: )?/, "");

const clock = (at: number) => new Date(at * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });

/** What the indicator and the tray say. */
export function backgroundWords(state: BackgroundState): string | null {
  if (!state.on) return null;
  if (!state.paused) return "Background capture is on";
  return state.pausedUntil === null ? "Background capture is paused" : `Background capture is paused until ${clock(state.pausedUntil)}`;
}

export function toIndicator(status: BackgroundStatus): BackgroundState {
  const until = status.paused_until !== undefined && status.paused_until < 1e12 ? status.paused_until : null;
  return { on: status.on, paused: status.on && status.paused, pausedUntil: until, keepDays: status.keep_days };
}

export function BackgroundSettings(props: { onChange: (state: BackgroundState) => void }) {
  const [status, setStatus] = useState<BackgroundStatus | null>(null);
  const [asking, setAsking] = useState<"on" | "clear" | null>(null);
  const [message, setMessage] = useState<{ text: string; error: boolean } | null>(null);
  const { onChange } = props;

  const load = useCallback(async () => {
    if (!bridge) return;
    try {
      const next = await bridge.background.status();
      setStatus(next);
      onChange(toIndicator(next));
    } catch (e) {
      setMessage({ text: errorText(e), error: true });
    }
  }, [onChange]);
  useEffect(() => { void load(); }, [load]);

  const run = async (work: () => Promise<EngineResult>, done: string) => {
    setMessage(null);
    try {
      const result = await work();
      if (!result.ok) throw new Error(result.stderr.trim() || "That didn’t work.");
      setMessage({ text: done, error: false });
    } catch (e) {
      setMessage({ text: errorText(e), error: true });
    }
    await load();
  };

  if (!bridge) return null;
  const keep = status?.keep_days ?? 7;
  const state = status ? toIndicator(status) : null;
  const size = status ? `${status.captures} captures, ${(status.bytes / (1024 * 1024)).toFixed(1)} MB` : "";
  return (
    <div className="panel" aria-labelledby="background-title">
      <h3 id="background-title">Background mode</h3>
      <p className="muted small">Off unless you turn it on. While it’s on and OpenAgora runs, OpenAgora looks at the window you’re in when you switch windows, and every few minutes while you stay, and keeps what apps you always allowed Sunday to see show (a picture and the window’s text) on this computer, for Sunday to search when you ask about something you saw. Apps you allowed once, and the never-capture list, are never captured. {PROVIDER_NOTICE}</p>
      <ul className="rows compact" aria-label="Background mode">
        <li className="row">
          <span>
            <strong>{!state ? "…" : !state.on ? "Off" : backgroundWords(state)}</strong>
            {status && <span className="muted block small">Kept: {size}, for {keep} {keep === 1 ? "day" : "days"}.</span>}
          </span>
          <span className="bar-actions">
            {state && !state.on && <button className="btn" onClick={() => setAsking("on")}>Turn on…</button>}
            {state?.on && !state.paused && <>
              <button className="btn ghost" onClick={() => void run(() => bridge!.background.pause(60), "Background capture is paused for an hour.")}>Pause for an hour</button>
              <button className="btn ghost" onClick={() => void run(() => bridge!.background.pause(null), "Background capture is paused until you resume it.")}>Pause</button>
            </>}
            {state?.on && state.paused && <button className="btn" onClick={() => void run(() => bridge!.background.resume(), "Background capture is on again.")}>Resume</button>}
            {state?.on && <button className="btn ghost" onClick={() => void run(() => bridge!.background.turn(false), "Background mode is off. What it kept stays until it expires, or until you clear it.")}>Turn off</button>}
          </span>
        </li>
        <li className="row">
          <label className="small">Keep captures for{" "}
            <select aria-label="Keep captures for" value={keep} onChange={(e) => {
              const days = Number(e.target.value);
              void run(() => bridge!.background.keep(days), `Background mode keeps captures for ${days} ${days === 1 ? "day" : "days"}.`);
            }}>
              {KEEP_CHOICES.map((d) => <option key={d} value={d}>{d} {d === 1 ? "day" : "days"}</option>)}
            </select>
          </label>
          <button className="btn ghost" disabled={!status || status.captures === 0} onClick={() => setAsking("clear")}>Clear all…</button>
        </li>
      </ul>
      {message && <p role={message.error ? "alert" : "status"} className={message.error ? "agent-error" : undefined}>{message.text}</p>}

      {asking && (
        <div className="modal-backdrop" onClick={() => setAsking(null)}>
          <CoversTabs />
          <div className="modal" role="dialog" aria-modal="true" aria-labelledby="background-ask-title" onClick={(e) => e.stopPropagation()}>
            {asking === "on" ? <>
              <h2 id="background-ask-title">Turn on background mode?</h2>
              <p>While OpenAgora runs, it looks at the window you’re in when you switch windows, and every few minutes while you stay. For apps you always allowed Sunday to see, it keeps a picture and the window’s text on this computer for {keep} {keep === 1 ? "day" : "days"}, so Sunday can find what you saw.</p>
              <p className="muted small">Apps you allowed once, and apps and windows on the never-capture list, are never captured. The menu bar or tray icon shows a dot while it captures. You can pause it, or clear what it kept, any time. {PROVIDER_NOTICE}</p>
            </> : <>
              <h2 id="background-ask-title">Delete everything background mode kept?</h2>
              <p>{size} will be deleted from this computer.</p>
            </>}
            <footer className="modal-foot">
              <button className="btn ghost" onClick={() => setAsking(null)}>Cancel</button>
              <button className="btn primary" autoFocus onClick={() => {
                const what = asking;
                setAsking(null);
                void (what === "on"
                  ? run(() => bridge!.background.turn(true), "Background mode is on.")
                  : run(() => bridge!.background.clear(), "Everything background mode kept is deleted."));
              }}>{asking === "on" ? "Turn on" : "Delete"}</button>
            </footer>
          </div>
        </div>
      )}
    </div>
  );
}
