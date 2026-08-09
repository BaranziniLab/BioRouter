export interface Skill {
  folderPath: string; // absolute path to the skill folder  e.g. ~/.config/biorouter/skills/my-skill
  sourceDir: string; // parent directory (one of the watched dirs)
  name: string; // from SKILL.md frontmatter
  description: string; // from SKILL.md frontmatter
  content: string; // raw SKILL.md content
  bundleName?: string; // optional: parent bundle name if part of a bundle
}

export interface SkillBundle {
  bundleName: string; // folder name of the bundle
  folderPath: string; // absolute path to the bundle folder
  sourceDir: string; // parent directory (one of the watched dirs)
  skills: Skill[]; // array of skills in this bundle
}

export const BIOROUTER_SKILLS_DIR = '~/.config/biorouter/skills';

// Skills that ship with Biorouter. The backend re-seeds them on every session
// start ('about-biorouter', 'develop-biorouter', 'develop-biorouter-extension'
// and 'develop-biorouter-skill' via skills_extension.rs's BUILTIN_SKILLS;
// 'update-soul' via knowledge/soul.rs), so deleting their folder has no lasting
// effect — the UI therefore offers only the enable/disable toggle for them, not
// deletion.
export const BUILTIN_SKILL_NAMES = [
  'about-biorouter',
  'develop-biorouter',
  'develop-biorouter-extension',
  'develop-biorouter-skill',
  'update-soul',
];

export function isBuiltinSkill(name: string): boolean {
  return BUILTIN_SKILL_NAMES.includes(name);
}
export const OTHER_SKILL_DIRS = ['~/.claude/skills', '~/.config/agents/skills'];
export const ALL_SKILL_DIRS = [BIOROUTER_SKILLS_DIR, ...OTHER_SKILL_DIRS];

/**
 * Read a single top-level frontmatter field, supporting both inline values
 * (`key: value`, optionally quoted) and YAML block scalars
 * (`key: >-` / `|` followed by indented continuation lines). The built-in
 * skills use a folded block scalar for `description`, so a naive
 * same-line-only parse would surface the literal `>-` indicator instead of the
 * text. Folded (`>`) blocks join lines with spaces (blank line → newline);
 * literal (`|`) blocks keep their newlines.
 */
function readFrontmatterField(fm: string, key: string): string | null {
  const lines = fm.split(/\r?\n/);
  const idx = lines.findIndex((l) => new RegExp(`^${key}:`).test(l));
  if (idx === -1) return null;

  const head = (lines[idx].match(new RegExp(`^${key}:\\s*(.*)$`))?.[1] ?? '').trim();

  // Block scalar indicator: `|` or `>`, optional chomping (+/-) / indent digit,
  // optional trailing comment.
  const block = head.match(/^([|>])[+-]?\d*\s*(#.*)?$/);
  if (block) {
    const folded = block[1] === '>';
    const body: string[] = [];
    for (let i = idx + 1; i < lines.length; i++) {
      const line = lines[i];
      if (line.trim() === '') {
        body.push('');
        continue;
      }
      // Continuation lines are indented; a column-0 line starts the next key.
      if (!/^\s/.test(line)) break;
      body.push(line.replace(/^\s+/, ''));
    }
    while (body.length && body[body.length - 1] === '') body.pop();
    if (!folded) return body.join('\n').trim() || null;
    let out = '';
    for (const l of body) {
      if (l === '') out += '\n';
      else out += (out && !out.endsWith('\n') ? ' ' : '') + l;
    }
    return out.trim() || null;
  }

  // Inline value — strip a single layer of matching surrounding quotes.
  const unquoted = head.replace(/^"([\s\S]*)"$/, '$1').replace(/^'([\s\S]*)'$/, '$1');
  return unquoted.trim() || null;
}

/**
 * Parse YAML frontmatter from a SKILL.md file.
 * Returns { name, description } if valid, null if missing or malformed.
 */
export function parseSkillFrontmatter(
  content: string
): { name: string; description: string } | null {
  const match = content.match(/^---\r?\n([\s\S]*?)\r?\n---/);
  if (!match) return null;
  const fm = match[1];
  const name = readFrontmatterField(fm, 'name');
  const description = readFrontmatterField(fm, 'description');
  if (!name || !description) return null;
  return { name, description };
}

/**
 * Load all skills from a list of directories using Electron IPC.
 *
 * Detection rule per directory entry `<slug>`:
 *   - `<dir>/<slug>/SKILL.md` exists → single skill
 *   - No root SKILL.md, but sub-dirs of `<dir>/<slug>/` contain SKILL.md → bundle
 *   - Otherwise → ignored
 */
export async function loadSkillsFromDirs(
  dirs: string[]
): Promise<{ singles: Skill[]; bundles: SkillBundle[] }> {
  const singles: Skill[] = [];
  const bundles: SkillBundle[] = [];

  for (const dir of dirs) {
    const folders: string[] = await window.electron.listSkillDirs(dir);

    for (const folder of folders) {
      const skillMdPath = `${dir}/${folder}/SKILL.md`;
      const result = await window.electron.readFile(skillMdPath);

      if (result.found && result.file) {
        const parsed = parseSkillFrontmatter(result.file);
        if (!parsed) continue;
        singles.push({
          folderPath: `${dir}/${folder}`,
          sourceDir: dir,
          name: parsed.name,
          description: parsed.description,
          content: result.file,
        });
      } else {
        // No SKILL.md at root — check if sub-dirs have SKILL.md (bundle)
        const subFolders: string[] = await window.electron.listSkillDirs(`${dir}/${folder}`);
        const bundleSkills: Skill[] = [];

        for (const sub of subFolders) {
          const subPath = `${dir}/${folder}/${sub}/SKILL.md`;
          const subResult = await window.electron.readFile(subPath);
          if (!subResult.found || !subResult.file) continue;
          const parsed = parseSkillFrontmatter(subResult.file);
          if (!parsed) continue;
          bundleSkills.push({
            folderPath: `${dir}/${folder}/${sub}`,
            sourceDir: dir,
            name: parsed.name,
            description: parsed.description,
            content: subResult.file,
            bundleName: folder,
          });
        }

        if (bundleSkills.length > 0) {
          bundles.push({
            bundleName: folder,
            folderPath: `${dir}/${folder}`,
            sourceDir: dir,
            skills: bundleSkills,
          });
        }
      }
    }
  }

  return { singles, bundles };
}

/**
 * Derive a safe folder/file slug from a skill name or filename.
 * e.g. "My Skill!" → "my-skill"
 */
export function toSlug(input: string): string {
  return input
    .replace(/\.md$/i, '')
    .replace(/[^a-z0-9-_]/gi, '-')
    .replace(/-{2,}/g, '-')
    .replace(/^-|-$/g, '')
    .toLowerCase();
}
