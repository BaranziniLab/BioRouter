import { describe, expect, it } from 'vitest';
import { formatTooltipMoney, billedTokensSummary } from './CostTracker';

describe('formatTooltipMoney', () => {
  it('keeps tooltip costs compact', () => {
    expect(formatTooltipMoney(0)).toBe('$0.00');
    expect(formatTooltipMoney(0.0000042)).toBe('<$0.01');
    expect(formatTooltipMoney(12.345)).toBe('$12.35');
  });
});

describe('billedTokensSummary', () => {
  it('sums input+output into the billed total (Issue #1 accumulated figure)', () => {
    const summary = billedTokensSummary(12_000_000, 1_300_000);
    expect(summary).toContain('13,300,000 billed tokens');
    expect(summary).toContain('12,000,000 in');
    expect(summary).toContain('1,300,000 out');
    expect(summary).toContain('accumulated across all turns');
  });

  it('handles zero tokens without producing a wrong total', () => {
    expect(billedTokensSummary(0, 0)).toContain('0 billed tokens');
  });
});
