import { describe, expect, it } from 'vitest';
import {
  billedSessionTokenEstimate,
  billedSessionTokens,
  formatBilledTokenEstimate,
  selectBilledTokens,
} from './billedTokens';

describe('selectBilledTokens', () => {
  it('prefers the live TokenState accumulated counters', () => {
    // Issue #1 scenario: last-turn totalTokens is 218k but the billed figure is
    // the 13.3M accumulated amount. The helper cannot even see last-turn.
    const result = selectBilledTokens(
      {
        accumulatedInputTokens: 12_000_000,
        accumulatedOutputTokens: 1_300_000,
        accumulatedTotalTokens: 13_300_000,
      },
      null
    );
    expect(result).toBe(13_300_000);
  });

  it('sums input+output even when only one accumulated field is present', () => {
    expect(
      selectBilledTokens({ accumulatedInputTokens: 500, accumulatedOutputTokens: null }, null)
    ).toBe(500);
    expect(
      selectBilledTokens({ accumulatedInputTokens: null, accumulatedOutputTokens: 42 }, null)
    ).toBe(42);
  });

  it('uses accumulatedTotalTokens when the store coalesced a NULL input/output split to 0', () => {
    // Regression for the must-fix in issue #1 review: a Versa-style
    // OpenAI-compatible gateway can report total_tokens with no input/output
    // split, so accumulated_input/output_tokens stay NULL in SQLite and
    // chatStreamStore coalesces them to 0 (not null) when building the live
    // TokenState. The 0+0 sum must not mask the 13.3M total.
    const result = selectBilledTokens(
      {
        accumulatedInputTokens: 0,
        accumulatedOutputTokens: 0,
        accumulatedTotalTokens: 13_300_000,
      },
      null
    );
    expect(result).toBe(13_300_000);
  });

  it('uses the input+output sum when the store coalesced a NULL total to 0', () => {
    // Symmetric to the case above: a provider that reports only the split
    // leaves accumulated_total_tokens NULL, which the store also coalesces
    // to 0 — the genuine sum must win over that coalesced 0.
    const result = selectBilledTokens(
      {
        accumulatedInputTokens: 5_000_000,
        accumulatedOutputTokens: 500_000,
        accumulatedTotalTokens: 0,
      },
      null
    );
    expect(result).toBe(5_500_000);
  });

  it('applies the same coalesced-zero protection to the persisted session row', () => {
    expect(
      selectBilledTokens(null, {
        accumulated_input_tokens: 0,
        accumulated_output_tokens: 0,
        accumulated_total_tokens: 9_900_000,
      })
    ).toBe(9_900_000);
    expect(
      selectBilledTokens(null, {
        accumulated_input_tokens: 2_000_000,
        accumulated_output_tokens: 200_000,
        accumulated_total_tokens: 0,
      })
    ).toBe(2_200_000);
  });

  it('falls back to accumulatedTotalTokens when input/output are absent', () => {
    expect(
      selectBilledTokens(
        {
          accumulatedInputTokens: null,
          accumulatedOutputTokens: null,
          accumulatedTotalTokens: 900,
        },
        null
      )
    ).toBe(900);
  });

  it('returns a genuine zero (not null) when accumulated counters are zero', () => {
    const result = selectBilledTokens(
      { accumulatedInputTokens: 0, accumulatedOutputTokens: 0, accumulatedTotalTokens: 0 },
      null
    );
    expect(result).toBe(0);
  });

  it('does not treat store-coalesced zeroes as exact when persisted counters are unknown', () => {
    const result = selectBilledTokens(
      { accumulatedInputTokens: 0, accumulatedOutputTokens: 0, accumulatedTotalTokens: 0 },
      {
        accumulated_input_tokens: null,
        accumulated_output_tokens: null,
        accumulated_total_tokens: null,
      }
    );

    expect(result).toBeNull();
  });

  it('falls back to the persisted session accumulated_total_tokens when no TokenState', () => {
    // Resumed session: no live stream yet.
    const result = selectBilledTokens(null, {
      accumulated_total_tokens: 7_500_000,
      accumulated_input_tokens: 7_000_000,
      accumulated_output_tokens: 500_000,
    });
    expect(result).toBe(7_500_000);
  });

  it('falls back to the session accumulated input+output sum when total is missing', () => {
    const result = selectBilledTokens(null, {
      accumulated_total_tokens: null,
      accumulated_input_tokens: 3_000_000,
      accumulated_output_tokens: 300_000,
    });
    expect(result).toBe(3_300_000);
  });

  it('prefers the live TokenState over the persisted session row', () => {
    const result = selectBilledTokens(
      { accumulatedInputTokens: 100, accumulatedOutputTokens: 100, accumulatedTotalTokens: 200 },
      { accumulated_total_tokens: 999_999 }
    );
    expect(result).toBe(200);
  });

  it('returns null when neither source has any data', () => {
    expect(selectBilledTokens(null, null)).toBeNull();
    expect(selectBilledTokens({}, {})).toBeNull();
    expect(
      selectBilledTokens(
        {
          accumulatedInputTokens: null,
          accumulatedOutputTokens: null,
          accumulatedTotalTokens: null,
        },
        { accumulated_total_tokens: null }
      )
    ).toBeNull();
  });

  it('ignores non-finite values', () => {
    expect(
      selectBilledTokens(
        {
          accumulatedInputTokens: Number.NaN,
          accumulatedOutputTokens: Number.POSITIVE_INFINITY,
          accumulatedTotalTokens: 1234,
        },
        null
      )
    ).toBe(1234);
  });
});

