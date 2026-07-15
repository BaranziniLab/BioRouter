import { useState } from 'react';
import type { UsageReportRow, UsageSummaryResponse, UsageTotals } from '../../../api';
import {
  billedTokens,
  cacheTokens as combinedCacheTokens,
  knownBilledTokens,
} from '../../../utils/usageAccounting';
import { Button } from '../../ui/button';

const COLLAPSED_DAY_COUNT = 3;

function emptyDayRow(date: string): UsageReportRow {
  return {
    date,
    modelId: null,
    provider: null,
    inputTokens: 0,
    outputTokens: 0,
    totalTokens: 0,
    cacheReadTokens: 0,
    cacheCreationTokens: 0,
    turns: 0,
    cost: 0,
    hasUnpriced: false,
    costExcludesCache: false,
  };
}

/** The report omits inactive dates; restore them so recency remains a calendar sequence. */
export function fillCalendarDays(rows: UsageReportRow[], through: Date): UsageReportRow[] {
  if (rows.length === 0) return [];

  const year = through.getFullYear();
  const month = through.getMonth() + 1;
  const monthPrefix = `${year}-${String(month).padStart(2, '0')}`;
  const rowsByDate = new Map(
    rows.filter((row) => row.date?.startsWith(monthPrefix)).map((row) => [row.date, row])
  );

  return Array.from({ length: through.getDate() }, (_, index) => {
    const date = `${monthPrefix}-${String(index + 1).padStart(2, '0')}`;
    return rowsByDate.get(date) ?? emptyDayRow(date);
  });
}

export interface UsagePanelProps {
  summary: UsageSummaryResponse;
  dayRows: UsageReportRow[];
  modelRows: UsageReportRow[];
}

export function formatTokens(n: number | null | undefined): string {
  return typeof n === 'number' && Number.isFinite(n) && n >= 0
    ? n.toLocaleString('en-US')
    : 'Not recorded';
}

export function formatCost(cost: number | null | undefined): string {
  if (cost === null || cost === undefined || !Number.isFinite(cost) || cost < 0) {
    return 'Unavailable';
  }
  if (cost > 0 && cost < 0.01) return '<$0.01';
  return `$${cost.toFixed(2)}`;
}

export function formatCostEstimate(cost: number | null | undefined): string {
  return formatCost(cost);
}

export function formatUsageDate(date: string | null | undefined): string {
  const match = date?.match(/^(\d{4})-(\d{2})-(\d{2})$/);
  if (!match) return date ?? 'Unknown date';
  const [, year, month, day] = match;
  return new Date(Number(year), Number(month) - 1, Number(day)).toLocaleDateString('en-US', {
    weekday: 'short',
    month: 'short',
    day: 'numeric',
  });
}

export function modelLabel(row: Pick<UsageReportRow, 'modelId' | 'provider'>): string {
  if (row.modelId && row.provider) return `${row.provider}/${row.modelId}`;
  if (row.modelId) return row.modelId;
  return 'unknown';
}

export function cacheTokens(
  row: Pick<UsageReportRow, 'cacheReadTokens' | 'cacheCreationTokens'>
): number | null {
  return combinedCacheTokens(row);
}

export function formatBilledTokens(row: UsageReportRow | UsageTotals): string {
  const exact = billedTokens(row);
  if (exact !== null) return formatTokens(exact);
  const knownSubtotal = knownBilledTokens(row);
  return knownSubtotal > 0 ? formatTokens(knownSubtotal) : 'Unavailable';
}

function costIsPartial(row: Pick<UsageReportRow, 'hasUnpriced' | 'costExcludesCache'>) {
  return row.hasUnpriced || row.costExcludesCache;
}

function rowHasUnknownCost(
  row: Pick<UsageReportRow | UsageTotals, 'turns' | 'cost' | 'hasUnpriced'>
) {
  return row.hasUnpriced || (row.turns > 0 && formatCost(row.cost) === 'Unavailable');
}

function hasIncompleteTokens(row: UsageReportRow | UsageTotals): boolean {
  return (
    billedTokens(row) === null || row.cacheReadTokens == null || row.cacheCreationTokens == null
  );
}

function hasRecordedCacheUsage(row: UsageReportRow | UsageTotals): boolean {
  return (
    (row.cacheReadTokens != null && row.cacheReadTokens > 0) ||
    (row.cacheCreationTokens != null && row.cacheCreationTokens > 0)
  );
}

