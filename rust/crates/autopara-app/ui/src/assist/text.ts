// The assistant panel's small pure pieces, kept apart from the DOM so they can be tested.

/** `m:ss`, or `h:mm:ss` past an hour. */
export function elapsed(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const [h, m, s] = [Math.floor(total / 3600), Math.floor(total / 60) % 60, total % 60];
  const pad = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${m}:${pad(s)}`;
}

/** The last `keep` lines, the oldest dropped first. */
export function tail<T>(lines: T[], item: T, keep: number): T[] {
  return [...lines, item].slice(-keep);
}
