import { useEffect, useState } from 'react';
import {
  getUsageReport,
  getUsageSummary,
  UsageReportRow,
  UsageSummaryResponse,
} from '../../../api';
import { Button } from '../../ui/button';
import { Skeleton } from '../../ui/skeleton';
import { UsagePanel } from './UsagePanel';

/** Selectable report windows, in days. */
const RANGES = [
  { label: '7d', days: 7 },
  { label: '30d', days: 30 },
  { label: '90d', days: 90 },
] as const;

/**
 * Settings-embedded Usage surface: fetches the month-to-date summary and the
 * day- and model-grouped reports for the selected range, and hands them to the
 * pure {@link UsagePanel}. Fetch failures remain visible and retryable instead
 * of making the entire settings section disappear.
 */
export default function UsageSection() {
  const [days, setDays] = useState<number>(30);
  const [summary, setSummary] = useState<UsageSummaryResponse | null>(null);
  const [dayRows, setDayRows] = useState<UsageReportRow[]>([]);
  const [modelRows, setModelRows] = useState<UsageReportRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [loadedDays, setLoadedDays] = useState<number | null>(null);
  const [retryVersion, setRetryVersion] = useState(0);

  useEffect(() => {
    let cancelled = false;
    const load = async () => {
      setLoading(true);
      setLoadError(null);
      try {
        const now = Math.floor(Date.now() / 1000);
        const from = now - days * 86_400;
        const [summaryRes, dayRes, modelRes] = await Promise.all([
          getUsageSummary<true>({ throwOnError: true }),
          getUsageReport<true>({ query: { from, to: now, group: 'day' }, throwOnError: true }),
          getUsageReport<true>({ query: { from, to: now, group: 'model' }, throwOnError: true }),
        ]);
        if (cancelled) return;
        setSummary(summaryRes.data);
        setDayRows(dayRes.data.rows);
        setModelRows(modelRes.data.rows);
        setLoadedDays(days);
      } catch (error) {
        if (cancelled) return;
        console.error('Failed to load usage:', error);
        setLoadError('Usage data could not be loaded. Check the backend connection and try again.');
      } finally {
        if (!cancelled) setLoading(false);
      }
    };
    load();
    return () => {
      cancelled = true;
    };
  }, [days, retryVersion]);

  return (
    <div className="biorouter-settings-section">
      <div className="biorouter-settings-section-header flex items-center justify-between">
        <div>
          <h2 className="text-[11px] font-medium text-text-muted uppercase tracking-wider mb-1">
            Usage
          </h2>
          <p className="text-xs text-text-muted">
            Accumulated (billed) tokens and cost per day and per model, with month-to-date usage
            against your configured budget.
          </p>
        </div>
        <div className="flex gap-1" role="group" aria-label="Usage range">
          {RANGES.map((r) => (
            <Button
              key={r.days}
              type="button"
              size="xs"
              variant={days === r.days ? 'secondary' : 'ghost'}
              aria-pressed={days === r.days}
              onClick={() => setDays(r.days)}
            >
              {r.label}
            </Button>
          ))}
        </div>
      </div>

      {loadError && (
        <div
          className="flex items-center justify-between gap-3 rounded-md border border-border-subtle bg-background-medium px-3 py-2"
          role="alert"
          data-testid="usage-load-error"
        >
          <p className="text-xs text-text-muted">
            {loadError}
            {summary && loadedDays != null ? ` Showing the last loaded ${loadedDays}d report.` : ''}
          </p>
          <Button
            type="button"
            size="xs"
            variant="outline"
            onClick={() => setRetryVersion((version) => version + 1)}
          >
            Retry
          </Button>
        </div>
      )}

      {loading && !summary ? (
        <div className="flex flex-col gap-2">
          <Skeleton className="h-4 w-40" />
          <Skeleton className="h-16 w-full" />
          <Skeleton className="h-24 w-full" />
        </div>
      ) : summary ? (
        <UsagePanel summary={summary} dayRows={dayRows} modelRows={modelRows} />
      ) : null}
      {loading && summary && (
        <p className="text-xs text-text-muted" role="status">
          Refreshing usage…
        </p>
      )}
    </div>
  );
}
