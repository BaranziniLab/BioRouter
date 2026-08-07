import { describe, it, expect } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {
  UsagePanel,
  UsageReport,
  fillCalendarDays,
  formatBilledTokens,
  formatCompactTokens,
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
    expect(formatCompactTokens(9_999)).toBe('9,999');
    expect(formatCompactTokens(12_345)).toBe('12.35K');
    expect(formatCompactTokens(3_000_000)).toBe('3M');
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

describe('UsageReport', () => {
  it('renders a reverse-chronological day table, the model table and MTD figures', () => {
    render(<UsageReport summary={summary()} dayRows={dayRows} modelRows={modelRows} />);

    const dayTable = screen.getByTestId('usage-day-table');
    const dayTableRows = within(dayTable).getAllByRole('row');
    expect(dayTableRows).toHaveLength(3);
    expect(
      within(dayTableRows[1])
        .getAllByRole('cell')
        .map((cell) => cell.textContent)
    ).toEqual(['Sat, Jul 11', '3', '3M', '$7.20']);
    expect(
      within(dayTableRows[2])
        .getAllByRole('cell')
        .map((cell) => cell.textContent)
    ).toEqual(['Fri, Jul 10', '7', '1M', '$1.40']);

    // Model identity and token details are grouped instead of compressed into
    // a long run of narrow numeric columns.
    const table = screen.getByTestId('usage-model-table');
    const glmRow = within(table).getByText('glm-5.2').closest('tr')!;
    expect(table).toHaveClass('border-collapse');
    // `text-caps` IS the caps style — it carries the transform as well as the
    // 11/500/+0.08em metrics, so asserting the role covers the casing.
    expect(within(glmRow).getByText('zai')).toHaveClass('text-caps');
    const glmCells = within(glmRow).getAllByRole('cell');
    expect(glmCells).toHaveLength(5);
    expect(glmCells[1]).toHaveTextContent('8');
    expect(glmCells[2]).toHaveTextContent('Fresh in3MOut1M');
    expect(glmCells[3]).toHaveTextContent('4M');
    expect(glmCells[4]).toHaveTextContent('$8.60');
    const unknownRow = within(table).getByText('unknown').closest('tr')!;
    // Unknown pricing is explicit and is never presented as $0.
    expect(within(unknownRow).getAllByRole('cell')[4].textContent).toBe('Unavailable');
  });

  it('shows the unpriced note when any row lacks pricing', () => {
    render(<UsageReport summary={summary()} dayRows={dayRows} modelRows={modelRows} />);
    expect(screen.getByTestId('usage-unpriced-note')).toBeTruthy();
  });

  it('shows the unpriced note when a used row has null cost even if its flag is stale', () => {
    render(
      <UsageReport
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
    render(<UsageReport summary={summary()} dayRows={pricedDays} modelRows={pricedModels} />);
    expect(screen.queryByTestId('usage-unpriced-note')).toBeNull();
  });

  it('hides both gauges when no limit is configured', () => {
    render(<UsageReport summary={summary()} dayRows={dayRows} modelRows={modelRows} />);
    expect(screen.queryByTestId('usage-gauge-tokens')).toBeNull();
    expect(screen.queryByTestId('usage-gauge-dollars')).toBeNull();
  });

  it('renders the server-provided token and dollar percentages without recomputing them', () => {
    render(
      <UsageReport
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
      <UsageReport
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

  it('uses a simple daily ledger and a grouped model token-flow table', () => {
    render(<UsageReport summary={summary()} dayRows={dayRows} modelRows={modelRows} />);

    const dayTable = screen.getByTestId('usage-day-table');
    const modelTable = screen.getByTestId('usage-model-table');
    expect(screen.getByTestId('usage-day-table-wrap')).toHaveClass('w-full', 'overflow-x-auto');
    expect(screen.getByTestId('usage-model-table-wrap')).toHaveClass('w-full', 'overflow-x-auto');
    expect(dayTable).toHaveClass('min-w-[520px]', 'border-collapse');
    expect(modelTable).toHaveClass('min-w-[720px]', 'border-collapse');
    expect(within(dayTable).queryByRole('columnheader', { name: 'Activity' })).toBeNull();
    expect(within(dayTable).queryByRole('progressbar')).toBeNull();
    expect(within(modelTable).getByRole('columnheader', { name: 'Token flow' })).toBeTruthy();
    expect(screen.getAllByTestId('usage-model-token-flow')).toHaveLength(2);
    // A section card is a CONTAINER, named by its role on the radius ladder
    // rather than by the deprecated size alias that happens to render 12px.
    expect(dayTable.closest('section')).toHaveClass('rounded-container', 'border-border-subtle');
    expect(modelTable.closest('section')).toHaveClass('rounded-container', 'border-border-subtle');
  });

  it('marks the dollar gauge unavailable when MTD cost is unknown', () => {
    render(
      <UsageReport
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
    render(<UsageReport summary={summary()} dayRows={[]} modelRows={[]} />);
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
      <UsageReport
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
    const monthCache = within(screen.getByTestId('usage-mtd-cache'));
    expect(monthCache.getByText('Cache read')).toBeTruthy();
    expect(monthCache.getByText('500')).toBeTruthy();
    expect(monthCache.getByText('Cache write')).toBeTruthy();
    expect(monthCache.getByText('100')).toBeTruthy();
    // Model token flow exposes both cache buckets and the cache-aware billed total.
    const table = screen.getByTestId('usage-model-table');
    const row = within(table).getByText('claude-sonnet-4').closest('tr')!;
    expect(within(row).getByText('anthropic')).toBeTruthy();
    const flow = within(row).getByTestId('usage-model-token-flow');
    for (const label of ['Fresh in', 'Cache read', 'Cache write', 'Out']) {
      expect(within(flow).getByText(label)).toBeTruthy();
    }
    expect(within(flow).getByText('Cache write')).toHaveClass('whitespace-nowrap', 'text-caps');
    expect(within(row).getAllByRole('cell')).toHaveLength(5);
    expect(within(row).getAllByRole('cell')[3]).toHaveTextContent('1,800');
    expect(within(row).getAllByRole('cell')[4]).toHaveTextContent('$0.01');
    // The cost-excludes-cache note appears.
    expect(screen.getByTestId('usage-cache-excluded-note')).toBeTruthy();
  });

  it('omits the Cache column when no row has cache tokens', () => {
    render(<UsageReport summary={summary()} dayRows={dayRows} modelRows={modelRows} />);
    const table = screen.getByTestId('usage-model-table');
    // No Cache header cell.
    expect(within(table).queryByText('Cache')).toBeNull();
    expect(screen.queryByTestId('usage-mtd-cache')).toBeNull();
    expect(screen.queryByTestId('usage-cache-excluded-note')).toBeNull();
  });

  it('does not draw a dollar gauge for a known but partial subtotal', () => {
    render(
      <UsageReport
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
      <UsageReport
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

    render(<UsageReport summary={summary()} dayRows={[]} modelRows={[incompleteModel]} />);

    const row = screen.getByText('legacy-model').closest('tr')!;
    expect(within(row).getByText('anthropic')).toBeTruthy();
    const flow = within(row).getByTestId('usage-model-token-flow');
    expect(within(flow).getByText('Not recorded')).toBeTruthy();
    const cells = within(row).getAllByRole('cell');
    expect(cells).toHaveLength(5);
    expect(cells[3]).toHaveTextContent('125');
    expect(cells[4]).toHaveTextContent('$1.25');
    expect(screen.getByTestId('usage-incomplete-note')).toBeTruthy();
  });

  it('shows the complete calendar month newest-first in the detailed report', () => {
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

    render(<UsageReport summary={summary()} dayRows={calendarRows} modelRows={modelRows} />);

    const table = screen.getByTestId('usage-day-table');
    const visibleDates = Array.from(table.querySelectorAll('time')).map((time) =>
      time.getAttribute('datetime')
    );
    expect(visibleDates).toHaveLength(14);
    expect(visibleDates.slice(0, 3)).toEqual(['2026-07-14', '2026-07-13', '2026-07-12']);
    expect(visibleDates[visibleDates.length - 1]).toBe('2026-07-01');
    const emptyDay = within(table).getByText('Mon, Jul 13').closest('tr')!;
    expect(
      within(emptyDay)
        .getAllByRole('cell')
        .slice(1)
        .map((cell) => cell.textContent)
    ).toEqual(['0', '0', '$0.00']);
    expect(within(table).getAllByRole('row')).toHaveLength(15);
    expect(table.querySelector('time[datetime="2026-07-07"]')).toBeTruthy();
    expect(screen.queryByRole('button', { name: /calendar days/i })).toBeNull();
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
      <UsageReport
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
    expect(screen.getByTestId('usage-report').textContent).not.toContain('N/A');
  });

  it('keeps internal labels subordinate to the settings section title', () => {
    render(<UsageReport summary={summary()} dayRows={dayRows} modelRows={modelRows} />);

    for (const label of ['Month to date', 'By day', 'By model']) {
      expect(screen.getByText(label).tagName).toBe('H3');
      expect(screen.getByText(label)).toHaveClass('text-sm', 'font-medium');
    }
  });
});

describe('UsagePanel', () => {
  it('keeps the settings surface concise until the user opens the report', async () => {
    const user = userEvent.setup();
    render(<UsagePanel summary={summary()} dayRows={dayRows} modelRows={modelRows} />);

    const panel = screen.getByTestId('usage-panel');
    expect(within(panel).getByText('2026-07')).toBeTruthy();
    expect(within(panel).getByText('33M')).toHaveAttribute('title', '33,000,000');
    expect(within(panel).getByText('$125.00')).toBeTruthy();
    expect(within(panel).getByText('42')).toBeTruthy();
    expect(screen.queryByTestId('usage-day-table')).toBeNull();
    expect(screen.queryByTestId('usage-model-table')).toBeNull();

    await user.click(screen.getByRole('button', { name: 'Open detailed usage report' }));

    expect(screen.getByTestId('usage-report-dialog')).toBeTruthy();
    expect(screen.getByRole('heading', { name: 'Usage report' })).toBeTruthy();
    expect(screen.getByTestId('usage-day-table')).toBeTruthy();
    expect(screen.getByTestId('usage-model-table')).toBeTruthy();
    expect(screen.queryByRole('columnheader', { name: 'Activity' })).toBeNull();
  });
});
