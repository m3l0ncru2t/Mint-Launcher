import type { TextRun } from "../types";

/** Splits styled MOTD runs into lines on literal newlines - servers use
 * these to lay the MOTD out in two lines with deliberate spacing/centering,
 * which a single wrapped block of text would otherwise flatten away. */
export function splitMotdLines(runs: TextRun[]): TextRun[][] {
  const lines: TextRun[][] = [[]];
  for (const run of runs) {
    run.text.split("\n").forEach((part, i) => {
      if (i > 0) lines.push([]);
      if (part.length > 0) lines[lines.length - 1].push({ ...run, text: part });
    });
  }
  return lines;
}

/** A server MOTD with its Minecraft colors/styles, up to two lines like the
 * in-game server list. */
export function Motd({ runs, className = "server-motd" }: { runs: TextRun[]; className?: string }) {
  return (
    <div className={className}>
      {runs.length > 0
        ? splitMotdLines(runs)
            .slice(0, 2)
            .map((line, li) => (
              <div key={li} className="server-motd-line">
                {line.map((run, i) => (
                  <span
                    key={i}
                    style={{
                      color: run.color ?? undefined,
                      fontWeight: run.bold ? 700 : undefined,
                      fontStyle: run.italic ? "italic" : undefined,
                      textDecoration:
                        [run.underlined && "underline", run.strikethrough && "line-through"]
                          .filter(Boolean)
                          .join(" ") || undefined,
                    }}
                  >
                    {run.text}
                  </span>
                ))}
              </div>
            ))
        : "A Minecraft Server"}
    </div>
  );
}
