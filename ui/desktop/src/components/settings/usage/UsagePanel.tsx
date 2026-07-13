import type { UsageReportRow, UsageSummaryResponse } from '../../../api';

/**
 * Pure presentational Usage panel: per-day bars, a per-model token+cost table,
 * and a month-to-date gauge against the configured budget. It renders only from
 * the props it is given (no fetching), so it can be exercised from fixed
 * payloads with and without a configured limit / pricing.
 */
export interface UsagePanelProps {
  summary: UsageSummaryResponse;
  /** Day-grouped report rows (each `date` set), oldest → newest. */
  dayRows: UsageReportRow[];
  /** Model-grouped report rows (each `modelId`/`provider` set, or the unknown bucket). */
  modelRows: UsageReportRow[];
}

export function formatTokens(n: number): string {
  return n.toLocaleString('en-US');
}

/** A dollar figure, or an em dash when cost is unknown (never a fake $0). */
export function formatCost(cost: number | null | undefined): string {
  if (cost === null || cost === undefined) return '—';
  if (cost > 0 && cost < 0.01) return '<$0.01';
  return `$${cost.toFixed(2)}`;
}

export function modelLabel(row: Pick<UsageReportRow, 'modelId' | 'provider'>): string {
  if (row.modelId && row.provider) return `${row.provider}/${row.modelId}`;
  if (row.modelId) return row.modelId;
  return 'unknown';
}

/** Combined prompt-cache tokens (read + creation) for a row or totals. */
export function cacheTokens(
  row: Pick<UsageReportRow, 'cacheReadTokens' | 'cacheCreationTokens'>
): number {
  return (row.cacheReadTokens ?? 0) + (row.cacheCreationTokens ?? 0);
}

