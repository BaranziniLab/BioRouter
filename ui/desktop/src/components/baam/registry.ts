// Types + loader for the BAAM marketplace registry consumed by the in-app
// Browse Skills / Browse Extensions modals. The registry is published at
// biorouter.ucsf.edu/registry.json (generated from baam.html). We fetch it live
// for freshness and fall back to a snapshot bundled with the app when offline.

import fallback from './registry.fallback.json';

export interface RegistryExtension {
  id: string;
  name: string;
  organization: string;
  version: string;
  description: string;
  tags: string[];
  github: string;
  download: string;
  filename: string;
  license?: string;
}

export type SkillCategory = 'Core' | 'Developer' | 'Biomedical';

export interface RegistrySkill {
  id: string;
  name: string;
  category: SkillCategory;
  type: string;
  description: string;
  tags: string[];
  keywords: string[];
  download: string;
  filename: string;
  license?: string;
}

export interface BaamRegistry {
  version: number;
  source: string;
  extensions: RegistryExtension[];
  skills: RegistrySkill[];
}

export const FALLBACK_REGISTRY = fallback as BaamRegistry;

/**
 * Load the marketplace registry. Tries the live registry.json published with
 * the website; on any failure returns the bundled snapshot so Browse always
 * works offline. The second element flags whether the live copy was used.
 */
export async function loadRegistry(): Promise<{ registry: BaamRegistry; live: boolean }> {
  try {
    const result = await window.electron.fetchRegistry();
    if (
      'registry' in result &&
      result.registry &&
      Array.isArray(result.registry.skills) &&
      Array.isArray(result.registry.extensions)
    ) {
      return { registry: result.registry, live: true };
    }
  } catch {
    // fall through to the bundled snapshot
  }
  return { registry: FALLBACK_REGISTRY, live: false };
}

/** Case-insensitive match of a query against a skill's searchable fields. */
export function skillMatches(skill: RegistrySkill, q: string): boolean {
  if (!q) return true;
  const needle = q.toLowerCase();
  return (
    skill.name.toLowerCase().includes(needle) ||
    skill.description.toLowerCase().includes(needle) ||
    skill.category.toLowerCase().includes(needle) ||
    skill.tags.some((t) => t.toLowerCase().includes(needle)) ||
    skill.keywords.some((t) => t.toLowerCase().includes(needle)) ||
    (skill.license?.toLowerCase().includes(needle) ?? false)
  );
}

/** Case-insensitive match of a query against an extension's searchable fields. */
export function extensionMatches(ext: RegistryExtension, q: string): boolean {
  if (!q) return true;
  const needle = q.toLowerCase();
  return (
    ext.name.toLowerCase().includes(needle) ||
    ext.description.toLowerCase().includes(needle) ||
    ext.organization.toLowerCase().includes(needle) ||
    ext.tags.some((t) => t.toLowerCase().includes(needle)) ||
    (ext.license?.toLowerCase().includes(needle) ?? false)
  );
}
