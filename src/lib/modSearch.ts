import type { ModFile, ModUpdateInfo } from "../types";

/** Mods whose file name or Modrinth title contains every word of `query`. */
export function filterMods(mods: ModFile[], updates: Record<string, ModUpdateInfo>, query: string): ModFile[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return mods;
  return mods.filter((m) => {
    const hay = `${m.fileName} ${updates[m.fileName]?.title ?? ""}`.toLowerCase();
    return words.every((w) => hay.includes(w));
  });
}
