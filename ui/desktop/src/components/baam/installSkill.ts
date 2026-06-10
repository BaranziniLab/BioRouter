// Install a skill from a registry download URL: fetch the .zip to a temp file,
// extract + parse it through the existing skills:extract-zip flow, then write
// the text files into ~/.config/biorouter/skills/<slug>/. Mirrors the on-disk
// layout produced by AddSkillModal so a downloaded skill is indistinguishable
// from a hand-imported one.

import { BIOROUTER_SKILLS_DIR } from '../skills/skillUtils';
import type { RegistrySkill } from './registry';

const TEXT_EXTENSIONS = new Set(['.md', '.txt', '.yaml', '.yml', '.json', '.py', '.sh']);

export interface InstallResult {
  ok: boolean;
  name: string;
  error?: string;
}

export async function installRegistrySkill(skill: RegistrySkill): Promise<InstallResult> {
  const dl = await window.electron.downloadRegistryAsset(skill.download);
  if ('error' in dl) return { ok: false, name: skill.name, error: dl.error };

  const extracted = await window.electron.extractSkillZip(dl.path);
  if ('error' in extracted) return { ok: false, name: skill.name, error: extracted.error };

  const destFolder = `${BIOROUTER_SKILLS_DIR}/${extracted.slug}`;
  await window.electron.ensureDirectory(destFolder);

  const textFiles = extracted.files.filter(([relPath]) => {
    const ext = relPath.slice(relPath.lastIndexOf('.')).toLowerCase();
    return TEXT_EXTENSIONS.has(ext) || !relPath.includes('.');
  });

  for (const [relPath, content] of textFiles) {
    const ok = await window.electron.writeFile(`${destFolder}/${relPath}`, content);
    if (!ok) {
      return { ok: false, name: skill.name, error: `Could not write to ${destFolder}` };
    }
  }

  return { ok: true, name: skill.name };
}
