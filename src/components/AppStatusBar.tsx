import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { getAvailableUpdateVersion, startUpdate, UPDATE_AVAILABLE_EVENT } from "./UpdateBanner";

/** Thin bar along the bottom of the window: the running launcher version,
 * and an Update button once the startup check finds a newer release. */
export function AppStatusBar() {
  const [version, setVersion] = useState<string | null>(null);
  const [available, setAvailable] = useState<string | null>(getAvailableUpdateVersion());

  useEffect(() => {
    getVersion()
      .then(setVersion)
      .catch(() => {});
    const onAvailable = (e: Event) => setAvailable((e as CustomEvent<string>).detail);
    window.addEventListener(UPDATE_AVAILABLE_EVENT, onAvailable);
    return () => window.removeEventListener(UPDATE_AVAILABLE_EVENT, onAvailable);
  }, []);

  return (
    <div className="app-status-bar">
      <span>Mint Launcher{version ? ` v${version}` : ""}</span>
      {available && (
        <button className="app-status-update" onClick={startUpdate} title="Download and install, then restart">
          Update to v{available}
        </button>
      )}
    </div>
  );
}
