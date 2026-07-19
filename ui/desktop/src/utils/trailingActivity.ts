import { Message } from '../api';
import { ChatState } from '../types/chatState';
import { getToolRequests, getToolResponses } from '../types/message';

export type TrailingPhase = 'thinking' | 'running' | 'compacting';

export interface TrailingActivity {
  phase: TrailingPhase;
  /** Label shown next to the pulse. Never contains the elapsed time. */
  label: string;
  /**
   * Client-clock ms the indicator counts from. `undefined` means "we have no
   * trustworthy origin" (a reload mid-turn) — render the pulse with NO timer
   * rather than counting from mount, which would be a fabricated number.
   */
  since?: number;
}

export interface DeriveTrailingActivityArgs {
  messages: Message[];
  /** chatState !== Idle. False on any historical/read-only render. */
  isTurnActive: boolean;
  chatState?: ChatState;
  /** Store-owned client timestamps. */
  turnStartedAt?: number;
  lastMessageAt?: number;
}

const isToolResponseOnly = (m: Message) =>
  m.content.length > 0 && m.content.every((c) => c.type === 'toolResponse');

const hasVisibleText = (m: Message) =>
  m.content.some((c) => c.type === 'text' && c.text.trim().length > 0);

const awaitsToolConfirmation = (m: Message) =>
  m.content.some(
    (c) =>
      c.type === 'toolConfirmationRequest' ||
      (c.type === 'actionRequired' && c.data.actionType === 'elicitation')
  );

/**
 * Derives the trailing "still working" indicator from facts alone.
 *
 * This is a PURE function on purpose. `ProgressiveMessageList`'s render
 * callback has `messages` in its deps, whose identity changes on every stream
 * event, so nothing downstream can memoise across a turn — any component state
 * here (a mount-time timestamp especially) would be reset by the list's churn
 * and would lie after a reload. Every input is either message content or a
 * store-owned client timestamp.
 */
export function deriveTrailingActivity({
  messages,
  isTurnActive,
  chatState,
  turnStartedAt,
  lastMessageAt,
}: DeriveTrailingActivityArgs): TrailingActivity | null {
  // (a) Historical / read-only / finished turn. The single most important gate:
  //     SessionHistoryView never passes isStreamingMessage, so this is false
  //     there and a replayed session can never show a live indicator.
  if (!isTurnActive) return null;

  // (b) The pill above the composer already narrates these, and a card is on
  //     screen demanding input. Do not double-narrate.
  if (chatState === ChatState.WaitingForUserInput) return null;
  if (chatState === ChatState.LoadingConversation) return null;

  const last = messages[messages.length - 1];
  if (!last) return null;
  if (awaitsToolConfirmation(last)) return null;

  const since = lastMessageAt ?? turnStartedAt;

  if (chatState === ChatState.Compacting) {
    return { phase: 'compacting', label: 'Compacting the conversation', since };
  }

  // (c) THE CASE THIS FEATURE EXISTS FOR: a tool just returned, the model is
  //     mid round-trip, and nothing at all is on screen. A tool-response
  //     message is role 'user' with only toolResponse content, which
  //     ProgressiveMessageList renders as an empty div.
  if (last.role === 'user' && isToolResponseOnly(last)) {
    return { phase: 'thinking', label: 'Working on the result', since };
  }

  // (d) The assistant message with tool calls is still last and some calls have
  //     no response yet -> tools are executing. The cards carry their own
  //     status, so we only label; `since: undefined` suppresses our timer to
  //     keep exactly one clock on screen at a time.
  if (last.role === 'assistant') {
    const requests = getToolRequests(last);
    if (requests.length > 0) {
      const answered = new Set(getToolResponses(last).map((r) => r.id));
      const outstanding = requests.filter((r) => !answered.has(r.id)).length;
      if (outstanding > 0) {
        return {
          phase: 'running',
          label: outstanding === 1 ? 'Running the tool' : `Running ${outstanding} tools`,
          since: undefined,
        };
      }
    }
    // (e) Assistant prose is streaming in. The visible text IS the feedback.
    if (hasVisibleText(last)) return null;
    return { phase: 'thinking', label: 'Thinking', since };
  }

  // (f) User just submitted; nothing back yet.
  return { phase: 'thinking', label: 'Thinking', since };
}
