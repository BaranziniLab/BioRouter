import { describe, it, expect } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import { UsagePanel, formatCost, formatTokens, modelLabel } from './UsagePanel';
import type { UsageReportRow, UsageSummaryResponse } from '../../../api';

const dayRows: UsageReportRow[] = [
  {
    date: '2026-07-10',
    modelId: null,
    provider: null,
    inputTokens: 1_000_100,
    outputTokens: 0,
    totalTokens: 1_000_100,
    turns: 7,
    cost: 1.4,
    hasUnpriced: true,
  },
  {
    date: '2026-07-11',
    modelId: null,
    provider: null,
    inputTokens: 2_000_000,
    outputTokens: 1_000_000,
    totalTokens: 3_000_000,
    turns: 3,
    cost: 7.2,
    hasUnpriced: false,
  },
];

const modelRows: UsageReportRow[] = [
  {
    date: null,
    modelId: 'glm-5.2',
    provider: 'zai',
    inputTokens: 3_000_000,
    outputTokens: 1_000_000,
    totalTokens: 4_000_000,
    turns: 8,
    cost: 8.6,
    hasUnpriced: false,
  },
  {
    date: null,
    modelId: null,
    provider: null,
    inputTokens: 500,
    outputTokens: 500,
    totalTokens: 1_000,
    turns: 2,
    cost: null,
    hasUnpriced: true,
  },
];

function summary(overrides: Partial<UsageSummaryResponse> = {}): UsageSummaryResponse {
  return {
    month: '2026-07',
    monthToDate: {
      inputTokens: 33_000_000,
      outputTokens: 0,
      totalTokens: 33_000_000,
      turns: 42,
      cost: 125,
      hasUnpriced: false,
    },
    allTime: {
      inputTokens: 66_000_000,
      outputTokens: 0,
      totalTokens: 66_000_000,
      turns: 84,
      cost: 250,
      hasUnpriced: false,
    },
    monthlyTokenLimit: null,
    monthlyDollarLimit: null,
    tokenPercent: null,
    dollarPercent: null,
    ...overrides,
  };
}

describe('formatters', () => {
  it('formats tokens with thousands separators', () => {
    expect(formatTokens(13_300_000)).toBe('13,300,000');
    expect(formatTokens(0)).toBe('0');
  });

  it('formats cost, folding sub-cent and null', () => {
    expect(formatCost(5)).toBe('$5.00');
    expect(formatCost(0.004)).toBe('<$0.01');
    expect(formatCost(null)).toBe('—');
    expect(formatCost(undefined)).toBe('—');
  });

  it('labels models and folds the unknown bucket', () => {
    expect(modelLabel({ modelId: 'glm-5.2', provider: 'zai' })).toBe('zai/glm-5.2');
    expect(modelLabel({ modelId: 'glm-5.2', provider: null })).toBe('glm-5.2');
    expect(modelLabel({ modelId: null, provider: null })).toBe('unknown');
  });
});

describe('UsagePanel', () => {
  it('renders day bars, the model table and MTD figures', () => {
    render(<UsagePanel summary={summary()} dayRows={dayRows} modelRows={modelRows} />);

    // Day bars: one per day.
    const bars = screen.getByTestId('usage-day-bars');
    expect(within(bars).getAllByTestId('usage-day-bar-fill')).toHaveLength(2);
    expect(within(bars).getByText('2026-07-10')).toBeTruthy();
    expect(within(bars).getByText('1,000,100')).toBeTruthy();
    expect(within(bars).getByText('$7.20')).toBeTruthy();

    // Model table: glm row with hand-checked cells + unknown null-cost row.
    const table = screen.getByTestId('usage-model-table');
    const glmRow = within(table).getByText('zai/glm-5.2').closest('tr')!;
    expect(
      within(glmRow)
        .getAllByRole('cell')
        .map((c) => c.textContent)
    ).toEqual(['zai/glm-5.2', '8', '3,000,000', '1,000,000', '$8.60']);
    const unknownRow = within(table).getByText('unknown').closest('tr')!;
    // Unknown model shows an em-dash cost, never $0.
    expect(within(unknownRow).getAllByRole('cell')[4].textContent).toBe('—');
  });

  it('shows the unpriced note when any row lacks pricing', () => {
    render(<UsagePanel summary={summary()} dayRows={dayRows} modelRows={modelRows} />);
    expect(screen.getByTestId('usage-unpriced-note')).toBeTruthy();
  });

  it('hides the unpriced note when everything is priced', () => {
    const pricedDays = dayRows.map((r) => ({ ...r, hasUnpriced: false }));
    const pricedModels = [modelRows[0]];
    render(<UsagePanel summary={summary()} dayRows={pricedDays} modelRows={pricedModels} />);
    expect(screen.queryByTestId('usage-unpriced-note')).toBeNull();
  });

  it('hides both gauges when no limit is configured', () => {
    render(<UsagePanel summary={summary()} dayRows={dayRows} modelRows={modelRows} />);
    expect(screen.queryByTestId('usage-gauge-tokens')).toBeNull();
    expect(screen.queryByTestId('usage-gauge-dollars')).toBeNull();
  });

  it('renders the token + dollar gauges with percents when limits are set', () => {
    render(
      <UsagePanel
        summary={summary({
          monthlyTokenLimit: 66_000_000,
          monthlyDollarLimit: 250,
          tokenPercent: 50,
          dollarPercent: 50,
        })}
        dayRows={dayRows}
        modelRows={modelRows}
      />
    );
    const tokenGauge = screen.getByTestId('usage-gauge-tokens');
    // 33M / 66M = 50%.
    expect(within(tokenGauge).getByText('(50.0%)')).toBeTruthy();
    expect(within(tokenGauge).getByText(/33,000,000 \/ 66,000,000/)).toBeTruthy();
    // Fill width tracks the percent.
    expect(screen.getByTestId('usage-gauge-tokens-fill').style.width).toBe('50%');

    const dollarGauge = screen.getByTestId('usage-gauge-dollars');
    expect(within(dollarGauge).getByText('(50.0%)')).toBeTruthy();
  });

  it('marks the dollar gauge unavailable when MTD cost is unknown', () => {
    render(
      <UsagePanel
        summary={summary({
          monthToDate: {
            inputTokens: 500,
            outputTokens: 0,
            totalTokens: 500,
            turns: 1,
            cost: null,
            hasUnpriced: true,
          },
          monthlyDollarLimit: 100,
          dollarPercent: null,
        })}
        dayRows={[]}
        modelRows={[]}
      />
    );
    const dollarGauge = screen.getByTestId('usage-gauge-dollars');
    expect(within(dollarGauge).getByText('(cost unavailable)')).toBeTruthy();
    // No percent fabricated from a missing cost.
    expect(screen.getByTestId('usage-gauge-dollars-fill').style.width).toBe('0%');
  });

  it('renders empty-state copy when a range has no usage', () => {
    render(<UsagePanel summary={summary()} dayRows={[]} modelRows={[]} />);
    expect(screen.getAllByText('No usage in this range.').length).toBe(2);
  });
});
