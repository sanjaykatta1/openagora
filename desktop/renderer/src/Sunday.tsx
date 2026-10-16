// Sunday, OpenAgora's personal agent: Hermes under its own `sunday` profile.
// OpenAgora shows Hermes's own dashboard and settings; nothing here
// reimplements them or touches credentials.
import { useEffect, useRef, useState } from "react";
import { agentWorkflow } from "../../electron/agent-workflows";
import { bridge, type App as Listing, type SundayStatus, type SundaySetupChoice } from "./api";
import { ExternalLink } from "./PersonalAgent";

const HERMES_SOURCE = "https://github.com/NousResearch/hermes-agent";
const HERMES_LICENSE = "https://github.com/NousResearch/hermes-agent/blob/main/LICENSE";
const NOUS_RESEARCH = "https://nousresearch.com";

/** Hermes's own dashboard pages for Sunday's profile, opened by Settings → Sunday. */
export const DASHBOARD_PAGES: readonly { route: string; label: string; about: string }[] = [
  { route: "/models", label: "Models", about: "Model provider and sign-in" },
  { route: "/channels", label: "Channels", about: "Discord, Telegram, WhatsApp, Slack, Signal and more" },
  { route: "/pairing", label: "Pairing", about: "Who may message Sunday" },
  { route: "/skills", label: "Skills", about: "What Sunday knows how to do" },
  { route: "/plugins", label: "Plugins", about: "Hermes plugins for Sunday" },
  { route: "/mcp", label: "MCP", about: "Sunday’s tool servers" },
  { route: "/cron", label: "Cron", about: "Scheduled jobs" },
  { route: "/config", label: "Config", about: "Every Hermes setting" },
  { route: "/env", label: "Keys", about: "API keys" },
  { route: "/sessions", label: "Sessions", about: "Past conversations" },
  { route: "/logs", label: "Logs", about: "What Hermes logged" },
  { route: "/system", label: "System", about: "Messaging gateway, memory and hooks" },
];

/** A page of Sunday's dashboard, always scoped to Sunday's profile. */
export function dashboardUrl(base: string, route: string, profile: string): string {
  const url = new URL(route || "/", base);
  url.searchParams.set("profile", profile);
  return url.toString();
}

export type SundayStage = "checking" | "install" | "update" | "choose" | "consent" | "setting-up" | "start" | "running";

export function sundayStage(status: SundayStatus | null): SundayStage {
  if (!status) return "checking";
  if (!status.hermes) return "install";
  if (status.version_ok === false) return "update";
  if (status.setup === "foreign") return "consent";
  if (status.setup !== "ready") {
    // Only a Hermes OpenAgora just installed has no earlier setup to bring along.
    return status.ask_how_to_start ? "choose" : "setting-up";
  }
  return status.dashboard === "running" && status.url ? "running" : "start";
}

/** Sunday is Hermes Agent, renamed. Wherever Sunday is shown, it says so: its
 *  makers and its license. */
export function HermesCredit() {
  return <span className="credit">
    powered by <ExternalLink url={HERMES_SOURCE}>Hermes Agent</ExternalLink> from <ExternalLink url={NOUS_RESEARCH}>Nous Research</ExternalLink> · <ExternalLink url={HERMES_LICENSE}>MIT license</ExternalLink>
  </span>;
}

