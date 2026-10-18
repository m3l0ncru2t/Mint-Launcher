/** User-made groups for an instance's mods list. Saved as JSON in the mods
 * folder (see get_mod_groups/save_mod_groups). */
export interface ModGroup {
  id: string;
  name: string;
  collapsed: boolean;
}

export interface ModGroupData {
  groups: ModGroup[];
  /** modKey(fileName) -> group id */
  assignments: Record<string, string>;
}

export function normalizeGroupData(raw: unknown): ModGroupData {
  const r = (raw ?? {}) as Partial<ModGroupData>;
  return {
    groups: Array.isArray(r.groups) ? r.groups.filter((g) => g && typeof g.id === "string") : [],
    assignments: r.assignments && typeof r.assignments === "object" ? r.assignments : {},
  };
}

/** A mod's identity that survives updates and enable/disable: the file name
 * without `.disabled`/`.jar` and without its version, e.g.
 * "lithium-fabric-0.26.2+mc26.3.jar" -> "lithium-fabric",
 * "ferritecore-9.0.0-fabric(1).jar" -> "ferritecore". Everything from the
 * first separator-led token that starts with a digit (or "v1"/"mc1") on is
 * treated as version. */
export function modKey(fileName: string): string {
  const base = fileName
    .replace(/\.disabled$/i, "")
    .replace(/\.jar$/i, "")
    .toLowerCase();
  const tokens = base.split(/([-_+ ])/);
  let out = tokens[0];
  for (let i = 1; i < tokens.length; i += 2) {
    const next = tokens[i + 1] ?? "";
    if (/^(v|mc)?\d/.test(next)) break;
    out += tokens[i] + next;
  }
  return out || base;
}
