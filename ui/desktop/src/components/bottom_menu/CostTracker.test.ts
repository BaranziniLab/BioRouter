import { describe, expect, it } from 'vitest';
import {
  aggregateModelRowsCost,
  billedTokensSummary,
  formatCostEstimate,
  formatTooltipMoney,
} from './CostTracker';

describe('formatTooltipMoney', () => {
  it('keeps tooltip costs compact', () => {
    expect(formatTooltipMoney(0)).toBe('$0.00');
    expect(formatTooltipMoney(0.0000042)).toBe('<$0.01');
    expect(formatTooltipMoney(12.345)).toBe('$12.35');
    expect(formatTooltipMoney(null)).toBe('—');
    expect(formatTooltipMoney(Number.NaN)).toBe('—');
  });
});

describe('billedTokensSummary', () => {
  it('sums input+output into the billed total (Issue #1 accumulated figure)', () => {
    const summary = billedTokensSummary(12_000_000, 1_300_000);
    expect(summary).toContain('13,300,000 billed tokens');
    expect(summary).toContain('12,000,000 fresh in');
    expect(summary).toContain('1,300,000 out');
    expect(summary).toContain('accumulated across all turns');
  });

  it('handles zero tokens without producing a wrong total', () => {
    expect(billedTokensSummary(0, 0)).toContain('0 billed tokens');
  });

  it('includes cache reads and writes in the billed headline', () => {
    const summary = billedTokensSummary(100, 25, 300, 50);
    expect(summary).toContain('475 billed tokens');
    expect(summary).toContain('300 cache read');
    expect(summary).toContain('50 cache write');
  });

  it('labels incomplete history as a lower bound without turning unknown cache into zero', () => {
    const summary = billedTokensSummary(100, 20, null, 5, null);
    expect(summary).toContain('≥125 billed tokens');
    expect(summary).toContain('— cache read');
    expect(summary).not.toContain('0 cache read');
  });
});

describe('cost estimates', () => {
  it('labels mixed known and unknown rows as a lower-bound subtotal', () => {
    const estimate = aggregateModelRowsCost([
      {
        provider: 'openai',
        model: 'priced',
        inputTokens: 100,
        outputTokens: 20,
        cacheReadTokens: 0,
        cacheCreationTokens: 0,
        totalTokens: 120,
        turns: 1,
        totalCost: 1.25,
        costIsPartial: false,
      },
      {
        provider: 'unknown',
        model: 'unpriced',
        inputTokens: 50,
        outputTokens: 10,
        cacheReadTokens: 0,
        cacheCreationTokens: 0,
        totalTokens: 60,
        turns: 1,
        totalCost: null,
        costIsPartial: false,
      },
    ]);

    expect(estimate).toEqual({ amount: 1.25, partial: true });
    expect(formatCostEstimate(estimate)).toBe('≥$1.25');
  });

  it('keeps an entirely unknown total unavailable', () => {
    const estimate = aggregateModelRowsCost([
      {
        provider: undefined,
        model: undefined,
        inputTokens: 50,
        outputTokens: 10,
        cacheReadTokens: 5,
        cacheCreationTokens: 0,
        totalTokens: 65,
        turns: 1,
        totalCost: null,
        costIsPartial: false,
      },
    ]);

    expect(estimate).toEqual({ amount: null, partial: true });
    expect(formatCostEstimate(estimate)).toBe('—');
  });
});
