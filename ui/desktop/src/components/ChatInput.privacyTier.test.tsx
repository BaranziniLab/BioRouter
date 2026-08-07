import React from 'react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, act } from '@testing-library/react';

/**
 * #56 — the composer's session privacy tier must describe THIS chat, and the
 * most recently *issued* read of it.
 *
 * Both consumers of `sessionPrivacyTier` treat `private` as a licence to
 * restrict: `ModelsBottomBar` paints a Private dot and `SwitchModelModal` greys
 * out every public model. A tier left over from a previous chat is therefore
 * not a cosmetic lag — it is a false claim in the direction that both walls a
 * working model and asserts a confidentiality guarantee the chat does not have.
 *
 * Two distinct failure modes, one effect:
 *   1. rebinding to another chat, where the old tier survives until (and, on a
 *      failed read, past) the new one resolves;
 *   2. two overlapping reads for the SAME chat — the initial fetch and the
 *      `message-stream-finished` refresh — where the slower one wins because it
 *      landed last.
 */

// Heavy children that are irrelevant here. The two tier consumers are replaced
// by probes so the test asserts on what the composer actually hands down,
// rather than on a badge's rendering.
vi.mock('./ConfigContext', () => ({
  useConfig: () => ({
    getProviders: vi.fn(async () => []),
    read: vi.fn(async () => null),
  }),
}));
vi.mock('./ModelAndProviderContext', () => ({
  useModelAndProvider: () => ({
    getCurrentModelAndProvider: vi.fn(async () => ({ model: null, provider: null })),
    currentModel: null,
    currentProvider: null,
    currentModelSupportsVision: false,
    currentModelSupportedInputMimeTypes: null,
  }),
}));
vi.mock('../hooks/useDiverge', () => ({
  useDiverge: () => ({ diverge: vi.fn() }),
}));
vi.mock('./settings/models/bottom_bar/ModelsBottomBar', () => ({
  default: ({ privacyTier }: { privacyTier?: string }) => (
    <div data-testid="tier-probe">{privacyTier ?? 'unresolved'}</div>
  ),
}));
vi.mock('./bottom_menu/BottomMenuExtensionSelection', () => ({
  BottomMenuExtensionSelection: () => null,
}));
vi.mock('./bottom_menu/BottomMenuSkillSelection', () => ({
  BottomMenuSkillSelection: () => null,
}));
vi.mock('./bottom_menu/BottomMenuKnowledgeSelection', () => ({
  BottomMenuKnowledgeSelection: () => null,
}));
vi.mock('./bottom_menu/BottomMenuReasoningEffort', () => ({
  BottomMenuReasoningEffort: () => null,
}));
vi.mock('./bottom_menu/CostTracker', () => ({
  CostTracker: () => null,
}));
vi.mock('./MessageQueue', () => ({
  default: () => null,
}));
vi.mock('./MentionPopover', () => {
  const MentionPopoverMock = React.forwardRef(() => null);
  MentionPopoverMock.displayName = 'MentionPopoverMock';
  return { default: MentionPopoverMock };
});
vi.mock('../api', () => ({
  getSession: vi.fn(),
  llamacppStatus: vi.fn(async () => ({ data: {} })),
  updateWorkingDir: vi.fn(async () => ({ data: {} })),
}));

import ChatInput from './ChatInput';
import { ChatState } from '../types/chatState';
import { getSession } from '../api';

const getSessionMock = vi.mocked(getSession);

beforeEach(() => {
  vi.clearAllMocks();
  Object.assign(window, {
    appConfig: { get: () => '/default/workdir' },
    electron: {
      directoryChooser: vi.fn(),
      addRecentDir: vi.fn(),
      logInfo: vi.fn(),
      getPathForFile: vi.fn(() => ''),
      on: vi.fn(),
      off: vi.fn(),
    },
  });
});

function renderChatInput(sessionId: string | null) {
  return render(
    <ChatInput
      sessionId={sessionId}
      handleSubmit={vi.fn()}
      chatState={ChatState.Idle}
      onStop={vi.fn()}
      initialValue=""
      setView={vi.fn()}
      totalTokens={0}
      accumulatedInputTokens={0}
      accumulatedOutputTokens={0}
      droppedFiles={[]}
      onFilesProcessed={vi.fn()}
      messagesLength={0}
      disableAnimation={false}
      toolCount={0}
      onWorkingDirChange={vi.fn()}
    />
  );
}

// A promise whose resolution this test controls, so the order responses LAND
// can be made to differ from the order the requests were ISSUED.
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

describe('ChatInput session privacy tier (#56)', () => {
  it('drops the previous chat`s tier the moment the session rebinds', async () => {
    // BaseChat is keyed by TAB, not by session, so a tab that navigates from a
    // private chat to another chat re-renders this same ChatInput instance with
    // a new id. The old tier must not survive that.
    const second = deferred<{ data: { privacy_tier: string } }>();
    getSessionMock
      .mockResolvedValueOnce({ data: { privacy_tier: 'private' } } as never)
      .mockResolvedValueOnce({ data: { privacy_tier: 'private' } } as never)
      .mockReturnValue(second.promise as never);

    const { rerender } = renderChatInput('chat-a');
    await waitFor(() => expect(screen.getByTestId('tier-probe')).toHaveTextContent('private'));

    rerender(
      <ChatInput
        sessionId="chat-b"
        handleSubmit={vi.fn()}
        chatState={ChatState.Idle}
        onStop={vi.fn()}
        initialValue=""
        setView={vi.fn()}
        totalTokens={0}
        accumulatedInputTokens={0}
        accumulatedOutputTokens={0}
        droppedFiles={[]}
        onFilesProcessed={vi.fn()}
        messagesLength={0}
        disableAnimation={false}
        toolCount={0}
        onWorkingDirChange={vi.fn()}
      />
    );

    // chat-b's read has not landed (and may never land, if it throws). Until it
    // does, the composer knows nothing about chat-b — and "nothing" is the only
    // honest answer. Rendering chat-a's `private` here is what greys out every
    // public model in a chat that permits them.
    expect(screen.getByTestId('tier-probe')).toHaveTextContent('unresolved');

    await act(async () => {
      second.resolve({ data: { privacy_tier: 'public' } });
      await second.promise;
    });
    await waitFor(() => expect(screen.getByTestId('tier-probe')).toHaveTextContent('public'));
  });

  it('keeps the newest issued read when an older one lands after it', async () => {
    // The mount fetch and the `message-stream-finished` refresh share no
    // ordering. Without a generation check the LAST TO LAND wins, so a slow
    // first read can overwrite the fresher answer that already arrived.
    const slowFirst = deferred<{ data: { privacy_tier: string } }>();
    getSessionMock
      // The working-directory effect's own read — settled, and irrelevant.
      .mockResolvedValueOnce({ data: { working_dir: '/w' } } as never)
      .mockReturnValueOnce(slowFirst.promise as never)
      .mockResolvedValue({ data: { privacy_tier: 'public' } } as never);

    renderChatInput('chat-a');
    await waitFor(() => expect(getSessionMock).toHaveBeenCalled());

    // The refresh is issued second and lands first.
    await act(async () => {
      window.dispatchEvent(new Event('message-stream-finished'));
      await Promise.resolve();
    });
    await waitFor(() => expect(screen.getByTestId('tier-probe')).toHaveTextContent('public'));

    // The mount read finally lands, carrying the OLDER answer.
    await act(async () => {
      slowFirst.resolve({ data: { privacy_tier: 'private' } });
      await slowFirst.promise;
    });

    expect(screen.getByTestId('tier-probe')).toHaveTextContent('public');
  });
});
