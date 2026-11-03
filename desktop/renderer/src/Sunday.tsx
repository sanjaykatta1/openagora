// Sunday, OpenAgora's personal agent: Hermes under its own `sunday` profile.
// OpenAgora shows Hermes's own dashboard and settings; nothing here
// reimplements them or touches credentials.
import { useEffect, useRef, useState, type ReactNode } from "react";
import { bridge, errorText, osName, type App as Listing, type SundaySetupChoice, type SundayStatus } from "./api";
import { Commands, Dialog, ExternalLink, Row, SwitchRow, useNotify } from "./ui";

const HERMES_SOURCE = "https://github.com/NousResearch/hermes-agent";
const HERMES_LICENSE = "https://github.com/NousResearch/hermes-agent/blob/main/LICENSE";
const NOUS_RESEARCH = "https://nousresearch.com";

/** Said wherever the user lets Sunday see more. */
export const PROVIDER_NOTICE = "What Sunday looks at is sent to its model provider.";

/** Hermes's own dashboard pages for Sunday's profile, opened from Settings. */
export const DASHBOARD_PAGES: readonly { route: string; label: string }[] = [
  { route: "/models", label: "Models" },
  { route: "/channels", label: "Channels" },
  { route: "/pairing", label: "Pairing" },
  { route: "/skills", label: "Skills" },
  { route: "/plugins", label: "Plugins" },
  { route: "/mcp", label: "MCP" },
  { route: "/cron", label: "Cron" },
  { route: "/config", label: "Config" },
  { route: "/env", label: "Keys" },
  { route: "/sessions", label: "Sessions" },
  { route: "/logs", label: "Logs" },
  { route: "/system", label: "System" },
];

/** What `openagora sunday connect` changes in Sunday's profile, with
 *  Hermes's own command (engine/src/sunday.rs). */
const CONNECT_COMMANDS = [
  "hermes -p sunday config set mcp_servers.openagora …",
  "hermes -p sunday config set hooks.pre_llm_call …",
  "hermes -p sunday config set skills.external_dirs …",
  "hermes -p sunday config set hooks_auto_accept true",
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

/** Sunday's home until its dashboard runs: one step at a time, from using
 *  Sunday at all to starting it. */
export function SundayHome(props: {
  status: SundayStatus | null;
  error: string | null;
  listing: Listing | undefined;
  /** The user uses Sunday; `decided`: they have answered at all. */
  chosen: boolean;
  decided: boolean;
  busy?: string;
  onChoose: () => void;
  onDecline: () => void;
  onInstall: () => void;
  onSetup: (choice: SundaySetupChoice) => void;
  onUpdate: () => void;
  onStart: () => void;
  onRetry: () => void;
}) {
  const { status, listing, busy } = props;
  const stage = sundayStage(status);
  const [start, setStart] = useState<"fresh" | "clone" | null>(null);
  const [bringBots, setBringBots] = useState(false);
  const autoSetup = useRef(false);
  useEffect(() => {
    if (stage !== "setting-up") autoSetup.current = false;
    // OpenAgora just installed Hermes: set Sunday's profile up straight away.
    if (props.chosen && stage === "setting-up" && !autoSetup.current && !busy && !props.error) {
      autoSetup.current = true;
      props.onSetup("default");
    }
  }, [props.chosen, stage, busy, props.error]);

  if (props.chosen && stage === "running") return null; // Its dashboard shows instead.

  const primary = (label: string, onClick: () => void, disabled = false) =>
    <button className="btn primary big" disabled={disabled || !!busy} onClick={onClick}>{busy ?? label}</button>;
  let body: ReactNode = null;
  if (!bridge) {
    body = <p className="muted">Sunday runs in the desktop app.</p>;
  } else if (!props.chosen) {
    body = <div className="actions">
      <button className="btn primary big" onClick={props.onChoose}>Use Sunday</button>
      {!props.decided && <button className="btn big" onClick={props.onDecline}>Not now</button>}
    </div>;
  } else if (stage === "install") {
    body = listing && !listing.available
      ? <p className="muted">Not available on {osName(bridge.platform)}</p>
      : primary("Install Hermes", props.onInstall, !listing);
  } else if (stage === "update") {
    body = primary("Update Hermes", props.onUpdate);
  } else if (stage === "choose") {
    body = <>
      <fieldset className="choices" disabled={!!busy}>
        <legend className="sr-only">How Sunday starts</legend>
        <label className="choice"><input type="radio" name="sunday-start" checked={start === "fresh"} onChange={() => setStart("fresh")} />Start fresh</label>
        <label className="choice"><input type="radio" name="sunday-start" checked={start === "clone"} onChange={() => setStart("clone")} />Bring my Hermes setup</label>
        {start === "clone" && <label className="choice sub">
          <input type="checkbox" checked={bringBots} onChange={(e) => setBringBots(e.target.checked)} />Include messaging bots
        </label>}
      </fieldset>
      {primary("Set up Sunday", () => props.onSetup(start === "fresh" ? "fresh" : bringBots ? "clone-channels" : "clone"), !start)}
    </>;
  } else if (stage === "consent") {
    body = primary("Use existing “sunday” profile", () => props.onSetup("existing"));
  } else if (stage === "setting-up") {
    body = <p className="muted" role="status">{busy ?? "Setting up…"}</p>;
  } else if (stage === "start") {
    body = primary("Start Sunday", props.onStart);
  }

  return (
    <section className="home">
      <div className="sunday-mark" aria-hidden>☀</div>
      <h1>Sunday</h1>
      <p className="muted">Hermes Agent by Nous Research</p>
      <div className="home-body">{body}</div>
      {props.error && <p className="notice error" role="alert">
        {props.error} <button className="link" disabled={!!busy} onClick={() => { autoSetup.current = false; props.onRetry(); }}>Try again</button>
      </p>}
    </section>
  );
}

/** Asks, above Sunday's dashboard, whether Sunday may use OpenAgora. */
function ConnectBanner(props: { busy?: string; onConnect: () => void }) {
  const [dismissed, setDismissed] = useState(false);
  if (dismissed) return null;
  return (
    <div className="banner">
      <span>Let Sunday use OpenAgora?</span>
      <button className="btn primary" disabled={!!props.busy} onClick={props.onConnect}>{props.busy ?? "Connect…"}</button>
      <button className="btn ghost" onClick={() => setDismissed(true)}>Not now</button>
    </div>
  );
}

/** Before OpenAgora changes Sunday's Hermes profile: exactly what changes. */
export function ConnectDialog(props: { onCancel: () => void; onConnect: () => void }) {
  return (
    <Dialog title="Let Sunday use OpenAgora?" onCancel={props.onCancel} actions={<>
      <button className="btn" onClick={props.onCancel}>Cancel</button>
      <button className="btn primary" autoFocus onClick={props.onConnect}>Connect</button>
    </>}>
      <Commands lines={CONNECT_COMMANDS} />
    </Dialog>
  );
}

/** The subset of Electron's <webview> element used here. */
interface WebviewElement extends HTMLElement {
  reload(): void;
  loadURL(url: string): Promise<void>;
}

/** Hermes's dashboard for Sunday's profile. */
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

  const reload = () => { setFailure(null); ref.current?.reload(); };
  return (
    <div className="webview-wrap" style={{ display: props.visible ? "flex" : "none" }} aria-hidden={!props.visible}>
      <div className="bar">
        <span>Sunday</span>
        <span className="bar-actions">
          <button className="btn ghost" onClick={reload}>Reload</button>
          <button className="btn ghost" onClick={() => void bridge?.openExternal(dashboardUrl(status.url!, props.route, status.profile))}>Open in browser</button>
        </span>
      </div>
      {!status.connected && <ConnectBanner busy={props.busy} onConnect={props.onConnect} />}
      {failure && <p className="notice error tab-notice">Sunday’s dashboard didn’t load: {failure}. <button className="link" onClick={reload}>Retry</button></p>}
      {/* Sunday's own saved session, separate from every app tab. */}
      <webview ref={ref} src={src} partition="persist:sunday" className="webview" />
    </div>
  );
}

