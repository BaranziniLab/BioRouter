import { render, screen, act } from '@testing-library/react';
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { MemoryRouter, Routes, Route, useNavigate } from 'react-router-dom';
import { ChatGroupsProvider, useChatGroups } from '../../contexts/ChatGroupsContext';
import { createNavigationHandler } from '../../utils/navigationUtils';

/**
 * REGRESSION GATE — a message submitted from the HOME surface reaches the tab.
 *
 * The bug (pre-existing, NOT introduced by the tab-cargo fix f1f1d6b6): Hub's
 * handleSubmit creates the session and then calls
 *
 *   setView('pair', { resumeSessionId, initialMessage, initialAttachments })
 *
 * createNavigationHandler's 'pair' case navigated to the bare path '/pair',
 * putting the whole options object in location.state and NOTHING in the query
 * string. But /pair's deep-link inbox — useChatGroupsUrlSync's IN effect — is
 * gated on searchParams.get('resumeSessionId') and returns early when that param
 * is absent, several lines BEFORE it reads location.state. So onOpen never fired,
 * ChatGroupsContext never dispatched openTab, the tab was born with no cargo, and
 * ChatGroupsShell handed BaseChat initialMessage={undefined}. Session created,
 * blank tab, text silently gone.
 *
 * This drives the REAL chain end to end — the real createNavigationHandler, the
 * real react-router, the real useChatGroupsUrlSync, the real ChatGroupsProvider
 * and the real chatGroupsReducer — and asserts on activeTab.pendingInitialMessage,
 * which is verbatim what ChatGroupsShell:320 passes to BaseChat as initialMessage.
 * duplicateSubmissionWiring.test.tsx covers the remaining hop (cargo -> submit).
 *
 * Only useRunningChats is stubbed: it needs a chat-stream registry provider and
 * feeds nothing but the tab strip's "running" pulse.
 */

vi.mock('../../hooks/chatStreamStore', () => ({
  useRunningChats: () => [],
}));

/** Stands in for Hub: same setView call, same shape, real navigation handler. */
function HomeStub({ sessionId, message }: { sessionId: string; message: string }) {
  const navigate = useNavigate();
  const setView = createNavigationHandler(navigate);
  return (
    <button
      data-testid="home-submit"
      onClick={() =>
        setView('pair', {
          resumeSessionId: sessionId,
          initialMessage: message,
          initialAttachments: [],
        })
      }
    >
      send
    </button>
  );
}

/** Mirrors ChatGroupsShell:320 — the exact value BaseChat receives. */
function PairStub() {
  const groups = useChatGroups();
  return (
    <div
      data-testid="pair"
      data-initial-message={groups?.activeTab?.pendingInitialMessage ?? ''}
      data-session-id={groups?.activeTab?.sessionId ?? ''}
    />
  );
}

function renderApp(sessionId: string, message: string) {
  return render(
    <MemoryRouter initialEntries={['/']}>
      <ChatGroupsProvider>
        <Routes>
          <Route path="/" element={<HomeStub sessionId={sessionId} message={message} />} />
          <Route path="/pair" element={<PairStub />} />
        </Routes>
      </ChatGroupsProvider>
    </MemoryRouter>
  );
}

beforeEach(() => {
  localStorage.clear();
  sessionStorage.clear();
});

describe('a Home-surface submit delivers its message to the chat tab', () => {
  it('lands the typed text on the tab as pendingInitialMessage', () => {
    renderApp('session-home', 'WWMARKER name one animal');

    act(() => {
      screen.getByTestId('home-submit').click();
    });

    const pair = screen.getByTestId('pair');
    expect(pair.getAttribute('data-session-id')).toBe('session-home');
    // The whole bug in one assertion: '' here means the message was dropped.
    expect(pair.getAttribute('data-initial-message')).toBe('WWMARKER name one animal');
  });

  /**
   * Guards the gate itself. If the transport regressed to state-only, the tab
   * would never be created at all — so pin the tab's existence separately from
   * its cargo, and pin that the id genuinely travels in the query string (the
   * only channel useChatGroupsUrlSync reads).
   */
  it('puts resumeSessionId in the query string, not only in location.state', () => {
    const navigate = vi.fn();
    createNavigationHandler(navigate)('pair', {
      resumeSessionId: 's1',
      initialMessage: 'hi',
    });

    expect(navigate).toHaveBeenCalledWith(
      '/pair?resumeSessionId=s1',
      expect.objectContaining({
        state: expect.objectContaining({ initialMessage: 'hi' }),
      })
    );
  });

  it('still navigates to a bare /pair when there is no session to resume', () => {
    const navigate = vi.fn();
    createNavigationHandler(navigate)('pair', {});
    expect(navigate).toHaveBeenCalledWith('/pair', expect.anything());
  });

  it('url-encodes a session id containing url-unsafe characters', () => {
    const navigate = vi.fn();
    createNavigationHandler(navigate)('pair', { resumeSessionId: 'a b&c' });
    expect(navigate).toHaveBeenCalledWith('/pair?resumeSessionId=a%20b%26c', expect.anything());
  });
});
