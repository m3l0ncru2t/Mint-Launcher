// Mint itself polls the console with `list` and `time query gametime` every
// few seconds (see InstanceDetail/PlayersPanel, and the equivalent polling
// the host runs for remote admins) to read player count and TPS/MSPT without
// needing a network ping - useful data, but their raw responses are pure
// implementation noise to a human watching the console. Filtered only for
// display/copy - the log on disk (and the Logs tab, which reads it) stays
// complete and untouched.
const MINT_POLL_LINE_PATTERN = /\bof a max of \d+ players online\b|\btime is \d+/;

export function isMintPollLine(line: string): boolean {
  return MINT_POLL_LINE_PATTERN.test(line);
}