/** Settings → Sunday: Hermes's own pages for Sunday's profile, OpenAgora's
 *  connection to it, and leaving or removing Sunday. */
export function SundaySettings(props: {
  status: SundayStatus | null;
  busy?: string;
  onConnect: (connect: boolean) => void;
  onOpenPage: (route: string) => void;
  onStopUsing: () => void;
  onRemove: () => void;
}) {
  const { status } = props;
  const notify = useNotify();
  const [removing, setRemoving] = useState(false);
  if (!status) return null;
  const gateway = status.dashboard === "running" ? status.gateway : null;
  const platforms = Object.entries(gateway?.platforms ?? {}).map(([name, p]) => `${name}: ${p?.state ?? "unknown"}`);
  return (
    <>
      {status.setup === "ready" && <>
        <ul className="group" aria-label="Sunday">
          <SwitchRow label="Let Sunday use OpenAgora" checked={status.connected} disabled={!!props.busy} onChange={props.onConnect} />
          {gateway && gateway.running !== null && <Row label="Messaging gateway">
            <span className="value">{[gateway.running ? "Running" : "Stopped", ...platforms].join(" · ")}</span>
          </Row>}
        </ul>
        <section>
          <h2>Hermes</h2>
          <div className="pages">
            {DASHBOARD_PAGES.map((page) => <button key={page.route} className="btn" onClick={() => props.onOpenPage(page.route)}>{page.label}</button>)}
          </div>
          <button className="btn" disabled={!status.hermes}
            onClick={() => bridge?.launchAgent("hermes", "setup").catch((e: unknown) => notify(errorText(e), true))}>Run setup in Terminal</button>
        </section>
      </>}
      <section className="actions">
        {props.busy ? <p className="muted" role="status">{props.busy}</p> : <>
          <button className="btn" onClick={props.onStopUsing}>Stop using Sunday</button>
          {status.installed && <button className="btn" onClick={() => setRemoving(true)}>Remove Sunday…</button>}
        </>}
      </section>
      <p className="credit">
        <ExternalLink url={HERMES_SOURCE}>Hermes Agent</ExternalLink> by <ExternalLink url={NOUS_RESEARCH}>Nous Research</ExternalLink> · <ExternalLink url={HERMES_LICENSE}>MIT license</ExternalLink>
      </p>
      {removing && (
        <Dialog title="Remove Sunday?" onCancel={() => setRemoving(false)} actions={<>
          <button className="btn" autoFocus onClick={() => setRemoving(false)}>Cancel</button>
          <button className="btn danger" onClick={() => { setRemoving(false); props.onRemove(); }}>Remove</button>
        </>}>
          <p>Deletes Sunday’s Hermes profile, with its keys, memory and sessions.</p>
        </Dialog>
      )}
    </>
  );
}
