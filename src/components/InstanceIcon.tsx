import { useEffect, useState } from "react";
import { api } from "../api";
import type { Instance } from "../types";

interface Props {
  instance: Instance;
  className: string;
}

/**
 * Falls back to a letter avatar until an icon (if any) loads. A custom
 * icon set via instance settings always wins; a server instance with no
 * custom icon shows its own server-icon.png instead (the same favicon a
 * vanilla client shows for it in the multiplayer list) rather than a
 * generic letter.
 */
export function InstanceIcon({ instance, className }: Props) {
  const [dataUrl, setDataUrl] = useState<string | null>(null);

  useEffect(() => {
    setDataUrl(null);
    let cancelled = false;
    if (instance.hasIcon) {
      api.getInstanceIcon(instance.id).then((url) => !cancelled && setDataUrl(url));
    } else if (instance.kind === "server") {
      api
        .getServerIcon(instance.id)
        .then((url) => !cancelled && setDataUrl(url))
        .catch(() => {});
    }
    return () => {
      cancelled = true;
    };
  }, [instance.id, instance.hasIcon, instance.kind]);

  if (dataUrl) {
    return <img className={className} src={dataUrl} alt="" draggable={false} />;
  }
  return <div className={className}>{instance.name.slice(0, 1).toUpperCase()}</div>;
}
