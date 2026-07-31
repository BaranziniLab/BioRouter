import { describe, expect, it, beforeEach } from 'vitest';
import {
  shouldSuggestChatrecall,
  markChatrecallSuggestionSeen,
  resetChatrecallSuggestionForTests,
} from './chatrecallSuggestion';

describe('shouldSuggestChatrecall', () => {
  beforeEach(() => resetChatrecallSuggestionForTests());

  it('suggests when workspace was just enabled and chatrecall is off', () => {
    expect(
      shouldSuggestChatrecall({ name: 'workspace', nowEnabled: true }, { chatrecallEnabled: false })
    ).toBe(true);
  });

  it('stays quiet when chatrecall is already on', () => {
    expect(
      shouldSuggestChatrecall({ name: 'workspace', nowEnabled: true }, { chatrecallEnabled: true })
    ).toBe(false);
  });

  it('stays quiet for other extensions and for disabling workspace', () => {
    expect(
      shouldSuggestChatrecall({ name: 'developer', nowEnabled: true }, { chatrecallEnabled: false })
    ).toBe(false);
    expect(
      shouldSuggestChatrecall(
        { name: 'workspace', nowEnabled: false },
        { chatrecallEnabled: false }
      )
    ).toBe(false);
  });

  /**
   * The daemon does NOT send the config key. A platform extension's entry
   * carries `PlatformExtensionDef.name`, which is
   * `workspace_extension::EXTENSION_NAME` = `"Workspace"` (and chatrecall's is
   * `"Chat Recall"`) — measured off the live React props in the dev GUI during
   * Task 31, where the suggestion consequently never fired once. Everything
   * else in the settings tree already normalises with `formatExtensionName`'s
   * `toLowerCase().replace(/\s+/g, '')`; this must too, or decision 14 is dead
   * in the shipped app while every unit test passes on a lowercase fixture.
   */
  it('accepts the display name the daemon actually sends', () => {
    expect(
      shouldSuggestChatrecall({ name: 'Workspace', nowEnabled: true }, { chatrecallEnabled: false })
    ).toBe(true);
  });

  it('stays quiet when chatrecall is on, under its real display name too', () => {
    expect(
      shouldSuggestChatrecall({ name: 'Workspace', nowEnabled: true }, { chatrecallEnabled: true })
    ).toBe(false);
  });

  it('never nags: once seen, it does not fire again', () => {
    const args = [{ name: 'workspace', nowEnabled: true }, { chatrecallEnabled: false }] as const;
    expect(shouldSuggestChatrecall(...args)).toBe(true);
    markChatrecallSuggestionSeen();
    expect(shouldSuggestChatrecall(...args)).toBe(false);
  });
});
