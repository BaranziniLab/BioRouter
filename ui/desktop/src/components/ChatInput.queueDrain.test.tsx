import React from 'react';
import { describe, it, expect, vi, beforeEach, type Mock } from 'vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';

/**
 * A message typed while a turn is running is QUEUED, and the queue drains when
 * the turn ends. The drain used to fire the submit and forget it: it dequeued
 * whether or not the submit had taken the message, and the store's submit
 * refuses silently while the finishing turn's `submitInFlight` latch is still
 * held (see chatStreamStore.submitVerdict.test.tsx). The user's text vanished
 * with no error, every time.
 *
 * These tests drive the REAL composer and vary one thing: what the submit
 * answers. What they can and cannot show:
 *
 *  - They CAN show ownership of the message across a refusal — how many times
 *    it is offered, whether it comes back to the queue, whether the composer
 *    gets its text back — because that is state this component owns.
 *  - They CANNOT show the ordering that produces the refusal in the real app
 *    (React's effect scheduling against the store's promise chain). The store
 *    test above covers that half, in the store, with no fake ordering.
 *  - jsdom has no layout engine and does not run Tailwind, so nothing here says
 *    anything about how the queue chip or the toast LOOK.
 */

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
vi.mock('./MentionPopover', () => {
  const MentionPopoverMock = React.forwardRef(() => null);
  MentionPopoverMock.displayName = 'MentionPopoverMock';
  return { default: MentionPopoverMock };
});
// The real queue UI is a drag-and-drop list; what these tests need from it is
// only WHICH messages are in the queue, so it stands in as a plain list.
vi.mock('./MessageQueue', () => ({
  default: ({ queuedMessages }: { queuedMessages: Array<{ id: string; content: string }> }) => (
    <ul data-testid="queue">
      {queuedMessages.map((msg) => (
        <li key={msg.id} data-testid="queued-item">
          {msg.content}
        </li>
      ))}
    </ul>
  ),
}));
vi.mock('../api', () => ({
  getSession: vi.fn(async () => ({ data: null })),
  llamacppStatus: vi.fn(async () => ({ data: {} })),
  updateWorkingDir: vi.fn(async () => ({ data: {} })),
}));
vi.mock('../toasts', () => ({
  toastWarning: vi.fn(),
  toastError: vi.fn(),
  toastInfo: vi.fn(),
  toastSuccess: vi.fn(),
  toastLoading: vi.fn(),
}));

import ChatInput from './ChatInput';
import { ChatState } from '../types/chatState';
import { toastWarning } from '../toasts';

const QUEUED_TEXT = 'summarise the second table too';
const DIRECT_TEXT = 'plot the residuals';

/** The composer's own prop signature, so a mock cannot drift from it. */
type SubmitFn = (e: React.FormEvent) => void | Promise<boolean | void>;
type SubmitMock = Mock<SubmitFn>;

beforeEach(() => {
  vi.clearAllMocks();
  Object.assign(window, {
    appConfig: {
      get: (key: string) => (key === 'BIOROUTER_WORKING_DIR' ? '/tmp/workdir' : undefined),
    },
    electron: {
      directoryChooser: vi.fn(async () => ({ canceled: true, filePaths: [] })),
      addRecentDir: vi.fn(),
      logInfo: vi.fn(),
      getPathForFile: vi.fn(() => ''),
      on: vi.fn(),
      off: vi.fn(),
    },
  });
});

const composer = () => screen.getByTestId('chat-input') as HTMLTextAreaElement;
const queued = () => screen.queryAllByTestId('queued-item').map((el) => el.textContent);
const submittedTexts = (handleSubmit: SubmitMock) =>
  handleSubmit.mock.calls.map(
    (call) => (call[0] as unknown as CustomEvent).detail.value as string
  );

/** How many offers actually landed a turn, as opposed to being refused. */
async function acceptedCount(handleSubmit: SubmitMock): Promise<number> {
  const verdicts = await Promise.all(
    handleSubmit.mock.results.map((result) =>
      Promise.resolve(result.value as boolean | undefined)
    )
  );
  return verdicts.filter((verdict) => verdict !== false).length;
}

function renderComposer(handleSubmit: SubmitMock, chatState: ChatState) {
  const props = (state: ChatState) => (
    <ChatInput
      sessionId="session-under-test"
      handleSubmit={handleSubmit}
      chatState={state}
      onStop={vi.fn()}
      initialValue=""
      setView={vi.fn()}
      totalTokens={0}
      accumulatedInputTokens={0}
      accumulatedOutputTokens={0}
      droppedFiles={[]}
      onFilesProcessed={vi.fn()}
      messagesLength={2}
      disableAnimation
      sessionCosts={undefined}
      toolCount={0}
    />
  );
  const view = render(props(chatState));
  return {
    setChatState: (state: ChatState) => view.rerender(props(state)),
  };
}

