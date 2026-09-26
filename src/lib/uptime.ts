import { useEffect, useState } from "react";

export function formatDuration(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  if (h >= 24) return `${Math.floor(h / 24)}d ${h % 24}h`;
  if (h >= 1) return `${h}h ${Math.floor((seconds % 3600) / 60)}m`;
  return `${Math.max(0, Math.floor(seconds / 60))}m`;
}

/** "3h 12m" since `startedAtSecs` (unix seconds), re-rendering every 30s so
 * it keeps counting without another backend call. Null when not running. */
export function useUptime(startedAtSecs: number | null | undefined): string | null {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!startedAtSecs) return;
    setNow(Date.now());
    const id = window.setInterval(() => setNow(Date.now()), 30000);
    return () => window.clearInterval(id);
  }, [startedAtSecs]);
  if (!startedAtSecs) return null;
  return formatDuration(now / 1000 - startedAtSecs);
}
