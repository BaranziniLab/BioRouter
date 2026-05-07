export interface Skill {
  filePath: string;    // absolute path to the .md file
  sourceDir: string;   // directory it came from
  name: string;        // from YAML frontmatter
  description: string; // from YAML frontmatter
  content: string;     // full raw file content
}

export const BIOROUTER_SKILLS_DIR = '~/.config/biorouter/skills';
export const OTHER_SKILL_DIRS = [
  '~/.claude/skills',
  '~/.config/agents/skills',
];
export const ALL_SKILL_DIRS = [BIOROUTER_SKILLS_DIR, ...OTHER_SKILL_DIRS];

/**
 * Parse YAML frontmatter from a skill .md file.
 * Returns { name, description } if valid, null if missing or malformed.
 */
export function parseSkillFrontmatter(
  content: string
): { name: string; description: string } | null {
  const match = content.match(/^---\r?\n([\s\S]*?)\r?\n---/);
  if (!match) return null;
  const fm = match[1];
  const nameMatch = fm.match(/^name:\s*(.+)$/m);
  const descMatch = fm.match(/^description:\s*(.+)$/m);
  if (!nameMatch?.[1]?.trim() || !descMatch?.[1]?.trim()) return null;
  return { name: nameMatch[1].trim(), description: descMatch[1].trim() };
}

/**
 * Load all skills from a list of directories using Electron IPC.
 * Returns a flat array of Skill objects. Silently skips files that fail
 * to read or have invalid frontmatter.
 */
export async function loadSkillsFromDirs(dirs: string[]): Promise<Skill[]> {
  const skills: Skill[] = [];
  for (const dir of dirs) {
    const filenames: string[] = await window.electron.listFiles(dir, '.md');
    for (const filename of filenames) {
      const filePath = `${dir}/${filename}`;
      const result = await window.electron.readFile(filePath);
      if (!result.found || !result.file) continue;
      const parsed = parseSkillFrontmatter(result.file);
      if (!parsed) continue;
      skills.push({
        filePath,
        sourceDir: dir,
        name: parsed.name,
        description: parsed.description,
        content: result.file,
      });
    }
  }
  return skills;
}
