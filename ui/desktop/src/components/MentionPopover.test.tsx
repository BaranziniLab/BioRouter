import { describe, expect, it } from 'vitest';
import { DisplayItem, getMentionInsertText } from './MentionPopover';

const item = (overrides: Partial<DisplayItem>): DisplayItem => ({
  name: 'example',
  extra: 'Extra detail',
  itemType: 'File',
  relativePath: 'example',
  ...overrides,
});

describe('getMentionInsertText', () => {
  it('inserts routed markers for skills and extensions', () => {
    expect(
      getMentionInsertText(
        item({
          name: 'skill:literature-review',
          itemType: 'Skill',
          relativePath: 'literature-review',
        })
      )
    ).toBe('/skill:literature-review ');

    expect(
      getMentionInsertText(
        item({
          name: 'ext:pubmed',
          itemType: 'Extension',
          relativePath: 'pubmed',
        })
      )
    ).toBe('/ext:pubmed ');
  });

  it('inserts routed markers for knowledge bases', () => {
    expect(
      getMentionInsertText(
        item({
          name: 'kb:Project notes',
          itemType: 'KnowledgeBase',
          relativePath: 'project-notes',
        })
      )
    ).toBe('/kb:project-notes ');
  });

  // Issue #60: the backend's `extract_inline_refs` splits the message on
  // whitespace, so an `/ext:` marker that contains a space arrives truncated at
  // the first word — `/ext:Chat Recall` resolves as `Chat`, matches nothing, and
  // the agent reports "not a known built-in extension".
  describe('extension names containing whitespace', () => {
    const extensionInsert = (name: string) =>
      getMentionInsertText(
        item({ name: `ext:${name}`, itemType: 'Extension', relativePath: name })
      );

    it('emits a single whitespace-free token for a bundled extension whose name has a space', () => {
      expect(extensionInsert('Chat Recall')).toBe('/ext:ChatRecall ');
    });

    it('emits a marker that survives the backend whitespace split', () => {
      for (const name of ['Chat Recall', 'Extension Manager', 'My  Spaced\tTool']) {
        const inserted = extensionInsert(name);
        const marker = inserted.trim();
        expect(marker.split(/\s+/)).toHaveLength(1);
        expect(marker.startsWith('/ext:')).toBe(true);
      }
    });

    it('leaves an extension name without whitespace exactly as configured', () => {
      expect(extensionInsert('pubmed')).toBe('/ext:pubmed ');
      expect(extensionInsert('agent_drafter')).toBe('/ext:agent_drafter ');
      expect(extensionInsert('my-lab-tools')).toBe('/ext:my-lab-tools ');
    });

    // Only whitespace is dropped. `_` and `-` survive the backend's `normalize`
    // and so are part of a user extension's key, which is why the tempting
    // "normalise to an id" fix is wrong: `/ext:mytool` would not match an
    // enabled extension stored under `my_tool`.
    it('keeps separators that are part of a user extension key', () => {
      expect(extensionInsert('my_tool')).toBe('/ext:my_tool ');
      expect(extensionInsert('my-tool')).toBe('/ext:my-tool ');
      expect(extensionInsert('my_lab tools')).toBe('/ext:my_labtools ');
    });

    // A user extension may be named like a bundled one. Which of the two wins is
    // the backend resolver's call (`resolve_bundled_extension` filters a
    // reference down to alphanumerics, so it already shadows `chat_recall` and
    // `chat-recall`); the popover must not change the answer by rewriting the
    // name it was configured with.
    it('leaves a user extension colliding with a bundled id spelled as configured', () => {
      expect(extensionInsert('chat_recall')).toBe('/ext:chat_recall ');
      expect(extensionInsert('chat-recall')).toBe('/ext:chat-recall ');
      expect(extensionInsert('Chat Recall')).toBe('/ext:ChatRecall ');
    });
  });

  it('keeps backend slash commands executable when selected by keyboard', () => {
    expect(
      getMentionInsertText(
        item({
          name: 'compact',
          itemType: 'Builtin',
          relativePath: 'compact',
        })
      )
    ).toBe('/compact');
  });

  it('uses routed markers for UI-only resource commands', () => {
    expect(
      getMentionInsertText(
        item({
          name: 'knowledge',
          itemType: 'Builtin',
          relativePath: 'knowledge',
        })
      )
    ).toBe('/ext:knowledge ');
  });
});
