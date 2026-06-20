import { nameToKey } from '../extensions/utils';
import type { ExtensionConfig } from '../../../api/types.gen';

/**
 * Foundational built-in capabilities of Biorouter.
 *
 * These were previously listed alongside ordinary extensions, but they are so
 * core to how Biorouter works that they are surfaced as first-class
 * "Capabilities" in Settings → Chat instead. They are enabled by default and
 * hidden from the Extensions sub-panel so users don't have to configure them
 * by hand — though they can still be turned off here if truly needed.
 *
 * Keys are the normalized (whitespace-stripped, lower-cased) extension names,
 * matching `nameToKey(extension.name)`.
 */
export interface CapabilityMeta {
  /** Normalized extension key — see nameToKey(). */
  key: string;
  label: string;
  description: string;
}

export const CAPABILITIES: CapabilityMeta[] = [
  {
    key: 'developer',
    label: 'Developer',
    description: 'General development tools for reading, writing, and running code.',
  },
  {
    key: 'extensionmanager',
    label: 'Extension Manager',
    description: 'Lets Biorouter discover, enable, and disable extensions on its own.',
  },
  {
    key: 'skills',
    label: 'Skills',
    description: 'Load and use reusable skills from your skill directories.',
  },
  {
    key: 'todo',
    label: 'Todo',
    description: 'Keep track of multi-step tasks with a working to-do list.',
  },
  {
    key: 'memory',
    label: 'Memory',
    description: 'Teach Biorouter your preferences so it remembers them as you go.',
  },
  {
    key: 'knowledge',
    label: 'Knowledge',
    description: 'Personal, LLM-maintained knowledge bases backed by markdown + git history.',
  },
];

/** Set of normalized capability keys, for quick membership checks. */
export const CAPABILITY_KEYS: ReadonlySet<string> = new Set(CAPABILITIES.map((c) => c.key));

/** True when the given extension is one of the foundational capabilities. */
export function isCapabilityExtension(extension: { name: string } | ExtensionConfig): boolean {
  return CAPABILITY_KEYS.has(nameToKey(extension.name));
}
