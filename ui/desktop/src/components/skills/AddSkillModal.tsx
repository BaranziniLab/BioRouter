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

/** Walk a FileList from a webkitdirectory input, detect single skill or bundle. */
function parseUploadedFolder(fileList: FileList): Promise<Preview> {
  return new Promise((resolve, reject) => {
    const files = Array.from(fileList);
    if (files.length === 0) { reject(new Error('No files found in folder.')); return; }

    const topFolder = files[0].webkitRelativePath.split('/')[0] ?? 'skill';

    // Check for root SKILL.md: path like "topFolder/SKILL.md" (2 parts)
    const rootSkillMdFile = files.find((f) => {
      const parts = f.webkitRelativePath.split('/');
      return parts.length === 2 && f.name === 'SKILL.md';
    });

    if (rootSkillMdFile) {
      // --- Single skill ---
      const skillReader = new FileReader();
      skillReader.onerror = () => reject(new Error('Failed to read SKILL.md'));
      skillReader.onload = (e) => {
        const skillMdContent = e.target?.result as string;
        const parsed = parseSkillFrontmatter(skillMdContent);
        if (!parsed) {
          reject(new Error('SKILL.md must have valid YAML frontmatter with "name" and "description".'));
          return;
        }
        const slug = toSlug(parsed.name) || toSlug(topFolder);
        const filePairs: [string, string][] = [];
        let remaining = files.length;
        files.forEach((file) => {
          const rel = file.webkitRelativePath.replace(/^[^/]+\/?/, '') || file.name;
          const fr = new FileReader();
          fr.onerror = () => reject(new Error(`Failed to read ${file.name}`));
          fr.onload = (ev) => {
            filePairs.push([rel, ev.target?.result as string]);
            remaining--;
            if (remaining === 0) {
              resolve({ isBundle: false, name: parsed.name, description: parsed.description, slug, files: filePairs, label: topFolder });
            }
          };
          fr.readAsText(file);
        });
      };
      skillReader.readAsText(rootSkillMdFile);
      return;
    }

    // --- Bundle detection: sub-level SKILL.md at "topFolder/<sub>/SKILL.md" (3 parts) ---
    const subSkillMdFiles = files.filter((f) => {
      const parts = f.webkitRelativePath.split('/');
      return parts.length === 3 && f.name === 'SKILL.md';
    });

    if (subSkillMdFiles.length === 0) {
      reject(new Error('Folder must contain a SKILL.md file, or sub-folders that each contain a SKILL.md.'));
      return;
    }

    const bundleSkills: Array<{ name: string; description: string }> = [];
    let skillPending = subSkillMdFiles.length;
    const filePairs: [string, string][] = [];
    let filePending = files.length;

    const tryResolve = () => {
      if (skillPending !== 0 || filePending !== 0) return;
      if (bundleSkills.length === 0) {
        reject(new Error('No valid SKILL.md files found in sub-folders.'));
        return;
      }
      resolve({
        isBundle: true,
        bundleName: topFolder,
        slug: toSlug(topFolder),
        bundleSkills,
        files: filePairs,
        label: topFolder,
      });
    };

    subSkillMdFiles.forEach((skillMdFile) => {
      const fr = new FileReader();
      fr.onerror = () => { skillPending--; tryResolve(); };
      fr.onload = (e) => {
        const content = e.target?.result as string;
        const parsed = parseSkillFrontmatter(content);
        if (parsed) bundleSkills.push(parsed);
        skillPending--;
        tryResolve();
      };
      fr.readAsText(skillMdFile);
    });

    files.forEach((file) => {
      const rel = file.webkitRelativePath.replace(/^[^/]+\/?/, '') || file.name;
      const fr = new FileReader();
      fr.onerror = () => { filePending--; tryResolve(); };
      fr.onload = (ev) => {
        filePairs.push([rel, ev.target?.result as string]);
        filePending--;
        tryResolve();
      };
      fr.readAsText(file);
    });
  });
}

export default function AddSkillModal({ onClose, onSaved }: Props) {
  const [isDragging, setIsDragging] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState<Preview | null>(null);
  const [isInstalling, setIsInstalling] = useState(false);
  const mdInputRef = useRef<HTMLInputElement>(null);
  const folderInputRef = useRef<HTMLInputElement>(null);
  const zipInputRef = useRef<HTMLInputElement>(null);

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

  const handleMdBrowse = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (file) processMdFile(file);
    e.target.value = '';
  };

  const handleFolderBrowse = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const files = e.target.files;
    if (!files || files.length === 0) return;
    try {
      const p = await parseUploadedFolder(files);
      setError(null);
      setPreview(p);
    } catch (err) {
      setError((err as Error).message);
      setPreview(null);
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

  const handleZipBrowse = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (file) await processZipFile(file);
    e.target.value = '';
  };

  const handleInstall = async () => {
    if (!preview) return;
    setIsInstalling(true);

    const destFolder = `${BIOROUTER_SKILLS_DIR}/${preview.slug}`;
    await window.electron.ensureDirectory(destFolder);
    let allOk = true;
    for (const [relPath, content] of preview.files) {
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
            className={`border-2 border-dashed rounded-lg p-8 text-center cursor-pointer transition-colors ${
              isDragging
                ? 'border-blue-500 bg-blue-50 dark:bg-blue-900/20'
                : 'border-border-subtle hover:border-blue-400 hover:bg-background-medium/30'
            }`}
          >
            <p className="text-sm text-text-muted">
              Drop a <code>.md</code> or <code>.zip</code> skill file here, or{' '}
              <span className="text-blue-600 underline">browse for file</span>
            </p>
            <p className="text-xs text-text-subtle mt-1">
              File needs YAML frontmatter with <code>name</code> and <code>description</code>.
              A folder named after the skill with <code>SKILL.md</code> inside will be created.
            </p>
          </div>
          <input ref={mdInputRef} type="file" accept=".md" className="hidden" onChange={handleMdBrowse} />

          {/* Folder upload */}
          <div className="flex items-center gap-3">
            <div className="h-px flex-1 bg-border-subtle" />
            <span className="text-xs text-text-subtle">or</span>
            <div className="h-px flex-1 bg-border-subtle" />
          </div>
          <Button
            variant="outline"
            className="w-full"
            onClick={() => folderInputRef.current?.click()}
          >
            Browse for Skill Folder
          </Button>
          <p className="text-xs text-text-subtle -mt-2 text-center">
            Folder with <code>SKILL.md</code> (single skill) or sub-folders each with <code>SKILL.md</code> (bundle)
          </p>
          <input
            ref={folderInputRef}
            type="file"
            // @ts-expect-error -- webkitdirectory is non-standard but supported by Electron/Chromium
            webkitdirectory=""
            className="hidden"
            onChange={handleFolderBrowse}
          />
          <Button
            variant="outline"
            className="w-full"
            onClick={() => zipInputRef.current?.click()}
          >
            Browse for Skill ZIP
          </Button>
          <p className="text-xs text-text-subtle -mt-2 text-center">
            ZIP with <code>SKILL.md</code> (single skill) or a bundle folder containing sub-skills
          </p>
          <input
            ref={zipInputRef}
            type="file"
            accept=".zip"
            className="hidden"
            onChange={handleZipBrowse}
          />

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
                {preview.bundleSkills.map((s, i) => (
                  <p key={i} className="text-xs text-text-muted leading-relaxed">
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
