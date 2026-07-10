import { useState } from 'react';
import { Button } from '../ui/button';
import { parseSkillFrontmatter, toSlug, BIOROUTER_SKILLS_DIR } from './skillUtils';
import { toastSuccess, toastError } from '../../toasts';

const TEMPLATE = `---
name: example-skill
description: A brief description of what this skill does and when to use it.
---

# Instructions

Describe the step-by-step instructions for BioRouter to follow when this skill is activated.

## Steps

1. First, do X
2. Then, do Y
3. Finally, do Z

## Notes

- Keep instructions clear and specific
- Include any constraints or edge cases
`;

interface Props {
  onClose: () => void;
  onSaved: () => void;
}

export default function CustomSkillModal({ onClose, onSaved }: Props) {
  const [content, setContent] = useState(TEMPLATE);
  const [error, setError] = useState<string | null>(null);
  const [isSaving, setIsSaving] = useState(false);

  const handleSave = async () => {
    const parsed = parseSkillFrontmatter(content);
    if (!parsed) {
      setError('File must have valid YAML frontmatter with "name" and "description" fields.');
      return;
    }
    setError(null);
    setIsSaving(true);

    const slug = toSlug(parsed.name);
    const destFolder = `${BIOROUTER_SKILLS_DIR}/${slug}`;
    await window.electron.ensureDirectory(destFolder);
    const ok = await window.electron.writeFile(`${destFolder}/SKILL.md`, content);
    setIsSaving(false);
    if (ok) {
      toastSuccess({ title: parsed.name, msg: 'Skill saved to BioRouter Skills' });
      onSaved();
      onClose();
    } else {
      toastError({ title: 'Save failed', msg: `Could not write to ${destFolder}/SKILL.md` });
    }
  };

  return (
    <div className="biorouter-modal-overlay fixed inset-0 z-50 flex items-center justify-center">
      <div className="biorouter-modal-surface bg-background-default w-[640px] max-h-[85vh] flex flex-col overflow-hidden">
        <div className="px-6 pt-5 pb-4 border-b border-border-subtle flex items-center justify-between">
          <h2 className="text-base font-semibold">Add Custom Skill</h2>
          <Button variant="ghost" size="sm" className="h-7 w-7 p-0" onClick={onClose}>
            ✕
          </Button>
        </div>

        <div className="p-6 flex flex-col gap-3 flex-1 overflow-hidden">
          <p className="text-xs text-text-muted">
            Edit the YAML frontmatter (<code>name</code> and <code>description</code> required),
            then write your skill instructions below. A folder named after the skill will be created
            in BioRouter Skills with a <code>SKILL.md</code> inside.
          </p>
          <textarea
            className="biorouter-modal-panel flex-1 min-h-[300px] font-mono text-sm rounded-lg p-3 resize-none"
            value={content}
            onChange={(e) => setContent(e.target.value)}
            spellCheck={false}
          />
          {error && (
            <div className="text-sm text-text-danger bg-background-danger/10 rounded-lg px-4 py-2">
              {error}
            </div>
          )}
        </div>

        <div className="px-6 py-4 border-t border-border-subtle flex justify-end gap-2">
          <Button variant="outline" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="default" onClick={handleSave} disabled={isSaving}>
            {isSaving ? 'Saving…' : 'Save Skill'}
          </Button>
        </div>
      </div>
    </div>
  );
}
