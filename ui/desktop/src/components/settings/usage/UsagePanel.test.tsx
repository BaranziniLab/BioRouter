import { describe, it, expect } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {
  UsagePanel,
  fillCalendarDays,
  formatBilledTokens,
  formatCost,
  formatCostEstimate,
  formatTokens,
  formatUsageDate,
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
    expect(formatTokens(null)).toBe('Not recorded');
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
    ).toBe('125');
    expect(
      formatBilledTokens({
        ...summary().monthToDate,
        inputTokens: 0,
        outputTokens: 0,
        cacheReadTokens: null,
        cacheCreationTokens: null,
        totalTokens: null,
      })
    ).toBe('Unavailable');
  });

  it('formats cost, folding sub-cent and null', () => {
    expect(formatCost(5)).toBe('$5.00');
    expect(formatCost(0.004)).toBe('<$0.01');
    expect(formatCost(null)).toBe('Unavailable');
    expect(formatCost(undefined)).toBe('Unavailable');
    expect(formatCostEstimate(5)).toBe('$5.00');
    expect(formatCostEstimate(null)).toBe('Unavailable');
  });

  it('formats ledger dates for quick scanning while preserving invalid input', () => {
    expect(formatUsageDate('2026-07-14')).toBe('Tue, Jul 14');
    expect(formatUsageDate('legacy')).toBe('legacy');
    expect(formatUsageDate(null)).toBe('Unknown date');
  });

  it('labels models and folds the unknown bucket', () => {
    expect(modelLabel({ modelId: 'glm-5.2', provider: 'zai' })).toBe('zai/glm-5.2');
    expect(modelLabel({ modelId: 'glm-5.2', provider: null })).toBe('glm-5.2');
    expect(modelLabel({ modelId: null, provider: null })).toBe('unknown');
  });
});

