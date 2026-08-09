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
 * ⚠ The Rust side keeps its own list (`skills_extension.rs::is_builtin_skill_name`)
 * and the two are hand-synced with nothing asserting they agree. That was
 * already true of `skillUtils.BUILTIN_SKILL_NAMES`; this file does not make it
 * worse, but it does not fix it either. See #77.
 */

export interface ContextMeta {
  /** The skill's `name:` in its frontmatter, and its directory name. */
  id: string;
  label: string;
  description: string;
}

/**
 * ⚠ Four, not five. "Develop Biorouter" was asked for but does not exist:
 * `BUILTIN_SKILLS` (`skills_extension.rs:22-35`) carries three, plus
 * `update-soul`, which is defined separately as a Rust string in `soul.rs:240`
 * rather than in that array. Adding a fifth later is one entry here plus a
 * `SKILL.md`; inventing one now would have shipped a Context pointing at
 * nothing.
 */
export const CONTEXTS: readonly ContextMeta[] = [
  {
    id: 'about-biorouter',
    label: 'About Biorouter',
    description: 'What Biorouter is and how its pieces fit together.',
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
