import React from 'react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';

// #39 — the pre-session working-directory wiring contract. With no sessionId,
// a DirSwitcher change has nothing to persist to; the ONLY thing keeping the
// user's choice alive is ChatInput forwarding it through its
// onWorkingDirChange prop (Hub threads it into createSession; BaseChat keeps
// it as pendingWorkingDir for the pre-session createSession). This test pins
// that contract at the ChatInput level.

// Heavy or context-hungry children that are irrelevant to the wiring under
// test. DirSwitcher itself stays REAL — its no-session branch is the contract.
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
  default: () => null,
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
  getSession: vi.fn(async () => ({ data: null })),
  llamacppStatus: vi.fn(async () => ({ data: {} })),
  updateWorkingDir: vi.fn(async () => ({ data: {} })),
}));

import ChatInput from './ChatInput';
import { ChatState } from '../types/chatState';
import { updateWorkingDir } from '../api';

const DEFAULT_DIR = '/default/workdir';
const CHOSEN_DIR = '/Users/wgu/Desktop/data';

beforeEach(() => {
  vi.clearAllMocks();
  Object.assign(window, {
    appConfig: {
      get: (key: string) => (key === 'BIOROUTER_WORKING_DIR' ? DEFAULT_DIR : undefined),
    },
    electron: {
      directoryChooser: vi.fn(async () => ({ canceled: false, filePaths: [CHOSEN_DIR] })),
      addRecentDir: vi.fn(),
      logInfo: vi.fn(),
      getPathForFile: vi.fn(() => ''),
      on: vi.fn(),
      off: vi.fn(),
    },
  });
});

function renderChatInput(
  onWorkingDirChange: (dir: string) => void,
  { sessionId = null as string | null, messagesLength = 0 } = {}
) {
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
      messagesLength={messagesLength}
      disableAnimation={false}
      toolCount={0}
      onWorkingDirChange={onWorkingDirChange}
    />
  );
}

describe('ChatInput pre-session working-directory wiring (#39)', () => {
  it('forwards a DirSwitcher change to onWorkingDirChange when there is no session', async () => {
    const onWorkingDirChange = vi.fn();
    renderChatInput(onWorkingDirChange);

    // The folder chip shows the app default before any choice is made.
    const chip = screen.getByText(DEFAULT_DIR);
    fireEvent.click(chip.closest('button')!);

    await waitFor(() => expect(onWorkingDirChange).toHaveBeenCalledWith(CHOSEN_DIR));

    // No session — nothing may be persisted server-side yet.
    expect(updateWorkingDir).not.toHaveBeenCalled();
    // The chip reflects the choice locally (ChatInput's own state).
    await waitFor(() => expect(screen.getByText(CHOSEN_DIR)).toBeInTheDocument());
  });
});

// #44 — once the chat has messages, ChatInput must render the folder chip
// locked: a read-only basename label (full path on hover) with no chooser.
describe('ChatInput working-directory lock once the chat has messages (#44)', () => {
  it('renders the chip as a read-only basename label when messagesLength > 0', async () => {
    renderChatInput(vi.fn(), { sessionId: 'session-1', messagesLength: 2 });

    // Basename of the app-default dir ('/default/workdir' -> 'workdir'),
    // never the interactive full-path chooser button.
    const label = await screen.findByTestId('dir-switcher-locked');
    expect(label).toHaveTextContent('workdir');
    expect(screen.queryByText(DEFAULT_DIR)).not.toBeInTheDocument();

    fireEvent.click(label);
    expect(window.electron.directoryChooser).not.toHaveBeenCalled();
    expect(updateWorkingDir).not.toHaveBeenCalled();
  });

  it('keeps the chip interactive for a session with zero messages', () => {
    renderChatInput(vi.fn(), { sessionId: 'session-1', messagesLength: 0 });

    const chip = screen.getByText(DEFAULT_DIR).closest('button');
    expect(chip).not.toBeNull();
    expect(screen.queryByTestId('dir-switcher-locked')).not.toBeInTheDocument();
  });
});
