import { nameToKey } from '../extensions/utils';
import type { ExtensionConfig } from '../../../api/types.gen';

/**
 * Built-in capabilities of Biorouter.
 *
 * These ship with Biorouter and are surfaced as first-class "Capabilities" in
 * Settings → Chat instead of being mixed with user-installed extensions.
 *
 * Keys are the normalized (whitespace-stripped, lower-cased) extension names,
 * matching `nameToKey(extension.name)`.
 */
export interface CapabilityMeta {
  /** Normalized extension key — see nameToKey(). */
  key: string;
  label: string;
  description: string;
  defaultEnabled: boolean;
}

export const CAPABILITIES: CapabilityMeta[] = [
  {
    key: 'developer',
    label: 'Developer',
    description: 'General development tools for reading, writing, and running code.',
    defaultEnabled: true,
  },
  {
    key: 'computercontroller',
    label: 'Computer Controller',
    description: 'Control desktop apps and work with local documents and files.',
    defaultEnabled: true,
  },
  {
    key: 'autovisualiser',
    label: 'Auto Visualiser',
    description: 'Render charts, diagrams, maps, and other rich visualizations inline.',
    defaultEnabled: true,
  },
  {
    key: 'code_execution',
    label: 'Code Execution',
    description: 'Execute JavaScript in a sandboxed environment for programmatic tool use.',
    defaultEnabled: true,
  },
  {
    key: 'extensionmanager',
    label: 'Extension Manager',
    description: 'Lets Biorouter discover, enable, and disable extensions on its own.',
    defaultEnabled: true,
  },
  {
    key: 'skills',
    label: 'Skills',
    description: 'Load and use reusable skills from your skill directories.',
    defaultEnabled: true,
  },
  {
    key: 'todo',
    label: 'Todo',
    description: 'Keep track of multi-step tasks with a working to-do list.',
    defaultEnabled: true,
  },
  {
    key: 'memory',
    label: 'Memory',
    description: 'Teach Biorouter your preferences so it remembers them as you go.',
    defaultEnabled: true,
  },
  {
    key: 'knowledge',
    label: 'Knowledge',
    description: 'Personal, LLM-maintained knowledge bases backed by markdown + git history.',
    defaultEnabled: true,
  },
  {
    key: 'agent_drafter',
    label: 'Agent Drafter',
    description: 'Build interactive artifacts and export them as standalone projects.',
    defaultEnabled: true,
  },
  {
    key: 'chatrecall',
    label: 'Chat Recall',
    description: 'Search past conversations and load session summaries for context.',
    defaultEnabled: false,
  },
  {
    key: 'tutorial',
    label: 'Tutorial',
    description: 'Access interactive tutorials and step-by-step guides.',
    defaultEnabled: false,
  },
];

/** Set of normalized capability keys, for quick membership checks. */
export const CAPABILITY_KEYS: ReadonlySet<string> = new Set(CAPABILITIES.map((c) => c.key));

const CAPABILITIES_BY_KEY: ReadonlyMap<string, CapabilityMeta> = new Map(
  CAPABILITIES.map((capability) => [capability.key, capability])
);

const PROMOTED_DEFAULT_ON_CAPABILITY_KEYS: ReadonlySet<string> = new Set([
  'autovisualiser',
  'code_execution',
  'computercontroller',
]);

/** True when the given extension is one of the shipped capabilities. */
export function isCapabilityExtension(extension: { name: string } | ExtensionConfig): boolean {
  return CAPABILITY_KEYS.has(nameToKey(extension.name));
}

export function isCapabilityDefaultEnabled(extension: { name: string } | ExtensionConfig): boolean {
  return CAPABILITIES_BY_KEY.get(nameToKey(extension.name))?.defaultEnabled ?? false;
}

export function shouldDefaultEnablePromotedCapability(extension: {
  name: string;
  enabled: boolean;
}): boolean {
  const key = nameToKey(extension.name);
  return !extension.enabled && PROMOTED_DEFAULT_ON_CAPABILITY_KEYS.has(key);
}

export function shouldDefaultEnableAgentDrafter(extension: {
  name: string;
  enabled: boolean;
}): boolean {
  return !extension.enabled && nameToKey(extension.name) === 'agent_drafter';
}
