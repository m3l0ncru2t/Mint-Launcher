import { useEffect, useState } from "react";
import { api } from "../api";
import type { Instance } from "../types";

interface Props {
  instance: Instance;
  className: string;
}

/** Falls back to a letter avatar until a custom icon (if any) loads. */
export function InstanceIcon({ instance, className }: Props) {
  const [dataUrl, setDataUrl] = useState<string | null>(null);

  useEffect(() => {
    setDataUrl(null);
    if (!instance.hasIcon) return;
    let cancelled = false;
    api.getInstanceIcon(instance.id).then((url) => {
      if (!cancelled) setDataUrl(url);
    });
    return () => {
      cancelled = true;
    };
  }, [instance.id, instance.hasIcon]);

  if (dataUrl) {
    return <img className={className} src={dataUrl} alt="" />;
  }
  return <div className={className}>{instance.name.slice(0, 1).toUpperCase()}</div>;
}
