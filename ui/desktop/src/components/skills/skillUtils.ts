export interface Skill {
  folderPath: string;   // absolute path to the skill folder  e.g. ~/.config/biorouter/skills/my-skill
  sourceDir: string;    // parent directory (one of the watched dirs)
  name: string;         // from SKILL.md frontmatter
  description: string;  // from SKILL.md frontmatter
  content: string;      // raw SKILL.md content
}

export const BIOROUTER_SKILLS_DIR = '~/.config/biorouter/skills';
export const OTHER_SKILL_DIRS = [
  '~/.claude/skills',
  '~/.config/agents/skills',
];
export const ALL_SKILL_DIRS = [BIOROUTER_SKILLS_DIR, ...OTHER_SKILL_DIRS];

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
  const nameMatch = fm.match(/^name:\s*([^\n]+)$/m);
  const descMatch = fm.match(/^description:\s*([^\n]+)$/m);
  if (!nameMatch?.[1]?.trim() || !descMatch?.[1]?.trim()) return null;
  return { name: nameMatch[1].trim(), description: descMatch[1].trim() };
}

/**
 * Load all skills from a list of directories using Electron IPC.
 * Each skill is a subdirectory containing a SKILL.md file.
 * Silently skips folders that have no readable SKILL.md or invalid frontmatter.
 */
export async function loadSkillsFromDirs(dirs: string[]): Promise<Skill[]> {
  const skills: Skill[] = [];
  for (const dir of dirs) {
    const folders: string[] = await window.electron.listSkillDirs(dir);
    for (const folder of folders) {
      const skillMdPath = `${dir}/${folder}/SKILL.md`;
      const result = await window.electron.readFile(skillMdPath);
      if (!result.found || !result.file) continue;
      const parsed = parseSkillFrontmatter(result.file);
      if (!parsed) continue;
      skills.push({
        folderPath: `${dir}/${folder}`,
        sourceDir: dir,
        name: parsed.name,
        description: parsed.description,
        content: result.file,
      });
    }
  }
  return skills;
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
