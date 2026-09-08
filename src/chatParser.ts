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
}

// "[13:14:15] [Server thread/INFO]: " - strips the log4j prefix so the
// patterns below only need to match the actual message content.
const LOG_PREFIX = /^\[\d{1,2}:\d{2}:\d{2}\]\s*\[[^\]]*\]:\s*/;

export function parseChatLine(rawLine: string): ChatEntry | null {
  const text = rawLine.replace(LOG_PREFIX, "").trim();
  if (!text) return null;

  let m = text.match(/^<(.+?)>\s?(.*)$/);
  if (m) return { type: "chat", raw: rawLine, player: m[1], message: m[2] };

  m = text.match(/^(.+?) joined the game$/);
  if (m) return { type: "join", raw: rawLine, player: m[1] };

  m = text.match(/^(.+?) left the game$/);
  if (m) return { type: "leave", raw: rawLine, player: m[1] };

  // "[Steve -> Alex] hi" - a common shape for commands-mod whispers.
  m = text.match(/^\[(.+?)\s*(?:->|→)\s*(.+?)\]\s*(.*)$/);
  if (m) return { type: "whisper", raw: rawLine, player: m[1], target: m[2], message: m[3] };

  // Vanilla's own "/msg" wording, both directions.
  m = text.match(/^(.+?) whispers? to you:\s*(.*)$/i);
  if (m) return { type: "whisper", raw: rawLine, player: m[1], message: m[2] };

  m = text.match(/^You whisper to (.+?):\s*(.*)$/i);
  if (m) return { type: "whisper", raw: rawLine, target: m[1], message: m[2] };

  return null;
}

export function parseChatLines(text: string): ChatEntry[] {
  return text
    .split("\n")
    .map(parseChatLine)
    .filter((entry): entry is ChatEntry => entry !== null);
}
