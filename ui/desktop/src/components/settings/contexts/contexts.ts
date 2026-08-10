/**
 * Contexts: the skills that ship with Biorouter rather than being installed.
 *
 * They are skills mechanically — same `SKILL.md`, same loader, same
 * `/skill:<name>` reference — but not skills from the user's point of view.
 * The user did not choose them, cannot delete them (the seeder rewrites them on
 * every start), and did not mean them when they asked how many skills are
 * loaded. Counting them made "5 skills enabled" mostly a statement about the
 * install.
 *
 * ⚠ **Disabling a Context must NOT go through `skills-config.json`.** That
 * file's `disabled[]` array is honoured by `handle_load_skill`, which refuses a
 * disabled skill outright (`skills_extension.rs:704-713`) — while
 * `prompts/system.md:31-34` unconditionally instructs the model to load
 * `about-biorouter`. Route a Context through that array and turning one off
 * makes the agent inject "Could not load this selected skill" into every
 * prompt. Enablement therefore lives in ordinary config keys, so a disabled
 * Context stops being *surfaced* without becoming *unloadable*.
 *
 * ⚠ The Rust side keeps its own list (`skills_extension.rs::is_builtin_skill_name`,
 * i.e. `BUILTIN_SKILLS` plus `soul.rs`'s `SOUL_SKILL_DIR`) and so does
 * `skillUtils.BUILTIN_SKILL_NAMES`. All three are hand-synced, but they are no
 * longer unasserted: `contexts.test.ts` parses the Rust source and fails if
 * either TypeScript copy disagrees with it (#77). Rust owns the truth — the
 * seeder writes those files — so a mismatch is fixed by moving the TS list, not
 * the assertion.
 */

export interface ContextMeta {
  /** The skill's `name:` in its frontmatter, and its directory name. */
  id: string;
  label: string;
  description: string;
}

/**
 * ⚠ Five, and every one of them must exist on the Rust side before it is
 * listed here: `BUILTIN_SKILLS` (`skills_extension.rs`) carries four, plus
 * `update-soul`, which is defined separately as a Rust string in `soul.rs`
 * rather than in that array. A Context whose `SKILL.md` does not ship renders
 * and toggles while pointing at nothing, so add the skill first.
 */
export const CONTEXTS: readonly ContextMeta[] = [
  {
    id: 'about-biorouter',
    label: 'About Biorouter',
    description: 'What Biorouter is and how its pieces fit together.',
  },
  {
    id: 'develop-biorouter',
    label: 'Develop Biorouter',
    description: 'Work on the Biorouter codebase: layout, commands and checks.',
  },
  {
    id: 'develop-biorouter-extension',
    label: 'Develop Biorouter Extension',
    description: 'Build, package and publish an extension.',
  },
  {
    id: 'develop-biorouter-skill',
    label: 'Develop Biorouter Skill',
    description: 'Write, test and package a skill.',
  },
  {
    id: 'update-soul',
    label: 'Updates',
    description: 'Keep the personal knowledge base current as a chat goes on.',
  },
];

export const CONTEXT_IDS: ReadonlySet<string> = new Set(CONTEXTS.map((c) => c.id));

/** Is this skill a shipped Context rather than one the user installed? */
export function isContextSkill(name: string): boolean {
  return CONTEXT_IDS.has(name);
}

/** The config key holding one Context's enablement. Default on when absent. */
export function contextConfigKey(id: string): string {
  return `context_${id.replace(/-/g, '_')}`;
}
