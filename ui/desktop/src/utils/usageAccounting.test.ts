import { describe, expect, it } from 'vitest';
import {
  billedTokens,
  cacheTokens,
  knownBilledTokens,
  mostCompleteBilledTokens,
  sumBilledTokens,
} from './usageAccounting';

describe('usage accounting', () => {
  it('keeps the backend total exact and exposes bucket sums only as known subtotals', () => {
    const row = {
      inputTokens: 100,
      outputTokens: 25,
      cacheReadTokens: 300,
      cacheCreationTokens: 50,
      totalTokens: 125,
    };

    expect(cacheTokens(row)).toBe(350);
    expect(knownBilledTokens(row)).toBe(475);
    expect(billedTokens(row)).toBe(125);
  });

  it('keeps an authoritative total without reconstructing it from buckets', () => {
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

  it('preserves unknown billed and cache totals instead of coercing them to zero', () => {
    const incomplete = {
      inputTokens: 10,
      outputTokens: 5,
      cacheReadTokens: null,
      cacheCreationTokens: 2,
      totalTokens: null,
    };

    expect(cacheTokens(incomplete)).toBeNull();
    expect(knownBilledTokens(incomplete)).toBe(17);
    expect(billedTokens(incomplete)).toBeNull();
  });

  it('requires every ledger row to have an exact total before summing', () => {
    expect(sumBilledTokens([])).toBeNull();
    expect(
      sumBilledTokens([
        {
          inputTokens: 10,
          outputTokens: 5,
          cacheReadTokens: 20,
          cacheCreationTokens: 2,
          totalTokens: 37,
        },
      ])
    ).toBe(37);
    expect(
      sumBilledTokens([
        {
          inputTokens: 10,
          outputTokens: 5,
          cacheReadTokens: null,
          cacheCreationTokens: null,
          totalTokens: null,
        },
      ])
    ).toBeNull();
  });

  it('prefers an exact ledger total but never hides an incomplete ledger behind live counters', () => {
    expect(
      mostCompleteBilledTokens(125, [
        {
          inputTokens: 100,
          outputTokens: 25,
          cacheReadTokens: 300,
          cacheCreationTokens: 50,
          totalTokens: 475,
        },
      ])
    ).toBe(475);
    expect(
      mostCompleteBilledTokens(125, [
        {
          inputTokens: 100,
          outputTokens: 25,
          cacheReadTokens: null,
          cacheCreationTokens: null,
          totalTokens: null,
        },
      ])
    ).toBeNull();
    expect(mostCompleteBilledTokens(125, [])).toBe(125);
    expect(mostCompleteBilledTokens(null, [])).toBeNull();
  });
});