export function UsagePanel({ summary, dayRows, modelRows }: UsagePanelProps) {
  const mtd = summary.monthToDate;
  const maxDayTotal = Math.max(1, ...dayRows.map((r) => r.totalTokens));
  const anyUnpriced =
    dayRows.some((r) => r.hasUnpriced) || modelRows.some((r) => r.hasUnpriced || r.cost === null);
  const anyCache = modelRows.some((r) => cacheTokens(r) > 0) || cacheTokens(mtd) > 0;
  const anyCostExcludesCache =
    dayRows.some((r) => r.costExcludesCache) ||
    modelRows.some((r) => r.costExcludesCache) ||
    mtd.costExcludesCache;

  return (
    <div className="flex flex-col gap-5" data-testid="usage-panel">
      {/* Month-to-date gauge */}
      <div>
        <div className="flex items-baseline justify-between">
          <p className="text-sm font-medium text-text-default">Month to date</p>
          <p className="text-xs text-text-muted">{summary.month}</p>
        </div>
        <p className="text-xs text-text-muted mt-0.5">
          {formatTokens(mtd.totalTokens)} tokens
          {' · '}
          {formatCost(mtd.cost)}
          {' · '}
          {mtd.turns.toLocaleString('en-US')} turns
          {cacheTokens(mtd) > 0 && (
            <span data-testid="usage-mtd-cache">
              {' · '}
              {formatTokens(cacheTokens(mtd))} cached
            </span>
          )}
        </p>

        {summary.monthlyTokenLimit != null && (
          <UsageGauge
            testid="usage-gauge-tokens"
            label="Token budget"
            used={formatTokens(mtd.totalTokens)}
            limit={formatTokens(summary.monthlyTokenLimit)}
            percent={summary.tokenPercent ?? 0}
          />
        )}
        {summary.monthlyDollarLimit != null && (
          <UsageGauge
            testid="usage-gauge-dollars"
            label="Dollar budget"
            used={summary.tokenPercent != null && mtd.cost != null ? formatCost(mtd.cost) : '—'}
            limit={`$${summary.monthlyDollarLimit.toFixed(2)}`}
            percent={summary.dollarPercent}
            unavailable={summary.dollarPercent == null}
          />
        )}
      </div>

      {/* Per-day bars */}
      <div>
        <p className="text-sm font-medium text-text-default mb-2">Usage by day</p>
        {dayRows.length === 0 ? (
          <p className="text-xs text-text-muted">No usage in this range.</p>
        ) : (
          <div className="flex flex-col gap-1" data-testid="usage-day-bars">
            {dayRows.map((row) => (
              <div key={row.date} className="flex items-center gap-2 text-xs">
                <span className="w-24 shrink-0 text-text-muted tabular-nums">{row.date}</span>
                <div className="flex-1 h-4 bg-background-muted rounded-sm overflow-hidden">
                  <div
                    className="h-full bg-text-default/70 rounded-sm"
                    style={{ width: `${Math.max(2, (row.totalTokens / maxDayTotal) * 100)}%` }}
                    data-testid="usage-day-bar-fill"
                  />
                </div>
                <span className="w-24 shrink-0 text-right tabular-nums text-text-default">
                  {formatTokens(row.totalTokens)}
                </span>
                <span className="w-16 shrink-0 text-right tabular-nums text-text-muted">
                  {formatCost(row.cost)}
                </span>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Per-model table */}
      <div>
        <p className="text-sm font-medium text-text-default mb-2">Usage by model</p>
        {modelRows.length === 0 ? (
          <p className="text-xs text-text-muted">No usage in this range.</p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-xs" data-testid="usage-model-table">
              <thead>
                <tr className="text-text-muted text-left">
                  <th className="font-medium py-1 pr-2">Model</th>
                  <th className="font-medium py-1 px-2 text-right">Turns</th>
                  <th className="font-medium py-1 px-2 text-right">Input</th>
                  <th className="font-medium py-1 px-2 text-right">Output</th>
                  {anyCache && <th className="font-medium py-1 px-2 text-right">Cache</th>}
                  <th className="font-medium py-1 pl-2 text-right">Cost</th>
                </tr>
              </thead>
              <tbody>
                {modelRows.map((row, i) => (
                  <tr key={`${modelLabel(row)}-${i}`} className="text-text-default">
                    <td className="py-1 pr-2">{modelLabel(row)}</td>
                    <td className="py-1 px-2 text-right tabular-nums">
                      {row.turns.toLocaleString('en-US')}
                    </td>
                    <td className="py-1 px-2 text-right tabular-nums">
                      {formatTokens(row.inputTokens)}
                    </td>
                    <td className="py-1 px-2 text-right tabular-nums">
                      {formatTokens(row.outputTokens)}
                    </td>
                    {anyCache && (
                      <td className="py-1 px-2 text-right tabular-nums">
                        {formatTokens(cacheTokens(row))}
                      </td>
                    )}
                    <td className="py-1 pl-2 text-right tabular-nums">{formatCost(row.cost)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {anyUnpriced && (
        <p className="text-xs text-text-muted" data-testid="usage-unpriced-note">
          Some models have no known pricing; their cost is excluded (shown as —), so dollar totals
          are a lower bound.
        </p>
      )}

      {anyCostExcludesCache && (
        <p className="text-xs text-text-muted" data-testid="usage-cache-excluded-note">
          Some priced models have no cache pricing; their cache tokens are excluded from cost, so
          dollar totals are a lower bound.
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
  percent: number | null | undefined;
  unavailable?: boolean;
}

function UsageGauge({ testid, label, used, limit, percent, unavailable }: UsageGaugeProps) {
  const pct = percent ?? 0;
  const clamped = Math.max(0, Math.min(100, pct));
  const over = pct > 100;
  return (
    <div className="mt-2" data-testid={testid}>
      <div className="flex items-baseline justify-between text-xs">
        <span className="text-text-muted">{label}</span>
        <span className="text-text-default tabular-nums">
          {used} / {limit}
          {!unavailable && percent != null && (
            <span className={`ml-1 ${over ? 'text-red-500' : 'text-text-muted'}`}>
              ({pct.toFixed(1)}%)
            </span>
          )}
          {unavailable && <span className="ml-1 text-text-muted">(cost unavailable)</span>}
        </span>
      </div>
      <div className="h-2 mt-1 bg-background-muted rounded-full overflow-hidden">
        <div
          className={`h-full rounded-full ${over ? 'bg-red-500' : 'bg-text-default/70'}`}
          style={{ width: `${clamped}%` }}
          data-testid={`${testid}-fill`}
        />
      </div>
    </div>
  );
}
