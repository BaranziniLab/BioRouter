import type { UsageReportRow, UsageSummaryResponse, UsageTotals } from '../../../api';
import {
  billedTokens,
  cacheTokens as combinedCacheTokens,
  knownBilledTokens,
} from '../../../utils/usageAccounting';

export interface UsagePanelProps {
  summary: UsageSummaryResponse;
  dayRows: UsageReportRow[];
  modelRows: UsageReportRow[];
}

export function formatTokens(n: number | null | undefined): string {
  return typeof n === 'number' && Number.isFinite(n) && n >= 0 ? n.toLocaleString('en-US') : '—';
}

export function formatCost(cost: number | null | undefined): string {
  if (cost === null || cost === undefined || !Number.isFinite(cost) || cost < 0) return '—';
  if (cost > 0 && cost < 0.01) return '<$0.01';
  return `$${cost.toFixed(2)}`;
}

export function formatCostEstimate(cost: number | null | undefined, partial: boolean): string {
  const formatted = formatCost(cost);
  return partial && formatted !== '—' ? `≥${formatted}` : formatted;
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
  return knownSubtotal > 0 ? `≥${formatTokens(knownSubtotal)}` : '—';
}

function costIsPartial(row: Pick<UsageReportRow, 'hasUnpriced' | 'costExcludesCache'>) {
  return row.hasUnpriced || row.costExcludesCache;
}

function rowHasUnknownCost(
  row: Pick<UsageReportRow | UsageTotals, 'turns' | 'cost' | 'hasUnpriced'>
) {
  return row.hasUnpriced || (row.turns > 0 && formatCost(row.cost) === '—');
}

function hasIncompleteTokens(row: UsageReportRow | UsageTotals): boolean {
  return (
    billedTokens(row) === null || row.cacheReadTokens == null || row.cacheCreationTokens == null
  );
}

function showsCacheBuckets(row: UsageReportRow | UsageTotals): boolean {
  return (
    row.cacheReadTokens == null ||
    row.cacheCreationTokens == null ||
    row.cacheReadTokens > 0 ||
    row.cacheCreationTokens > 0
  );
}

function chartTokens(row: UsageReportRow | UsageTotals): number {
  return billedTokens(row) ?? knownBilledTokens(row);
}

function serverPercent(percent: number | null | undefined): number | null {
  return typeof percent === 'number' && Number.isFinite(percent) && percent >= 0 ? percent : null;
}

