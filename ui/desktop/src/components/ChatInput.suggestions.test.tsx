import React from 'react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, act } from '@testing-library/react';

// The landing suggestion chips write into the composer through the
// `insert-chat-input` channel rather than through `initialValue`. That choice is
// load-bearing and invisible from either side on its own — this file is where
// the two halves meet.

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
vi.mock('../hooks/useDiverge', () => ({ useDiverge: () => ({ diverge: vi.fn() }) }));
vi.mock('./settings/models/bottom_bar/ModelsBottomBar', () => ({ default: () => null }));
vi.mock('./bottom_menu/BottomMenuExtensionSelection', () => ({
  BottomMenuExtensionSelection: () => null,
}));
vi.mock('./bottom_menu/BottomMenuSkillSelection', () => ({ BottomMenuSkillSelection: () => null }));
vi.mock('./bottom_menu/BottomMenuKnowledgeSelection', () => ({
  BottomMenuKnowledgeSelection: () => null,
}));
vi.mock('./bottom_menu/BottomMenuReasoningEffort', () => ({
  BottomMenuReasoningEffort: () => null,
}));
vi.mock('./bottom_menu/CostTracker', () => ({ CostTracker: () => null }));
vi.mock('./MessageQueue', () => ({ default: () => null }));
vi.mock('./MentionPopover', () => {
  const MentionPopoverMock = React.forwardRef(() => null);
  MentionPopoverMock.displayName = 'MentionPopoverMock';
  return { default: MentionPopoverMock };
});
vi.mock('../api', () => ({
  getSession: vi.fn(async () => ({ data: null })),
  llamacppStatus: vi.fn(async () => ({ data: {} })),
  updateWorkingDir: vi.fn(async () => ({ data: {} })),
}));

import ChatInput from './ChatInput';
import { ChatState } from '../types/chatState';

beforeEach(() => {
  vi.clearAllMocks();
  Object.assign(window, {
    appConfig: { get: () => '/workdir' },
    electron: {
      directoryChooser: vi.fn(),
      addRecentDir: vi.fn(),
      logInfo: vi.fn(),
      getPathForFile: vi.fn(() => ''),
      on: vi.fn(),
      off: vi.fn(),
      deleteTempFile: vi.fn(),
    },
  });
});

function renderChatInput(sessionId: string | null, handleSubmit = vi.fn()) {
  return render(
    <ChatInput
      sessionId={sessionId}
      handleSubmit={handleSubmit}
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
    />
  );
}

function insert(sessionId: string | null, value: string) {
  act(() => {
    window.dispatchEvent(new CustomEvent('insert-chat-input', { detail: { sessionId, value } }));
  });
}

describe('ChatInput suggestion insert channel', () => {
  it('fills the textarea without submitting', async () => {
    const handleSubmit = vi.fn();
    renderChatInput(null, handleSubmit);

    insert(null, 'Look at the data files in my working directory.');

    const textarea = await screen.findByTestId('chat-input');
    await waitFor(() =>
      expect(textarea).toHaveValue('Look at the data files in my working directory.')
    );
    // The user still gets to edit it. A chip that sent the turn would spend a
    // model call on a prompt nobody meant literally.
    expect(handleSubmit).not.toHaveBeenCalled();
  });

  it('ignores an insert addressed to a different session', async () => {
    renderChatInput('session-a');

    insert('session-b', 'not for this chat');

    const textarea = await screen.findByTestId('chat-input');
    // Two chats can be open side by side; a window-level broadcast must not land
    // in the one the user is not looking at.
    expect(textarea).toHaveValue('');
  });

  it('matches the pre-session composer on an explicit null', async () => {
    renderChatInput(null);
    insert(null, 'for the pre-session composer');
    const textarea = await screen.findByTestId('chat-input');
    await waitFor(() => expect(textarea).toHaveValue('for the pre-session composer'));
  });
});
