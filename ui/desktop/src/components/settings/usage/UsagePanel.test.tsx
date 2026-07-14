import { describe, it, expect } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import {
  UsagePanel,
  formatBilledTokens,
  formatCost,
  formatCostEstimate,
  formatTokens,
  modelLabel,
} from './UsagePanel';
import type { UsageReportRow, UsageSummaryResponse } from '../../../api';

const dayRows: UsageReportRow[] = [
  {
    date: '2026-07-10',
    modelId: null,
    provider: null,
    inputTokens: 1_000_100,
    outputTokens: 0,
    totalTokens: 1_000_100,
    cacheReadTokens: 0,
    cacheCreationTokens: 0,
    turns: 7,
    cost: 1.4,
    hasUnpriced: true,
    costExcludesCache: false,
  },
  {
    date: '2026-07-11',
    modelId: null,
    provider: null,
    inputTokens: 2_000_000,
    outputTokens: 1_000_000,
    totalTokens: 3_000_000,
    cacheReadTokens: 0,
    cacheCreationTokens: 0,
    turns: 3,
    cost: 7.2,
    hasUnpriced: false,
    costExcludesCache: false,
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
    cacheReadTokens: 0,
    cacheCreationTokens: 0,
    turns: 8,
    cost: 8.6,
    hasUnpriced: false,
    costExcludesCache: false,
  },
  {
    date: null,
    modelId: null,
    provider: null,
    inputTokens: 500,
    outputTokens: 500,
    totalTokens: 1_000,
    cacheReadTokens: 0,
    cacheCreationTokens: 0,
    turns: 2,
    cost: null,
    hasUnpriced: true,
    costExcludesCache: false,
  },
];

