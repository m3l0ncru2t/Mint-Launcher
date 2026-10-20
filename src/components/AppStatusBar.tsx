import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import {
  CHECK_STATUS_EVENT,
  getAvailableUpdateVersion,
  requestUpdateCheck,
  startUpdate,
  UPDATE_AVAILABLE_EVENT,
  type UpdateCheckStatus,
} from "./UpdateBanner";

/** Thin bar under the sidebar: the running launcher version, a button to
 * check for updates now (Mint also checks by itself every few hours - see
 * UpdateBanner), and an Update button once a newer release is found. */
export function AppStatusBar() {
  const [version, setVersion] = useState<string | null>(null);
  const [available, setAvailable] = useState<string | null>(getAvailableUpdateVersion());
  const [checkStatus, setCheckStatus] = useState<UpdateCheckStatus | null>(null);
  // "Up to date"/"Check failed" only show briefly after a manual check.
  const [flash, setFlash] = useState<string | null>(null);
  const [manualPending, setManualPending] = useState(false);

  useEffect(() => {
    getVersion()
      .then(setVersion)
      .catch(() => {});
    const onAvailable = (e: Event) => setAvailable((e as CustomEvent<string>).detail);
    const onStatus = (e: Event) => setCheckStatus((e as CustomEvent<UpdateCheckStatus>).detail);
    window.addEventListener(UPDATE_AVAILABLE_EVENT, onAvailable);
    window.addEventListener(CHECK_STATUS_EVENT, onStatus);
    return () => {
      window.removeEventListener(UPDATE_AVAILABLE_EVENT, onAvailable);
      window.removeEventListener(CHECK_STATUS_EVENT, onStatus);
    };
  }, []);

  useEffect(() => {
    if (!manualPending || checkStatus === "checking" || checkStatus === null) return;
    setManualPending(false);
    if (checkStatus === "none" || checkStatus === "error") {
      setFlash(checkStatus === "none" ? "Up to date" : "Check failed");
      const t = window.setTimeout(() => setFlash(null), 3000);
      return () => window.clearTimeout(t);
    }
  }, [checkStatus, manualPending]);

  function checkNow() {
    setFlash(null);
    setManualPending(true);
    requestUpdateCheck();
  }

  const checking = checkStatus === "checking";

  return (
    <div className="app-status-bar">
      <span>Mint Launcher{version ? ` v${version}` : ""}</span>
      {available ? (
        <button className="app-status-update" onClick={startUpdate} title="Download and install, then restart">
          Update to v{available}
        </button>
      ) : (
        <button
          className="app-status-check"
          onClick={checkNow}
          disabled={checking}
          title="Mint also checks by itself every few hours"
        >
          {checking ? "Checking…" : (flash ?? "Check for updates")}
        </button>
      )}
    </div>
  );
}
