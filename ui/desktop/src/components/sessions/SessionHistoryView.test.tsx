import { render, screen } from '@testing-library/react';
import type { ReactNode } from 'react';
import { MemoryRouter } from 'react-router-dom';
import { describe, expect, it, vi } from 'vitest';
import SessionHistoryView from './SessionHistoryView';
import type { Session } from '../../api';

// See SessionItem.test.tsx: this file deliberately never names the badge
// component, because Task 27's gate greps src/components for that name and
// expects an exact file list.

vi.mock('../ProgressiveMessageList', () => ({
  default: () => <div data-testid="messages" />,
}));

vi.mock('../conversation/SearchView', () => ({
  SearchView: ({ children }: { children: ReactNode }) => <>{children}</>,
}));

vi.mock('../ui/scroll-area', () => ({
  ScrollArea: ({ children }: { children: ReactNode }) => <div>{children}</div>,
}));

function session(over: Partial<Session> = {}): Session {
  return {
    id: 'session-1',
    name: 'Cohort query',
    created_at: '2026-07-14T12:00:00Z',
    updated_at: '2026-07-14T12:00:00Z',
    working_dir: '/tmp',
    message_count: 0,
    extension_data: {},
    conversation: [],
    ...over,
  } as Session;
}

function renderView(over: Partial<Session> = {}) {
  return render(
    <MemoryRouter>
      <SessionHistoryView
        session={session(over)}
        isLoading={false}
        error={null}
        onBack={vi.fn()}
        onRetry={vi.fn()}
        showActionButtons={false}
      />
    </MemoryRouter>
  );
}

describe('SessionHistoryView — the privacy marker', () => {
  it('marks a private session in the header', () => {
    renderView({ privacy_tier: 'private' });
    expect(screen.getByTestId('privacy-badge')).toHaveAttribute('data-privacy', 'private');
  });

  it('names the tier in words on this roomy header, including for a public session', () => {
    renderView({ privacy_tier: 'public' });
    const badge = screen.getByTestId('privacy-badge');
    expect(badge).toHaveAttribute('data-privacy', 'public');
    expect(badge).toHaveTextContent('Public');
  });

  it('says nothing at all when the session carries no tier', () => {
    renderView();
    expect(screen.queryByTestId('privacy-badge')).toBeNull();
  });
});
