import { describe, expect, it } from 'vitest';
// Re-exported from BaseChat so the billed-token helpers live "alongside" the
// other pure BaseChat helpers (BaseChat.artifacts.test.ts precedent).
import { selectBilledTokens, formatCompactNumber } from './BaseChat';

describe('BaseChat billed-token headline', () => {
  it('re-exports selectBilledTokens and never surfaces last-turn as billed', () => {
    // The Issue #1 numbers: 218k last-turn context vs 13.3M accumulated.
    // selectBilledTokens has no way to read last-turn — it only takes
    // accumulated inputs — so the headline can only ever be the billed figure.
    const billed = selectBilledTokens(
      {
        accumulatedInputTokens: 12_000_000,
        accumulatedOutputTokens: 1_300_000,
        accumulatedTotalTokens: 13_300_000,
      },
      { accumulated_total_tokens: 13_300_000 }
    );
    expect(billed).toBe(13_300_000);
    expect(billed).not.toBe(218_000);
  });

  it('formats the billed headline compactly (13.3M-style)', () => {
    expect(formatCompactNumber(3_300_000)).toBe('3.3M');
    expect(formatCompactNumber(13_300_000)).toBe('13M');
    expect(formatCompactNumber(218_000)).toBe('218k');
    expect(formatCompactNumber(1_300)).toBe('1.3k');
    expect(formatCompactNumber(0)).toBe('0');
  });
});
