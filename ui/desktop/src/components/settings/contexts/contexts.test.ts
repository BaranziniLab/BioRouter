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
   * `skills_extension.rs` plus `SOUL_SKILL_DIR` in `soul.rs`.
   *
   * ⚠ Sorted, so `develop-biorouter` sits before its two longer namesakes.
   * The three are distinct skills, not one with variants: this one is about
   * changing Biorouter's own source, the others about authoring a skill and
   * packaging an extension.
   */
  it('names exactly the five skills that actually ship', () => {
    expect([...CONTEXT_IDS].sort()).toEqual([
      'about-biorouter',
      'develop-biorouter',
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
    const src = readFileSync(join(__dirname, '..', 'chat', 'ChatSettingsSection.tsx'), 'utf8');
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

/**
 * ⚠ **Three copies of this list exist and nothing asserted they agree.**
 *
 * Rust owns the truth: `BUILTIN_SKILLS` in `skills_extension.rs` plus
 * `SOUL_SKILL_DIR` in `soul.rs`, combined by `is_builtin_skill_name`, which is
 * what the seeder, the reset path and now the CLI count all use. The UI carries
 * two more — `skillUtils.BUILTIN_SKILL_NAMES` (for the "Built-in" badge and for
 * hiding Delete) and `CONTEXTS` here (for filtering and the Settings section).
 *
 * Drift between them is silent and asymmetric, which is what makes it worth a
 * test: a name missing from `CONTEXTS` leaves a shipped skill sitting in the
 * user's skill list and counted; a name missing from Rust means the seeder
 * stops writing a file the UI still advertises.
 *
 * ⚠ **There are TWO copies now, not three.** `skillUtils.BUILTIN_SKILL_NAMES`
 * is gone: it answered "did Biorouter put this here?", which the daemon answers
 * directly as `CatalogSkill.builtin`. A pinning test over a list nothing reads
 * guards nothing, so what is asserted below is that the copy has not come
 * back.
 *
 * Reading the Rust source is the only way to check this from here, and it is
 * the same approach `measures.test.ts` takes for a value only the source can
 * settle.
 */
describe('the built-in skill list, across all three copies', () => {
  const rustNames = () => {
    const root = join(__dirname, '..', '..', '..', '..', '..', '..', 'crates', 'biorouter', 'src');
    const skills = readFileSync(join(root, 'agents', 'skills_extension.rs'), 'utf8');
    const soul = readFileSync(join(root, 'knowledge', 'soul.rs'), 'utf8');

    // The `("<name>", include_str!(...))` pairs of BUILTIN_SKILLS.
    const block = skills.slice(
      skills.indexOf('BUILTIN_SKILLS'),
      skills.indexOf('];', skills.indexOf('BUILTIN_SKILLS'))
    );
    // rustfmt breaks each pair across lines, so the name and `include_str!`
    // are not adjacent. Matching on the name that PRECEDES an include_str! is
    // what survives that, and it still cannot pick up an unrelated string.
    const bundled = [...block.matchAll(/"([a-z0-9-]+)",\s*\n?\s*include_str!/g)].map((m) => m[1]);

    const soulMatch = soul.match(/SOUL_SKILL_DIR:\s*&str\s*=\s*"([a-z0-9-]+)"/);
    expect(soulMatch, 'SOUL_SKILL_DIR not found in soul.rs').not.toBeNull();

    return [...bundled, soulMatch![1]].sort();
  };

  /**
   * Every skill Biorouter SEEDS, which is a larger set than the Contexts.
   *
   * ⚠ The two lists answer different questions, and asserting both against
   * `rustNames()` was wrong in a way that could not fail. Rust says it with two
   * functions: `context_skill_names()` (BUILTIN_SKILLS + soul) is what the
   * Settings toggles mirror, and `is_builtin_skill_name()` is over
   * `shipped_skills()` — BUILTIN_SKILLS ++ KNOWLEDGE_SKILLS — which is what
   * hiding Delete and showing the Built-in badge must mirror.
   *
   * Because this helper sliced only `BUILTIN_SKILLS`, the four knowledge skills
   * were invisible to it: the census could not see the drift it exists to
   * catch, and the Skills pane offered a working Delete on a seeded skill.
   */
  const shippedNames = () => {
    const root = join(__dirname, '..', '..', '..', '..', '..', '..', 'crates', 'biorouter', 'src');
    const skills = readFileSync(join(root, 'agents', 'skills_extension.rs'), 'utf8');
    const block = skills.slice(
      skills.indexOf('KNOWLEDGE_SKILLS'),
      skills.indexOf('];', skills.indexOf('KNOWLEDGE_SKILLS'))
    );
    const knowledge = [...block.matchAll(/"([a-z0-9-]+)",\s*\n?\s*include_str!/g)].map((m) => m[1]);
    expect(knowledge.length, 'KNOWLEDGE_SKILLS not found in skills_extension.rs').toBeGreaterThan(
      0
    );
    return [...rustNames(), ...knowledge].sort();
  };

  it('CONTEXTS names exactly what Rust offers as a Context', () => {
    expect([...CONTEXT_IDS].sort()).toEqual(rustNames());
  });

  it('the renderer keeps no second list of the skills Rust seeds', () => {
    const source = readFileSync(join(__dirname, '..', '..', 'skills', 'skillUtils.ts'), 'utf8');
    // `shippedNames()` is still called, so the extractor it exercises stays
    // honest: a name Rust seeds must be findable from here even though nothing
    // in the renderer mirrors the list any more.
    expect(shippedNames().length).toBeGreaterThan(CONTEXT_IDS.size);
    for (const name of shippedNames()) {
      expect(
        source.includes(`'${name}'`),
        `skillUtils.ts names '${name}' again — read CatalogSkill.builtin instead`
      ).toBe(false);
    }
  });
});
