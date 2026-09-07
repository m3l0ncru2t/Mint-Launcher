import { useState } from "react";
import { ConfigsPanel } from "./ConfigsPanel";
import { LogsPanel } from "./LogsPanel";
import { ModsPanel } from "./ModsPanel";
import { PlayersPanel } from "./PlayersPanel";
import { ResourcePacksPanel } from "./ResourcePacksPanel";
import { ServerConsolePanel } from "./ServerConsolePanel";

interface Props {
  instanceId: string;
  isServer: boolean;
  logLines: string[];
  isRunning: boolean;
  showConfigsLogsTabs: boolean;
}

type Tab = "mods" | "resourcepacks" | "configs" | "logs" | "console" | "players";

export function InstanceFilesPanel({ instanceId, isServer, logLines, isRunning, showConfigsLogsTabs }: Props) {
  const [tab, setTab] = useState<Tab>("mods");

  const tabs: { id: Tab; label: string }[] = [
    { id: "mods", label: "Mods" },
    { id: "resourcepacks", label: "Resource Packs" },
    ...(showConfigsLogsTabs ? [{ id: "configs" as Tab, label: "Configs" }] : []),
    ...(showConfigsLogsTabs ? [{ id: "logs" as Tab, label: "Logs" }] : []),
    ...(isServer ? [{ id: "console" as Tab, label: "Console" }] : []),
    ...(isServer ? [{ id: "players" as Tab, label: "Players" }] : []),
  ];
  // Falls back to Mods if the currently selected tab was hidden by a
  // settings change (e.g. the experimental toggle got turned off while a
  // now-gone tab was active) rather than rendering a blank panel.
  const activeTab = tabs.some((t) => t.id === tab) ? tab : "mods";

  return (
    <div className="mods-panel">
      <div className="files-tab-bar">
        {tabs.map((t) => (
          <button
            key={t.id}
            className={`files-tab${activeTab === t.id ? " active" : ""}`}
            onClick={() => setTab(t.id)}
          >
            {t.label}
          </button>
        ))}
      </div>
      {activeTab === "mods" && <ModsPanel instanceId={instanceId} />}
      {activeTab === "resourcepacks" && <ResourcePacksPanel instanceId={instanceId} />}
      {activeTab === "configs" && showConfigsLogsTabs && <ConfigsPanel instanceId={instanceId} />}
      {activeTab === "logs" && showConfigsLogsTabs && <LogsPanel instanceId={instanceId} />}
      {activeTab === "console" && isServer && (
        <ServerConsolePanel instanceId={instanceId} logLines={logLines} isRunning={isRunning} />
      )}
      {activeTab === "players" && isServer && <PlayersPanel instanceId={instanceId} isRunning={isRunning} />}
    </div>
  );
}
