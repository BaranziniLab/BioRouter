export interface TokenBuckets {
  inputTokens: number;
  outputTokens: number;
  totalTokens: number;
  cacheReadTokens?: number | null;
  cacheCreationTokens?: number | null;
}

function finiteNonNegative(value: number | null | undefined): number {
  return typeof value === 'number' && Number.isFinite(value) && value > 0 ? value : 0;
}

export function cacheTokens(row: Pick<TokenBuckets, 'cacheReadTokens' | 'cacheCreationTokens'>) {
  return finiteNonNegative(row.cacheReadTokens) + finiteNonNegative(row.cacheCreationTokens);
}

/**
 * Total billable tokens represented by a ledger bucket. The backend's total
 * includes cache reads and writes, while the individual input bucket is fresh
 * input only. Taking the larger candidate also keeps older or partial payloads
 * from silently dropping cache tokens.
 */
export function billedTokens(row: TokenBuckets): number {
  const bucketSum =
    finiteNonNegative(row.inputTokens) + finiteNonNegative(row.outputTokens) + cacheTokens(row);
  return Math.max(finiteNonNegative(row.totalTokens), bucketSum);
}

export function sumBilledTokens(rows: TokenBuckets[]): number | null {
  return rows.length === 0 ? null : rows.reduce((total, row) => total + billedTokens(row), 0);
}

export function mostCompleteBilledTokens(
  accumulatedCounters: number | null,
  ledgerRows: TokenBuckets[]
): number | null {
  const ledgerTotal = sumBilledTokens(ledgerRows);
  if (accumulatedCounters === null) return ledgerTotal;
  if (ledgerTotal === null) return accumulatedCounters;
  return Math.max(accumulatedCounters, ledgerTotal);
}
