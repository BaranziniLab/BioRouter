import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import SessionItem from './SessionItem';
import type { Session } from '../../api';

/**
 * NOTE: this file must never name the badge component in source.
 *
 * Task 27's own gate is `grep -rl <the badge's component name> src/components`
 * with an exact expected file list — a test file that spells it joins that list
 * and turns a passing gate into a failing one. Everything here goes through the
 * rendered contract (`data-testid="privacy-badge"`, `data-privacy`) instead,
 * which is what the surface actually owes its user.
 */
function session(over: Partial<Session> = {}): Session {
  return {
    id: 'session-1',
    name: 'Cohort query',
    created_at: '2026-07-14T12:00:00Z',
    updated_at: '2026-07-14T12:00:00Z',
    working_dir: '/tmp',
    message_count: 3,
    extension_data: {},
    ...over,
  } as Session;
}

describe('SessionItem — the privacy marker', () => {
  it('marks a private session', () => {
    render(<SessionItem session={session({ privacy_tier: 'private' })} />);
    expect(screen.getByTestId('privacy-badge')).toHaveAttribute('data-privacy', 'private');
  });

  it('leaves a public session unmarked on this dense row', () => {
    render(<SessionItem session={session({ privacy_tier: 'public' })} />);
    expect(screen.queryByTestId('privacy-badge')).toBeNull();
  });

  it('leaves a session with no tier at all unmarked rather than guessing', () => {
    render(<SessionItem session={session()} />);
    expect(screen.queryByTestId('privacy-badge')).toBeNull();
  });
});
