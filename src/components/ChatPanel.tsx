import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import { parseChatLines, type ChatEntry } from "../chatParser";

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
  const logRef = useRef<HTMLDivElement>(null);
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

  useEffect(() => {
    if (logRef.current) logRef.current.scrollTop = logRef.current.scrollHeight;
  }, [entries.length]);

  return (
    <>
      <div className="panel-header">
        <h4>Chat</h4>
      </div>
      <div className="log-console chat-console" ref={logRef}>
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

export function ChatLine({ entry }: { entry: ChatEntry }) {
  if (entry.type === "chat") {
    return (
      <div className="chat-line">
        <span className="chat-line-player">{entry.player}</span>
        <span className="chat-line-text">{entry.message}</span>
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
      <span className="chat-line-badge">Private</span>
      <span className="chat-line-player">{entry.player ?? "You"}</span>
      {entry.target && <span className="chat-line-text"> → {entry.target}</span>}
      <span className="chat-line-text">: {entry.message}</span>
    </div>
  );
}
