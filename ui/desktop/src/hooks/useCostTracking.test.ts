import { describe, expect, it, vi } from 'vitest';
import type { ModelUsageRow } from '../api';
import { buildLegacySessionCosts, buildModelCostRows } from './useCostTracking';

const row = (overrides: Partial<ModelUsageRow> = {}): ModelUsageRow => ({
  provider: 'anthropic',
  modelId: 'claude-test',
  inputTokens: 100,
  outputTokens: 20,
  cacheReadTokens: 300,
  cacheCreationTokens: 50,
  totalTokens: 470,
  turns: 2,
  ...overrides,
});

describe('buildModelCostRows', () => {
  it('preserves cache buckets and prices every available rate', async () => {
    const rows = await buildModelCostRows(
      [row()],
      vi.fn().mockResolvedValue({
        input_token_cost: 0.01,
        output_token_cost: 0.02,
        cache_read_cost: 0.001,
        cache_write_cost: 0.0125,
      })
    );

    expect(rows[0]).toMatchObject({
      cacheReadTokens: 300,
      cacheCreationTokens: 50,
      totalTokens: 470,
      totalCost: 2.325,
      costIsPartial: false,
    });
  });

  it('labels a fresh-token subtotal partial when cache rates are unavailable', async () => {
    const rows = await buildModelCostRows(
      [row()],
      vi.fn().mockResolvedValue({ input_token_cost: 0.01, output_token_cost: 0.02 })
    );

    expect(rows[0].totalCost).toBe(1.4);
    expect(rows[0].costIsPartial).toBe(true);
  });

  it('keeps wholly unknown pricing null instead of coercing it to zero', async () => {
    const rows = await buildModelCostRows([row()], vi.fn().mockResolvedValue(null));

    expect(rows[0].totalCost).toBeNull();
    expect(rows[0].costIsPartial).toBe(false);
  });

  it('recovers cache tokens when an older total omitted them', async () => {
    const rows = await buildModelCostRows(
      [row({ totalTokens: 120 })],
      vi.fn().mockResolvedValue({ input_token_cost: 0.01, output_token_cost: 0.02 })
    );

    expect(rows[0].totalTokens).toBe(470);
  });

  it('never converts unknown or partial rows into exact legacy costs', () => {
    expect(
      buildLegacySessionCosts([
        {
          provider: 'openai',
          model: 'known',
          inputTokens: 10,
          outputTokens: 2,
          cacheReadTokens: 0,
          cacheCreationTokens: 0,
          totalTokens: 12,
          turns: 1,
          totalCost: 1,
          costIsPartial: false,
        },
        {
          provider: 'anthropic',
          model: 'partial',
          inputTokens: 10,
          outputTokens: 2,
          cacheReadTokens: 20,
          cacheCreationTokens: 0,
          totalTokens: 32,
          turns: 1,
          totalCost: 2,
          costIsPartial: true,
        },
        {
          provider: undefined,
          model: undefined,
          inputTokens: 10,
          outputTokens: 2,
          cacheReadTokens: 0,
          cacheCreationTokens: 0,
          totalTokens: 12,
          turns: 1,
          totalCost: null,
          costIsPartial: false,
        },
      ])
    ).toEqual({
      'openai/known': { inputTokens: 10, outputTokens: 2, totalCost: 1 },
    });
  });
});
