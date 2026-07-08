import { describe, expect, it } from 'vitest';
import { formatTooltipMoney } from './CostTracker';

describe('formatTooltipMoney', () => {
  it('keeps tooltip costs compact', () => {
    expect(formatTooltipMoney(0)).toBe('$0.00');
    expect(formatTooltipMoney(0.0000042)).toBe('<$0.01');
    expect(formatTooltipMoney(12.345)).toBe('$12.35');
  });
});
