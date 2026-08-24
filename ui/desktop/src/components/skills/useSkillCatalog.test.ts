import { describe, expect, it } from 'vitest';
import { applyOptimistically, CATALOG_CHANGED_EVENT } from './useSkillCatalog';
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
