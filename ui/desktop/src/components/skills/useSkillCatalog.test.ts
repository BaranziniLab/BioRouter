import { describe, expect, it } from 'vitest';
import {
  applyOptimistically,
  CATALOG_CHANGED_EVENT,
  pickerBundles,
  standaloneSkills,
} from './useSkillCatalog';
import type { CatalogBundle, CatalogSkill, CatalogView } from '../../api';

const state = (effective: boolean, machineEnabled = true) => ({
  machineEnabled,
  session: 'default' as const,
  sessionViaBundle: false,
  hiddenContext: false,
  effective,
});

const skill = (name: string, bundle: string | null, effective = true): CatalogSkill => ({
  name,
  description: '',
  slug: name,
  directory: `/skills/${name}`,
  sourceRoot: '/skills',
  source: { kind: 'biorouter', extension: null, label: 'Biorouter' },
  bundle,
  builtin: false,
  state: state(effective),
});

const bundle = (name: string, members: string[]): CatalogBundle => ({
  name,
  displayName: name,
  directory: `/skills/${name}`,
  sourceRoot: '/skills',
  source: { kind: 'biorouter', extension: null, label: 'Biorouter' },
  skills: members,
  package: null,
  builtin: false,
  state: state(true),
});

const view: CatalogView = {
  generation: 3,
  roots: [],
  skills: [skill('solo', null), skill('media-use', 'hyperframes'), skill('other', null)],
  bundles: [bundle('hyperframes', ['media-use'])],
};

describe('applyOptimistically', () => {
  it('moves only the entries the toggle named, and their bundle members', () => {
    const next = applyOptimistically(view, ['hyperframes'], false, true);
    expect(next.bundles[0].state.effective).toBe(false);
    expect(next.skills.find((s) => s.name === 'media-use')!.state.effective).toBe(false);
    expect(next.skills.find((s) => s.name === 'solo')!.state.effective).toBe(true);
    expect(next.skills.find((s) => s.name === 'other')!.state.effective).toBe(true);
  });

  /**
   * A per-chat toggle changes `workspace_skills/v1` and nothing else. Guessing
   * at `machineEnabled` here would have the interface claim, for one frame,
   * that a chat-scoped click had rewritten the machine-wide preference every
   * other chat and the CLI share.
   */
  it('never claims a per-chat toggle changed the machine-wide preference', () => {
    const next = applyOptimistically(view, ['solo'], false, true);
    const solo = next.skills.find((s) => s.name === 'solo')!;
    expect(solo.state.effective).toBe(false);
    expect(solo.state.machineEnabled).toBe(true);
  });

  it('does move the machine-wide answer when the toggle was machine-wide', () => {
    const next = applyOptimistically(view, ['solo'], false, false);
    const solo = next.skills.find((s) => s.name === 'solo')!;
    expect(solo.state.machineEnabled).toBe(false);
  });

  it('leaves the source catalog untouched, so a refusal can restore it', () => {
    const before = JSON.stringify(view);
    applyOptimistically(view, ['hyperframes', 'solo'], false, true);
    expect(JSON.stringify(view)).toBe(before);
  });
});

describe('CATALOG_CHANGED_EVENT', () => {
  it('is the name worktree 4 publishes', () => {
    expect(CATALOG_CHANGED_EVENT).toBe('catalog:changed');
  });
});

/**
 * ⚠ **A Context that is a BUNDLE has to be filtered as a bundle.** Every
 * Context filter used to test a skill's own `name`, and a bundle row carries
 * none of its members' names — so promoting the knowledge skills to a bundle
 * put a row back into the composer picker, the `@`-mention list and the
 * workflow resource picker at once, with `activeCount` up by one to match.
 *
 * The composer one is the dangerous one: its rows feed "Enable all", which
 * writes `skills-config.json`, and `handle_load_skill` refuses anything listed
 * there — while the system prompt asks for `about-biorouter` on every turn.
 * That is the failure `contexts.ts` was written to make impossible.
 */
describe('Context bundles are not picker rows', () => {
  const contextView: CatalogView = {
    generation: 1,
    roots: [],
    skills: [
      skill('solo', null),
      skill('media-use', 'hyperframes'),
      skill('knowledge-lint', 'knowledge-bases'),
      skill('update-soul', 'knowledge-bases'),
    ],
    bundles: [
      bundle('hyperframes', ['media-use']),
      bundle('knowledge-bases', ['knowledge-lint', 'update-soul']),
    ],
  };

  it('drops the Context bundle and keeps an installed package', () => {
    expect(pickerBundles(contextView).map((b) => b.name)).toEqual(['hyperframes']);
  });

  /**
   * The members were already excluded by the `!skill.bundle` clause, so this
   * would pass with the Context filter deleted. It is here to pin that the two
   * clauses agree: nothing about the knowledge bundle reaches a skill row.
   */
  it('drops the Context bundle members from the standalone list', () => {
    expect(standaloneSkills(contextView).map((s) => s.name)).toEqual(['solo']);
  });

  /**
   * ⚠ The regression this exists for: a member seeded flat rather than into the
   * bundle — an install that predates the bundle, or a fallback that forgot the
   * placement — arrives with `bundle: null` and would be a standalone row.
   * `isContextSkill` covers it by name as well, so it stays out either way.
   */
  it('drops a member that arrives without its bundle', () => {
    const stray: CatalogView = {
      ...contextView,
      skills: [skill('solo', null), skill('knowledge-bases', null)],
      bundles: [],
    };
    expect(standaloneSkills(stray).map((s) => s.name)).toEqual(['solo']);
  });
});
