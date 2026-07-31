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

  it('never nags: once seen, it does not fire again', () => {
    const args = [{ name: 'workspace', nowEnabled: true }, { chatrecallEnabled: false }] as const;
    expect(shouldSuggestChatrecall(...args)).toBe(true);
    markChatrecallSuggestionSeen();
    expect(shouldSuggestChatrecall(...args)).toBe(false);
  });
});