/** Type a message while the turn is running, so it lands in the queue. */
async function queueOneMessage(text = QUEUED_TEXT) {
  fireEvent.change(composer(), { target: { value: text } });
  fireEvent.submit(composer().closest('form')!);
  await waitFor(() => expect(queued()).toEqual([text]));
}

/** Let every pending macrotask (the bounded re-offer timers) run out. */
async function settle() {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 50));
  });
}

describe('draining the message queue', () => {
  it('sends an accepted message exactly once and clears it from the queue', async () => {
    const handleSubmit = vi.fn<SubmitFn>(async () => true);
    const { setChatState } = renderComposer(handleSubmit, ChatState.Streaming);
    await queueOneMessage();

    setChatState(ChatState.Idle);

    await waitFor(() => expect(handleSubmit).toHaveBeenCalledTimes(1));
    await settle();
    // Exactly one: the re-offer must never turn an accepted message into two
    // turns, which is the failure the store's own re-entrancy latch exists to
    // prevent. "At least one" would not catch that.
    expect(handleSubmit).toHaveBeenCalledTimes(1);
    expect(submittedTexts(handleSubmit)).toEqual([QUEUED_TEXT]);
    expect(queued()).toEqual([]);
  });

  it('keeps a refused message and re-offers it, so it is sent exactly once', async () => {
    // The real shape of the bug: the first offer lands while the finishing
    // turn's submit latch is still held, the next one does not.
    let offers = 0;
    const handleSubmit = vi.fn<SubmitFn>(async () => (offers++ === 0 ? false : true));
    const { setChatState } = renderComposer(handleSubmit, ChatState.Streaming);
    await queueOneMessage();

    setChatState(ChatState.Idle);

    await waitFor(() => expect(handleSubmit).toHaveBeenCalledTimes(2));
    await settle();
    expect(handleSubmit).toHaveBeenCalledTimes(2);
    expect(submittedTexts(handleSubmit)).toEqual([QUEUED_TEXT, QUEUED_TEXT]);
    // Two offers, ONE send: the refused offer sent nothing at all.
    expect(await acceptedCount(handleSubmit)).toBe(1);
    expect(queued()).toEqual([]);
    expect(toastWarning).not.toHaveBeenCalled();
  });

  it('puts a message refused past the retry bound back in the queue and says so', async () => {
    const handleSubmit = vi.fn<SubmitFn>(async () => false);
    const { setChatState } = renderComposer(handleSubmit, ChatState.Streaming);
    await queueOneMessage();

    setChatState(ChatState.Idle);

    await waitFor(() => expect(toastWarning).toHaveBeenCalledTimes(1));
    await settle();
    // Bounded, not a spin: three offers and no more.
    expect(handleSubmit).toHaveBeenCalledTimes(3);
    // And the message is still the user's, visible in the queue rather than
    // dropped on the floor.
    expect(queued()).toEqual([QUEUED_TEXT]);
  });

  it('offers a refused message only while it is OUT of the queue', async () => {
    // While the re-offer is in flight the message must not also be sitting in
    // the queue, where a second drain edge or a Stop-and-send would pick up the
    // same message and send it a second time.
    let release: (accepted: boolean) => void = () => {};
    const handleSubmit = vi.fn<SubmitFn>(
      () =>
        new Promise<boolean>((resolve) => {
          release = resolve;
        })
    );
    const { setChatState } = renderComposer(handleSubmit, ChatState.Streaming);
    await queueOneMessage();

    setChatState(ChatState.Idle);
    await waitFor(() => expect(handleSubmit).toHaveBeenCalledTimes(1));

    expect(queued()).toEqual([]);
    await act(async () => release(true));
    expect(queued()).toEqual([]);
  });
});

describe('a direct send that is refused', () => {
  it('puts the text back in the composer', async () => {
    const handleSubmit = vi.fn<SubmitFn>(async () => false);
    renderComposer(handleSubmit, ChatState.Idle);

    fireEvent.change(composer(), { target: { value: DIRECT_TEXT } });
    fireEvent.submit(composer().closest('form')!);

    await waitFor(() => expect(handleSubmit).toHaveBeenCalledTimes(1));
    // The composer clears itself synchronously on submit, so this is a restore,
    // not a "never cleared".
    await waitFor(() => expect(composer().value).toBe(DIRECT_TEXT));
    // One send attempt only — restoring the text must not also re-send it.
    expect(handleSubmit).toHaveBeenCalledTimes(1);
  });

  it('control: an accepted send leaves the composer empty', async () => {
    const handleSubmit = vi.fn<SubmitFn>(async () => true);
    renderComposer(handleSubmit, ChatState.Idle);

    fireEvent.change(composer(), { target: { value: DIRECT_TEXT } });
    fireEvent.submit(composer().closest('form')!);

    await waitFor(() => expect(handleSubmit).toHaveBeenCalledTimes(1));
    await settle();
    expect(composer().value).toBe('');
  });
});
