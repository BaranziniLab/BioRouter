/**
 * What is left of the renderer's own view of skills.
 *
 * ⚠ **The filesystem scanner is gone** (#113). `loadSkillsFromDirs`,
 * `ALL_SKILL_DIRS` and `OTHER_SKILL_DIRS` walked three roots against the
 * backend's seven, so a skill bundled inside an installed extension was
 * loadable by the model and invisible in every interface surface that used
 * them — Settings, the composer picker, the `@`-mention list and the workflow
 * resource picker alike. They all read `useSkillCatalog` now, and the
 * `Skill`/`SkillBundle` shapes it produced are the generated `CatalogSkill` and
 * `CatalogBundle`.
 *
 * What remains is the two things a renderer legitimately does without asking
 * the daemon: parse the frontmatter of a file the user is *authoring*
 * (`CustomSkillModal`), and derive a folder name from it.
 *
 * ⚠ **The `BUILTIN_SKILL_NAMES` copy is gone too**, and deliberately. Its job
 * was to hide Delete and show the "Built-in" badge, and a hand-synced list is a
 * bad way to answer "did Biorouter put this here?" — it had already drifted
 * once, and the Skills pane offered a Delete that succeeded, toasted, and was
 * silently rewritten by the next startup. `CatalogSkill.builtin` answers it
 * from `is_builtin_skill_name`, in the process that owns the seeder.
 */
export const BIOROUTER_SKILLS_DIR = '~/.config/biorouter/skills';

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
