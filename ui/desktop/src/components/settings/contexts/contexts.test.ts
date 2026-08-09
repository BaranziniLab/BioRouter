import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { CONTEXTS, CONTEXT_IDS, contextConfigKey, isContextSkill } from './contexts';

describe('contexts', () => {
  /**
   * ⚠ These ids are the skills' frontmatter `name:` values, and a typo here is
   * invisible: the Context row would render and toggle, the skill would simply
   * never be filtered out, and the counts this exists to fix would stay wrong.
   * Pinned against the Rust source of truth — `BUILTIN_SKILLS` in
   * `skills_extension.rs:22-35` plus `SOUL_SKILL_DIR` in `soul.rs:33`.
   */
  it('names exactly the four skills that actually ship', () => {
    expect([...CONTEXT_IDS].sort()).toEqual([
      'about-biorouter',
      'develop-biorouter-extension',
      'develop-biorouter-skill',
      'update-soul',
    ]);
  });

  it('recognises a shipped skill and leaves a user skill alone', () => {
    expect(isContextSkill('about-biorouter')).toBe(true);
    expect(isContextSkill('update-soul')).toBe(true);
    // The trap: a user-installed skill whose name merely resembles one.
    expect(isContextSkill('about-biorouter-notes')).toBe(false);
    expect(isContextSkill('single-cell')).toBe(false);
  });

  /**
   * ⚠ The key must not collide with, or be routed into, `skills-config.json`'s
   * `disabled[]`. That array is honoured by `handle_load_skill`, which refuses
   * a disabled skill outright, while the system prompt unconditionally tells
   * the model to load `about-biorouter` — so a Context disabled through that
   * path would make the agent report a load failure on every turn.
   */
  it('derives a config key that is a plain identifier', () => {
    expect(contextConfigKey('about-biorouter')).toBe('context_about_biorouter');
    for (const c of CONTEXTS) {
      expect(contextConfigKey(c.id)).toMatch(/^context_[a-z0-9_]+$/);
    }
  });

  it('gives every context a label and a description', () => {
    for (const c of CONTEXTS) {
      expect(c.label.length).toBeGreaterThan(0);
      expect(c.description.length).toBeGreaterThan(0);
      expect(c.description).not.toContain('—');
    }
  });
});

/**
 * ⚠ The section's placement is otherwise unprotected: `SettingsView.test.tsx`
 * asserts the App tab's order and nothing asserts the Chat tab's, so a
 * reordering would go unnoticed. This pins the sequence at the source, which is
 * the only place it can be checked without a layout engine.
 */
describe('Settings -> Chat section order', () => {
  it('puts Contexts after Capabilities and Memory, and before App SDK', () => {
    const src = readFileSync(
      join(__dirname, '..', 'chat', 'ChatSettingsSection.tsx'),
      'utf8'
    );
    const at = (needle: string) => {
      const i = src.indexOf(needle);
      expect(i, `${needle} missing from ChatSettingsSection`).toBeGreaterThan(-1);
      return i;
    };
    // Capabilities owns the switch that turns memory on, which is why Memory
    // sits directly under it; Contexts goes after that pair.
    expect(at('<CapabilitiesSection />')).toBeLessThan(at('<MemorySection />'));
    expect(at('<MemorySection />')).toBeLessThan(at('<ContextsSection />'));
    // ⚠ `>App SDK<`, not `App SDK`. The bare string also matches the comment
    // in ChatSettingsSection explaining this very ordering, which sits ABOVE
    // the section — so the loose form failed on correct code. Third time this
    // repo has produced a guard that matched its own prose.
    expect(at('<ContextsSection />')).toBeLessThan(at('>App SDK<'));
  });
});
