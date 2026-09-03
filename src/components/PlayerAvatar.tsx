import { useEffect, useState } from "react";
import { api } from "../api";

interface Props {
  uuid: string;
  username: string;
  className?: string;
  size?: number;
}

// Looked up once per uuid and reused everywhere that account's avatar shows
// up (switcher trigger, saved-account rows) rather than re-fetching on every
// mount.
const skinUrlCache = new Map<string, string | null>();

const SKIN_UPDATED_EVENT = "mint-player-skin-updated";

/** Call after changing an account's skin so every mounted avatar for that
 * uuid (e.g. the account switcher trigger, still showing behind the Skin &
 * Cape dialog) refetches instead of continuing to show the old cached head. */
export function invalidatePlayerAvatar(uuid: string) {
  skinUrlCache.delete(uuid);
  window.dispatchEvent(new CustomEvent<string>(SKIN_UPDATED_EVENT, { detail: uuid }));
}

export function PlayerAvatar({ uuid, username, className, size = 28 }: Props) {
  const [skinUrl, setSkinUrl] = useState<string | null>(skinUrlCache.get(uuid) ?? null);

  useEffect(() => {
    let cancelled = false;

    function fetchFresh() {
      api
        .getPlayerSkinUrl(uuid)
        .then((url) => {
          skinUrlCache.set(uuid, url);
          if (!cancelled) setSkinUrl(url);
        })
        .catch(() => {
          skinUrlCache.set(uuid, null);
        });
    }

    const cached = skinUrlCache.get(uuid);
    if (cached !== undefined) {
      setSkinUrl(cached ?? null);
    } else {
      fetchFresh();
    }

    function handleSkinUpdated(e: Event) {
      if ((e as CustomEvent<string>).detail === uuid) fetchFresh();
    }
    window.addEventListener(SKIN_UPDATED_EVENT, handleSkinUpdated);

    return () => {
      cancelled = true;
      window.removeEventListener(SKIN_UPDATED_EVENT, handleSkinUpdated);
    };
  }, [uuid]);

  // The front of the head sits at (8,8)-(16,16) on the 64x64 skin texture -
  // same crop technique as SkinCapeDialog's larger face preview, scaled down
  // to avatar size.
  const scale = size / 8;

  return (
    <div
      className={className ?? "account-avatar"}
      style={
        skinUrl
          ? {
              backgroundImage: `url(${skinUrl})`,
              backgroundSize: `${scale * 64}px ${scale * 64}px`,
              backgroundPosition: `-${scale * 8}px -${scale * 8}px`,
              imageRendering: "pixelated",
            }
          : undefined
      }
    >
      {!skinUrl && username.slice(0, 1).toUpperCase()}
    </div>
  );
}
