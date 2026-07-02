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
  it('inserts visible references for skills and extensions', () => {
    expect(
      getMentionInsertText(
        item({
          name: 'skill:literature-review',
          itemType: 'Skill',
          relativePath: 'literature-review',
        })
      )
    ).toBe('Use the "literature-review" skill for this request, ');

    expect(
      getMentionInsertText(
        item({
          name: 'ext:pubmed',
          itemType: 'Extension',
          relativePath: 'pubmed',
        })
      )
    ).toBe('Use the "pubmed" extension for this request, ');
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

  it('uses client insert templates for UI-only commands', () => {
    expect(
      getMentionInsertText(
        item({
          name: 'knowledge',
          itemType: 'Builtin',
          relativePath: 'knowledge',
        })
      )
    ).toContain('Using the Knowledge extension');
  });
});
