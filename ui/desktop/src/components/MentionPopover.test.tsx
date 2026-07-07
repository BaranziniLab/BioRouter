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
