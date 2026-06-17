import { describe, it, expect } from 'vitest';
import { parseSkillFrontmatter, toSlug } from './skillUtils';

describe('parseSkillFrontmatter', () => {
  it('returns name and description from valid frontmatter', () => {
    const content = `---\nname: my-skill\ndescription: A test skill\n---\nBody here`;
    expect(parseSkillFrontmatter(content)).toEqual({
      name: 'my-skill',
      description: 'A test skill',
    });
  });

  it('returns null when frontmatter is missing', () => {
    expect(parseSkillFrontmatter('# No frontmatter')).toBeNull();
  });

  it('returns null when name is missing', () => {
    const content = `---\ndescription: only desc\n---\nBody`;
    expect(parseSkillFrontmatter(content)).toBeNull();
  });

  it('ignores extra frontmatter fields (user-invocable, hooks)', () => {
    const content = `---\nname: ralph\ndescription: Test\nuser-invocable: true\n---\nBody`;
    expect(parseSkillFrontmatter(content)).toEqual({ name: 'ralph', description: 'Test' });
  });

  it('strips surrounding quotes from an inline description', () => {
    const content = `---\nname: q\ndescription: "Quoted desc with: colon"\n---\nBody`;
    expect(parseSkillFrontmatter(content)).toEqual({
      name: 'q',
      description: 'Quoted desc with: colon',
    });
  });

  it('folds a `>-` block-scalar description into spaced text', () => {
    const content = [
      '---',
      'name: update-soul',
      'description: >-',
      "  Update the user's personal Soul knowledge base from their",
      '  conversation history. Load this skill when running a Meditation.',
      'skills:',
      '- update-soul',
      '---',
      'Body',
    ].join('\n');
    expect(parseSkillFrontmatter(content)).toEqual({
      name: 'update-soul',
      description:
        "Update the user's personal Soul knowledge base from their conversation history. Load this skill when running a Meditation.",
    });
  });

  it('keeps newlines for a `|` literal block scalar', () => {
    const content = `---\nname: lit\ndescription: |\n  line one\n  line two\n---\nBody`;
    expect(parseSkillFrontmatter(content)).toEqual({
      name: 'lit',
      description: 'line one\nline two',
    });
  });

  it('returns null when a block-scalar description is empty', () => {
    const content = `---\nname: empty\ndescription: >-\n---\nBody`;
    expect(parseSkillFrontmatter(content)).toBeNull();
  });
});

describe('toSlug', () => {
  it('lowercases and replaces special chars with hyphens', () => {
    expect(toSlug('My Skill!')).toBe('my-skill');
  });

  it('strips .md extension', () => {
    expect(toSlug('my-skill.md')).toBe('my-skill');
  });

  it('collapses multiple hyphens', () => {
    expect(toSlug('a  b')).toBe('a-b');
  });
});
