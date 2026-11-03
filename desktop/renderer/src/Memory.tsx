// Settings → Memory: what Sunday remembers. Hermes's built-in memory is
// always on; long-term memory (Holographic), the digest that keeps it fresh,
// and background mode are the user's choice, each set up with the exact
// commands shown first. The engine runs them, and writes them to the audit log.
import { useState } from "react";
import { bridge, type BackgroundState } from "./api";
import { BackgroundSettings } from "./Background";
import { Commands, Dialog, Row, SwitchRow, useAction, useEngine } from "./ui";

interface Asking {
  title: string;
  text?: string;
  commands: string[];
  confirm: string;
  run: () => Promise<unknown>;
}

const DIGEST = "hermes -p sunday -z \"OpenAgora digest…\" -s openagora-memory -t memory,mcp-openagora";

export function MemorySettings(props: { onBackground: (state: BackgroundState) => void }) {
  const [state, reload] = useEngine((b) => b.memory.state());
  const { run, busy } = useAction(reload);
  const [asking, setAsking] = useState<Asking | null>(null);
  if (!bridge) return null;
  const provider = state?.providers.find((p) => p.id === state.provider);
  const offered = state?.providers[0];
  const agents = state?.sessions.map((s) => s.name) ?? [];

  const askUse = () => offered && setAsking({
    title: `Use ${offered.name} for long-term memory?`,
    commands: offered.commands,
    confirm: `Use ${offered.name}`,
    run: () => bridge!.memory.use(offered.id),
  });
  const askOff = () => setAsking({
    title: "Back to built-in memory?",
    commands: ["hermes -p sunday memory off"],
    confirm: "Turn off",
    run: () => bridge!.memory.off(),
  });
  const askDigest = () => setAsking({
    title: "Keep Sunday’s memory up to date?",
    text: `Every two hours, Sunday reads your coding sessions${agents.length ? ` in ${agents.join(", ")}` : ""} and saves what matters. Each run uses one model turn.`,
    commands: [DIGEST],
    confirm: "Turn on",
    run: () => bridge!.memory.digest(true),
  });

  return (
    <>
      {state && <ul className="group" aria-label="Memory">
        <Row label="Long-term memory">
          <span className="value">{provider?.name ?? "Built-in"}</span>
          {provider ? <button className="btn" disabled={busy} onClick={askOff}>Turn off…</button>
            : offered && <button className="btn" disabled={busy} onClick={askUse}>Use {offered.name}…</button>}
        </Row>
        <SwitchRow label="Digest" checked={state.digest} disabled={busy}
          onChange={(on) => (on ? askDigest() : void run(() => bridge!.memory.digest(false)))}>
          {state.digest && state.digest_last_run && <span className="value" title={state.digest_last_result}>
            {new Date(state.digest_last_run * 1000).toLocaleString([], { dateStyle: "medium", timeStyle: "short" })}
          </span>}
        </SwitchRow>
        {busy && <li className="row muted" role="status">Working…</li>}
      </ul>}

      <BackgroundSettings onChange={props.onBackground} />

      {state && state.sessions.length > 0 && <section>
        <h2>Coding sessions Sunday can read</h2>
        <ul className="group" aria-label="Coding sessions Sunday can read">
          {state.sessions.map((s) => (
            <Row key={s.agent} label={s.name}>
              <span className="value">{s.always ? "Always" : "Asks first"}</span>
              {s.always && <button className="btn" disabled={busy} onClick={() => void run(() => bridge!.memory.revokeSessions(s.agent))}>Take back</button>}
            </Row>
          ))}
        </ul>
      </section>}

      {asking && (
        <Dialog title={asking.title} onCancel={() => setAsking(null)} actions={<>
          <button className="btn" onClick={() => setAsking(null)}>Cancel</button>
          <button className="btn primary" autoFocus onClick={() => { setAsking(null); void run(asking.run); }}>{asking.confirm}</button>
        </>}>
          {asking.text && <p>{asking.text}</p>}
          <Commands lines={asking.commands} />
        </Dialog>
      )}
    </>
  );
}
