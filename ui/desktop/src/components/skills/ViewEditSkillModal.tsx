import { useState } from 'react';
import { Button } from '../ui/button';
import { Skill, parseSkillFrontmatter } from './skillUtils';
import { toastSuccess, toastError } from '../../toasts';
import { Share2 } from '../icons/app-icons';

interface Props {
  skill: Skill;
  onClose: () => void;
  onSaved: () => void;
}

export default function ViewEditSkillModal({ skill, onClose, onSaved }: Props) {
  const [content, setContent] = useState(skill.content);
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const isDirty = content !== skill.content;

  const handleSave = async () => {
    const parsed = parseSkillFrontmatter(content);
    if (!parsed) {
      setError('File must have valid YAML frontmatter with "name" and "description" fields.');
      return;
    }
    setError(null);
    setIsSaving(true);
    const ok = await window.electron.writeFile(skill.filePath, content);
    setIsSaving(false);
    if (ok) {
      toastSuccess({ title: parsed.name, msg: 'Skill saved' });
      onSaved();
      onClose();
    } else {
      toastError({ title: 'Save failed', msg: `Could not write to ${skill.filePath}` });
    }
  };

  const handleClose = () => {
    if (isDirty) {
      const confirmed = window.confirm('You have unsaved changes. Discard them?');
      if (!confirmed) return;
    }
    onClose();
  };

  const handleCopyToClipboard = async () => {
    try {
      await navigator.clipboard.writeText(content);
      toastSuccess({ title: skill.name, msg: 'Copied to clipboard as Markdown' });
    } catch {
      toastError({ title: 'Copy failed', msg: 'Could not copy to clipboard' });
    }
  };

  const handleExportToFile = async () => {
    const result = await window.electron.showSaveDialog({
      title: 'Export Skill',
      defaultPath: `${skill.name}.md`,
      filters: [{ name: 'Markdown', extensions: ['md'] }],
    });
    if (result.canceled || !result.filePath) return;
    const ok = await window.electron.writeFile(result.filePath, content);
    if (ok) {
      toastSuccess({ title: skill.name, msg: 'Exported successfully' });
    } else {
      toastError({ title: 'Export failed', msg: 'Could not write file' });
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="bg-background-default rounded-xl border border-border-subtle shadow-lg w-[640px] max-h-[85vh] flex flex-col">
        <div className="px-6 pt-5 pb-4 border-b border-border-subtle flex items-center justify-between">
          <div>
            <h2 className="text-base font-semibold">{skill.name}</h2>
            <p className="text-xs text-text-muted mt-0.5 font-mono">{skill.filePath}</p>
          </div>
          <div className="flex items-center gap-2">
            <Button
              variant="ghost"
              size="sm"
              className="flex items-center gap-1.5 text-xs"
              onClick={handleCopyToClipboard}
              title="Copy to clipboard"
            >
              <Share2 className="h-3.5 w-3.5" />
              Copy
            </Button>
            <Button
              variant="ghost"
              size="sm"
              className="text-xs"
              onClick={handleExportToFile}
            >
              Export
            </Button>
            <Button variant="ghost" size="sm" className="h-7 w-7 p-0" onClick={handleClose}>✕</Button>
          </div>
        </div>

        <div className="p-6 flex flex-col gap-3 flex-1 overflow-hidden">
          <textarea
            className="flex-1 min-h-[300px] font-mono text-sm bg-background-medium/30 border border-border-subtle rounded-lg p-3 resize-none outline-none focus:ring-1 focus:ring-blue-500"
            value={content}
            onChange={(e) => setContent(e.target.value)}
            spellCheck={false}
          />
          {error && (
            <div className="text-sm text-destructive bg-destructive/10 rounded-lg px-4 py-2">
              {error}
            </div>
          )}
        </div>

        <div className="px-6 py-4 border-t border-border-subtle flex justify-between items-center">
          <span className="text-xs text-text-subtle">
            {isDirty ? 'Unsaved changes' : 'No changes'}
          </span>
          <div className="flex gap-2">
            <Button variant="outline" onClick={handleClose}>Close</Button>
            <Button variant="default" onClick={handleSave} disabled={!isDirty || isSaving}>
              {isSaving ? 'Saving…' : 'Save'}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
