import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../api";

interface Props {
  instanceId: string;
  logLines: string[];
  isRunning: boolean;
}

// Mint itself polls the console with `list` and `time query gametime` every
// few seconds (see InstanceDetail/PlayersPanel) to read player count and
// TPS/MSPT without needing a network ping - useful data, but their raw
// responses are pure implementation noise to a human watching the console.
// Filtered only here, for display/copy - the actual log file on disk (and
// the Logs tab, which reads it) stays complete and untouched.
const MINT_POLL_LINE_PATTERN = /\bof a max of \d+ players online\b|\btime is \d+/;

function stripMintPolling(text: string): string {
  return text
    .split("\n")
    .filter((line) => !MINT_POLL_LINE_PATTERN.test(line))
    .join("\n");
}

export function ServerConsolePanel({ instanceId, logLines, isRunning }: Props) {
  const [copied, setCopied] = useState(false);
  const [command, setCommand] = useState("");
  const [sendingCommand, setSendingCommand] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [autoFollow, setAutoFollow] = useState(true);
  const logRef = useRef<HTMLDivElement>(null);

  // A "This instance isn't a running server" error from before a Stop/Start
  // cycle would otherwise stick around forever - nothing previously cleared
  // it except clicking Send again, even though the console is perfectly
  // usable again the moment the server comes back up.
  useEffect(() => {
    if (isRunning) setError(null);
  }, [isRunning]);

  // A server that was already running before Mint itself last restarted has
  // no live output stream - its stdout pipe belonged to the old process
  // instance, which is gone. `logLines` stays empty forever for it even
  // though the server is genuinely running (same underlying issue as the
  // pid-vs-progress desync fixed in InstanceDetail). Falling back to tailing
  // the same logs/latest.log the Logs tab reads gives it something real to
  // show instead of a permanently blank console.
  const hasLiveLines = logLines.length > 0;
  const [fallbackContent, setFallbackContent] = useState("");
  // Without this memo, every unrelated re-render (TPS/stats/player-count
  // polling in InstanceDetail fires every few seconds) re-joined and
  // re-filtered the *entire* console text again, even though nothing here
  // had changed - increasingly expensive the longer a busy server ran and
  // the more it had logged. Only actually recompute when the text itself
  // (a new line, or a fresh fallback poll) changes.
  const displayText = useMemo(
    () => (hasLiveLines ? stripMintPolling(logLines.join("\n")) : stripMintPolling(fallbackContent)),
    [hasLiveLines, logLines, fallbackContent],
  );

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

  // Only snaps to the bottom while auto-follow is on - otherwise scrolling
  // up to read past output would keep getting yanked back down by the next
  // line. Also re-snaps immediately when the button turns it back on.
  useEffect(() => {
    if (autoFollow && logRef.current) {
      logRef.current.scrollTop = logRef.current.scrollHeight;
    }
  }, [logLines, fallbackContent, autoFollow]);

  async function handleCopy() {
    await navigator.clipboard.writeText(displayText);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  }

  async function handleSendCommand() {
    if (!command.trim()) return;
    setSendingCommand(true);
    setError(null);
    try {
      await api.sendInstanceCommand(instanceId, command.trim());
      setCommand("");
    } catch (e) {
      setError(String(e));
    } finally {
      setSendingCommand(false);
    }
  }

  return (
    <>
      <div className="panel-header">
        <h4>Console</h4>
        <div className="panel-actions">
          <button
            className={`ghost-btn small${autoFollow ? " selected" : ""}`}
            onClick={() => setAutoFollow((f) => !f)}
            title={autoFollow ? "New output keeps scrolling this into view" : "Scrolling stays put as new output arrives"}
          >
            Auto-follow
          </button>
          <button className="ghost-btn small" onClick={handleCopy} disabled={!displayText}>
            {copied ? "Copied!" : "Copy"}
          </button>
        </div>
      </div>
      <div className="log-console" ref={logRef}>
        {displayText || <span className="placeholder">Server output will appear here once you hit Start.</span>}
      </div>
      {error && <div className="error-text">{error}</div>}
      {isRunning && (
        <div className="server-command-bar">
          <input
            type="text"
            placeholder="Type a server command…"
            value={command}
            onChange={(e) => setCommand(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") handleSendCommand();
            }}
            disabled={sendingCommand}
          />
          <button
            className="ghost-btn small"
            onClick={handleSendCommand}
            disabled={sendingCommand || !command.trim()}
          >
            Send
          </button>
        </div>
      )}
    </>
  );
}