describe('UsagePanel', () => {
  it('renders a reverse-chronological day table, the model table and MTD figures', () => {
    render(<UsagePanel summary={summary()} dayRows={dayRows} modelRows={modelRows} />);

    const dayTable = screen.getByTestId('usage-day-table');
    const dayTableRows = within(dayTable).getAllByRole('row');
    expect(dayTableRows).toHaveLength(3);
    expect(
      within(dayTableRows[1])
        .getAllByRole('cell')
        .map((cell) => cell.textContent)
    ).toEqual(['Sat, Jul 11', '3', '3,000,000', '$7.20']);
    expect(
      within(dayTableRows[2])
        .getAllByRole('cell')
        .map((cell) => cell.textContent)
    ).toEqual(['Fri, Jul 10', '7', '1,000,100', '$1.40']);

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
    // Unknown pricing is explicit and is never presented as $0.
    expect(within(unknownRow).getAllByRole('cell')[5].textContent).toBe('Unavailable');
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

  it('uses the same quiet table grammar for daily and model usage', () => {
    render(<UsagePanel summary={summary()} dayRows={dayRows} modelRows={modelRows} />);

    expect(screen.queryByTestId('usage-day-bar-fill')).toBeNull();
    const dayTable = screen.getByTestId('usage-day-table');
    const modelTable = screen.getByTestId('usage-model-table');
    expect(screen.getByText('By day').parentElement).toHaveClass('max-w-[680px]', 'w-full');
    expect(screen.getByTestId('usage-day-table-wrap')).toHaveClass('max-w-[680px]', 'w-full');
    expect(screen.getByTestId('usage-model-table-wrap')).toHaveClass('max-w-[680px]', 'w-full');
    expect(dayTable).toHaveClass('table-fixed', 'min-w-[640px]', 'border-collapse');
    expect(modelTable).toHaveClass('table-fixed', 'min-w-[640px]', 'border-collapse');
    const columnGeometry = (table: HTMLElement) =>
      Array.from(table.querySelectorAll('col')).map((column) => ({
        className: column.getAttribute('class'),
        span: column.getAttribute('span'),
      }));
    expect(columnGeometry(dayTable)).toEqual(columnGeometry(modelTable));
    expect(within(dayTable).getAllByRole('row')[0]).toHaveClass(
      'h-8',
      'border-b',
      'border-border-subtle'
    );
    expect(within(dayTable).getAllByRole('row')[1]).toHaveClass(
      'h-10',
      'border-b',
      'border-border-subtle'
    );
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

  it('renders empty-state copy when the current month has no usage', () => {
    render(<UsagePanel summary={summary()} dayRows={[]} modelRows={[]} />);
    expect(screen.getAllByText('No usage this month.').length).toBe(2);
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
      '$0.01',
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
    expect(within(gauge).getByText(/\$12.00 \/ \$100.00/)).toBeTruthy();
    expect(
      within(gauge).getByText(
        'Budget percentage unavailable because the known cost is only a partial subtotal.'
      )
    ).toBeTruthy();
    expect(screen.queryByTestId('usage-gauge-dollars-fill')).toBeNull();
  });

  it('shows incomplete billed history as a conservative estimate and leaves the gauge unavailable', () => {
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
    expect(within(gauge).getByText(/1,800 \/ 1,800/)).toBeTruthy();
    expect(
      within(gauge).getByText(
        'Budget percentage unavailable because billed token history is incomplete.'
      )
    ).toBeTruthy();
    expect(screen.queryByTestId('usage-gauge-tokens-fill')).toBeNull();
    expect(screen.getByTestId('usage-incomplete-note')).toBeTruthy();
  });

  it('labels an unrecorded cache bucket and shows conservative token and cost estimates', () => {
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
    ).toEqual(['anthropic/legacy-model', '2', '100', 'Not recorded', '5', '20', '125', '$1.25']);
    expect(screen.getByTestId('usage-incomplete-note')).toBeTruthy();
  });

  it('shows three adjacent calendar days newest-first until the user expands the month', async () => {
    const user = userEvent.setup();
    const sparseRows = [7, 11, 14].map((day, index): UsageReportRow => {
      const tokens = (index + 1) * 100;
      return {
        ...dayRows[1],
        date: `2026-07-${String(day).padStart(2, '0')}`,
        inputTokens: tokens,
        outputTokens: 0,
        totalTokens: tokens,
      };
    });
    const calendarRows = fillCalendarDays(sparseRows, new Date(2026, 6, 14, 12));

    render(<UsagePanel summary={summary()} dayRows={calendarRows} modelRows={modelRows} />);

    const table = screen.getByTestId('usage-day-table');
    const visibleDates = Array.from(table.querySelectorAll('time')).map((time) =>
      time.getAttribute('datetime')
    );
    expect(visibleDates).toEqual(['2026-07-14', '2026-07-13', '2026-07-12']);
    const emptyDay = within(table).getByText('Mon, Jul 13').closest('tr')!;
    expect(
      within(emptyDay)
        .getAllByRole('cell')
        .slice(1)
        .map((cell) => cell.textContent)
    ).toEqual(['0', '0', '$0.00']);

    const expand = screen.getByRole('button', {
      name: 'Show all 14 calendar days this month',
    });
    expect(expand).toHaveAttribute('aria-expanded', 'false');
    await user.click(expand);

    expect(within(table).getAllByRole('row')).toHaveLength(15);
    expect(table.querySelector('time[datetime="2026-07-07"]')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Show recent 3 calendar days' })).toHaveAttribute(
      'aria-expanded',
      'true'
    );
  });

  it('hides empty cache and cost columns instead of repeating unavailable values', () => {
    const unrecordedDay: UsageReportRow = {
      ...dayRows[0],
      cacheReadTokens: null,
      cacheCreationTokens: null,
      cost: null,
    };
    const unrecordedModel: UsageReportRow = {
      ...modelRows[1],
      cacheReadTokens: null,
      cacheCreationTokens: null,
      cost: null,
    };

    render(
      <UsagePanel
        summary={summary({
          monthToDate: {
            ...summary().monthToDate,
            cacheReadTokens: null,
            cacheCreationTokens: null,
            cost: null,
            hasUnpriced: true,
          },
        })}
        dayRows={[unrecordedDay]}
        modelRows={[unrecordedModel]}
      />
    );

    for (const table of [
      screen.getByTestId('usage-day-table'),
      screen.getByTestId('usage-model-table'),
    ]) {
      expect(within(table).queryByRole('columnheader', { name: 'Cache read' })).toBeNull();
      expect(within(table).queryByRole('columnheader', { name: 'Cache write' })).toBeNull();
      expect(within(table).queryByRole('columnheader', { name: 'Cost' })).toBeNull();
    }
    expect(screen.queryByTestId('usage-mtd-cache')).toBeNull();
    expect(screen.getByTestId('usage-panel').textContent).not.toContain('N/A');
  });

  it('keeps internal labels subordinate to the settings section title', () => {
    render(<UsagePanel summary={summary()} dayRows={dayRows} modelRows={modelRows} />);

    for (const label of ['Month to date', 'By day', 'By model']) {
      expect(screen.getByText(label)).toHaveClass('text-[11px]', 'font-normal', 'text-text-subtle');
      expect(screen.getByText(label)).not.toHaveClass('text-sm', 'font-medium');
    }
  });
});
