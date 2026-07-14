import { describe, expect, it } from 'vitest';
import {
  billedTokens,
  cacheTokens,
  mostCompleteBilledTokens,
  sumBilledTokens,
} from './usageAccounting';

describe('usage accounting', () => {
  it('includes both cache buckets in the billed total', () => {
    const row = {
      inputTokens: 100,
      outputTokens: 25,
      cacheReadTokens: 300,
      cacheCreationTokens: 50,
      totalTokens: 125,
    };

    expect(cacheTokens(row)).toBe(350);
    expect(billedTokens(row)).toBe(475);
  });

  it('keeps an authoritative total when it is larger than the bucket sum', () => {
    expect(
      billedTokens({
        inputTokens: 100,
        outputTokens: 25,
        cacheReadTokens: 0,
        cacheCreationTokens: 0,
        totalTokens: 500,
      })
    ).toBe(500);
  });

  it('returns null only when no ledger rows exist', () => {
    expect(sumBilledTokens([])).toBeNull();
    expect(
      sumBilledTokens([
        {
          inputTokens: 10,
          outputTokens: 5,
          cacheReadTokens: 20,
          cacheCreationTokens: 2,
          totalTokens: 15,
        },
      ])
    ).toBe(37);
  });

  it('uses the ledger when cache buckets make it more complete than live counters', () => {
    expect(
      mostCompleteBilledTokens(125, [
        {
          inputTokens: 100,
          outputTokens: 25,
          cacheReadTokens: 300,
          cacheCreationTokens: 50,
          totalTokens: 125,
        },
      ])
    ).toBe(475);
    expect(mostCompleteBilledTokens(null, [])).toBeNull();
  });
});
