/**
 * BR-71 §3.2 (decision 14): enabling Workspace Control SUGGESTS enabling
 * chatrecall — it never enables it for the user.
 *
 * The two are complementary and the workspace instruction block routes content
 * questions ("what did we conclude about X?") to chatrecall; without it the
 * agent is told to use a tool it does not have. One prompt, dismissible,
 * remembered — a suggestion that reappears is a nag.
 */
import { nameToKey } from './utils';

const SEEN_KEY = 'biorouter.workspace.chatrecallSuggestionSeen';

/**
 * The config key for `workspace`. An extension entry does NOT necessarily carry
 * its config key in `name`: a platform extension carries its
 * `PlatformExtensionDef.name`, i.e. the `EXTENSION_NAME` constant — `"Workspace"`
 * here, `"Chat Recall"` for the extension this suggests. `nameToKey` is the
 * app's existing display-name → key mapping and the mirror of the daemon's
 * `name_to_key` (`crates/biorouter/src/config/extensions.rs:23`), which is what
 * keys `config.yaml`'s `extensions` map in the first place. Comparing raw
 * `name`s instead made this return `false` for every real toggle.
 */
export const WORKSPACE_KEY = 'workspace';
export const CHATRECALL_KEY = 'chatrecall';

export function shouldSuggestChatrecall(
  toggled: { name: string; nowEnabled: boolean },
  state: { chatrecallEnabled: boolean }
): boolean {
  if (nameToKey(toggled.name) !== WORKSPACE_KEY || !toggled.nowEnabled) return false;
  if (state.chatrecallEnabled) return false;
  return localStorage.getItem(SEEN_KEY) !== '1';
}

export function markChatrecallSuggestionSeen(): void {
  localStorage.setItem(SEEN_KEY, '1');
}

export function resetChatrecallSuggestionForTests(): void {
  localStorage.removeItem(SEEN_KEY);
}
