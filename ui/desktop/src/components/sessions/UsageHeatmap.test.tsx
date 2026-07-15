import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { ActivityWindow } from '../../api';
import { UsageHeatmap, UsageHeatmapLoading } from './UsageHeatmap';

function windowOf(overrides: Partial<ActivityWindow> = {}): ActivityWindow {
  return {
    start: '2026-03-01', // a Sunday
    end: '2026-03-14', // a Saturday
    maxSessions: 3,
    maxTokens: 128402,
    tokensComplete: true,
    currentStreak: 0,
    longestStreak: 0,
    days: [],
    ...overrides,
  };
}

const day = (date: string, level: number, sessions = 1, tokens = 1000, tokensComplete = true) => ({
  date,
  sessions,
  tokens,
  tokensComplete,
  inputTokens: 0,
  outputTokens: 0,
  messages: 4,
  level,
});

describe('UsageHeatmap', () => {
  it('does not repeat the hover explanation above the grid', () => {
    render(<UsageHeatmap window={windowOf()} />);
    expect(screen.queryByText(/Daily usage intensity/)).not.toBeInTheDocument();
  });

  it('renders whole weeks, padded to Sunday..Saturday', () => {
    render(<UsageHeatmap window={windowOf()} />);
    const cells = screen.getAllByRole('button');
    // Mar 1 2026 is a Sunday and Mar 14 a Saturday: exactly two full weeks.
    expect(cells).toHaveLength(14);
    expect(cells.length % 7).toBe(0);
  });

  it('pads a window that does not start on a Sunday', () => {
    // Mar 4 is a Wednesday; Mar 11 a Wednesday. The grid must still be whole weeks.
    render(<UsageHeatmap window={windowOf({ start: '2026-03-04', end: '2026-03-11' })} />);
    expect(screen.getAllByRole('button').length % 7).toBe(0);
  });

  it('omitted days render as level 0, present days keep their level', () => {
    render(
      <UsageHeatmap window={windowOf({ days: [day('2026-03-02', 4), day('2026-03-05', 1)] })} />
    );
    const busy = screen.getByLabelText(/2026-03-02: 1 sessions, 1000 tokens/);
    expect(busy.className).toContain('bg-heat-4');
    const quiet = screen.getByLabelText(/2026-03-05: 1 sessions, 1000 tokens/);
    expect(quiet.className).toContain('bg-heat-1');
    const idle = screen.getByLabelText('2026-03-03: no activity');
    expect(idle.className).toContain('bg-heat-0');
  });

  it('opens the tooltip on hover with the real numbers', () => {
    render(
      <UsageHeatmap
        window={windowOf({ days: [day('2026-03-02', 3, 3, 128402)], currentStreak: 0 })}
      />
    );
    expect(screen.queryByRole('tooltip')).toBeNull();

    fireEvent.mouseEnter(screen.getByLabelText(/2026-03-02/));
    const tip = screen.getByRole('tooltip');
    expect(tip).toHaveTextContent('Mar 2, 2026');
    expect(tip).toHaveTextContent('128,402');
    expect(tip).toHaveTextContent('Sessions started');

    fireEvent.mouseLeave(screen.getByLabelText(/2026-03-02/));
    expect(screen.queryByRole('tooltip')).toBeNull();
  });

  it('marks activity token counts as lower bounds when the server reports incomplete history', () => {
    render(
      <UsageHeatmap
        window={windowOf({
          tokensComplete: false,
          days: [day('2026-03-02', 3, 3, 128402, false)],
        })}
      />
    );

    expect(screen.getByRole('status')).toHaveTextContent('conservative estimates');
    expect(screen.getByText(/Highest recorded estimate/)).toHaveTextContent(
      'Highest recorded estimate · 128.4K tokens'
    );
    const cell = screen.getByLabelText(/2026-03-02: 3 sessions, 128402 estimated tokens/);
    fireEvent.mouseEnter(cell);
    expect(screen.getByRole('tooltip')).toHaveTextContent('128,402');
    expect(screen.getByRole('tooltip')).toHaveTextContent('Conservative estimate');
    expect(screen.getByRole('tooltip')).not.toHaveTextContent('≥');
  });

  it('shows unavailable instead of a meaningless lower bound of zero', () => {
    render(
      <UsageHeatmap
        window={windowOf({
          tokensComplete: false,
          maxTokens: 0,
          days: [day('2026-03-02', 3, 3, 0, false)],
        })}
      />
    );

    expect(screen.getByText('Token totals unavailable for older activity')).toBeInTheDocument();
    const cell = screen.getByLabelText(/2026-03-02: 3 sessions, token total unavailable/);
    fireEvent.mouseEnter(cell);
    expect(screen.getByRole('tooltip')).toHaveTextContent('Tokens processedUnavailable');
    expect(screen.getByRole('tooltip')).toHaveTextContent('exact total cannot be shown');
  });

  it('keeps exact days exact when another day makes the window incomplete', () => {
    render(
      <UsageHeatmap
        window={windowOf({
          tokensComplete: false,
          days: [day('2026-03-02', 3, 3, 42000, true)],
        })}
      />
    );

    const cell = screen.getByLabelText(/2026-03-02: 3 sessions, 42000 tokens/);
    fireEvent.mouseEnter(cell);
    expect(screen.getByRole('tooltip')).toHaveTextContent('42,000');
    expect(screen.getByRole('tooltip')).not.toHaveTextContent('Conservative estimate');
  });

  it('opens the tooltip on keyboard focus, not hover alone', () => {
    // The tooltip is the only way to read a cell's numbers, so a keyboard user
    // must be able to reach it.
    render(<UsageHeatmap window={windowOf({ days: [day('2026-03-02', 2)] })} />);
    fireEvent.focus(screen.getByLabelText(/2026-03-02/));
    expect(screen.getByRole('tooltip')).toHaveTextContent('Mar 2, 2026');
  });

  it('says "no activity" for an idle day', () => {
    render(<UsageHeatmap window={windowOf()} />);
    fireEvent.mouseEnter(screen.getByLabelText('2026-03-03: no activity'));
    expect(screen.getByRole('tooltip')).toHaveTextContent('No activity');
  });

  it('marks exactly the current streak', () => {
    render(
      <UsageHeatmap
        window={windowOf({
          days: [day('2026-03-12', 2), day('2026-03-13', 2), day('2026-03-14', 2)],
          currentStreak: 3,
          longestStreak: 5,
        })}
      />
    );
    const outlined = screen
      .getAllByRole('button')
      .filter((b) => b.className.includes('shadow-[inset_0_0_0_2px_var(--text-default)]'));
    expect(outlined).toHaveLength(3);
    for (const cell of outlined) {
      expect(cell).toHaveAccessibleName(/part of current streak/);
    }
    expect(screen.getByText('Current streak')).toBeInTheDocument();
    expect(screen.getByText('3 day streak')).toBeInTheDocument();
    expect(screen.getByText(/Longest streak · 5 days/)).toBeInTheDocument();

    fireEvent.mouseEnter(outlined[0]);
    expect(screen.getByRole('tooltip')).toHaveTextContent('Part of your current streak');
  });

  it('a streak that ended yesterday still highlights the right cells', () => {
    // The server reports currentStreak counting back from `end`; if the user has
    // not opened the app today, the run ends on `end - 1`.
    render(
      <UsageHeatmap
        window={windowOf({ days: [day('2026-03-12', 2), day('2026-03-13', 2)], currentStreak: 2 })}
      />
    );
    const outlined = screen
      .getAllByRole('button')
      .filter((b) => b.className.includes('inset_0_0_0_2px'));
    expect(outlined.map((b) => b.getAttribute('aria-label'))).toEqual([
      expect.stringContaining('2026-03-12'),
      expect.stringContaining('2026-03-13'),
    ]);
  });

  it('singularises the streak header', () => {
    render(<UsageHeatmap window={windowOf({ currentStreak: 1, longestStreak: 1 })} />);
    expect(screen.getByText('1 day streak')).toBeInTheDocument();
    expect(screen.getByText(/Longest streak · 1 day$/)).toBeInTheDocument();
  });
});

describe('UsageHeatmapLoading', () => {
  it('fills the loading state with an animated heatmap-shaped placeholder', () => {
    render(<UsageHeatmapLoading />);

    expect(screen.getByRole('status', { name: 'Loading usage activity' })).toBeInTheDocument();
    expect(screen.getAllByTestId('heatmap-loading-cell')).toHaveLength(154);
    expect(screen.getByText('Loading activity')).toBeInTheDocument();
  });
});
