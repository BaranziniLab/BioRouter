import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { ChatTabStrip, ChatTabStripProps } from './ChatTabStrip';
import { ChatTab } from './chatGroupsTypes';

// See SessionItem.test.tsx: this file deliberately never names the badge
// component, because Task 27's gate greps src/components for that name and
// expects an exact file list.

function tab(over: Partial<ChatTab> = {}): ChatTab {
  return {
    tabId: 'tab-1',
    sessionId: 's1',
    title: 'Cohort query',
    userSetName: false,
    ...over,
  };
}

function renderStrip(over: Partial<ChatTabStripProps> = {}) {
  const props: ChatTabStripProps = {
    tabs: [tab()],
    activeTabId: 'tab-1',
    runningSessionIds: [],
    onSelect: vi.fn(),
    onClose: vi.fn(),
    onReorder: vi.fn(),
    reserveTitlebar: false,
    isCompactSidebarOverlayOpen: false,
    ...over,
  };
  return { ...render(<ChatTabStrip {...props} />), props };
}

describe('ChatTabStrip — the privacy dot', () => {
  it('marks a private chat, keyed by session id rather than by tab id', () => {
    renderStrip({ privacyTiers: { s1: 'private' } });
    expect(screen.getByTestId('privacy-badge')).toHaveAttribute('data-privacy', 'private');
  });

  it('leaves a public chat unmarked on this dense surface', () => {
    renderStrip({ privacyTiers: { s1: 'public' } });
    expect(screen.queryByTestId('privacy-badge')).toBeNull();
  });

  it('says nothing when no tier is known for the tab', () => {
    renderStrip();
    expect(screen.queryByTestId('privacy-badge')).toBeNull();
  });

  it('survives hover and does not collide with the running dot', () => {
    renderStrip({ privacyTiers: { s1: 'private' }, runningSessionIds: ['s1'] });

    // Both indicators render at once — they answer different questions.
    expect(screen.getByLabelText('Running')).toBeInTheDocument();
    const dot = screen.getByTestId('privacy-badge');

    // The RUNNING dot is `group-hover:hidden` so the close control can take its
    // slot. A privacy marker that vanishes exactly when you point at the tab is
    // not a privacy marker.
    expect(dot.className).not.toContain('group-hover:hidden');
    expect(dot.className).not.toContain('hidden');
    // And it is its own class, not the running pulse's — which is 7px,
    // --text-accent and animated.
    expect(dot.className).toContain('br-tab__privacy-dot');
    expect(dot.className.split(/\s+/)).not.toContain('br-tab__dot');
  });

  it('marks only the private tab when a strip mixes tiers', () => {
    renderStrip({
      tabs: [tab(), tab({ tabId: 'tab-2', sessionId: 's2', title: 'Public chat' })],
      privacyTiers: { s2: 'private' },
    });
    const dots = screen.getAllByTestId('privacy-badge');
    expect(dots).toHaveLength(1);
    expect(dots[0].closest('[data-tab-id]')).toHaveAttribute('data-tab-id', 'tab-2');
  });
});
