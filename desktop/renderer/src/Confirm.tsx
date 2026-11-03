// The engine asking, in OpenAgora's own window, whether an agent may do
// something. The user's choice here is the permission; "Don't allow" has the
// focus and is the safe way out (Escape too).
import { useEffect, useRef } from "react";
import type { ConfirmRequest } from "./api";
import { Dialog } from "./ui";

const CLIENT_NAMES: Record<string, string> = {
  sunday: "Sunday",
  "claude-code": "Claude Code",
  codex: "Codex",
  opencode: "OpenCode",
  t3code: "T3 Code",
};

export const clientName = (id: string) => CLIENT_NAMES[id] ?? id;

export function AgentConfirm(props: { request: ConfirmRequest; waiting: number; onAnswer: (allowed: boolean, choice?: string) => void }) {
  const { request, onAnswer } = props;
  const deny = useRef<HTMLButtonElement | null>(null);
  useEffect(() => deny.current?.focus(), [request.id]);
  const choices = request.choices ?? [];
  return (
    <Dialog alert label={clientName(request.client)} title={request.title} onCancel={() => onAnswer(false)} actions={<>
      {props.waiting > 0 && <span className="modal-note">{props.waiting} more waiting</span>}
      <button ref={deny} className="btn" onClick={() => onAnswer(false)}>Don’t allow</button>
      {choices.length === 0
        ? <button className="btn primary" onClick={() => onAnswer(true)}>Allow</button>
        : choices.map((choice, i) => (
          <button key={choice.id} className={i === choices.length - 1 ? "btn primary" : "btn"} onClick={() => onAnswer(true, choice.id)}>{choice.label}</button>
        ))}
    </>}>
      <pre className="code">{request.message}</pre>
    </Dialog>
  );
}
