// The engine asking, in OpenAgora's own window, whether an agent may do
// something. The user's choice here is the permission; "Don't allow" is the
// default and the safe way out (Escape too).
import { useEffect, useRef } from "react";
import type { ConfirmRequest } from "./api";
import { CoversTabs } from "./Covers";

const CLIENT_NAMES: Record<string, string> = {
  sunday: "Sunday",
  "claude-code": "Claude Code",
  codex: "Codex",
  opencode: "OpenCode",
  t3code: "T3 Code",
};

export const clientName = (id: string) => CLIENT_NAMES[id] ?? id;

export function AgentConfirm(props: { request: ConfirmRequest; waiting: number; onAnswer: (allowed: boolean, choice?: string) => void }) {
  const { request } = props;
  const deny = useRef<HTMLButtonElement | null>(null);
  useEffect(() => {
    deny.current?.focus();
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && props.onAnswer(false);
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [request.id]);
  const choices = request.choices ?? [];
  return (
    <div className="modal-backdrop">
      <CoversTabs />
      <div className="modal" role="alertdialog" aria-modal="true" aria-labelledby="agent-confirm-title" aria-describedby="agent-confirm-message">
        <header className="modal-head">
          <div className="sunday-mark small-mark" aria-hidden>☀</div>
          <div>
            <p className="muted small">{clientName(request.client)} asks OpenAgora</p>
            <h2 id="agent-confirm-title">{request.title}</h2>
          </div>
        </header>
        <pre id="agent-confirm-message" className="confirm-message">{request.message}</pre>
        <p className="muted small">OpenAgora is asking you directly; {clientName(request.client)} can’t answer this for you.{props.waiting > 0 ? ` ${props.waiting} more waiting.` : ""}</p>
        <footer className="modal-foot">
          <button ref={deny} className="btn ghost" onClick={() => props.onAnswer(false)}>Don’t allow</button>
          {choices.length === 0
            ? <button className="btn primary" onClick={() => props.onAnswer(true)}>Allow</button>
            : choices.map((choice, i) => (
              <button key={choice.id} className={i === choices.length - 1 ? "btn primary" : "btn"} onClick={() => props.onAnswer(true, choice.id)}>{choice.label}</button>
            ))}
        </footer>
      </div>
    </div>
  );
}