export function SundayHome(props: {
  status: SundayStatus | null;
  error: string | null;
  listing: Listing | undefined;
  chosen: boolean;
  busy?: string;
  onChoose: () => void;
  onInstall: () => void;
  onSetup: (choice: SundaySetupChoice) => void;
  onUpdate: () => void;
  onStart: () => void;
  onConnect: () => void;
  onRetry: () => void;
}) {
  const { status, listing } = props;
  const stage = sundayStage(status);
  const [bringBots, setBringBots] = useState(false);
  const [start, setStart] = useState<"fresh" | "clone" | null>(null);
  const autoSetup = useRef(false);
  useEffect(() => {
    if (stage !== "setting-up") autoSetup.current = false;
    // OpenAgora just installed Hermes: set Sunday's profile up straight away.
    if (props.chosen && stage === "setting-up" && !autoSetup.current && !props.busy && !props.error) {
      autoSetup.current = true;
      props.onSetup("default");
    }
  }, [props.chosen, stage, props.busy, props.error]);

  const head = <header className="sunday-head">
    <div className="sunday-mark" aria-hidden>☀</div>
    <div>
      <h1>Sunday</h1>
      <p className="muted">OpenAgora’s personal agent · <HermesCredit /></p>
    </div>
  </header>;

  if (!bridge) return <section className="page sunday">{head}<p className="empty">Sunday runs in the desktop app.</p></section>;

  if (!props.chosen) return <section className="page sunday">
    {head}
    <div className="panel sunday-card">
      <h2>Meet Sunday</h2>
      <p>Sunday remembers across your apps and carries your work to the coding agents you use. It is Hermes Agent, the open-source agent by Nous Research, running in a Hermes profile of its own.</p>
      <button className="btn primary big" onClick={props.onChoose}>Use Sunday</button>
    </div>
  </section>;

  let body: React.ReactNode;
  switch (stage) {
    case "checking":
      body = props.error ? null : <p className="muted" role="status">Checking Sunday…</p>;
      break;
    case "install":
      body = <div className="panel sunday-card">
        <h2>Install Hermes to meet Sunday</h2>
        <p>Sunday remembers across your apps and carries your work to the coding agents you use. It is Hermes Agent, the open-source agent by Nous Research, installed from its official source.</p>
        <p className="muted small">Hermes’s own setup then asks for your model provider. OpenAgora never sees your keys or sign-ins.</p>
        {listing && !listing.available ? <p className="muted">Hermes isn’t available on this computer.</p> : (
          <button className="btn primary big" disabled={!!props.busy || !listing} onClick={props.onInstall}>{props.busy ?? "Install Hermes"}</button>
        )}
      </div>;
      break;
    case "update":
      body = <div className="panel sunday-card">
        <h2>Update Hermes for Sunday</h2>
        <p>Sunday needs Hermes {status!.min_version} or later, and this computer has {status!.version}. Hermes updates itself with its own <code>hermes update</code>.</p>
        <button className="btn primary big" disabled={!!props.busy} onClick={props.onUpdate}>{props.busy ?? "Update Hermes"}</button>
      </div>;
      break;
    case "choose":
      body = <div className="panel sunday-card">
        <h2>You already use Hermes</h2>
        <p>Sunday gets a Hermes profile of its own, so your current Hermes, its memory and its bots stay exactly as they are. How should Sunday start?</p>
        <fieldset className="sunday-choices" disabled={!!props.busy}>
          <legend className="sr-only">How Sunday starts</legend>
          <label className={start === "fresh" ? "choice selected" : "choice"}>
            <input type="radio" name="sunday-start" checked={start === "fresh"} onChange={() => setStart("fresh")} />
            <span><strong>Start fresh</strong><span className="muted block">A new, empty setup. Hermes’s own setup asks for a model.</span></span>
          </label>
          <label className={start === "clone" ? "choice selected" : "choice"}>
            <input type="radio" name="sunday-start" checked={start === "clone"} onChange={() => setStart("clone")} />
            <span><strong>Bring my setup</strong><span className="muted block">Copies your settings, API keys, SOUL.md, skills and memory into Sunday. Not your history.</span></span>
          </label>
          {start === "clone" && <label className="choice-detail">
            <input type="checkbox" checked={bringBots} onChange={(e) => setBringBots(e.target.checked)} />
            <span>Also copy my messaging bots
              <span className="muted block small">A messaging bot can answer for only one profile at a time. Your bots stay with your current Hermes unless you copy them. If you do, turn each bot off there; Hermes warns you about the clash. You can instead create a new bot just for Sunday later, in Settings → Sunday → Channels.</span>
            </span>
          </label>}
        </fieldset>
        <p className="muted small">Model sign-ins are shared with your main Hermes either way, so you won’t sign in again.</p>
        <button className="btn primary big" disabled={!start || !!props.busy}
          onClick={() => props.onSetup(start === "fresh" ? "fresh" : bringBots ? "clone-channels" : "clone")}>
          {props.busy ?? "Set up Sunday"}
        </button>
      </div>;
      break;
    case "consent":
      body = <div className="panel sunday-card">
        <h2>Use your “sunday” profile?</h2>
        <p>A Hermes profile named “sunday” already exists ({status!.profile_dir}), and OpenAgora didn’t create it. Sunday can use it: OpenAgora adds Sunday’s identity to the top of its SOUL.md and keeps everything else.</p>
        <button className="btn primary big" disabled={!!props.busy} onClick={() => props.onSetup("existing")}>{props.busy ?? "Use it for Sunday"}</button>
      </div>;
      break;
    case "setting-up":
      body = <p className="muted" role="status">{props.busy ?? "Setting up Sunday’s profile…"}</p>;
      break;
    case "start":
      body = <>
        {!status!.connected && <ConnectCard busy={props.busy} onConnect={props.onConnect} />}
        <div className="panel sunday-card">
          <h2>Sunday is ready</h2>
          <p>Sunday lives in Hermes’s “{status!.profile}” profile. Starting it opens Hermes’s dashboard for Sunday here.</p>
          <button className="btn primary big" disabled={!!props.busy} onClick={props.onStart}>{props.busy ?? "Start Sunday"}</button>
        </div>
      </>;
      break;
    case "running":
      return null; // The dashboard itself is shown (see SundayDashboard).
  }
  return <section className="page sunday">
    {head}
    {body}
    {props.error && <div className="notice error sunday-error" role="alert">
      {props.error} <button className="link" disabled={!!props.busy} onClick={() => { autoSetup.current = false; props.onRetry(); }}>Try again</button>
    </div>}
  </section>;
}

