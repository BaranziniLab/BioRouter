import { useState, useRef, DragEvent } from 'react';
import { Button } from '../ui/button';
import { parseSkillFrontmatter, toSlug, BIOROUTER_SKILLS_DIR } from './skillUtils';
import { toastSuccess, toastError } from '../../toasts';

interface Props {
  onClose: () => void;
  onSaved: () => void;
}

interface SinglePreview {
  isBundle: false;
  name: string;
  description: string;
  slug: string;
  files: [string, string][];
  label: string;
}

interface BundlePreview {
  isBundle: true;
  bundleName: string;
  slug: string;
  bundleSkills: Array<{ name: string; description: string }>;
  files: [string, string][];
  label: string;
}

type Preview = SinglePreview | BundlePreview;


export default function AddSkillModal({ onClose, onSaved }: Props) {
  const [isDragging, setIsDragging] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState<Preview | null>(null);
  const [isInstalling, setIsInstalling] = useState(false);
  const mdInputRef = useRef<HTMLInputElement>(null);

  const processMdFile = (file: File) => {
    if (!file.name.endsWith('.md')) {
      setError('Only .md files or folders with SKILL.md are accepted.');
      setPreview(null);
      return;
    }
    const reader = new FileReader();
    reader.onerror = () => { setError('Failed to read file.'); setPreview(null); };
    reader.onload = (e) => {
      const content = e.target?.result as string;
      const parsed = parseSkillFrontmatter(content);
      if (!parsed) {
        setError('File must have valid YAML frontmatter with "name" and "description" fields.');
        setPreview(null);
        return;
      }
      setError(null);
      setPreview({
        isBundle: false,
        name: parsed.name,
        description: parsed.description,
        slug: toSlug(parsed.name) || toSlug(file.name),
        files: [['SKILL.md', content]],
        label: file.name,
      });
    };
    reader.readAsText(file);
  };

  const handleDrop = async (e: DragEvent<HTMLDivElement>) => {
    e.preventDefault();
    setIsDragging(false);
    const file = e.dataTransfer.files[0];
    if (!file) return;
    if (file.name.endsWith('.zip')) {
      await processZipFile(file);
    } else {
      processMdFile(file);
    }
  };

  const handleMdBrowse = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (file) {
      if (file.name.endsWith('.zip')) {
        await processZipFile(file);
      } else {
        processMdFile(file);
      }
    }
    e.target.value = '';
  };

  const processZipFile = async (file: File) => {
    const filePath = window.electron.getPathForFile(file);
    const result = await window.electron.extractSkillZip(filePath);
    if ('error' in result) {
      setError(result.error);
      setPreview(null);
      return;
    }
    setError(null);
    if (result.isBundle) {
      setPreview({
        isBundle: true,
        bundleName: result.bundleName,
        slug: result.slug,
        bundleSkills: result.bundleSkills,
        files: result.files,
        label: file.name,
      });
    } else {
      setPreview({
        isBundle: false,
        name: result.name,
        description: result.description,
        slug: result.slug,
        files: result.files,
        label: file.name,
      });
    }
  };

  const handleInstall = async () => {
    if (!preview) return;
    setIsInstalling(true);

    const destFolder = `${BIOROUTER_SKILLS_DIR}/${preview.slug}`;
    await window.electron.ensureDirectory(destFolder);
    const TEXT_EXTENSIONS = new Set(['.md', '.txt', '.yaml', '.yml', '.json', '.py', '.sh']);
    const textFiles = preview.files.filter(([relPath]) => {
      const ext = relPath.slice(relPath.lastIndexOf('.')).toLowerCase();
      return TEXT_EXTENSIONS.has(ext) || !relPath.includes('.');
    });
    let allOk = true;
    for (const [relPath, content] of textFiles) {
      const ok = await window.electron.writeFile(`${destFolder}/${relPath}`, content);
      if (!ok) { allOk = false; break; }
    }
    setIsInstalling(false);

    if (allOk) {
      const displayName = preview.isBundle ? preview.bundleName : preview.name;
      toastSuccess({ title: displayName, msg: 'Installed to BioRouter Skills' });
      onSaved();
      onClose();
    } else {
      toastError({ title: 'Install failed', msg: `Could not write to ${destFolder}` });
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
            onClick={() => mdInputRef.current?.click()}
            className={[
              'border rounded-xl p-10 text-center cursor-pointer select-none transition-colors duration-150',
              isDragging
                ? 'border-block-teal bg-block-teal/5'
                : error
                ? 'border-border-danger bg-background-danger/10'
                : 'border-border-subtle bg-background-muted hover:border-border-strong hover:bg-background-medium',
            ].join(' ')}
          >
            <p className="text-sm font-medium text-text-default mb-1">
              Drop a skill file here
            </p>
            <p className="text-xs text-text-muted">
              or click to browse — accepts <code>.md</code> or <code>.zip</code>
            </p>
          </div>
          <input ref={mdInputRef} type="file" accept=".md,.zip" className="hidden" onChange={handleMdBrowse} />

          {error && (
            <div className="text-sm text-destructive bg-destructive/10 rounded-lg px-4 py-3">
              {error}
            </div>
          )}

          {preview && !preview.isBundle && (
            <div className="bg-background-medium/30 rounded-lg px-4 py-3">
              <p className="text-sm font-semibold">{preview.name}</p>
              <p className="text-xs text-text-muted mt-0.5">{preview.description}</p>
              <p className="text-[11px] text-text-subtle mt-1 font-mono">
                {preview.files.length} file{preview.files.length !== 1 ? 's' : ''} · from {preview.label}
              </p>
            </div>
          )}

          {preview && preview.isBundle && (
            <div className="bg-background-medium/30 rounded-lg px-4 py-3">
              <p className="text-sm font-semibold">
                Bundle: {preview.bundleName}
                <span className="ml-2 text-xs text-text-subtle font-normal">
                  {preview.bundleSkills.length} skills
                </span>
              </p>
              <div className="mt-1.5 max-h-[120px] overflow-y-auto">
                {preview.bundleSkills.map((s) => (
                  <p key={s.name} className="text-xs text-text-muted leading-relaxed">
                    · {s.name}
                    {s.description && (
                      <span className="text-text-subtle"> — {s.description}</span>
                    )}
                  </p>
                ))}
              </div>
              <p className="text-[11px] text-text-subtle mt-1.5 font-mono">
                {preview.files.length} file{preview.files.length !== 1 ? 's' : ''} · from {preview.label}
              </p>
            </div>
          )}
        </div>

        <div className="px-6 py-4 border-t border-border-subtle flex justify-end gap-2">
          <Button variant="outline" onClick={onClose}>Cancel</Button>
          <Button variant="default" onClick={handleInstall} disabled={!preview || isInstalling}>
            {isInstalling
              ? 'Installing…'
              : preview?.isBundle
              ? `Install Bundle (${preview.bundleSkills.length} skills)`
              : 'Install Skill'}
          </Button>
        </div>
      </div>
    </div>
  );
}
