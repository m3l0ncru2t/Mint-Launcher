import { useState } from "react";
import { ModsPanel } from "./ModsPanel";
import { ResourcePacksPanel } from "./ResourcePacksPanel";

interface Props {
  instanceId: string;
}

type Tab = "mods" | "resourcepacks";

export function InstanceFilesPanel({ instanceId }: Props) {
  const [tab, setTab] = useState<Tab>("mods");

  return (
    <div className="mods-panel">
      <div className="files-tab-bar">
        <button className={`files-tab${tab === "mods" ? " active" : ""}`} onClick={() => setTab("mods")}>
          Mods
        </button>
        <button
          className={`files-tab${tab === "resourcepacks" ? " active" : ""}`}
          onClick={() => setTab("resourcepacks")}
        >
          Resource Packs
        </button>
      </div>
      {tab === "mods" ? <ModsPanel instanceId={instanceId} /> : <ResourcePacksPanel instanceId={instanceId} />}
    </div>
  );
}