function hasKnownCost(row: UsageReportRow | UsageTotals): boolean {
  return row.cost != null && Number.isFinite(row.cost) && row.cost >= 0;
}

function hasUsage(row: UsageReportRow): boolean {
  return row.turns > 0 || knownBilledTokens(row) > 0;
}

function serverPercent(percent: number | null | undefined): number | null {
  return typeof percent === 'number' && Number.isFinite(percent) && percent >= 0 ? percent : null;
}

function UsageTableColumns({
  detailColumns,
  showCost,
}: {
  detailColumns: number;
  showCost: boolean;
}) {
  return (
    <colgroup>
      <col className={showCost ? 'w-[38%]' : 'w-[40%]'} />
      <col className="w-[10%]" />
      <col span={detailColumns} />
      <col className={showCost ? 'w-[16%]' : 'w-[18%]'} />
      {showCost && <col className="w-[14%]" />}
    </colgroup>
  );
}

export function UsagePanel({ summary, dayRows, modelRows }: UsagePanelProps) {
  const [showAllDays, setShowAllDays] = useState(false);
  const mtd = summary.monthToDate;
  const anyUnpriced =
    rowHasUnknownCost(mtd) || dayRows.some(rowHasUnknownCost) || modelRows.some(rowHasUnknownCost);
  const showMtdCache = hasRecordedCacheUsage(mtd);
  const showModelCache = modelRows.some(hasRecordedCacheUsage);
  const showDayCost = dayRows.some((row) => hasUsage(row) && hasKnownCost(row));
  const showModelCost = modelRows.some(hasKnownCost);
  const showSharedCost = showDayCost || showModelCost;
  const detailColumnCount = showModelCache ? 4 : 2;
  const alignedTableClass = showModelCache
    ? 'w-full min-w-[960px] table-fixed border-collapse text-xs'
    : 'w-full min-w-[640px] table-fixed border-collapse text-xs';
  const anyIncompleteTokens =
    hasIncompleteTokens(mtd) ||
    dayRows.some(hasIncompleteTokens) ||
    modelRows.some(hasIncompleteTokens);
  const anyCostExcludesCache =
    mtd.costExcludesCache ||
    dayRows.some((row) => row.costExcludesCache) ||
    modelRows.some((row) => row.costExcludesCache);
  const mtdCostPartial = costIsPartial(mtd);
  const knownMtdCost =
    mtd.cost != null && Number.isFinite(mtd.cost) && mtd.cost >= 0 ? mtd.cost : null;
  const tokenPercent = serverPercent(summary.tokenPercent);
  const dollarPercent = serverPercent(summary.dollarPercent);
  const orderedDayRows = [...dayRows].sort((a, b) => (b.date ?? '').localeCompare(a.date ?? ''));
  const visibleDayRows = showAllDays
    ? orderedDayRows
    : orderedDayRows.slice(0, COLLAPSED_DAY_COUNT);
  const tokenUnavailableReason =
    tokenPercent !== null
      ? null
      : summary.monthlyTokenLimit != null && summary.monthlyTokenLimit <= 0
        ? 'Budget percentage unavailable for a zero token limit.'
        : billedTokens(mtd) === null
          ? 'Budget percentage unavailable because billed token history is incomplete.'
          : 'Budget percentage unavailable.';
  const dollarUnavailableReason =
    dollarPercent !== null
      ? null
      : summary.monthlyDollarLimit != null && summary.monthlyDollarLimit <= 0
        ? 'Budget percentage unavailable for a zero dollar limit.'
        : knownMtdCost === null
          ? 'Budget percentage unavailable because cost is unknown.'
          : mtdCostPartial
            ? 'Budget percentage unavailable because the known cost is only a partial subtotal.'
            : 'Budget percentage unavailable.';

  return (
    <div className="flex flex-col gap-4" data-testid="usage-panel">
      <div>
        <div className="flex items-baseline justify-between">
          <p className="text-[11px] font-normal tracking-wide text-text-subtle">Month to date</p>
          <p className="text-xs text-text-muted">{summary.month}</p>
        </div>
        <p className="mt-0.5 text-xs text-text-muted">
          {formatBilledTokens(mtd)} billed tokens
          {' · '}
          {formatCostEstimate(mtd.cost)}
          {' · '}
          {mtd.turns.toLocaleString('en-US')} turns
        </p>
        {showMtdCache && (
          <p className="mt-0.5 text-xs text-text-muted" data-testid="usage-mtd-cache">
            {formatTokens(mtd.cacheReadTokens)} cache read
            {' · '}
            {formatTokens(mtd.cacheCreationTokens)} cache write
          </p>
        )}

        {summary.monthlyTokenLimit != null && (
          <UsageGauge
            testid="usage-gauge-tokens"
            label="Token budget"
            used={formatBilledTokens(mtd)}
            limit={formatTokens(summary.monthlyTokenLimit)}
            percent={tokenPercent}
            unavailableReason={tokenUnavailableReason}
          />
        )}
        {summary.monthlyDollarLimit != null && (
          <UsageGauge
            testid="usage-gauge-dollars"
            label="Dollar budget"
            used={formatCostEstimate(mtd.cost)}
            limit={`$${summary.monthlyDollarLimit.toFixed(2)}`}
            percent={dollarPercent}
            unavailableReason={dollarUnavailableReason}
          />
        )}
      </div>

      <div>
        <div className="mb-2 flex w-full max-w-[680px] items-center justify-between gap-3">
          <p className="text-[11px] font-normal tracking-wide text-text-subtle">By day</p>
          {orderedDayRows.length > COLLAPSED_DAY_COUNT && (
            <Button
              type="button"
              size="xs"
              variant="ghost"
              aria-expanded={showAllDays}
              aria-label={
                showAllDays
                  ? 'Show recent 3 calendar days'
                  : `Show all ${orderedDayRows.length} calendar days this month`
              }
              onClick={() => setShowAllDays((expanded) => !expanded)}
            >
              {showAllDays ? 'Show recent 3 days' : 'Show month'}
            </Button>
          )}
        </div>
        {dayRows.length === 0 ? (
          <p className="text-xs text-text-muted">No usage this month.</p>
        ) : (
          <div className="w-full max-w-[680px] overflow-x-auto" data-testid="usage-day-table-wrap">
            <table className={alignedTableClass} data-testid="usage-day-table">
              <UsageTableColumns detailColumns={detailColumnCount} showCost={showSharedCost} />
              <thead>
                <tr className="h-8 border-b border-border-subtle text-left text-[11px] uppercase tracking-wider text-text-muted">
                  <th className="pr-3 font-medium">Day</th>
                  <th className="px-3 text-right font-medium">Turns</th>
                  <th colSpan={detailColumnCount} aria-hidden="true" />
                  <th className="px-3 text-right font-medium">Billed</th>
                  {showSharedCost && <th className="pl-3 text-right font-medium">Cost</th>}
                </tr>
              </thead>
              <tbody>
                {visibleDayRows.map((row) => (
                  <tr
                    key={row.date}
                    className="h-10 border-b border-border-subtle text-text-default last:border-b-0"
                  >
                    <td className="pr-3 text-text-muted">
                      <time dateTime={row.date ?? undefined} title={row.date ?? undefined}>
                        {formatUsageDate(row.date)}
                      </time>
                    </td>
                    <td className="px-3 text-right text-text-muted tabular-nums">
                      {row.turns.toLocaleString('en-US')}
                    </td>
                    <td colSpan={detailColumnCount} aria-hidden="true" />
                    <td className="px-3 text-right font-medium tabular-nums">
                      {formatBilledTokens(row)}
                    </td>
                    {showSharedCost && (
                      <td className="pl-3 text-right text-text-muted tabular-nums">
                        {formatCostEstimate(row.cost)}
                      </td>
                    )}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      <div>
        <p className="mb-2 text-[11px] font-normal tracking-wide text-text-subtle">By model</p>
        {modelRows.length === 0 ? (
          <p className="text-xs text-text-muted">No usage this month.</p>
        ) : (
          <div
            className="w-full max-w-[680px] overflow-x-auto"
            data-testid="usage-model-table-wrap"
          >
            <table className={alignedTableClass} data-testid="usage-model-table">
              <UsageTableColumns detailColumns={detailColumnCount} showCost={showSharedCost} />
              <thead>
                <tr className="h-8 border-b border-border-subtle text-left text-[11px] uppercase tracking-wider text-text-muted">
                  <th className="pr-3 font-medium">Model</th>
                  <th className="px-3 text-right font-medium">Turns</th>
                  <th className="px-3 text-right font-medium">Fresh in</th>
                  {showModelCache && <th className="px-3 text-right font-medium">Cache read</th>}
                  {showModelCache && <th className="px-3 text-right font-medium">Cache write</th>}
                  <th className="px-3 text-right font-medium">Out</th>
                  <th className="px-3 text-right font-medium">Billed</th>
                  {showSharedCost && <th className="pl-3 text-right font-medium">Cost</th>}
                </tr>
              </thead>
              <tbody>
                {modelRows.map((row, index) => (
                  <tr
                    key={`${modelLabel(row)}-${index}`}
                    className="h-10 border-b border-border-subtle text-text-default last:border-b-0"
                  >
                    <td className="pr-3 font-mono">
                      <span className="block truncate" title={modelLabel(row)}>
                        {modelLabel(row)}
                      </span>
                    </td>
                    <td className="px-3 text-right tabular-nums">
                      {row.turns.toLocaleString('en-US')}
                    </td>
                    <td className="px-3 text-right tabular-nums">
                      {formatTokens(row.inputTokens)}
                    </td>
                    {showModelCache && (
                      <td className="px-3 text-right tabular-nums">
                        {formatTokens(row.cacheReadTokens)}
                      </td>
                    )}
                    {showModelCache && (
                      <td className="px-3 text-right tabular-nums">
                        {formatTokens(row.cacheCreationTokens)}
                      </td>
                    )}
                    <td className="px-3 text-right tabular-nums">
                      {formatTokens(row.outputTokens)}
                    </td>
                    <td className="px-3 text-right tabular-nums">{formatBilledTokens(row)}</td>
                    {showSharedCost && (
                      <td className="pl-3 text-right tabular-nums">
                        {formatCostEstimate(row.cost)}
                      </td>
                    )}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {anyIncompleteTokens && (
        <p className="text-xs text-text-muted" data-testid="usage-incomplete-note" role="status">
          Some historical token details were not recorded. Displayed totals are conservative
          estimates; cache and cost columns are hidden when they contain no usable data.
        </p>
      )}

      {anyUnpriced && (
        <p className="text-xs text-text-muted" data-testid="usage-unpriced-note">
          Some historical usage has no stored model or pricing attribution, so its cost cannot be
          recovered. Available cost estimates are conservative.
        </p>
      )}

      {anyCostExcludesCache && (
        <p className="text-xs text-text-muted" data-testid="usage-cache-excluded-note">
          Some cache cost is unavailable because a model has no cache rate or historical cache
          accounting is incomplete. The shown cost is conservative.
        </p>
      )}
    </div>
  );
}

interface UsageGaugeProps {
  testid: string;
  label: string;
  used: string;
  limit: string;
  percent: number | null;
  unavailableReason: string | null;
}

function UsageGauge({ testid, label, used, limit, percent, unavailableReason }: UsageGaugeProps) {
  const pct = percent ?? 0;
  const clamped = Math.max(0, Math.min(100, pct));
  const over = percent != null && percent > 100;
  return (
    <div className="mt-2" data-testid={testid}>
      <div className="flex items-baseline justify-between text-xs">
        <span className="text-text-muted">{label}</span>
        <span className="text-text-default tabular-nums">
          {used} / {limit}
          {percent != null && (
            <span className={`ml-1 ${over ? 'text-text-danger' : 'text-text-muted'}`}>
              ({pct.toFixed(1)}%)
            </span>
          )}
        </span>
      </div>
      {unavailableReason ? (
        <div
          className="mt-1 rounded-md border border-border-subtle bg-background-medium px-2 py-1 text-xs text-text-muted"
          role="status"
          data-testid={`${testid}-unavailable`}
        >
          {unavailableReason}
        </div>
      ) : (
        <div
          className="mt-1 h-2 overflow-hidden rounded-full bg-heat-0"
          role="progressbar"
          aria-label={label}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={clamped}
        >
          <div
            className={`h-full rounded-full ${over ? 'bg-background-danger' : 'bg-heat-3'}`}
            style={{ width: `${clamped}%` }}
            data-testid={`${testid}-fill`}
          />
        </div>
      )}
    </div>
  );
}