function summary(overrides: Partial<UsageSummaryResponse> = {}): UsageSummaryResponse {
  return {
    month: '2026-07',
    monthToDate: {
      inputTokens: 33_000_000,
      outputTokens: 0,
      totalTokens: 33_000_000,
      cacheReadTokens: 0,
      cacheCreationTokens: 0,
      turns: 42,
      cost: 125,
      hasUnpriced: false,
      costExcludesCache: false,
    },
    allTime: {
      inputTokens: 66_000_000,
      outputTokens: 0,
      totalTokens: 66_000_000,
      cacheReadTokens: 0,
      cacheCreationTokens: 0,
      turns: 84,
      cost: 250,
      hasUnpriced: false,
      costExcludesCache: false,
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
    expect(formatTokens(null)).toBe('—');
  });

  it('distinguishes exact totals, known subtotals, and wholly unknown totals', () => {
    expect(formatBilledTokens(summary().monthToDate)).toBe('33,000,000');
    expect(
      formatBilledTokens({
        ...summary().monthToDate,
        inputTokens: 100,
        outputTokens: 20,
        cacheReadTokens: null,
        cacheCreationTokens: 5,
        totalTokens: null,
      })
    ).toBe('≥125');
    expect(
      formatBilledTokens({
        ...summary().monthToDate,
        inputTokens: 0,
        outputTokens: 0,
        cacheReadTokens: null,
        cacheCreationTokens: null,
        totalTokens: null,
      })
    ).toBe('—');
  });

  it('formats cost, folding sub-cent and null', () => {
    expect(formatCost(5)).toBe('$5.00');
    expect(formatCost(0.004)).toBe('<$0.01');
    expect(formatCost(null)).toBe('—');
    expect(formatCost(undefined)).toBe('—');
    expect(formatCostEstimate(5, true)).toBe('≥$5.00');
    expect(formatCostEstimate(null, true)).toBe('—');
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
    expect(within(bars).getByText('1,000,100 billed')).toBeTruthy();
    expect(within(bars).getByText('≥$1.40')).toBeTruthy();
    expect(within(bars).getByText('$7.20')).toBeTruthy();

    // Model table: glm row with hand-checked cells + unknown null-cost row.
    const table = screen.getByTestId('usage-model-table');
    const glmRow = within(table).getByText('zai/glm-5.2').closest('tr')!;
    expect(table).toHaveClass('border-collapse');
    expect(within(table).getAllByRole('row')[0]).toHaveClass(
      'h-8',
      'border-b',
      'border-border-subtle'
    );
    expect(glmRow).toHaveClass('h-10', 'border-b', 'border-border-subtle');
    expect(within(glmRow).getAllByRole('cell')[0]).toHaveClass('font-mono');
    for (const cell of within(glmRow).getAllByRole('cell').slice(1)) {
      expect(cell).toHaveClass('tabular-nums');
    }
    expect(
      within(glmRow)
        .getAllByRole('cell')
        .map((c) => c.textContent)
    ).toEqual(['zai/glm-5.2', '8', '3,000,000', '1,000,000', '4,000,000', '$8.60']);
    const unknownRow = within(table).getByText('unknown').closest('tr')!;
    // Unknown model shows an em-dash cost, never $0.
    expect(within(unknownRow).getAllByRole('cell')[5].textContent).toBe('—');
  });

  it('shows the unpriced note when any row lacks pricing', () => {
    render(<UsagePanel summary={summary()} dayRows={dayRows} modelRows={modelRows} />);
    expect(screen.getByTestId('usage-unpriced-note')).toBeTruthy();
  });

  it('shows the unpriced note when a used row has null cost even if its flag is stale', () => {
    render(
      <UsagePanel
        summary={summary({
          monthToDate: {
            ...summary().monthToDate,
            cost: null,
            hasUnpriced: false,
          },
        })}
        dayRows={[]}
        modelRows={[]}
      />
    );

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

  it('renders the server-provided token and dollar percentages without recomputing them', () => {
    render(
      <UsagePanel
        summary={summary({
          monthlyTokenLimit: 66_000_000,
          monthlyDollarLimit: 250,
          tokenPercent: 37.5,
          dollarPercent: 41.25,
        })}
        dayRows={dayRows}
        modelRows={modelRows}
      />
    );
    const tokenGauge = screen.getByTestId('usage-gauge-tokens');
    expect(within(tokenGauge).getByText('(37.5%)')).toBeTruthy();
    expect(within(tokenGauge).getByText(/33,000,000 \/ 66,000,000/)).toBeTruthy();
    // Fill width tracks the percent.
    const tokenFill = screen.getByTestId('usage-gauge-tokens-fill');
    expect(tokenFill.style.width).toBe('37.5%');
    expect(tokenFill).toHaveClass('bg-heat-3');

    const dollarGauge = screen.getByTestId('usage-gauge-dollars');
    expect(within(dollarGauge).getByText('(41.3%)')).toBeTruthy();
  });

  it('uses semantic danger styling and clamps an over-budget gauge', () => {
    render(
      <UsagePanel
        summary={summary({
          monthlyTokenLimit: 30_000_000,
          tokenPercent: 110,
        })}
        dayRows={dayRows}
        modelRows={modelRows}
      />
    );

    const gauge = screen.getByTestId('usage-gauge-tokens');
    expect(within(gauge).getByText('(110.0%)')).toHaveClass('text-text-danger');
    const fill = screen.getByTestId('usage-gauge-tokens-fill');
    expect(fill).toHaveClass('bg-background-danger');
    expect(fill.style.width).toBe('100%');
  });

  it('uses the usage heat ramp for day bars', () => {
    render(<UsagePanel summary={summary()} dayRows={dayRows} modelRows={modelRows} />);

    for (const fill of screen.getAllByTestId('usage-day-bar-fill')) {
      expect(fill).toHaveClass('bg-heat-3');
      expect(fill.parentElement).toHaveClass('bg-heat-0');
    }
  });

  it('marks the dollar gauge unavailable when MTD cost is unknown', () => {
    render(
      <UsagePanel
        summary={summary({
          monthToDate: {
            inputTokens: 500,
            outputTokens: 0,
            totalTokens: 500,
            cacheReadTokens: 0,
            cacheCreationTokens: 0,
            turns: 1,
            cost: null,
            hasUnpriced: true,
            costExcludesCache: false,
          },
          monthlyDollarLimit: 100,
          dollarPercent: null,
        })}
        dayRows={[]}
        modelRows={[]}
      />
    );
    const dollarGauge = screen.getByTestId('usage-gauge-dollars');
    expect(
      within(dollarGauge).getByText('Budget percentage unavailable because cost is unknown.')
    ).toBeTruthy();
    expect(screen.queryByTestId('usage-gauge-dollars-fill')).toBeNull();
  });

  it('renders empty-state copy when a range has no usage', () => {
    render(<UsagePanel summary={summary()} dayRows={[]} modelRows={[]} />);
    expect(screen.getAllByText('No usage in this range.').length).toBe(2);
  });

  it('shows a Cache column, MTD cached figure, and the excluded-cost note when cache is present', () => {
    const cachedModels: UsageReportRow[] = [
      {
        date: null,
        modelId: 'claude-sonnet-4',
        provider: 'anthropic',
        inputTokens: 1_000,
        outputTokens: 200,
        totalTokens: 1_800,
        cacheReadTokens: 500,
        cacheCreationTokens: 100,
        turns: 1,
        cost: 0.01,
        hasUnpriced: false,
        costExcludesCache: true,
      },
    ];
    render(
      <UsagePanel
        summary={summary({
          monthToDate: {
            inputTokens: 1_000,
            outputTokens: 200,
            totalTokens: 1_800,
            cacheReadTokens: 500,
            cacheCreationTokens: 100,
            turns: 1,
            cost: 0.01,
            hasUnpriced: false,
            costExcludesCache: true,
          },
        })}
        dayRows={[]}
        modelRows={cachedModels}
      />
    );
    expect(within(screen.getByTestId('usage-mtd-cache')).getByText(/500 cache read/)).toBeTruthy();
    expect(within(screen.getByTestId('usage-mtd-cache')).getByText(/100 cache write/)).toBeTruthy();
    // Model table exposes both cache buckets and the cache-aware billed total.
    const table = screen.getByTestId('usage-model-table');
    const row = within(table).getByText('anthropic/claude-sonnet-4').closest('tr')!;
    const cells = within(row)
      .getAllByRole('cell')
      .map((c) => c.textContent);
    expect(cells).toEqual([
      'anthropic/claude-sonnet-4',
      '1',
      '1,000',
      '500',
      '100',
      '200',
      '1,800',
      '≥$0.01',
    ]);
    // The cost-excludes-cache note appears.
    expect(screen.getByTestId('usage-cache-excluded-note')).toBeTruthy();
  });

  it('omits the Cache column when no row has cache tokens', () => {
    render(<UsagePanel summary={summary()} dayRows={dayRows} modelRows={modelRows} />);
    const table = screen.getByTestId('usage-model-table');
    // No Cache header cell.
    expect(within(table).queryByText('Cache')).toBeNull();
    expect(screen.queryByTestId('usage-mtd-cache')).toBeNull();
    expect(screen.queryByTestId('usage-cache-excluded-note')).toBeNull();
  });

  it('does not draw a dollar gauge for a known but partial subtotal', () => {
    render(
      <UsagePanel
        summary={summary({
          monthToDate: {
            ...summary().monthToDate,
            cost: 12,
            hasUnpriced: true,
          },
          monthlyDollarLimit: 100,
          dollarPercent: null,
        })}
        dayRows={[]}
        modelRows={[]}
      />
    );

    const gauge = screen.getByTestId('usage-gauge-dollars');
    expect(within(gauge).getByText(/≥\$12.00 \/ \$100.00/)).toBeTruthy();
    expect(
      within(gauge).getByText(
        'Budget percentage unavailable because the known cost is only a partial subtotal.'
      )
    ).toBeTruthy();
    expect(screen.queryByTestId('usage-gauge-dollars-fill')).toBeNull();
  });

  it('shows incomplete billed history as a lower bound and leaves the gauge unavailable', () => {
    render(
      <UsagePanel
        summary={summary({
          monthToDate: {
            ...summary().monthToDate,
            inputTokens: 1_000,
            outputTokens: 200,
            cacheReadTokens: 500,
            cacheCreationTokens: 100,
            totalTokens: null,
          },
          monthlyTokenLimit: 1_800,
          tokenPercent: null,
        })}
        dayRows={[]}
        modelRows={[]}
      />
    );

    const gauge = screen.getByTestId('usage-gauge-tokens');
    expect(within(gauge).getByText(/≥1,800 \/ 1,800/)).toBeTruthy();
    expect(
      within(gauge).getByText(
        'Budget percentage unavailable because billed token history is incomplete.'
      )
    ).toBeTruthy();
    expect(screen.queryByTestId('usage-gauge-tokens-fill')).toBeNull();
    expect(screen.getByTestId('usage-incomplete-note')).toBeTruthy();
  });

  it('renders nullable model cache buckets as em dashes and billed tokens as a lower bound', () => {
    const incompleteModel: UsageReportRow = {
      date: null,
      modelId: 'legacy-model',
      provider: 'anthropic',
      inputTokens: 100,
      outputTokens: 20,
      totalTokens: null,
      cacheReadTokens: null,
      cacheCreationTokens: 5,
      turns: 2,
      cost: 1.25,
      hasUnpriced: true,
      costExcludesCache: true,
    };

    render(<UsagePanel summary={summary()} dayRows={[]} modelRows={[incompleteModel]} />);

    const row = screen.getByText('anthropic/legacy-model').closest('tr')!;
    expect(
      within(row)
        .getAllByRole('cell')
        .map((cell) => cell.textContent)
    ).toEqual(['anthropic/legacy-model', '2', '100', '—', '5', '20', '≥125', '≥$1.25']);
    expect(screen.getByTestId('usage-incomplete-note')).toBeTruthy();
  });
});
