import { describe, expect, it } from 'vitest';
import bundledExtensions from '../extensions/bundled-extensions.json';
import {
  CAPABILITIES,
  isCapabilityDefaultEnabled,
  isCapabilityExtension,
  shouldDefaultEnableAgentDrafter,
  shouldDefaultEnablePromotedCapability,
} from './capabilities';

const expectedDefaults = {
  developer: true,
  computercontroller: true,
  autovisualiser: true,
  code_execution: true,
  extensionmanager: true,
  skills: true,
  todo: true,
  memory: true,
  knowledge: true,
  agent_drafter: true,
  chatrecall: false,
  workspace: true,
};

describe('capabilities', () => {
  it('classifies every shipped built-in and platform extension as a capability', () => {
    expect(
      Object.fromEntries(CAPABILITIES.map(({ key, defaultEnabled }) => [key, defaultEnabled]))
    ).toEqual(expectedDefaults);

    for (const extension of bundledExtensions) {
      expect(isCapabilityExtension(extension), extension.name).toBe(true);
      expect(isCapabilityDefaultEnabled(extension), extension.name).toBe(extension.enabled);
    }

    for (const name of ['todo', 'chatrecall', 'extensionmanager', 'skills', 'code_execution']) {
      expect(isCapabilityExtension({ name }), name).toBe(true);
    }
  });

  it('only upgrade-enables newly promoted default-on capabilities', () => {
    for (const name of ['autovisualiser', 'code_execution', 'computercontroller']) {
      expect(shouldDefaultEnablePromotedCapability({ name, enabled: false }), name).toBe(true);
      expect(shouldDefaultEnablePromotedCapability({ name, enabled: true }), name).toBe(false);
    }

    for (const name of ['chatrecall', 'memory', 'developer']) {
      expect(shouldDefaultEnablePromotedCapability({ name, enabled: false }), name).toBe(false);
    }
  });

  it('upgrade-enables Agent Drafter when adopting its new default', () => {
    expect(shouldDefaultEnableAgentDrafter({ name: 'agent_drafter', enabled: false })).toBe(true);
    expect(shouldDefaultEnableAgentDrafter({ name: 'agent_drafter', enabled: true })).toBe(false);
    expect(shouldDefaultEnableAgentDrafter({ name: 'developer', enabled: false })).toBe(false);
  });
});
