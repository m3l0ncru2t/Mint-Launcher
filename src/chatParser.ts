/// Turns raw server console lines into structured chat events - shared by
/// the local ChatPanel and the remote Chat tab, since both start from the
/// exact same kind of text (a log4j line per message). Deliberately
/// generous rather than exhaustive: public chat/join/leave are matched with
/// high confidence (these formats are effectively universal across vanilla
/// and modded servers), but private-message formats vary by which commands
/// mod a pack uses - the whisper patterns here catch vanilla's own wording
/// and the shapes most "msg"/"tell"/"w" mods use, flagged distinctly since
/// a whisper is the clearest example of something other players *don't* see
/// in their own chat window, even though the server (and its console/log)
/// always does.

export type ChatEntryType = "chat" | "whisper" | "join" | "leave";

export interface ChatEntry {
  type: ChatEntryType;
  raw: string;
  player?: string;
  target?: string;
  message?: string;
  /** Seconds since midnight, from the log4j timestamp - used only to dedupe
   * near-simultaneous repeats (see `parseChatLines`); not rendered. */
  seconds?: number;
}

// "[13:14:15] [Server thread/INFO]: " - captured (not just stripped) so
// parseChatLines can dedupe by how close together two identical lines are.
const LOG_PREFIX = /^\[(\d{1,2}):(\d{2}):(\d{2})\]\s*\[[^\]]*\]:\s*/;

export function parseChatLine(rawLine: string): ChatEntry | null {
  const prefixMatch = rawLine.match(LOG_PREFIX);
  const seconds = prefixMatch
    ? Number(prefixMatch[1]) * 3600 + Number(prefixMatch[2]) * 60 + Number(prefixMatch[3])
    : undefined;
  const text = rawLine.replace(LOG_PREFIX, "").trim();
  if (!text) return null;

  let m = text.match(/^<(.+?)>\s?(.*)$/);
  if (m) return { type: "chat", raw: rawLine, player: m[1], message: m[2], seconds };

  m = text.match(/^(.+?) joined the game$/);
  if (m) return { type: "join", raw: rawLine, player: m[1], seconds };

  m = text.match(/^(.+?) left the game$/);
  if (m) return { type: "leave", raw: rawLine, player: m[1], seconds };

  // "[Steve -> Alex] hi" - a common shape for commands-mod whispers.
  m = text.match(/^\[(.+?)\s*(?:->|→)\s*(.+?)\]\s*(.*)$/);
  if (m) return { type: "whisper", raw: rawLine, player: m[1], target: m[2], message: m[3], seconds };

  // Vanilla's own "/msg" wording, both directions.
  m = text.match(/^(.+?) whispers? to you:\s*(.*)$/i);
  if (m) return { type: "whisper", raw: rawLine, player: m[1], message: m[2], seconds };

  m = text.match(/^You whisper to (.+?):\s*(.*)$/i);
  if (m) return { type: "whisper", raw: rawLine, target: m[1], message: m[2], seconds };

  return null;
}

// A Discord-bridge mod relaying a message back into chat (or a modpack's
// log4j config double-appending every line on its own) tends to produce a
// second, near-identical line within a couple of seconds - not necessarily
// the very next line, since other players' chat can land in between. A
// plain "is it the same as the line right before it" check (adjacent-only)
// misses that; comparing against the last *kept* occurrence of the same
// content within this window catches it regardless of what's interleaved.
const DEDUPE_WINDOW_SECONDS = 4;

export function parseChatLines(text: string): ChatEntry[] {
  const entries = text
    .split("\n")
    .map(parseChatLine)
    .filter((entry): entry is ChatEntry => entry !== null);

  const lastKeptSeconds = new Map<string, number>();
  return entries.filter((entry) => {
    const key = `${entry.type}|${entry.player ?? ""}|${entry.target ?? ""}|${entry.message ?? ""}`;
    const prev = lastKeptSeconds.get(key);
    // No timestamp on either side (shouldn't normally happen) - can't judge
    // proximity, so don't risk dropping a legitimate message.
    const isDuplicate =
      prev !== undefined && entry.seconds !== undefined && entry.seconds - prev >= 0 && entry.seconds - prev <= DEDUPE_WINDOW_SECONDS;
    if (!isDuplicate) {
      lastKeptSeconds.set(key, entry.seconds ?? prev ?? 0);
    }
    return !isDuplicate;
  });
}
