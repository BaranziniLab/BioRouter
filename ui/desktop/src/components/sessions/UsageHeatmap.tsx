import { useLayoutEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import type { ActivityWindow, DailyActivity } from '../../api';

/**
 * A GitHub-style contribution graph for BioRouter usage.
 *
 * Shading is computed server-side (see `build_activity_window`) from the
 * quartiles of the active days in the window, so a single 1.8M-token day cannot
 * flatten every ordinary day into the faintest shade. Absolute numbers live in
 * the tooltip, where they belong.
 */

const DAY_MS = 86_400_000;

/** Local calendar day, `YYYY-MM-DD`, matching the server's `date('now','localtime')`. */
function isoDay(d: Date): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, '0');
  const day = String(d.getDate()).padStart(2, '0');
  return `${y}-${m}-${day}`;
}

function parseIsoDay(s: string): Date {
  const [y, m, d] = s.split('-').map(Number);
  return new Date(y, m - 1, d);
}

type Cell = {
  key: string;
  date: Date;
  day: DailyActivity | null;
  inStreak: boolean;
};

/**
 * Lay the window out as whole weeks, Sunday-first, so the columns line up with
 * the day labels and the month ruler.
 */
function buildGrid(window: ActivityWindow): { cells: Cell[]; weeks: number } {
  const byDate = new Map(window.days.map((d) => [d.date, d]));

  const end = parseIsoDay(window.end);
  // pad forward to Saturday so the final column is complete
  end.setDate(end.getDate() + (6 - end.getDay()));
  const start = parseIsoDay(window.start);
  // pad back to Sunday
  start.setDate(start.getDate() - start.getDay());

  const total = Math.round((end.getTime() - start.getTime()) / DAY_MS) + 1;

  // The current streak is the run of active days ending at `window.end`. Mark
  // those cells so the user can see the thing the header is counting.
  const streakDays = new Set<string>();
  if (window.currentStreak > 0) {
    const cursor = parseIsoDay(window.end);
    if (!byDate.has(isoDay(cursor))) cursor.setDate(cursor.getDate() - 1);
    for (let i = 0; i < window.currentStreak; i++) {
      streakDays.add(isoDay(cursor));
      cursor.setDate(cursor.getDate() - 1);
    }
  }

  const cells: Cell[] = [];
  for (let i = 0; i < total; i++) {
    const date = new Date(start.getTime() + i * DAY_MS);
    const key = isoDay(date);
    cells.push({ key, date, day: byDate.get(key) ?? null, inStreak: streakDays.has(key) });
  }
  return { cells, weeks: total / 7 };
}

const LEVEL_CLASS: Record<number, string> = {
  0: 'bg-heat-0',
  1: 'bg-heat-1',
  2: 'bg-heat-2',
  3: 'bg-heat-3',
  4: 'bg-heat-4',
};

const compact = new Intl.NumberFormat('en', { notation: 'compact', maximumFractionDigits: 2 });
const full = new Intl.NumberFormat('en');

/** The subset of a cell's box the tooltip needs, as plain numbers. */
type Anchor = { left: number; top: number; bottom: number; width: number };

function Tooltip({ cell, anchor }: { cell: Cell; anchor: Anchor }) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);

  // Measure after paint: a tooltip positioned from a guessed width jumps.
  // The tooltip is portalled to <body> (below) so `fixed` is always
  // viewport-relative — a transformed ancestor would otherwise make `fixed`
  // resolve against that ancestor and let the tooltip escape the window. Clamp
  // both axes to the visual viewport (clientWidth/Height exclude the scrollbar).
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const { width, height } = el.getBoundingClientRect();
    const vw = document.documentElement.clientWidth;
    const vh = document.documentElement.clientHeight;
    const left = Math.max(8, Math.min(anchor.left + anchor.width / 2 - width / 2, vw - width - 8));
    const above = anchor.top - height - 10;
    const below = anchor.bottom + 10;
    const top = above >= 8 ? Math.min(above, vh - height - 8) : Math.min(below, vh - height - 8);
    setPos({ left, top: Math.max(8, top) });
  }, [anchor]);

  const d = cell.day;
  const label = cell.date.toLocaleDateString('en-US', {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
  });

  return (
    <div
      ref={ref}
      role="tooltip"
      className="pointer-events-none fixed z-[var(--z-toast)] w-max min-w-[228px] max-w-[calc(100vw-16px)] rounded-2xl border border-border-subtle bg-background-default p-3 shadow-popover"
      style={{
        left: pos?.left ?? -9999,
        top: pos?.top ?? -9999,
        visibility: pos ? 'visible' : 'hidden',
      }}
    >
      <p className="mb-2 text-[13px] font-semibold text-text-default">{label}</p>
      {d ? (
        <>
          <Row label="Sessions started" value={full.format(d.sessions)} />
          <Row label="Tokens processed" value={full.format(d.tokens)} />
          <Row label="Messages" value={full.format(d.messages)} />
          <p className="mt-2 border-t border-border-subtle pt-2 text-[11px] leading-snug text-text-subtle">
            Tokens are attributed to the turn that spent them.
          </p>
        </>
      ) : (
        <Row label="No activity" value="—" />
      )}
    </div>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-baseline justify-between gap-5 text-[12.5px] leading-[1.9] text-text-muted">
      <span>{label}</span>
      <b className="font-mono font-medium tabular-nums text-text-default">{value}</b>
    </div>
  );
}