export function UsagePanel({ summary, dayRows, modelRows }: UsagePanelProps) {
  const mtd = summary.monthToDate;
  const maxDayTotal = Math.max(1, ...dayRows.map(chartTokens));
  const anyUnpriced =
    rowHasUnknownCost(mtd) || dayRows.some(rowHasUnknownCost) || modelRows.some(rowHasUnknownCost);
  const anyCache =
    showsCacheBuckets(mtd) || dayRows.some(showsCacheBuckets) || modelRows.some(showsCacheBuckets);
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
    <div className="flex flex-col gap-5" data-testid="usage-panel">
      <div>
        <div className="flex items-baseline justify-between">
          <p className="text-sm font-medium text-text-default">Month to date</p>
          <p className="text-xs text-text-muted">{summary.month}</p>
        </div>
        <p className="mt-0.5 text-xs text-text-muted">
          {formatBilledTokens(mtd)} billed tokens
          {' · '}
          {formatCostEstimate(mtd.cost, mtdCostPartial)}
          {' · '}
          {mtd.turns.toLocaleString('en-US')} turns
        </p>
        {showsCacheBuckets(mtd) && (
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
            used={formatCostEstimate(mtd.cost, mtdCostPartial)}
            limit={`$${summary.monthlyDollarLimit.toFixed(2)}`}
            percent={dollarPercent}
            unavailableReason={dollarUnavailableReason}
          />
        )}
      </div>

      <div>
        <p className="mb-2 text-sm font-medium text-text-default">Usage by day</p>
        {dayRows.length === 0 ? (
          <p className="text-xs text-text-muted">No usage in this range.</p>
        ) : (
          <div className="flex flex-col gap-1" data-testid="usage-day-bars">
            {dayRows.map((row) => {
              const total = chartTokens(row);
              const totalLabel = formatBilledTokens(row);
              return (
                <div
                  key={row.date}
                  className="flex min-h-10 items-center gap-2 text-xs"
                  aria-label={`${row.date}: ${totalLabel} billed tokens`}
                >
                  <span className="w-24 shrink-0 text-text-muted tabular-nums">{row.date}</span>
                  <div className="h-4 flex-1 overflow-hidden rounded-sm bg-heat-0">
                    <div
                      className="h-full rounded-sm bg-heat-3"
                      style={{ width: `${Math.max(2, (total / maxDayTotal) * 100)}%` }}
                      data-testid="usage-day-bar-fill"
                    />
                  </div>
                  <span className="w-28 shrink-0 text-right text-text-default tabular-nums">
                    {totalLabel} billed
                  </span>
                  {anyCache && (
                    <span
                      className="w-40 shrink-0 text-right text-text-muted tabular-nums"
                      aria-label={`${formatTokens(row.cacheReadTokens)} cache read, ${formatTokens(row.cacheCreationTokens)} cache write`}
                    >
                      {formatTokens(row.cacheReadTokens)} read ·{' '}
                      {formatTokens(row.cacheCreationTokens)} write
                    </span>
                  )}
                  <span className="w-20 shrink-0 text-right text-text-muted tabular-nums">
                    {formatCostEstimate(row.cost, costIsPartial(row))}
                  </span>
                </div>
              );
            })}
          </div>
        )}
      </div>

      <div>
        <p className="mb-2 text-sm font-medium text-text-default">Usage by model</p>
        {modelRows.length === 0 ? (
          <p className="text-xs text-text-muted">No usage in this range.</p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full border-collapse text-xs" data-testid="usage-model-table">
              <thead>
                <tr className="h-8 border-b border-border-subtle text-left text-[11px] uppercase tracking-wider text-text-muted">
                  <th className="pr-2 font-medium">Model</th>
                  <th className="px-2 text-right font-medium">Turns</th>
                  <th className="px-2 text-right font-medium">Fresh in</th>
                  {anyCache && <th className="px-2 text-right font-medium">Cache read</th>}
                  {anyCache && <th className="px-2 text-right font-medium">Cache write</th>}
                  <th className="px-2 text-right font-medium">Out</th>
                  <th className="px-2 text-right font-medium">Billed</th>
                  <th className="pl-2 text-right font-medium">Cost</th>
                </tr>
              </thead>
              <tbody>
                {modelRows.map((row, index) => (
                  <tr
                    key={`${modelLabel(row)}-${index}`}
                    className="h-10 border-b border-border-subtle text-text-default last:border-b-0"
                  >
                    <td className="pr-2 font-mono">{modelLabel(row)}</td>
                    <td className="px-2 text-right tabular-nums">
                      {row.turns.toLocaleString('en-US')}
                    </td>
                    <td className="px-2 text-right tabular-nums">
                      {formatTokens(row.inputTokens)}
                    </td>
                    {anyCache && (
                      <td className="px-2 text-right tabular-nums">
                        {formatTokens(row.cacheReadTokens)}
                      </td>
                    )}
                    {anyCache && (
                      <td className="px-2 text-right tabular-nums">
                        {formatTokens(row.cacheCreationTokens)}
                      </td>
                    )}
                    <td className="px-2 text-right tabular-nums">
                      {formatTokens(row.outputTokens)}
                    </td>
                    <td className="px-2 text-right tabular-nums">{formatBilledTokens(row)}</td>
                    <td className="pl-2 text-right tabular-nums">
                      {formatCostEstimate(row.cost, costIsPartial(row))}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {anyIncompleteTokens && (
        <p className="text-xs text-text-muted" data-testid="usage-incomplete-note" role="status">
          Some historical token buckets are incomplete. Values marked ≥ are known subtotals; — means
          no trustworthy total is available.
        </p>
      )}

      {anyUnpriced && (
        <p className="text-xs text-text-muted" data-testid="usage-unpriced-note">
          Some usage could not be fully priced. Unknown costs show —; mixed or incomplete totals
          show ≥ because only the known subtotal can be reported.
        </p>
      )}

      {anyCostExcludesCache && (
        <p className="text-xs text-text-muted" data-testid="usage-cache-excluded-note">
          Some cache cost is unavailable because a model has no cache rate or historical cache
          accounting is incomplete. The shown cost is a lower bound.
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