describe('billedSessionTokens', () => {
  it('uses the accumulated billed figure, never last-turn total_tokens', () => {
    const result = billedSessionTokens({
      total_tokens: 218_000, // last-turn occupancy — must NOT win
      accumulated_total_tokens: 13_300_000,
    });
    expect(result).toBe(13_300_000);
  });

  it('shows the accumulated total for a total-only session with a zero input/output split', () => {
    const result = billedSessionTokens({
      total_tokens: 218_000,
      accumulated_input_tokens: 0,
      accumulated_output_tokens: 0,
      accumulated_total_tokens: 13_300_000,
    });
    expect(result).toBe(13_300_000);
  });

  it('falls back to last-turn total_tokens only when no accumulated data exists', () => {
    // Old session predating the accumulated backfill.
    const result = billedSessionTokens({
      total_tokens: 218_000,
      accumulated_total_tokens: null,
      accumulated_input_tokens: null,
      accumulated_output_tokens: null,
    });
    expect(result).toBe(218_000);
    expect(
      billedSessionTokenEstimate({
        total_tokens: 218_000,
        accumulated_total_tokens: null,
        accumulated_input_tokens: null,
        accumulated_output_tokens: null,
      })
    ).toEqual({ value: 218_000, lowerBound: true });
  });

  it('returns null when a session has no token data at all', () => {
    expect(billedSessionTokens({ total_tokens: null })).toBeNull();
    expect(billedSessionTokens(null)).toBeNull();
    expect(billedSessionTokens(undefined)).toBeNull();
  });

  it('returns a genuine zero when accumulated is zero even if last-turn is set', () => {
    const result = billedSessionTokens({
      total_tokens: 218_000,
      accumulated_total_tokens: 0,
    });
    expect(result).toBe(0);
  });

  it('formats a legacy fallback as a lower bound rather than an exact billed total', () => {
    const estimate = billedSessionTokenEstimate({ total_tokens: 218_000 })!;
    expect(formatBilledTokenEstimate(estimate)).toBe('≥218,000');
    expect(formatBilledTokenEstimate({ value: 13_300_000, lowerBound: false })).toBe('13,300,000');
  });
});
