import { useEffect, useMemo, useState } from "react";
import { api } from "../api";
import { parseChatLines, type ChatEntry } from "../chatParser";
import { useAutoFollow } from "../hooks/useAutoFollow";
import { enqueueMojangLookup } from "../lib/mojangLookupQueue";
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

  // Memoized for the same reason ServerConsolePanel memoizes its own
  // displayText - re-parsing the whole growing log on every unrelated
  // re-render (TPS/stats polling elsewhere) got more expensive the longer a
  // server had been logging.
  const entries = useMemo(
    () => parseChatLines(hasLiveLines ? logLines.join("\n") : fallbackContent),
    [hasLiveLines, logLines, fallbackContent],
  );

  // Off by default would mean chat you're actively reading keeps getting
  // yanked to the bottom on every new message; on by default (with a button
  // to turn it off, and turning itself off/on as the user scrolls away
  // from/back to the bottom - see useAutoFollow) lets you scroll back
  // through history without a fight, while still following along by default
  // like a normal chat window.
  const { ref: logRef, autoFollow, setAutoFollow, onScroll } = useAutoFollow<HTMLDivElement>(entries.length);

  return (
    <>
      <div className="panel-header">
        <h4>Chat</h4>
        <div className="panel-actions">
          <button
            className={`ghost-btn small${autoFollow ? " selected" : ""}`}
            onClick={() => setAutoFollow((f) => !f)}
            title={autoFollow ? "New messages keep scrolling this into view" : "Scrolling stays put as new messages arrive"}
          >
            Auto-follow
          </button>
        </div>
      </div>
      <div className="log-console chat-console" ref={logRef} onScroll={onScroll}>
        {entries.length === 0 ? (
          <span className="placeholder">
            Chat, joins/leaves, and private messages will appear here once the server's running.
          </span>
        ) : (
          entries.map((entry, i) => (
            <ChatLine
              key={i}
              entry={entry}
              onModerate={isRunning ? (command) => api.sendInstanceCommand(instanceId, command) : undefined}
            />
          ))
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
    enqueueMojangLookup(() => api.lookupPlayerUuid(name))
      .then((id) => {
        uuidCache.set(name, id);
        if (!cancelled) setUuid(id);
      })
      .catch(() => {
        // A transport-level failure (not a resolved "no such player", which
        // comes back as a plain `null` above) - transient, so left uncached
        // rather than permanently blacklisting this name.
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

/** Runs a raw moderation command against whichever backend a chat tab talks
 * to - `api.sendInstanceCommand` locally, `remoteApi(...).sendCommand` for a
 * remote-linked server (see ChatPanel/RemoteInstanceDetail). Undefined when
 * the server isn't running (nothing to send a console command to), in which
 * case `ChatLine` shows no moderation actions at all rather than a row of
 * buttons that would just error out. */
type ModerateFn = (command: string) => Promise<unknown>;

/** Kick/Ban/Ban IP for one chat participant, right where you're reading what
 * they said - saves a trip to the Players tab for the common case of
 * moderating someone based on something they just said in chat. Vanilla's
 * own `ban-ip` accepts an online player's name directly (resolving their
 * current IP itself), so this needs nothing beyond the name every other
 * action here already has. Hidden until the row is hovered, and entirely
 * absent when there's no console to send a command to. */
function ModerationActions({ name, onModerate }: { name: string; onModerate: ModerateFn }) {
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function run(action: string, command: string) {
    setBusy(action);
    setError(null);
    try {
      await onModerate(command);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }

  return (
    <span className="chat-line-actions" title={error ?? undefined}>
      <button className="ghost-btn small" disabled={busy !== null} onClick={() => run("kick", `kick ${name}`)}>
        {busy === "kick" ? "Kicking…" : "Kick"}
      </button>
      <button className="ghost-btn small" disabled={busy !== null} onClick={() => run("ban", `ban ${name}`)}>
        {busy === "ban" ? "Banning…" : "Ban"}
      </button>
      <button className="ghost-btn small" disabled={busy !== null} onClick={() => run("ban-ip", `ban-ip ${name}`)}>
        {busy === "ban-ip" ? "Banning…" : "Ban IP"}
      </button>
    </span>
  );
}

export function ChatLine({ entry, onModerate }: { entry: ChatEntry; onModerate?: ModerateFn }) {
  if (entry.type === "chat") {
    return (
      <div className="chat-line">
        <ChatAvatar name={entry.player} />
        <div className="chat-line-body">
          <span className="chat-line-player">{entry.player}</span>
          <span className="chat-line-text">{entry.message}</span>
        </div>
        {onModerate && entry.player && <ModerationActions name={entry.player} onModerate={onModerate} />}
      </div>
    );
  }
  if (entry.type === "join") {
    return <div className="chat-line chat-line-system">→ {entry.player} joined the game</div>;
  }
  if (entry.type === "leave") {
    return <div className="chat-line chat-line-system">← {entry.player} left the game</div>;
  }
  const whisperTarget = entry.player ?? entry.target;
  return (
    <div className="chat-line chat-line-whisper">
      <ChatAvatar name={whisperTarget} />
      <div className="chat-line-body">
        <span className="chat-line-badge">Private</span>
        <span className="chat-line-player">{entry.player ?? "You"}</span>
        {entry.target && <span className="chat-line-text"> → {entry.target}</span>}
        <span className="chat-line-text">: {entry.message}</span>
      </div>
      {onModerate && whisperTarget && <ModerationActions name={whisperTarget} onModerate={onModerate} />}
    </div>
  );
}
