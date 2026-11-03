// Background mode, in Settings → Memory: off unless the user turns it on.
// While on, OpenAgora keeps what the apps Sunday may always see show, on this
// computer, for Sunday to search; it can be paused, kept for fewer or more
// days, and cleared. The engine keeps and enforces all of it.
import { useState } from "react";
import { bridge, type BackgroundState, type BackgroundStatus } from "./api";
import { PROVIDER_NOTICE } from "./Sunday";
import { Dialog, Row, SwitchRow, useAction, useEngine } from "./ui";

const KEEP_CHOICES = [1, 3, 7, 14, 30];

const clock = (at: number) => new Date(at * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
const days = (n: number) => `${n} ${n === 1 ? "day" : "days"}`;

/** What the sidebar and the tray say while it's on. */
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
  const { onChange } = props;
  const [status, reload] = useEngine(async (b) => {
    const next = await b.background.status();
    onChange(toIndicator(next));
    return next;
  });
  const { run, busy } = useAction(reload);
  const [asking, setAsking] = useState<"on" | "clear" | null>(null);
  if (!bridge || !status) return null;
  const state = toIndicator(status);
  const size = `${status.captures} captures · ${(status.bytes / (1024 * 1024)).toFixed(1)} MB`;
  return (
    <>
      <ul className="group" aria-label="Background mode">
        <SwitchRow label="Background mode" checked={state.on} disabled={busy}
          onChange={(on) => (on ? setAsking("on") : void run(() => bridge!.background.turn(false)))} />
        {state.on && <Row label={!state.paused ? "Capturing" : state.pausedUntil === null ? "Paused" : `Paused until ${clock(state.pausedUntil)}`}>
          {state.paused
            ? <button className="btn" disabled={busy} onClick={() => void run(() => bridge!.background.resume())}>Resume</button>
            : <button className="btn" disabled={busy} onClick={() => void run(() => bridge!.background.pause(60))}>Pause for an hour</button>}
        </Row>}
        <Row label="Keep captures for">
          <select aria-label="Keep captures for" value={status.keep_days} disabled={busy}
            onChange={(e) => { const keep = Number(e.target.value); void run(() => bridge!.background.keep(keep)); }}>
            {KEEP_CHOICES.map((d) => <option key={d} value={d}>{days(d)}</option>)}
          </select>
        </Row>
        <Row label={size}>
          <button className="btn" disabled={busy || status.captures === 0} onClick={() => setAsking("clear")}>Clear all…</button>
        </Row>
      </ul>

      {asking === "on" && (
        <Dialog title="Turn on background mode?" onCancel={() => setAsking(null)} actions={<>
          <button className="btn" onClick={() => setAsking(null)}>Cancel</button>
          <button className="btn primary" autoFocus onClick={() => { setAsking(null); void run(() => bridge!.background.turn(true)); }}>Turn on</button>
        </>}>
          <p>OpenAgora keeps a picture and the text of the apps Sunday can always see, on this computer, for {days(status.keep_days)}.</p>
          <p className="muted">{PROVIDER_NOTICE}</p>
        </Dialog>
      )}
      {asking === "clear" && (
        <Dialog title="Delete everything background mode kept?" onCancel={() => setAsking(null)} actions={<>
          <button className="btn" autoFocus onClick={() => setAsking(null)}>Cancel</button>
          <button className="btn danger" onClick={() => { setAsking(null); void run(() => bridge!.background.clear()); }}>Delete</button>
        </>}>
          <p>{size}</p>
        </Dialog>
      )}
    </>
  );
}