/** Asks before OpenAgora changes Sunday's Hermes profile to use OpenAgora. */
function ConnectCard(props: { busy?: string; onConnect: () => void; compact?: boolean }) {
  const [dismissed, setDismissed] = useState(false);
  if (dismissed) return null;
  return <div className={props.compact ? "notice connect-notice" : "panel sunday-card connect-card"}>
    {!props.compact && <h2>Let Sunday use OpenAgora</h2>}
    <p>{props.compact ? "Let Sunday use OpenAgora: " : ""}Sunday can then find, install and open apps for you (OpenAgora asks you first every time), and knows which apps you have. This adds three things to Sunday’s Hermes profile, using Hermes’s own settings command: OpenAgora’s MCP server, a hook that tells Sunday about your apps with each reply, and OpenAgora’s skills. It also lets that profile run hooks without asking (<code>hooks_auto_accept</code>), for Sunday’s profile only.</p>
    <div className="actions">
      <button className="btn primary" disabled={!!props.busy} onClick={props.onConnect}>{props.busy ?? "Connect Sunday"}</button>
      <button className="btn ghost" onClick={() => setDismissed(true)}>Not now</button>
    </div>
  </div>;
}

/** The subset of Electron's <webview> element used here. */
interface WebviewElement extends HTMLElement {
  reload(): void;
  loadURL(url: string): Promise<void>;
}

/** Hermes's dashboard for Sunday's profile, labelled Sunday. */
export function SundayDashboard(props: { status: SundayStatus; visible: boolean; route: string; busy?: string; onConnect: () => void }) {
  const { status } = props;
  const ref = useRef<WebviewElement | null>(null);
  const [src] = useState(() => dashboardUrl(status.url!, props.route, status.profile));
  const shown = useRef(src);
  const [failure, setFailure] = useState<string | null>(null);

  useEffect(() => {
    const target = dashboardUrl(status.url!, props.route, status.profile);
    if (target === shown.current) return;
    shown.current = target;
    void ref.current?.loadURL(target).catch(() => {});
  }, [props.route, status.url, status.profile]);

  useEffect(() => {
    const view = ref.current;
    if (!view) return;
    const failed = (event: Event) => {
      const { errorCode, errorDescription, isMainFrame } = event as Event & { errorCode: number; errorDescription: string; isMainFrame: boolean };
      if (isMainFrame && errorCode !== -3) setFailure(errorDescription || `error ${errorCode}`);
    };
    const loaded = () => setFailure(null);
    view.addEventListener("did-fail-load", failed);
    view.addEventListener("did-finish-load", loaded);
    return () => {
      view.removeEventListener("did-fail-load", failed);
      view.removeEventListener("did-finish-load", loaded);
    };
  }, []);

  return <div className="webview-wrap" style={{ display: props.visible ? "flex" : "none" }} aria-hidden={!props.visible}>
    <div className="webview-bar">
      <span className="sunday-title"><span aria-hidden>☀</span> Sunday</span>
      <HermesCredit />
      <span className="bar-actions">
        <button className="link" onClick={() => (setFailure(null), ref.current?.reload())}>Reload</button>
        <button className="link" onClick={() => void bridge?.openExternal(dashboardUrl(status.url!, props.route, status.profile))}>Open in browser ↗</button>
      </span>
    </div>
    {!status.connected && <ConnectCard compact busy={props.busy} onConnect={props.onConnect} />}
    {failure && <div className="notice error tab-notice">Sunday’s dashboard didn’t load: {failure}. <button className="link" onClick={() => (setFailure(null), ref.current?.reload())}>Retry</button></div>}
    {/* Sunday's own saved session, separate from every app tab. */}
    <webview ref={ref} src={src} partition="persist:sunday" className="webview" />
  </div>;
}

