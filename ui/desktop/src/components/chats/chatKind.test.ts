import { describe, expect, it } from 'vitest';
import { chatIconFor, chatKindOf } from './chatKind';

describe('chatKindOf', () => {
  it('reads the durable lineage fields, not the title', () => {
    expect(chatKindOf({ name: 'Cohort query', diverged_from: 'session-0' })).toBe('branch');
    expect(chatKindOf({ name: 'Cohort query', parent_session_id: 'session-0' })).toBe('subagent');
    expect(chatKindOf({ name: 'Cohort query', session_type: 'sub_agent' })).toBe('subagent');
    expect(chatKindOf({ name: 'Nightly triage', session_type: 'scheduled' })).toBe('scheduled');
    expect(chatKindOf({ name: 'zsh', session_type: 'terminal' })).toBe('terminal');
    expect(chatKindOf({ name: 'app:spec-002' })).toBe('app');
    expect(chatKindOf({ name: 'Cohort query' })).toBe('chat');
  });

  /**
   * ⚠ **A renamed branch is still a branch.** The title regex was the only
   * signal before BR-45 recorded lineage, and it is defeated by anyone who
   * renames the chat — which is exactly why it stopped being the primary test.
   * It survives only as the fallback for rows written before the field existed.
   */
  it('still recognises a branch whose name was changed', () => {
    expect(chatKindOf({ name: 'Second attempt at the cohort', diverged_from: 'session-0' })).toBe(
      'branch'
    );
    // …and the legacy naming, for rows that predate `diverged_from`.
    expect(chatKindOf({ name: 'Greeting 2 (branch 1)' })).toBe('branch');
  });

  /**
   * ⚠ The regex must not fire on a chat that merely talks about branches. This
   * is the failure the sidebar's original resolver was already guarding, and
   * moving the resolver must not drop the guard.
   */
  it('does not mistake prose for a branch or an app', () => {
    expect(chatKindOf({ name: 'Which git branch 2 use?' })).toBe('chat');
    expect(chatKindOf({ name: 'Refactor the app: rename it' })).toBe('chat');
  });

  /**
   * ⚠ **Order is load-bearing.** A sub-agent that was itself diverged carries
   * BOTH fields; the delegation is the more consequential fact (it is not a
   * chat the user is holding), so it must win. Written down because the two
   * arms are adjacent and swapping them fails nothing else.
   */
  it('prefers delegation over divergence when a session is both', () => {
    expect(chatKindOf({ name: 'Worker', diverged_from: 'a', parent_session_id: 'b' })).toBe(
      'subagent'
    );
  });

  it('treats a missing name as a plain chat rather than throwing', () => {
    expect(chatKindOf({})).toBe('chat');
    expect(chatKindOf({ name: null })).toBe('chat');
  });
});

describe('chatIconFor', () => {
  /**
   * ⚠ **Privacy is a SHAPE difference for a plain chat, not a hue.** The dense
   * dot it replaced was a separate mark; folding the tier into a colour alone
   * would have made a safety-relevant marking invisible to anyone who cannot
   * separate the two inks.
   */
  it('gives a private chat a different glyph from a public one', () => {
    const priv = chatIconFor('chat', 'private');
    const pub = chatIconFor('chat', 'public');
    expect(priv.Icon).not.toBe(pub.Icon);
    expect(priv.label).toBe('Private chat');
    expect(pub.label).toBe('Chat');
  });

  it('says the word "private" for every kind, not just plain chats', () => {
    for (const kind of ['branch', 'subagent', 'app', 'scheduled', 'terminal'] as const) {
      expect(chatIconFor(kind, 'private').label).toMatch(/private/i);
      expect(chatIconFor(kind, 'public').label).not.toMatch(/private/i);
    }
  });

  /** An unknown tier must read as public, never as private. */
  it('treats an absent tier as unmarked', () => {
    expect(chatIconFor('chat', null).Icon).toBe(chatIconFor('chat', 'public').Icon);
    expect(chatIconFor('chat', undefined).label).toBe('Chat');
  });

  it('gives every kind a distinct glyph', () => {
    const icons = (['chat', 'branch', 'subagent', 'app', 'scheduled', 'terminal'] as const).map(
      (k) => chatIconFor(k, 'public').Icon
    );
    expect(new Set(icons).size).toBe(icons.length);
  });
});
