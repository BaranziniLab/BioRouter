import { useState, useRef, DragEvent } from 'react';
import { Button } from '../ui/button';
import { parseSkillFrontmatter, BIOROUTER_SKILLS_DIR } from './skillUtils';
import { toastSuccess, toastError } from '../../toasts';

interface Props {
  onClose: () => void;
  onSaved: () => void;
}

export default function AddSkillModal({ onClose, onSaved }: Props) {
  const [isDragging, setIsDragging] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState<{ name: string; description: string; content: string; filename: string } | null>(null);
  const [isInstalling, setIsInstalling] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const processFile = (file: File) => {
    if (!file.name.endsWith('.md')) {
      setError('Only .md files are accepted.');
      setPreview(null);
      return;
    }
    const reader = new FileReader();
    reader.onerror = () => {
      setError('Failed to read file. Please try again.');
      setPreview(null);
    };
    reader.onload = (e) => {
      const content = e.target?.result as string;
      const parsed = parseSkillFrontmatter(content);
      if (!parsed) {
        setError('File must have valid YAML frontmatter with "name" and "description" fields.');
        setPreview(null);
        return;
      }
      setError(null);
      setPreview({ name: parsed.name, description: parsed.description, content, filename: file.name });
    };
    reader.readAsText(file);
  };

  const handleDrop = (e: DragEvent<HTMLDivElement>) => {
    e.preventDefault();
    setIsDragging(false);
    const file = e.dataTransfer.files[0];
    if (file) processFile(file);
  };

  const handleBrowse = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (file) processFile(file);
    e.target.value = '';  // allow re-selecting same file
  };

  const handleInstall = async () => {
    if (!preview) return;
    setIsInstalling(true);
    await window.electron.ensureDirectory(BIOROUTER_SKILLS_DIR);
    const destPath = `${BIOROUTER_SKILLS_DIR}/${preview.filename}`;
    const ok = await window.electron.writeFile(destPath, preview.content);
    setIsInstalling(false);
    if (ok) {
      toastSuccess({ title: preview.name, msg: 'Skill added to BioRouter Skills' });
      onSaved();
      onClose();
    } else {
      toastError({ title: 'Install failed', msg: `Could not write to ${destPath}` });
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="bg-background-default rounded-xl border border-border-subtle shadow-lg w-[480px] max-h-[80vh] flex flex-col">
        <div className="px-6 pt-5 pb-4 border-b border-border-subtle flex items-center justify-between">
          <h2 className="text-base font-semibold">Add Skill</h2>
          <Button variant="ghost" size="sm" className="h-7 w-7 p-0" onClick={onClose}>✕</Button>
        </div>

        <div className="p-6 flex flex-col gap-4 overflow-y-auto">
          {/* Drop zone */}
          <div
            onDragOver={(e) => { e.preventDefault(); setIsDragging(true); }}
            onDragLeave={() => setIsDragging(false)}
            onDrop={handleDrop}
            onClick={() => fileInputRef.current?.click()}
            className={`border-2 border-dashed rounded-lg p-8 text-center cursor-pointer transition-colors ${
              isDragging
                ? 'border-blue-500 bg-blue-50 dark:bg-blue-900/20'
                : 'border-border-subtle hover:border-blue-400 hover:bg-background-medium/30'
            }`}
          >
            <p className="text-sm text-text-muted">
              Drop a <code>.md</code> skill file here, or <span className="text-blue-600 underline">browse</span>
            </p>
            <p className="text-xs text-text-subtle mt-1">File must have YAML frontmatter with name and description</p>
          </div>
          <input
            ref={fileInputRef}
            type="file"
            accept=".md"
            className="hidden"
            onChange={handleBrowse}
          />

          {/* Error */}
          {error && (
            <div className="text-sm text-destructive bg-destructive/10 rounded-lg px-4 py-3">
              {error}
            </div>
          )}

          {/* Preview */}
          {preview && (
            <div className="bg-background-medium/30 rounded-lg px-4 py-3">
              <p className="text-sm font-semibold">{preview.name}</p>
              <p className="text-xs text-text-muted mt-0.5">{preview.description}</p>
            </div>
          )}
        </div>

        <div className="px-6 py-4 border-t border-border-subtle flex justify-end gap-2">
          <Button variant="outline" onClick={onClose}>Cancel</Button>
          <Button
            variant="default"
            onClick={handleInstall}
            disabled={!preview || isInstalling}
          >
            {isInstalling ? 'Installing…' : 'Install Skill'}
          </Button>
        </div>
      </div>
    </div>
  );
}