function GatewayLine({ status }: { status: SundayStatus }) {
  const gateway = status.gateway;
  if (status.dashboard !== "running") return <p className="muted">Start Sunday to see its messaging gateway.</p>;
  if (!gateway || gateway.running === null) return <p className="muted">Hermes didn’t report its messaging gateway.</p>;
  const platforms = Object.entries(gateway.platforms ?? {}).map(([name, p]) => `${name}: ${p?.state ?? "unknown"}`);
  return <p>
    <span className={gateway.running ? "dot on" : "dot"} />{" "}
    {gateway.running ? "Messaging gateway running" : "Messaging gateway stopped"}
    {platforms.length > 0 && <span className="muted"> · {platforms.join(", ")}</span>}
  </p>;
}

export function SundaySettings(props: {
  status: SundayStatus | null;
  busy?: string;
  onConnect: (connect: boolean) => void;
  onOpenPage: (route: string) => void;
  onChangeAgent: () => void;
  onRemove: () => void;
  children?: React.ReactNode;
}) {
  const { status } = props;
  const [confirmRemove, setConfirmRemove] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const ready = status?.setup === "ready";
  const terminalSetup = async () => {
    setMessage(null);
    setError(null);
    try {
      await bridge?.launchAgent("hermes", "setup");
      setMessage("Opened Hermes’s own setup for Sunday in Terminal. Finish it there.");
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
  };
  const setupCommand = agentWorkflow("hermes").commands.setup;
  return <section className="page detail">
    <header className="page-head"><div><h1>Settings</h1><p className="muted">Sunday’s settings are Hermes’s own.</p></div></header>

    <div className="panel">
      <h3>Sunday</h3>
      <p className="credits">Sunday is <ExternalLink url={HERMES_SOURCE}>Hermes Agent</ExternalLink>, the open-source agent by <ExternalLink url={NOUS_RESEARCH}>Nous Research</ExternalLink>, under the <ExternalLink url={HERMES_LICENSE}>MIT license</ExternalLink>. OpenAgora gives it Sunday’s name and shows Hermes’s own dashboard, setup and settings; the agent itself is Hermes.</p>
      {!status ? <p className="muted">Checking Sunday…</p> : !ready ? <p className="muted">Sunday isn’t set up yet. Open Sunday from the sidebar to set it up.</p> : <>
        <p>Runs in Hermes’s “{status.profile}” profile{status.version ? ` · Hermes ${status.version}` : ""} · <span className="muted small">{status.profile_dir}</span></p>
        <GatewayLine status={status} />
        <p className="muted small">Hermes runs its messaging gateway; OpenAgora only shows its status. Gateway controls are on the System page.</p>
        <div className="settings-row">
          <p>{status.connected
            ? "Sunday uses OpenAgora: its MCP server (the only one OpenAgora adds), a per-reply hook and OpenAgora’s skills."
            : "Sunday doesn’t use OpenAgora’s tools yet."}</p>
          <button className="btn ghost" disabled={!!props.busy} onClick={() => props.onConnect(!status.connected)}>
            {props.busy ?? (status.connected ? "Disconnect" : "Connect Sunday")}
          </button>
        </div>
        <div className="settings-pages">
          {DASHBOARD_PAGES.map((page) => (
            <button key={page.route} className="settings-page" onClick={() => props.onOpenPage(page.route)}>
              <strong>{page.label}</strong><span className="muted small">{page.about}</span>
            </button>
          ))}
        </div>
        <p className="muted small">These open Hermes’s own pages for Sunday’s profile. Messaging channels you connect there answer as Sunday.</p>
        <div className="native-agent-actions">
          <div>
            <button className="btn ghost" disabled={!status.hermes} onClick={() => void terminalSetup()}>{setupCommand.label}</button>
            <code>hermes {setupCommand.args.join(" ")}</code>
          </div>
        </div>
        {message && <p role="status">{message}</p>}
        {error && <p role="alert" className="agent-error">{error}</p>}
      </>}
    </div>

    {props.children}

    <div className="panel">
      <h3>Personal agent</h3>
      <p>Sunday, powered by Hermes Agent from Nous Research. More agents are coming soon.</p>
      <button className="btn ghost" onClick={props.onChangeAgent}>Change personal agent</button>
    </div>

    {status?.installed && <div className="panel danger">
      <h3>Remove Sunday</h3>
      <p className="muted">Deletes Hermes’s “{status.profile}” profile: Sunday’s settings, keys, memory, sessions and skills. Hermes itself and your other Hermes profiles stay.</p>
      {confirmRemove ? <p>
        Remove Sunday?{" "}
        <button className="btn danger" onClick={() => (setConfirmRemove(false), props.onRemove())}>Remove Sunday</button>{" "}
        <button className="btn ghost" onClick={() => setConfirmRemove(false)}>Cancel</button>
      </p> : <button className="btn ghost" disabled={!!props.busy} onClick={() => setConfirmRemove(true)}>{props.busy ?? "Remove Sunday…"}</button>}
    </div>}
  </section>;
}
