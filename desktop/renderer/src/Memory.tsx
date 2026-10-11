// Settings → Memory: Sunday's memory is Hermes's. Built-in memory is always
// on; long-term memory (Holographic) and the digest that keeps it fresh are
// the user's choice, each set up with Hermes's own commands, shown here first.
// The engine runs them, and writes them to the audit log.
import { useCallback, useEffect, useState } from "react";
import { bridge, type EngineResult, type MemoryState } from "./api";

interface Asking {
  title: string;
  lines: string[];
  commands: string[];
  confirm: string;
  run: () => Promise<EngineResult>;
  done: string;
}

const errorText = (e: unknown) =>
  (e instanceof Error ? e.message : String(e)).replace(/^Error invoking remote method '[^']+': (Error: )?/, "");

const when = (at: number) => new Date(at * 1000).toLocaleString();

export function MemorySettings() {
  const [state, setState] = useState<MemoryState | null>(null);
  const [asking, setAsking] = useState<Asking | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ text: string; error: boolean } | null>(null);

  const load = useCallback(async () => {
    if (!bridge) return;
    try {
      setState(await bridge.memory.state());
    } catch (e) {
      setMessage({ text: errorText(e), error: true });
    }
  }, []);
  useEffect(() => { void load(); }, [load]);

  const run = async (work: () => Promise<EngineResult>, done: string) => {
    setBusy(true);
    setMessage(null);
    try {
      const result = await work();
      if (!result.ok) throw new Error(result.stderr.trim() || "That didn’t work.");
      setMessage({ text: done, error: false });
      await load();
    } catch (e) {
      setMessage({ text: errorText(e), error: true });
    } finally {
      setBusy(false);
    }
  };

  const answer = async () => {
    if (!asking) return;
    const { run: work, done } = asking;
    setAsking(null);
    await run(work, done);
  };

  if (!bridge || !state) return null;
  const provider = state.providers.find((p) => p.id === state.provider);
  const offered = state.providers[0];
  const agents = state.sessions.map((s) => s.name);

  const askUse = () => offered && setAsking({
    title: `Give Sunday long-term memory with ${offered.name}?`,
    lines: [
      `${offered.name} keeps facts in a local database in Sunday’s profile: no account, network or model calls. OpenAgora runs Hermes’s own commands, for Sunday’s profile only:`,
      `The first installs the plugin from Hermes’s catalog, at the version it reviewed, with its Python packages. It needs Hermes ${offered.needs} or later.`,
    ],
    commands: offered.commands,
    confirm: `Use ${offered.name}`,
    run: () => bridge!.memory.use(offered.id),
    done: `Sunday now remembers with ${offered.name}. Start a new Sunday session to pick it up.`,
  });

  const askOff = () => setAsking({
    title: "Back to built-in memory?",
    lines: [
      "OpenAgora runs Hermes’s own command:",
      `${provider?.name ?? "The provider"}’s facts stay in Sunday’s profile, and come back if you turn it on again.`,
    ],
    commands: ["hermes -p sunday memory off"],
    confirm: "Turn off",
    run: () => bridge!.memory.off(),
    done: "Sunday uses Hermes’s built-in memory only.",
  });

  const askDigest = () => setAsking({
    title: "Keep Sunday’s memory up to date?",
    lines: [
      `Every two hours while OpenAgora runs, if a session changed, Sunday reads your coding sessions${agents.length ? ` in ${agents.join(", ")}` : ""} and saves what’s worth remembering: decisions, conventions, open problems. It runs as one turn of Sunday’s, with only its memory and OpenAgora’s tools:`,
      "Sunday may then always read those sessions. What it reads goes to the model provider it uses, and each run costs one turn.",
    ],
    commands: ["hermes -p sunday -z \"OpenAgora digest…\" -s openagora-memory -t memory,mcp-openagora"],
    confirm: "Turn on",
    run: () => bridge!.memory.digest(true),
    done: "Sunday’s memory digest is on.",
  });

  return (
    <div className="panel" aria-labelledby="memory-title">
      <h3 id="memory-title">Memory</h3>
      <p className="muted small">Sunday remembers with Hermes’s memory, which stays across restarts. Ask it “what did I do in my coding sessions today?”: it reads your coding agents’ sessions on this computer, asking you first, and copies nothing.</p>
      <ul className="rows compact" aria-label="Sunday's memory">
        <li className="row">
          <span><strong>Long-term memory</strong> <span className="muted small">{provider
            ? `${provider.name}: facts in a local database in Sunday’s profile`
            : "built-in only (MEMORY.md and USER.md, a few dozen facts)"}</span></span>
          {provider
            ? <button className="btn ghost" disabled={busy} onClick={askOff}>Turn off…</button>
            : offered && <button className="btn" disabled={busy} onClick={askUse}>Use {offered.name}…</button>}
        </li>
        <li className="row">
          <span><strong>Digest</strong> <span className="muted small">{state.digest
            ? `on${state.digest_last_run ? `, last run ${when(state.digest_last_run)}${state.digest_last_result ? `: ${state.digest_last_result}` : ""}` : ""}`
            : "off: Sunday saves what you tell it, and looks up your sessions when you ask"}</span></span>
          {state.digest
            ? <button className="btn ghost" disabled={busy} onClick={() => void run(() => bridge!.memory.digest(false), "Sunday’s memory digest is off.")}>Turn off</button>
            : <button className="btn" disabled={busy} onClick={askDigest}>Turn on…</button>}
        </li>
      </ul>
      {state.sessions.length > 0 && <>
        <h4>Coding sessions Sunday may read</h4>
        <ul className="rows compact" aria-label="Coding sessions Sunday may read">
          {state.sessions.map((s) => (
            <li key={s.agent} className="row">
              <span><strong>{s.name}</strong> <span className="muted small">{s.always ? "always" : "asks you first"}</span></span>
              {s.always && <button className="btn ghost" disabled={busy}
                onClick={() => void run(() => bridge!.memory.revokeSessions(s.agent), `Sunday asks you before reading your ${s.name} sessions again.`)}>Take back</button>}
            </li>
          ))}
        </ul>
      </>}

      {asking && (
        <div className="modal-backdrop" onClick={() => !busy && setAsking(null)}>
          <div className="modal" role="dialog" aria-modal="true" aria-labelledby="memory-ask-title" onClick={(e) => e.stopPropagation()}>
            <h2 id="memory-ask-title">{asking.title}</h2>
            <p>{asking.lines[0]}</p>
            {asking.commands.map((command) => <pre key={command} className="confirm-message"><code>{command}</code></pre>)}
            {asking.lines.slice(1).map((line) => <p key={line} className="muted small">{line}</p>)}
            <footer className="modal-foot">
              <button className="btn ghost" onClick={() => setAsking(null)}>Cancel</button>
              <button className="btn primary" onClick={() => void answer()} autoFocus>{asking.confirm}</button>
            </footer>
          </div>
        </div>
      )}
      {busy && <p role="status" className="muted small">Working…</p>}
      {message && <p role={message.error ? "alert" : "status"} className={message.error ? "agent-error" : undefined}>{message.text}</p>}
    </div>
  );
}
