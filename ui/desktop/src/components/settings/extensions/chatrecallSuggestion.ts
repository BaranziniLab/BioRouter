/**
 * BR-71 §3.2 (decision 14): enabling Workspace Control SUGGESTS enabling
 * chatrecall — it never enables it for the user.
 *
 * The two are complementary and the workspace instruction block routes content
 * questions ("what did we conclude about X?") to chatrecall; without it the
 * agent is told to use a tool it does not have. One prompt, dismissible,
 * remembered — a suggestion that reappears is a nag.
 */
const SEEN_KEY = 'biorouter.workspace.chatrecallSuggestionSeen';

export function shouldSuggestChatrecall(
  toggled: { name: string; nowEnabled: boolean },
  state: { chatrecallEnabled: boolean }
): boolean {
  if (toggled.name !== 'workspace' || !toggled.nowEnabled) return false;
  if (state.chatrecallEnabled) return false;
  return localStorage.getItem(SEEN_KEY) !== '1';
}

export function markChatrecallSuggestionSeen(): void {
  localStorage.setItem(SEEN_KEY, '1');
}

export function resetChatrecallSuggestionForTests(): void {
  localStorage.removeItem(SEEN_KEY);
}
