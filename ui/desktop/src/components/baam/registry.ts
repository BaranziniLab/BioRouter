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
  /**
   * Registry v2. All three are optional so a v1 document cached before the
   * upgrade still parses — a stale snapshot must degrade, never throw.
   *
   * `extension_name` is the name the installed config entry carries, reduced to
   * its lookup **key**: whitespace stripped and lowercased, so the card's
   * `CDWAgent` is published here as `cdwagent`. That substitution is the
   * contract, not an accident of the generator — treat it as one.
   *
   * The key is always `/^[a-z0-9_-]+$/`. That character set is exactly where
   * `config::extensions::name_to_key` (which `classify_extension` applies) and
   * `agents::extension_manager::normalize` (which the manager applies to the
   * installed config name) provably agree; outside it they diverge, so the
   * generator refuses the name rather than publishing a key the running app
   * would never produce.
   *
   * It exists because `id` is derived from the download filename and agrees with
   * the installed name only by luck (`spokeagent-0.4.1` already diverges), and a
   * suffix-stripping heuristic in a security path is right until it isn't. The
   * generator hard-fails on a private entry without one.
   */
  extension_name?: string;
  /** Absent on a v1 document; the generator emits it for every v2 entry. */
  privacy?: 'private' | 'public';
  /**
   * DR-26. Institution ids from `BaamRegistry.institutions`. Absent means
   * unconstrained — any private model may use it. Never an empty array: the
   * generator rejects one, because "nothing is permitted" and "no constraint"
   * must not share a spelling.
   */
  affiliation?: string[];
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
  /**
   * Registry v2, DR-26. Institution id → display name. Cross-institutional
   * warning copy renders names from here rather than hardcoding them, so an
   * `affiliation` naming an id absent from this map has no name to render with
   * — which is why the generator treats that as a build failure. Optional so a
   * cached v1 document still parses.
   */
  institutions?: Record<string, string>;
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
