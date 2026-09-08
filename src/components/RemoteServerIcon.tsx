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
 * recognizable at a glance instead of every one showing the same initial. */
export function RemoteServerIcon({ link, className }: Props) {
  const [dataUrl, setDataUrl] = useState<string | null>(null);

  useEffect(() => {
    setDataUrl(null);
    if (!link.hasIcon) return;
    let cancelled = false;
    remoteApi(link)
      .getIcon()
      .then((info) => !cancelled && setDataUrl(info.iconUrl))
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [link.id, link.hasIcon]);

  if (dataUrl) {
    return <img className={className} src={dataUrl} alt="" draggable={false} />;
  }
  return <div className={className}>{link.name.slice(0, 1).toUpperCase()}</div>;
}
