import type { UsageReportRow, UsageSummaryResponse, UsageTotals } from '../../../api';
import { billedTokens, cacheTokens as combinedCacheTokens } from '../../../utils/usageAccounting';

export interface UsagePanelProps {
  summary: UsageSummaryResponse;
  dayRows: UsageReportRow[];
  modelRows: UsageReportRow[];
}

export function formatTokens(n: number): string {
  return n.toLocaleString('en-US');
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
): number {
  return combinedCacheTokens(row);
}

function costIsPartial(row: Pick<UsageReportRow, 'hasUnpriced' | 'costExcludesCache'>) {
  return row.hasUnpriced || row.costExcludesCache;
}

function rowHasUnknownCost(
  row: Pick<UsageReportRow | UsageTotals, 'turns' | 'cost' | 'hasUnpriced'>
) {
  return row.hasUnpriced || (row.turns > 0 && formatCost(row.cost) === '—');
}

function usageTotal(row: UsageReportRow | UsageTotals) {
  return billedTokens(row);
}

export function UsagePanel({ summary, dayRows, modelRows }: UsagePanelProps) {
  const mtd = summary.monthToDate;
  const mtdTokens = usageTotal(mtd);
  const maxDayTotal = Math.max(1, ...dayRows.map(usageTotal));
  const anyUnpriced =
    rowHasUnknownCost(mtd) || dayRows.some(rowHasUnknownCost) || modelRows.some(rowHasUnknownCost);
  const anyCache =
    cacheTokens(mtd) > 0 ||
    dayRows.some((row) => cacheTokens(row) > 0) ||
    modelRows.some((row) => cacheTokens(row) > 0);
  const anyCostExcludesCache =
    mtd.costExcludesCache ||
    dayRows.some((row) => row.costExcludesCache) ||
    modelRows.some((row) => row.costExcludesCache);
  const mtdCostPartial = costIsPartial(mtd);
  const knownMtdCost =
    mtd.cost != null && Number.isFinite(mtd.cost) && mtd.cost >= 0 ? mtd.cost : null;
  const tokenPercent =
    summary.monthlyTokenLimit != null && summary.monthlyTokenLimit > 0
      ? (mtdTokens / summary.monthlyTokenLimit) * 100
      : null;
  const dollarComplete = knownMtdCost !== null && !mtdCostPartial;
  const dollarPercent =
    dollarComplete && summary.monthlyDollarLimit != null && summary.monthlyDollarLimit > 0
      ? (knownMtdCost / summary.monthlyDollarLimit) * 100
      : null;

  return (
    <div className="flex flex-col gap-5" data-testid="usage-panel">
      <div>
        <div className="flex items-baseline justify-between">
          <p className="text-sm font-medium text-text-default">Month to date</p>
          <p className="text-xs text-text-muted">{summary.month}</p>
        </div>
        <p className="mt-0.5 text-xs text-text-muted">
          {formatTokens(mtdTokens)} billed tokens
          {' · '}
          {formatCostEstimate(mtd.cost, mtdCostPartial)}
          {' · '}
          {mtd.turns.toLocaleString('en-US')} turns
        </p>
        {cacheTokens(mtd) > 0 && (
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
            used={formatTokens(mtdTokens)}
            limit={formatTokens(summary.monthlyTokenLimit)}
            percent={tokenPercent}
            unavailableReason={
              tokenPercent === null ? 'Budget percentage unavailable for a zero token limit.' : null
            }
          />
        )}
        {summary.monthlyDollarLimit != null && (
          <UsageGauge
            testid="usage-gauge-dollars"
            label="Dollar budget"
            used={formatCostEstimate(mtd.cost, mtdCostPartial)}
            limit={`$${summary.monthlyDollarLimit.toFixed(2)}`}
            percent={dollarPercent}
            unavailableReason={
              dollarComplete
                ? dollarPercent === null
                  ? 'Budget percentage unavailable for a zero dollar limit.'
                  : null
                : knownMtdCost === null
                  ? 'Budget percentage unavailable because cost is unknown.'
                  : 'Budget percentage unavailable because the known cost is only a partial subtotal.'
            }
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
              const total = usageTotal(row);
              return (
                <div key={row.date} className="flex min-h-10 items-center gap-2 text-xs">
                  <span className="w-24 shrink-0 text-text-muted tabular-nums">{row.date}</span>
                  <div className="h-4 flex-1 overflow-hidden rounded-sm bg-heat-0">
                    <div
                      className="h-full rounded-sm bg-heat-3"
                      style={{ width: `${Math.max(2, (total / maxDayTotal) * 100)}%` }}
                      data-testid="usage-day-bar-fill"
                    />
                  </div>
                  <span className="w-28 shrink-0 text-right text-text-default tabular-nums">
                    {formatTokens(total)} billed
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
                    <td className="px-2 text-right tabular-nums">
                      {formatTokens(usageTotal(row))}
                    </td>
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

      {anyUnpriced && (
        <p className="text-xs text-text-muted" data-testid="usage-unpriced-note">
          Some models have no known pricing. Unknown rows show — and mixed totals show ≥ because
          only the known subtotal can be reported.
        </p>
      )}

      {anyCostExcludesCache && (
        <p className="text-xs text-text-muted" data-testid="usage-cache-excluded-note">
          Some priced models have no cache pricing. Their cache tokens remain in billed token
          totals, but their cache cost is excluded, so the shown cost is a lower bound.
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
