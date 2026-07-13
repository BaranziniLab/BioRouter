import { useEffect, useState } from 'react';
import {
  getUsageReport,
  getUsageSummary,
  UsageReportRow,
  UsageSummaryResponse,
} from '../../../api';
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
 * pure {@link UsagePanel}. A missing/old backend collapses the section rather
 * than leaving a permanent blank.
 */
export default function UsageSection() {
  const [days, setDays] = useState<number>(30);
  const [summary, setSummary] = useState<UsageSummaryResponse | null>(null);
  const [dayRows, setDayRows] = useState<UsageReportRow[]>([]);
  const [modelRows, setModelRows] = useState<UsageReportRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let cancelled = false;
    const load = async () => {
      setLoading(true);
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
        setFailed(false);
      } catch (error) {
        if (cancelled) return;
        // A 404 (old backend) or any failure collapses the section.
        console.error('Failed to load usage:', error);
        setFailed(true);
      } finally {
        if (!cancelled) setLoading(false);
      }
    };
    load();
    return () => {
      cancelled = true;
    };
  }, [days]);

  if (failed) return null;

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
            <button
              key={r.days}
              type="button"
              onClick={() => setDays(r.days)}
              className={`px-2 py-1 text-xs rounded-md border ${
                days === r.days
                  ? 'border-text-default text-text-default'
                  : 'border-border-subtle text-text-muted hover:text-text-default'
              }`}
            >
              {r.label}
            </button>
          ))}
        </div>
      </div>

      {loading || !summary ? (
        <div className="flex flex-col gap-2">
          <Skeleton className="h-4 w-40" />
          <Skeleton className="h-16 w-full" />
          <Skeleton className="h-24 w-full" />
        </div>
      ) : (
        <UsagePanel summary={summary} dayRows={dayRows} modelRows={modelRows} />
      )}
    </div>
  );
}
