/**
 * Compact elapsed time for live indicators: 3s · 59s · 1m 20s · 12m · 1h 4m.
 * Seconds are dropped past a minute (they add noise at that scale) except in
 * the first minute-and-a-bit, where they are the whole point.
 *
 * Deliberately NOT `formatTimeSinceLastWorked` (RecentChats.tsx) — that renders
 * "3m ago", past tense, which is wrong for a counter that is still running.
 */
export function formatElapsed(ms: number): string {
  const total = Math.floor(Math.max(0, ms) / 1000);
  if (total < 60) return `${total}s`;

  const minutes = Math.floor(total / 60);
  const seconds = total % 60;

  if (minutes < 60) {
    return seconds === 0 ? `${minutes}m` : `${minutes}m ${seconds}s`;
  }

  const hours = Math.floor(minutes / 60);
  const rem = minutes % 60;
  return rem === 0 ? `${hours}h` : `${hours}h ${rem}m`;
}
