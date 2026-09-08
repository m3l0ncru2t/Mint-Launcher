import { useEffect, useState } from "react";
import { api } from "../api";
import { parseChatLines, type ChatEntry } from "../chatParser";
import { PlayerAvatar } from "./PlayerAvatar";

interface Props {
  instanceId: string;
  logLines: string[];
  isRunning: boolean;
}

/// Same live/disk-tail split ServerConsolePanel uses (see there for why the
/// fallback exists), but parsed into chat/join/leave/whisper entries instead
/// of shown as raw text - see chatParser.ts. Whispers are the one thing here
/// that genuinely never reaches a normal player's own chat window (other
/// players don't see someone else's private message at all), so they're
/// called out distinctly rather than blended in with public chat.
export function ChatPanel({ instanceId, logLines, isRunning }: Props) {
  const hasLiveLines = logLines.length > 0;
  const [fallbackContent, setFallbackContent] = useState("");

  useEffect(() => {
    if (hasLiveLines || !isRunning) {
      setFallbackContent("");
      return;
    }
    let cancelled = false;
    function poll() {
      api
        .readLogFile(instanceId, "latest.log")
        .then((text) => !cancelled && setFallbackContent(text))
        .catch(() => !cancelled && setFallbackContent(""));
    }
    poll();
    const interval = setInterval(poll, 2000);
    return () => {
      cancelled = true;
      clearInterval(interval);
    };
  }, [instanceId, isRunning, hasLiveLines]);

  const entries = parseChatLines(hasLiveLines ? logLines.join("\n") : fallbackContent);

  return (
    <>
      <div className="panel-header">
        <h4>Chat</h4>
      </div>
      {/* Deliberately not auto-scrolled - unlike the raw console, chat is
          something you might scroll back through while it keeps growing,
          and having it yank back to the bottom on every new message makes
          that impossible. */}
      <div className="log-console chat-console">
        {entries.length === 0 ? (
          <span className="placeholder">
            Chat, joins/leaves, and private messages will appear here once the server's running.
          </span>
        ) : (
          entries.map((entry, i) => <ChatLine key={i} entry={entry} />)
        )}
      </div>
    </>
  );
}

/// Resolves a username to a Mojang UUID so a chat line can show a real
/// player head instead of just a name - cached at module scope (shared with
/// every `ChatAvatar` on the page, and surviving remounts) since the same
/// handful of names tend to repeat across a whole chat log.
const uuidCache = new Map<string, string | null>();

function usePlayerUuid(name: string | undefined): string | null {
  const [uuid, setUuid] = useState<string | null>(name ? (uuidCache.get(name) ?? null) : null);

  useEffect(() => {
    if (!name) {
      setUuid(null);
      return;
    }
    const cached = uuidCache.get(name);
    if (cached !== undefined) {
      setUuid(cached);
      return;
    }
    let cancelled = false;
    api
      .lookupPlayerUuid(name)
      .then((id) => {
        uuidCache.set(name, id);
        if (!cancelled) setUuid(id);
      })
      .catch(() => {
        uuidCache.set(name, null);
      });
    return () => {
      cancelled = true;
    };
  }, [name]);

  return uuid;
}

/** A player's head next to a chat line. Falls back to `PlayerAvatar`'s own
 * plain-initial behavior when the name doesn't resolve to a real account
 * (offline-mode name, typo, lookup failure) rather than showing nothing. */
function ChatAvatar({ name }: { name: string | undefined }) {
  const uuid = usePlayerUuid(name);
  if (!name) return null;
  return <PlayerAvatar uuid={uuid ?? `offline-${name}`} username={name} className="player-row-avatar chat-line-avatar" size={20} />;
}

export function ChatLine({ entry }: { entry: ChatEntry }) {
  if (entry.type === "chat") {
    return (
      <div className="chat-line">
        <ChatAvatar name={entry.player} />
        <div className="chat-line-body">
          <span className="chat-line-player">{entry.player}</span>
          <span className="chat-line-text">{entry.message}</span>
        </div>
      </div>
    );
  }
  if (entry.type === "join") {
    return <div className="chat-line chat-line-system">→ {entry.player} joined the game</div>;
  }
  if (entry.type === "leave") {
    return <div className="chat-line chat-line-system">← {entry.player} left the game</div>;
  }
  return (
    <div className="chat-line chat-line-whisper">
      <ChatAvatar name={entry.player ?? entry.target} />
      <div className="chat-line-body">
        <span className="chat-line-badge">Private</span>
        <span className="chat-line-player">{entry.player ?? "You"}</span>
        {entry.target && <span className="chat-line-text"> → {entry.target}</span>}
        <span className="chat-line-text">: {entry.message}</span>
      </div>
    </div>
  );
}
