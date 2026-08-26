import { useState, useRef, DragEvent } from 'react';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { toastSuccess, toastError } from '../../toasts';
import { Dialog, DialogContent, DialogTitle } from '../ui/dialog';
import { installSkillPackage, previewSkillPackage } from '../../api';
import type { ImportPreview, ImportRequest, ImportResult } from '../../api';

interface Props {
  onClose: () => void;
  onSaved: () => void;
}

/**
 * Add Skill.
 *
 * ⚠ **Nothing here parses an archive.** The modal used to read a `.md` in the
 * renderer and hand a `.zip` to a depth-counting daemon parser, and it had no
 * way at all to take a repository URL — so a user with
 * `https://github.com/heygen-com/hyperframes` had to ask the agent, which
 * improvised with shell commands and produced twenty unrelated top-level skills
 * (#115). Every source now goes to the one import pipeline, which reads the
 * package's own manifest and keeps a coordinated repository together.
 *
 * The preview shown below is the daemon's, not a second interpretation of the
 * same bytes.
 */
export default function AddSkillModal({ onClose, onSaved }: Props) {
  const [isDragging, setIsDragging] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [planId, setPlanId] = useState<string | null>(null);
  const [sourceLabel, setSourceLabel] = useState<string>('');
  const [url, setUrl] = useState('');
  const [busy, setBusy] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const errorText = (err: unknown) =>
    err instanceof Error ? err.message : typeof err === 'string' ? err : 'the request failed.';

  const runPreview = async (request: ImportRequest, label: string) => {
    setBusy(true);
    try {
      const response = await previewSkillPackage<true>({ body: request, throwOnError: true });
      const result = response.data as ImportResult;
      setError(null);
      setSourceLabel(label);
      setPreview(result.preview);
      setPlanId(result.status === 'needsChoice' ? result.planId : null);
    } catch (err) {
      setError(errorText(err));
      setPreview(null);
      setPlanId(null);
    } finally {
      setBusy(false);
    }
  };

  const previewFile = async (file: File) => {
    const filePath = window.electron.getPathForFile(file);
    // See `BrxtInstallModal`: an empty path means this surface cannot supply
    // one. Sending the bare name would have the daemon read whatever matching
    // archive sat in its own working directory.
    if (!filePath) {
      setError(
        'Biorouter is running on another machine, so it cannot read a file you ' +
          'drop here. Copy the skill onto that machine and add it with ' +
          '`biorouter skill install <path>`, or paste a repository URL above.'
      );
      setPreview(null);
      return;
    }
    await runPreview({ filePath }, file.name);
  };

  const handleDrop = async (e: DragEvent<HTMLDivElement>) => {
    e.preventDefault();
    setIsDragging(false);
    const file = e.dataTransfer.files[0];
    if (file) await previewFile(file);
  };

  const handleBrowse = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (file) await previewFile(file);
    e.target.value = '';
  };

  const previewUrl = async () => {
    const trimmed = url.trim();
    if (!trimmed) return;
    await runPreview({ url: trimmed }, trimmed);
  };

  const install = async (choice?: 'bundle' | 'individual') => {
    if (!preview || busy) return;
    setBusy(true);
    try {
      // Installing by `planId` rather than by source is what makes the preview
      // binding: it installs the archive that was previewed, not whatever the
      // branch points at now.
      const body: ImportRequest = planId ? { planId } : { url: url.trim() || null };
      if (choice) body.choice = choice;
      const response = await installSkillPackage<true>({ body, throwOnError: true });
      const result = response.data as ImportResult;
      if (result.status === 'needsChoice') {
        // The daemon still wants an answer; keep the fresh plan id.
        setPlanId(result.planId);
        setPreview(result.preview);
        setBusy(false);
        return;
      }
      const count = result.installed.reduce((total, one) => total + one.skills.length, 0);
      toastSuccess({
        title: result.installed[0]?.displayName ?? preview.displayName,
        msg:
          result.installed.length === 1 && result.installed[0].kind === 'bundle'
            ? `Installed ${count} skill${count === 1 ? '' : 's'}`
            : `Installed ${result.installed.length} skill${result.installed.length === 1 ? '' : 's'}`,
      });
      onSaved();
      onClose();
    } catch (err) {
      toastError({ title: 'Install failed', msg: errorText(err) });
      setBusy(false);
    }
  };

  const ambiguity = preview?.ambiguity ?? null;

  return (
    <Dialog open onOpenChange={(open) => !open && !busy && onClose()}>
      <DialogContent
        aria-describedby={undefined}
        dismissible={!busy}
        className="flex max-h-[80vh] w-[520px] flex-col gap-0 overflow-hidden p-0 sm:max-w-[520px]"
      >
        <div className="px-6 pt-5 pb-4 pr-14 border-b border-border-subtle">
          <DialogTitle>Add Skill</DialogTitle>
        </div>

        <div className="p-6 flex flex-col gap-4 overflow-y-auto">
          <div className="flex flex-col gap-1.5">
            <label htmlFor="skill-source-url" className="text-label text-text-default">
              From a repository
            </label>
            <div className="flex gap-2">
              <Input
                id="skill-source-url"
                type="text"
                placeholder="https://github.com/owner/repo"
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') void previewUrl();
                }}
                className="h-9 flex-1"
                disabled={busy}
              />
              <Button
                variant="outline"
                onClick={() => void previewUrl()}
                disabled={busy || !url.trim()}
              >
                Look up
              </Button>
            </div>
            <p className="text-supporting text-text-muted">
              A repository holding several skills stays one package, with its own name and entry
              point.
            </p>
          </div>

          <div
            onDragOver={(e) => {
              e.preventDefault();
              setIsDragging(true);
            }}
            onDragLeave={() => setIsDragging(false)}
            onDrop={handleDrop}
            onClick={() => fileInputRef.current?.click()}
            className={[
              'biorouter-modal-panel rounded-container p-8 text-center cursor-pointer select-none transition-colors',
              isDragging
                ? 'border-border-accent bg-background-accent/5'
                : error
                  ? 'border-border-danger bg-background-danger/10'
                  : 'border-border-subtle bg-background-muted hover:border-border-strong hover:bg-overlay-hover',
            ].join(' ')}
          >
            <p className="text-label text-text-default mb-1">Or drop a skill file here</p>
            <p className="text-supporting text-text-muted">
              Accepts <code>.zip</code>
            </p>
          </div>
          <input
            ref={fileInputRef}
            type="file"
            accept=".zip"
            className="hidden"
            onChange={handleBrowse}
          />

          {error && (
            <div className="text-body text-text-danger bg-background-danger/10 rounded-element px-4 py-3">
              {error}
            </div>
          )}

          {preview && <PreviewCard preview={preview} sourceLabel={sourceLabel} />}

          {ambiguity && (
            <div className="text-body text-text-default bg-background-muted rounded-element px-4 py-3">
              {ambiguity.reason}
            </div>
          )}
        </div>

        <div className="px-6 py-4 border-t border-border-subtle flex justify-end gap-2">
          <Button variant="outline" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          {ambiguity ? (
            <>
              <Button variant="outline" onClick={() => void install('individual')} disabled={busy}>
                Install separately
              </Button>
              <Button variant="default" onClick={() => void install('bundle')} disabled={busy}>
                Install as one bundle
              </Button>
            </>
          ) : (
            <Button variant="default" onClick={() => void install()} disabled={!preview || busy}>
              {busy ? 'Installing…' : installLabel(preview)}
            </Button>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}

function installLabel(preview: ImportPreview | null): string {
  if (!preview) return 'Install';
  if (preview.kind === 'single') return 'Install Skill';
  return `Install ${preview.components.length} skills`;
}

function PreviewCard({ preview, sourceLabel }: { preview: ImportPreview; sourceLabel: string }) {
  const entryPoint = preview.entryPoint;
  return (
    <div className="biorouter-modal-panel rounded-element px-4 py-3">
      <p className="text-label">
        {preview.displayName}
        {preview.version && (
          <span className="ml-2 text-supporting text-text-subtle">{preview.version}</span>
        )}
        {preview.kind === 'bundle' && (
          <span className="ml-2 text-supporting text-text-subtle">
            {preview.components.length} skill{preview.components.length === 1 ? '' : 's'}
          </span>
        )}
      </p>
      {entryPoint && (
        <p className="text-supporting text-text-muted mt-0.5">entry point: {entryPoint}</p>
      )}
      <div className="mt-1.5 max-h-[140px] overflow-y-auto">
        {preview.components.map((component) => (
          <p key={component.name} className="text-supporting text-text-muted">
            {component.entryPoint ? '→' : '·'} {component.name}
            {component.group && <span className="text-text-subtle"> [{component.group}]</span>}
            {component.description && (
              <span className="text-text-subtle">: {component.description}</span>
            )}
          </p>
        ))}
      </div>
      <p className="text-supporting text-text-subtle mt-1.5 font-mono">
        {preview.fileCount} file{preview.fileCount !== 1 ? 's' : ''} · from {sourceLabel}
      </p>
    </div>
  );
}
