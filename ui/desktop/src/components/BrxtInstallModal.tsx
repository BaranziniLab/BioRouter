// ui/desktop/src/components/BrxtInstallModal.tsx
import { useState, useCallback, useRef, useEffect } from 'react';
import { Package } from './icons/app-icons';
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from './ui/dialog';
import { Button } from './ui/button';
import { BrxtEnvVar, BrxtManifest } from '../types/brxt';
import { useConfig } from './ConfigContext';
import { activateExtensionDefault } from './settings/extensions';
import { upsertConfig } from '../api';
import { toastService } from '../toasts';

interface EnvEntry {
  key: string;
  value: string;
  secret: boolean;
  required: boolean;
  description: string;
  auto_propagate: boolean;
}

type Step = 'drop' | 'configure';

interface Props {
  onClose: () => void;
  onInstalled: () => void;
  preloadedFilePath?: string;
}

export function BrxtInstallModal({ onClose, onInstalled, preloadedFilePath }: Props) {
  const [step, setStep] = useState<Step>('drop');
  const [filePath, setFilePath] = useState<string | null>(null);
  const [manifest, setManifest] = useState<BrxtManifest | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isValidating, setIsValidating] = useState(false);
  const [isInstalling, setIsInstalling] = useState(false);
  const [isDragging, setIsDragging] = useState(false);
  const [envEntries, setEnvEntries] = useState<EnvEntry[]>([]);
  const [skillsPreview, setSkillsPreview] = useState<
    Array<{ slug: string; name: string; description: string }>
  >([]);
  const [showOptional, setShowOptional] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const { addExtension } = useConfig();

  const processFile = useCallback(async (fp: string) => {
    setError(null);
    setManifest(null);
    setSkillsPreview([]);
    setIsValidating(true);
    setFilePath(fp);

    try {
      const result = await window.electron.validateBrxtBundle(fp);
      if ('error' in result) {
        setError(result.error);
      } else {
        setManifest(result.manifest);
        setSkillsPreview(result.skillsPreview);
        setEnvEntries(
          result.manifest.env_vars.map((v: BrxtEnvVar) => ({
            key: v.key,
            value: v.auto_propagate && v.default ? v.default : '',
            secret: v.secret,
            required: v.required,
            description: v.description,
            auto_propagate: v.auto_propagate,
          }))
        );
      }
    } catch (error) {
      setError(error instanceof Error ? error.message : 'Could not read the extension bundle');
    } finally {
      setIsValidating(false);
    }
  }, []);

  useEffect(() => {
    if (preloadedFilePath) {
      processFile(preloadedFilePath);
    }
  }, [preloadedFilePath, processFile]);

  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      setIsDragging(false);
      const file = e.dataTransfer.files[0];
      if (!file) return;
      if (!file.name.endsWith('.brxt')) {
        setError('Please drop a .brxt file');
        return;
      }
      processFile(window.electron.getPathForFile(file));
    },
    [processFile]
  );

  const handleFileInput = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const file = e.target.files?.[0];
      if (file) processFile(window.electron.getPathForFile(file));
      // reset so same file can be re-selected after an error
      e.target.value = '';
    },
    [processFile]
  );

  const setEnvValue = (key: string, value: string) =>
    setEnvEntries((prev) => prev.map((e) => (e.key === key ? { ...e, value } : e)));

  const handleNext = () => {
    if (manifest && manifest.env_vars.length === 0) {
      handleInstall();
    } else {
      setStep('configure');
    }
  };

  const handleInstall = async () => {
    if (!filePath || !manifest || isInstalling) return;
    setIsInstalling(true);
    setError(null);

    try {
      const result = await window.electron.installBrxtBundle(filePath, manifest.name);

      if ('error' in result) {
        setError(result.error);
        return;
      }

      const { installDir } = result;

      // Store secrets in BioRouter's keyring; track which ones succeeded
      const secretEnvKeys: string[] = [];
      for (const entry of envEntries.filter((e) => e.secret && e.value.trim())) {
        const res = await upsertConfig({
          body: { is_secret: true, key: entry.key, value: entry.value },
        }).catch(() => null);
        if (res && !res.error) {
          secretEnvKeys.push(entry.key);
        }
      }

      // Non-secret values go in envs; secrets that failed keyring storage are fallback-included
      const envs: Record<string, string> = {};
      envEntries.forEach(({ key, value, secret }) => {
        if (!value.trim()) return;
        if (!secret || !secretEnvKeys.includes(key)) envs[key] = value;
      });

      const extensionConfig = {
        name: manifest.name,
        description: manifest.description,
        type: 'stdio' as const,
        cmd: 'uv',
        args: ['run', '--directory', installDir, manifest.entry_point],
        envs,
        env_keys: secretEnvKeys,
        timeout: 300,
      };

      try {
        await activateExtensionDefault({ addToConfig: addExtension, extensionConfig });
      } catch {
        setError(
          'Extension installed but failed to register. Try adding it manually from the Extensions tab.'
        );
        return;
      }
      toastService.success({
        title: manifest.display_name,
        msg: 'Extension installed and enabled',
      });
      onInstalled();
      onClose();
    } catch (error) {
      setError(error instanceof Error ? error.message : 'Could not install the extension bundle');
    } finally {
      setIsInstalling(false);
    }
  };

  const requiredVars = envEntries.filter((e) => e.required);
  const optionalVars = envEntries.filter((e) => !e.required);
  const requiredMissing = requiredVars.some((e) => !e.value.trim());
  const isBusy = isValidating || isInstalling;

  return (
    <Dialog open onOpenChange={(open) => !open && !isBusy && onClose()}>
      <DialogContent
        dismissible={!isBusy}
        className="sm:max-w-[560px] max-h-[90vh] overflow-y-auto"
      >
        <DialogHeader>
          <DialogTitle>
            {step === 'drop' ? 'Add Extension' : `Configure ${manifest?.display_name ?? ''}`}
          </DialogTitle>
        </DialogHeader>

        {step === 'drop' && (
          <div className="py-4 space-y-4">
            <p className="text-sm text-text-muted">
              Install a BioRouter extension bundle (.brxt file).
            </p>

            {/* Drop zone */}
            <div
              className={[
                'biorouter-modal-panel rounded-xl p-10 text-center transition-colors cursor-pointer select-none',
                isDragging
                  ? '!border-block-teal bg-block-teal/5'
                  : error
                    ? '!border-border-danger bg-background-danger/10'
                    : 'hover:bg-background-medium',
              ].join(' ')}
              onDragOver={(e) => {
                e.preventDefault();
                setIsDragging(true);
              }}
              onDragLeave={() => setIsDragging(false)}
              onDrop={handleDrop}
              onClick={() => fileInputRef.current?.click()}
            >
              <input
                ref={fileInputRef}
                type="file"
                accept=".brxt"
                className="hidden"
                onChange={handleFileInput}
              />
              {isValidating ? (
                <p className="text-sm text-text-muted animate-pulse">Reading bundle…</p>
              ) : (
                <>
                  <Package className="w-10 h-10 mx-auto mb-2 text-text-muted" />
                  <p className="text-sm font-medium mb-1">Drop your .brxt file here</p>
                  <p className="text-xs text-text-muted mb-3">or click to browse</p>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={async (e) => {
                      e.stopPropagation();
                      const fp = await window.electron.openBrxtFilePicker();
                      if (fp) processFile(fp);
                    }}
                  >
                    Browse file…
                  </Button>
                </>
              )}
            </div>

            {/* Error banner */}
            {error && (
              <div className="bg-background-danger/10 border border-border-danger/40 rounded-lg p-3">
                <p className="text-sm text-text-danger">{error}</p>
              </div>
            )}

            {/* Manifest preview card */}
            {manifest && !error && (
              <div className="biorouter-modal-panel rounded-xl p-4">
                <p className="text-xs text-text-muted uppercase tracking-wide mb-2">
                  Detected from bundle
                </p>
                <p className="text-sm font-semibold">{manifest.display_name}</p>
                <p className="text-xs text-text-muted mt-0.5">
                  v{manifest.version}
                  {manifest.tools_count ? ` · ${manifest.tools_count} tools` : ''}
                  {skillsPreview.length > 0
                    ? ` · ${skillsPreview.length} skill${skillsPreview.length !== 1 ? 's' : ''}`
                    : ''}
                  {' · '}
                  {requiredVars.length} required env var
                  {requiredVars.length !== 1 ? 's' : ''}
                </p>
                <p className="text-sm text-text-default mt-2">{manifest.description}</p>
                {skillsPreview.length > 0 && (
                  <div className="mt-2 pt-2 shadow-[inset_0_1px_0_color-mix(in_srgb,var(--border-subtle)_45%,transparent)]">
                    <p className="text-xs font-semibold text-text-muted uppercase tracking-wide mb-1">
                      Skills included
                    </p>
                    {skillsPreview.map((skill) => (
                      <p key={skill.slug} className="text-xs text-text-muted leading-relaxed">
                        · <span className="font-medium">{skill.name}</span> — {skill.description}
                      </p>
                    ))}
                  </div>
                )}
              </div>
            )}

            <DialogFooter>
              <Button variant="outline" onClick={onClose} disabled={isBusy}>
                Cancel
              </Button>
              <Button
                disabled={!manifest || !!error || isValidating || isInstalling}
                onClick={handleNext}
              >
                {isInstalling ? 'Installing…' : 'Next: Configure →'}
              </Button>
            </DialogFooter>
          </div>
        )}

        {step === 'configure' && manifest && (
          <div className="py-4 space-y-4">
            <p className="text-sm text-text-muted">
              Fill in required credentials. Optional fields are pre-filled with defaults.
            </p>

            {requiredVars.length > 0 && (
              <div className="space-y-3">
                <p className="text-xs font-semibold text-text-default uppercase tracking-wide">
                  Required
                </p>
                {requiredVars.map((entry) => (
                  <div key={entry.key}>
                    <label className="block text-xs font-semibold mb-1">
                      {entry.key} <span className="text-text-danger">*</span>
                    </label>
                    <input
                      type={entry.secret ? 'password' : 'text'}
                      className="biorouter-modal-panel w-full rounded-md px-3 py-2 text-sm "
                      placeholder={entry.description}
                      value={entry.value}
                      onChange={(e) => setEnvValue(entry.key, e.target.value)}
                      autoComplete="off"
                    />
                  </div>
                ))}
              </div>
            )}

            {optionalVars.length > 0 && (
              <div>
                <button
                  type="button"
                  className="text-xs text-text-muted underline"
                  onClick={() => setShowOptional((v) => !v)}
                >
                  {showOptional ? 'Hide' : 'Show'} {optionalVars.length} optional variable
                  {optionalVars.length !== 1 ? 's' : ''} (pre-filled with defaults)
                </button>
                {showOptional && (
                  <div className="space-y-3 mt-3">
                    <p className="text-xs font-semibold text-text-muted uppercase tracking-wide">
                      Optional
                    </p>
                    {optionalVars.map((entry) => (
                      <div key={entry.key}>
                        <label className="block text-xs font-medium text-text-muted mb-1">
                          {entry.key}
                        </label>
                        <input
                          type={entry.secret ? 'password' : 'text'}
                          className="biorouter-modal-panel w-full rounded-md px-3 py-2 text-sm "
                          placeholder={entry.description}
                          value={entry.value}
                          onChange={(e) => setEnvValue(entry.key, e.target.value)}
                          autoComplete="off"
                        />
                      </div>
                    ))}
                  </div>
                )}
              </div>
            )}

            {error && (
              <div className="bg-background-danger/10 border border-border-danger/40 rounded-lg p-3">
                <p className="text-sm text-text-danger">{error}</p>
              </div>
            )}

            <DialogFooter>
              <Button
                variant="outline"
                disabled={isBusy}
                onClick={() => {
                  setStep('drop');
                  setError(null);
                }}
              >
                ← Back
              </Button>
              <Button variant="outline" onClick={onClose} disabled={isBusy}>
                Cancel
              </Button>
              <Button disabled={requiredMissing || isInstalling} onClick={handleInstall}>
                {isInstalling ? 'Installing…' : 'Install Extension'}
              </Button>
            </DialogFooter>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
