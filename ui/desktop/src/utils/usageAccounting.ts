export interface TokenBuckets {
  inputTokens: number;
  outputTokens: number;
  totalTokens?: number | null;
  cacheReadTokens?: number | null;
  cacheCreationTokens?: number | null;
}

function finiteNonNegative(value: number | null | undefined): number {
  return typeof value === 'number' && Number.isFinite(value) && value >= 0 ? value : 0;
}

function finiteNonNegativeOrNull(value: number | null | undefined): number | null {
  return typeof value === 'number' && Number.isFinite(value) && value >= 0 ? value : null;
}

/** Exact cache total, or null when either historical cache bucket is unknown. */
export function cacheTokens(
  row: Pick<TokenBuckets, 'cacheReadTokens' | 'cacheCreationTokens'>
): number | null {
  const cacheRead = finiteNonNegativeOrNull(row.cacheReadTokens);
  const cacheCreation = finiteNonNegativeOrNull(row.cacheCreationTokens);
  return cacheRead === null || cacheCreation === null ? null : cacheRead + cacheCreation;
}

/** Sum only the token buckets the backend could measure. This is a lower bound. */
export function knownBilledTokens(row: TokenBuckets): number {
  return (
    finiteNonNegative(row.inputTokens) +
    finiteNonNegative(row.outputTokens) +
    finiteNonNegative(row.cacheReadTokens) +
    finiteNonNegative(row.cacheCreationTokens)
  );
}

/**
 * Exact billed total certified by the backend. A null total deliberately stays
 * null; rebuilding it from partial buckets would present incomplete history as
 * exact usage.
 */
export function billedTokens(row: TokenBuckets): number | null {
  return finiteNonNegativeOrNull(row.totalTokens);
}

export function sumBilledTokens(rows: TokenBuckets[]): number | null {
  if (rows.length === 0) return null;

  let total = 0;
  for (const row of rows) {
    const rowTotal = billedTokens(row);
    if (rowTotal === null) return null;
    total += rowTotal;
  }
  return total;
}

export function mostCompleteBilledTokens(
  accumulatedCounters: number | null,
  ledgerRows: TokenBuckets[]
): number | null {
  if (ledgerRows.length === 0) return accumulatedCounters;

  const ledgerTotal = sumBilledTokens(ledgerRows);
  if (ledgerTotal === null) return null;
  if (accumulatedCounters === null) return ledgerTotal;
  return Math.max(accumulatedCounters, ledgerTotal);
}
