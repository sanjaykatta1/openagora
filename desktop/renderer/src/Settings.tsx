// Settings, in tabs. Sunday's tabs show only to people who use Sunday; each
// tab reads what it shows from the engine when it opens.
import type { ReactNode } from "react";
import type { App, BackgroundState, SundayStatus } from "./api";
import { ActivitySettings } from "./Activity";
import { CodingAgentsSettings } from "./CodingAgents";
import { GeneralSettings } from "./General";
import { MemorySettings } from "./Memory";
import { PrivacySettings } from "./Privacy";
import { SundaySettings } from "./Sunday";

export type SettingsTab = "general" | "sunday" | "privacy" | "agents" | "memory" | "activity";

const TABS: { id: SettingsTab; label: string; sunday?: true }[] = [
  { id: "general", label: "General" },
  { id: "sunday", label: "Sunday", sunday: true },
  { id: "privacy", label: "Privacy", sunday: true },
  { id: "agents", label: "Coding agents", sunday: true },
  { id: "memory", label: "Memory", sunday: true },
  { id: "activity", label: "Activity" },
];

export function Settings(props: {
  tab: SettingsTab;
  onTab: (tab: SettingsTab) => void;
  /** The user uses Sunday. */
  sunday: boolean;
  status: SundayStatus | null;
  busy?: string;
  apps: App[];
  onConnect: (connect: boolean) => void;
  onOpenPage: (route: string) => void;
  onStopUsing: () => void;
  onRemove: () => void;
  onBackground: (state: BackgroundState) => void;
}) {
  const tabs = TABS.filter((t) => props.sunday || !t.sunday);
  const tab = tabs.find((t) => t.id === props.tab) ?? tabs[0];
  const content: Record<SettingsTab, () => ReactNode> = {
    general: () => <GeneralSettings sunday={props.sunday} />,
    sunday: () => <SundaySettings status={props.status} busy={props.busy} onConnect={props.onConnect}
      onOpenPage={props.onOpenPage} onStopUsing={props.onStopUsing} onRemove={props.onRemove} />,
    privacy: () => <PrivacySettings />,
    agents: () => <CodingAgentsSettings apps={props.apps} />,
    memory: () => <MemorySettings onBackground={props.onBackground} />,
    activity: () => <ActivitySettings />,
  };
  return (
    <section className="page">
      <header className="page-head"><h1>Settings</h1></header>
      <div className="tablist" role="tablist" aria-label="Settings">
        {tabs.map((t) => (
          <button key={t.id} role="tab" aria-selected={t.id === tab.id} onClick={() => props.onTab(t.id)}>{t.label}</button>
        ))}
      </div>
      <div className="tabpanel" role="tabpanel" aria-label={tab.label}>{content[tab.id]()}</div>
    </section>
  );
}
