import { useEffect, useState } from "react";
import { remoteApi } from "../remoteApi";
import type { RemoteServerLink } from "../types";

interface Props {
  link: RemoteServerLink;
  className: string;
}

/** Same fallback-to-letter-avatar shape as `InstanceIcon`, fetching the icon
 * over the remote admin API (see `RemoteInstanceDetail`'s header) instead of
 * a local Tauri command - used in the sidebar so a remote server's row is
 * recognizable at a glance instead of every one showing the same initial.
 * `/icon` resolves the server's own `server-icon.png` - the same favicon a
 * vanilla client shows for it in its own multiplayer list - independently
 * of `link.hasIcon` (which only tracks a *custom* Mint instance icon, a
 * separate and much rarer thing), so this fetches unconditionally rather
 * than gating on that flag the way an earlier version of this mistakenly
 * did. */
export function RemoteServerIcon({ link, className }: Props) {
  const [dataUrl, setDataUrl] = useState<string | null>(null);

  useEffect(() => {
    setDataUrl(null);
    let cancelled = false;
    remoteApi(link)
      .getIcon()
      .then((info) => !cancelled && setDataUrl(info.iconUrl))
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [link.id]);

  if (dataUrl) {
    return <img className={className} src={dataUrl} alt="" draggable={false} />;
  }
  return <div className={className}>{link.name.slice(0, 1).toUpperCase()}</div>;
}
