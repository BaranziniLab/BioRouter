import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { LintResult } from '../../../api/types.gen';
import { LintDrawer } from './LintDrawer';

const mocks = vi.hoisted(() => ({
  start: vi.fn(),
  reset: vi.fn(),
  knowledge: {
    primaryKbId: 'kb-1' as string | null,
    primaryKb: { id: 'kb-1', name: 'Notes', default_model: null } as {
      id: string;
      name: string;
      default_model: { provider: string; model: string } | null;
    } | null,
  },
  stream: {
    events: [] as unknown[],
    status: 'done' as 'idle' | 'starting' | 'streaming' | 'done' | 'error',
    finalResult: null as unknown,
    error: undefined as string | undefined,
  },
}));

vi.mock('../KnowledgeContext', () => ({ useKnowledge: () => mocks.knowledge }));

vi.mock('../../ModelAndProviderContext', () => ({
  useModelAndProvider: () => ({ currentProvider: 'anthropic', currentModel: 'claude' }),
}));

vi.mock('../hooks/useIngestStream', () => ({
  useIngestStream: () => ({
    ...mocks.stream,
    start: mocks.start,
    startMultipart: vi.fn(),
    abort: vi.fn(),
    reset: mocks.reset,
  }),
}));

/** A report with one of each severity, plus a capped total and one hygiene list. */
const REPORT: LintResult = {
  fixes_applied: 0,
  report: {
    contradictions: [],
    orphans: ['knowledge/lonely.md'],
    missing_concept_pages: [],
    stale_sources: [],
    diagnostics: {
      total: 9,
      items: [
        {
          rule: 'okf.type.unknown',
          severity: 'error',
          subject: 'Metformin',
          message: 'type "Drugg" is not in the vocabulary',
          path: 'knowledge/metformin.md',
        },
        {
          rule: 'biookf.predicate.unknown',
          severity: 'warning',
          subject: 'Metformin -tretas-> Diabetes',
          message: 'predicate "tretas" is not in the BioOKF vocabulary',
        },
        {
          rule: 'kb.orphan',
          severity: 'info',
          subject: 'knowledge/lonely.md',
          message: 'nothing links to this page',
        },
      ],
    },
  },
};

beforeEach(() => {
  vi.clearAllMocks();
  mocks.knowledge = {
    primaryKbId: 'kb-1',
    primaryKb: { id: 'kb-1', name: 'Notes', default_model: null },
  };
  mocks.stream = { events: [], status: 'done', finalResult: REPORT, error: undefined };
});

describe('LintDrawer', () => {
  it('runs the check on open, against the base and the resolved model', () => {
    render(<LintDrawer open onOpenChange={() => undefined} />);

    expect(mocks.start).toHaveBeenCalledTimes(1);
    expect(mocks.start).toHaveBeenCalledWith('/knowledge/bases/kb-1/lint', {
      model: { provider: 'anthropic', model: 'claude' },
    });
  });

  it('never asks for an autofix — the check reports, it does not rewrite', () => {
    render(<LintDrawer open onOpenChange={() => undefined} />);
    const body = mocks.start.mock.calls[0][1] as Record<string, unknown>;
    expect(body).not.toHaveProperty('autofix');
  });

  it('does not run while the drawer is closed', () => {
    render(<LintDrawer open={false} onOpenChange={() => undefined} />);
    expect(mocks.start).not.toHaveBeenCalled();
  });

  it('groups the diagnostics by severity', () => {
    render(<LintDrawer open onOpenChange={() => undefined} />);

    for (const [group, subject] of [
      ['Errors', 'Metformin'],
      ['Warnings', 'Metformin -tretas-> Diabetes'],
      ['Notes', 'knowledge/lonely.md'],
    ] as const) {
      expect(
        within(screen.getByRole('region', { name: group })).getByText(subject)
      ).toBeInTheDocument();
    }
  });

  it('shows the stable rule id beside every finding, not just the prose', () => {
    render(<LintDrawer open onOpenChange={() => undefined} />);
    for (const rule of ['okf.type.unknown', 'biookf.predicate.unknown', 'kb.orphan']) {
      expect(screen.getByText(rule)).toBeInTheDocument();
    }
  });

  it('reports the count BEFORE the cap, and says the list is capped', () => {
    render(<LintDrawer open onOpenChange={() => undefined} />);
    // 9 were raised; 3 came back. A surface that reported 3 would be telling the
    // user their base is fine when two thirds of the findings never arrived.
    expect(screen.getByTestId('knowledge-lint-count')).toHaveTextContent('9 findings');
    expect(screen.getByText(/first 3 shown/i)).toBeInTheDocument();
  });

  it('surfaces the hygiene lists the report carries beside the diagnostics', () => {
    render(<LintDrawer open onOpenChange={() => undefined} />);
    expect(
      within(screen.getByRole('region', { name: 'Orphans' })).getByText('knowledge/lonely.md')
    ).toBeInTheDocument();
  });

  it('says so plainly when there is nothing to fix', () => {
    mocks.stream.finalResult = {
      fixes_applied: 0,
      report: {
        contradictions: [],
        orphans: [],
        missing_concept_pages: [],
        stale_sources: [],
        diagnostics: { total: 0, items: [] },
      },
    } satisfies LintResult;
    render(<LintDrawer open onOpenChange={() => undefined} />);
    expect(screen.getByRole('heading', { name: 'Nothing to fix' })).toBeInTheDocument();
  });

  it('reports a failed stream as a failure rather than an empty base', () => {
    mocks.stream = {
      events: [],
      status: 'error',
      finalResult: null,
      error: 'HTTP 400: invalid model',
    };
    render(<LintDrawer open onOpenChange={() => undefined} />);
    expect(screen.getByRole('heading', { name: 'The check did not finish' })).toBeInTheDocument();
    expect(screen.getByText('HTTP 400: invalid model')).toBeInTheDocument();
    // A stream that died leaves NO report on screen to be mistaken for one.
    expect(screen.queryByTestId('knowledge-lint-diagnostic')).toBeNull();
  });

  it('dispatches nothing when there is no base to check', async () => {
    mocks.stream = { events: [], status: 'idle', finalResult: null, error: undefined };
    mocks.knowledge = { primaryKbId: null, primaryKb: null };
    render(<LintDrawer open onOpenChange={() => undefined} />);

    expect(mocks.start).not.toHaveBeenCalled();
    // And the control cannot be used to get there either — an agentic loop
    // dispatched at `/knowledge/bases/null/lint` is a request nothing answers.
    const run = screen.getByTestId('knowledge-lint-run');
    expect(run).toBeDisabled();
    await userEvent.click(run);
    expect(mocks.start).not.toHaveBeenCalled();
  });
});
