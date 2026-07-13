// The billed, per-conversation token figure.
//
// Issue #1: the chat surfaces used to show `total_tokens` / `tokenState.totalTokens`
// — the LAST turn's token count, i.e. current context-window occupancy. But every
// turn resends the whole conversation, so the amount actually billed is the
// ACCUMULATED sum across turns (a UCSF user saw 13.3M accumulated vs 218k
// last-turn). These helpers select the accumulated figure and NEVER the
// last-turn one.

export type BilledTokenState = {
  accumulatedInputTokens?: number | null;
  accumulatedOutputTokens?: number | null;
  accumulatedTotalTokens?: number | null;
} | null;

export type BilledSession = {
  accumulated_total_tokens?: number | null;
  accumulated_input_tokens?: number | null;
  accumulated_output_tokens?: number | null;
} | null;

function finiteOrNull(value: number | null | undefined): number | null {
  return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

// Best billed figure derivable from one source's accumulated counters.
//
// Providers report input/output/total as three INDEPENDENT optional fields
// (e.g. an OpenAI-compatible gateway like Versa can report only total_tokens),
// and both the SQL deltas and chatStreamStore coalesce a never-written column
// to 0 — so a genuine figure in one field can sit next to a 0 in another.
// Taking the max of `total` and `input + output` means a coalesced 0 on either
// side can never mask the real billed amount (each candidate only ever
// UNDER-counts, so the larger one is the closest to what was billed).
function billedFromCounters(
  input: number | null | undefined,
  output: number | null | undefined,
  total: number | null | undefined
): number | null {
  const inputTokens = finiteOrNull(input);
  const outputTokens = finiteOrNull(output);
  const sum =
    inputTokens !== null || outputTokens !== null ? (inputTokens ?? 0) + (outputTokens ?? 0) : null;
  const totalTokens = finiteOrNull(total);
  if (totalTokens !== null && sum !== null) return Math.max(totalTokens, sum);
  return totalTokens ?? sum;
}

/**
 * The accumulated (billed) tokens for a conversation. Prefers the live
 * streaming TokenState's accumulated counters; a resumed session with no live
 * TokenState yet falls back to the persisted `accumulated_*` columns on the
 * session row. Returns null when nothing is known, so callers can distinguish
 * "no data" from a genuine zero. NEVER returns the last-turn figure.
 */
export function selectBilledTokens(
  tokenState: BilledTokenState | undefined,
  session: BilledSession | undefined
): number | null {
  // Prefer the live accumulated counters from the streaming TokenState.
  if (tokenState) {
    const billed = billedFromCounters(
      tokenState.accumulatedInputTokens,
      tokenState.accumulatedOutputTokens,
      tokenState.accumulatedTotalTokens
    );
    if (billed !== null) return billed;
  }
  // Resumed session with no live TokenState: use the persisted accumulated row.
  if (session) {
    const billed = billedFromCounters(
      session.accumulated_input_tokens,
      session.accumulated_output_tokens,
      session.accumulated_total_tokens
    );
    if (billed !== null) return billed;
  }
  return null;
}

/**
 * Billed tokens for a session row in a list, with a last-turn fallback for old
 * sessions whose `accumulated_*` columns were never backfilled. `total_tokens`
 * is last-turn occupancy, so it is only used when no accumulated data exists at
 * all — it is strictly a floor, never preferred over the billed figure.
 */
export function billedSessionTokens(
  session: (BilledSession & { total_tokens?: number | null }) | null | undefined
): number | null {
  const billed = selectBilledTokens(null, session ?? null);
  if (billed !== null) return billed;
  return finiteOrNull(session?.total_tokens ?? null);
}