export function UsageHeatmap({ window: activity }: { window: ActivityWindow }) {
  const { cells, weeks } = useMemo(() => buildGrid(activity), [activity]);
  const [hovered, setHovered] = useState<{ cell: Cell; rect: Anchor } | null>(null);

  // One label per month, placed on the first column whose Sunday falls in it.
  const months = useMemo(() => {
    const out: (string | null)[] = [];
    let last = '';
    for (let w = 0; w < weeks; w++) {
      const d = cells[w * 7].date;
      const key = `${d.getFullYear()}-${d.getMonth()}`;
      if (key !== last && d.getDate() <= 7) {
        last = key;
        out.push(d.toLocaleDateString('en-US', { month: 'short' }));
      } else {
        out.push(null);
      }
    }
    return out;
  }, [cells, weeks]);

  const show = (cell: Cell) => (e: React.SyntheticEvent<HTMLElement>) => {
    const { left, top, bottom, width } = e.currentTarget.getBoundingClientRect();
    setHovered({ cell, rect: { left, top, bottom, width } });
  };
  const hide = () => setHovered(null);

  return (
    <div className="biorouter-heatmap">
      <div className="mb-1.5 flex items-baseline justify-between gap-4">
        <h2 className="text-lg font-semibold tracking-tight text-text-default">
          {activity.currentStreak === 1 ? '1 day streak' : `${activity.currentStreak} day streak`}
        </h2>
        <span className="text-[10px] font-medium uppercase tracking-wider text-text-muted">
          Longest streak · {activity.longestStreak} {activity.longestStreak === 1 ? 'day' : 'days'}
        </span>
      </div>
      <p className="mb-4 text-[13px] text-text-muted">
        Daily usage intensity — sessions started and tokens processed.
      </p>

      <div className="grid grid-cols-[var(--heat-labels)_1fr] gap-[var(--heat-gap)]">
        <span aria-hidden="true" />
        <div className="flex gap-[var(--heat-gap)] text-[11px] text-text-muted">
          {months.map((m, i) => (
            <span
              key={i}
              className="w-[var(--heat-cell)] flex-none overflow-visible whitespace-nowrap"
            >
              {m}
            </span>
          ))}
        </div>
      </div>

      <div className="mt-1.5 grid grid-cols-[var(--heat-labels)_1fr] gap-[var(--heat-gap)]">
        <div className="grid grid-rows-7 gap-[var(--heat-gap)] text-[10px] leading-none text-text-muted">
          {['Sun', '', 'Tue', '', 'Thu', '', 'Sat'].map((d, i) => (
            <span key={i} className="flex h-[var(--heat-cell)] items-center">
              {d}
            </span>
          ))}
        </div>

        {/* `auto` implicit columns stretch to fill a definite-width grid, which
            would pull the cells away from the month ruler. Pin them. */}
        <div
          className="grid grid-flow-col justify-start gap-[var(--heat-gap)]"
          style={{
            gridTemplateRows: 'repeat(7, var(--heat-cell))',
            gridAutoColumns: 'var(--heat-cell)',
          }}
          role="grid"
          aria-label="Daily usage heatmap"
        >
          {cells.map((cell) => (
            <button
              key={cell.key}
              type="button"
              // The tooltip is the only way to read the numbers, so it must open
              // on keyboard focus, not hover alone.
              onMouseEnter={show(cell)}
              onFocus={show(cell)}
              onMouseLeave={hide}
              onBlur={hide}
              aria-label={
                cell.day
                  ? `${cell.key}: ${cell.day.sessions} sessions, ${cell.day.tokens} tokens`
                  : `${cell.key}: no activity`
              }
              className={[
                'relative h-[var(--heat-cell)] w-[var(--heat-cell)] appearance-none rounded-[5px] border-0 p-0',
                // hover:z-10 lifts the grown cell above its neighbors so scaling
                // up doesn't get clipped by later-painted cells.
                'transition-transform duration-[var(--motion-fast)] hover:z-10 hover:scale-110',
                LEVEL_CLASS[cell.day?.level ?? 0],
                cell.inStreak ? 'shadow-[inset_0_0_0_2px_var(--text-default)]' : '',
              ].join(' ')}
            />
          ))}
        </div>
      </div>

      <div className="mt-4 flex items-center justify-between text-[11px] text-text-muted">
        <div className="flex items-center gap-1">
          <span className="mr-1">Less</span>
          {[0, 1, 2, 3, 4].map((l) => (
            <i key={l} className={`block h-3 w-3 rounded-[3px] ${LEVEL_CLASS[l]}`} />
          ))}
          <span className="ml-1">More</span>
        </div>
        <span className="tabular-nums">
          {compact.format(activity.maxTokens)} tokens on your busiest day
        </span>
      </div>

      {hovered &&
        createPortal(<Tooltip cell={hovered.cell} anchor={hovered.rect} />, document.body)}
    </div>
  );
}
