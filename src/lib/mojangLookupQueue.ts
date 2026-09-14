// A chat backlog can hold dozens of distinct player names, each mounting its
// own avatar the moment the Chat tab opens - every one of them firing a
// UUID-lookup or skin-URL request at once. Mojang's lookup APIs rate-limit
// fairly aggressively, so that burst got some of those requests 429'd; the
// caller (PlayerAvatar/usePlayerUuid) then cached the failure as "no skin
// for this player" *forever*, which is why some player heads never showed
// even for perfectly real accounts. Funneling every such request through one
// shared, small-concurrency queue keeps a big backlog from ever bursting
// Mojang in the first place, so it doesn't need to be retried in the first
// place.
const MAX_CONCURRENT = 4;

let active = 0;
const queue: (() => void)[] = [];

function next() {
  if (active >= MAX_CONCURRENT) return;
  const run = queue.shift();
  if (!run) return;
  active++;
  run();
}

/** Runs `fn` once fewer than `MAX_CONCURRENT` other queued calls are still
 * in flight - first come, first served. */
export function enqueueMojangLookup<T>(fn: () => Promise<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    queue.push(() => {
      fn()
        .then(resolve, reject)
        .finally(() => {
          active--;
          next();
        });
    });
    next();
  });
}
