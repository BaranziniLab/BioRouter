// ui/desktop/src/components/BrxtInstallModal.tsx
import { useState, useCallback, useRef } from 'react';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from './ui/dialog';
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
}

export function BrxtInstallModal({ onClose, onInstalled }: Props) {
  const [step, setStep] = useState<Step>('drop');
  const [filePath, setFilePath] = useState<string | null>(null);
  const [manifest, setManifest] = useState<BrxtManifest | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isValidating, setIsValidating] = useState(false);
  const [isInstalling, setIsInstalling] = useState(false);
  const [isDragging, setIsDragging] = useState(false);
  const [envEntries, setEnvEntries] = useState<EnvEntry[]>([]);
  const [showOptional, setShowOptional] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const { addExtension } = useConfig();

  const processFile = useCallback(async (file: File) => {
    setError(null);
    setManifest(null);
    setIsValidating(true);
    const fp = window.electron.getPathForFile(file);
    setFilePath(fp);

    const result = await window.electron.validateBrxtBundle(fp);
    setIsValidating(false);

    if ('error' in result) {
      setError(result.error);
    } else {
      setManifest(result.manifest);
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
  }, []);

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
      processFile(file);
    },
    [processFile]
  );

  const handleFileInput = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const file = e.target.files?.[0];
      if (file) processFile(file);
      // reset so same file can be re-selected after an error
      e.target.value = '';
    },
    [processFile]
  );

  const setEnvValue = (key: string, value: string) =>
    setEnvEntries((prev) => prev.map((e) => (e.key === key ? { ...e, value } : e)));

  const handleInstall = async () => {
    if (!filePath || !manifest) return;
    setIsInstalling(true);
    setError(null);

    const result = await window.electron.installBrxtBundle(filePath, manifest.name);

    if ('error' in result) {
      setError(result.error);
      setIsInstalling(false);
      return;
    }

    const { installDir } = result;

    // Store secrets in BioRouter's secret store
    for (const entry of envEntries.filter((e) => e.secret && e.value)) {
      await upsertConfig({
        body: { is_secret: true, key: entry.key, value: entry.value },
      }).catch(() => {});
    }

    // Build envs map for extension config
    const envs: Record<string, string> = {};
    envEntries.forEach(({ key, value }) => { if (value) envs[key] = value; });

    const extensionConfig = {
      name: manifest.name,
      display_name: manifest.display_name,
      description: manifest.description,
      type: 'stdio' as const,
      cmd: `uv run --directory "${installDir}" ${manifest.entry_point}`,
      args: [] as string[],
      envs,
      timeout: 300,
    };

    try {
      await activateExtensionDefault({ addToConfig: addExtension, extensionConfig });
      toastService.success({
        title: manifest.display_name,
        msg: 'Extension installed and enabled',
      });
      onInstalled();
      onClose();
    } catch {
      setError('Extension installed but failed to register. Try adding it manually from the Extensions tab.');
      setIsInstalling(false);
    }
  };

  const requiredVars = envEntries.filter((e) => e.required);
  const optionalVars = envEntries.filter((e) => !e.required);
  const requiredMissing = requiredVars.some((e) => !e.value.trim());

  return (
    <Dialog open={true} onOpenChange={onClose}>
      <DialogContent className="sm:max-w-[560px] max-h-[90vh] overflow-y-auto">
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
                'border-2 border-dashed rounded-xl p-10 text-center transition-colors cursor-pointer select-none',
                isDragging
                  ? 'border-blue-400 bg-blue-50 dark:bg-blue-900/10'
                  : error
                  ? 'border-red-400 bg-red-50 dark:bg-red-900/10'
                  : 'border-border-subtle hover:border-border-strong',
              ].join(' ')}
              onDragOver={(e) => { e.preventDefault(); setIsDragging(true); }}
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
                  <p className="text-3xl mb-2">📦</p>
                  <p className="text-sm font-medium mb-1">Drop your .brxt file here</p>
                  <p className="text-xs text-text-muted mb-3">or click to browse</p>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={(e) => { e.stopPropagation(); fileInputRef.current?.click(); }}
                  >
                    Browse file…
                  </Button>
                </>
              )}
            </div>

            {/* Error banner */}
            {error && (
              <div className="bg-red-50 dark:bg-red-900/10 border border-red-300 dark:border-red-700 rounded-lg p-3">
                <p className="text-sm text-red-600 dark:text-red-400">{error}</p>
              </div>
            )}

            {/* Manifest preview card */}
            {manifest && !error && (
              <div className="bg-background-medium border border-border-subtle rounded-lg p-4">
                <p className="text-xs text-text-muted uppercase tracking-wide mb-2">
                  Detected from bundle
                </p>
                <p className="text-sm font-semibold">{manifest.display_name}</p>
                <p className="text-xs text-text-muted mt-0.5">
                  v{manifest.version}
                  {manifest.tools_count ? ` · ${manifest.tools_count} tools` : ''}
                  {' · '}
                  {requiredVars.length} required env var
                  {requiredVars.length !== 1 ? 's' : ''}
                </p>
                <p className="text-sm text-text-default mt-2">{manifest.description}</p>
              </div>
            )}

            <DialogFooter>
              <Button variant="outline" onClick={onClose}>
                Cancel
              </Button>
              <Button
                disabled={!manifest || !!error || isValidating}
                onClick={() => setStep('configure')}
              >
                Next: Configure →
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
                      {entry.key} <span className="text-red-500">*</span>
                    </label>
                    <input
                      type={entry.secret ? 'password' : 'text'}
                      className="w-full border border-border-subtle rounded-md px-3 py-2 text-sm bg-background-default focus:outline-none focus:ring-2 focus:ring-blue-400"
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
                          className="w-full border border-border-subtle rounded-md px-3 py-2 text-sm bg-background-subtle focus:outline-none focus:ring-2 focus:ring-blue-400"
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
              <div className="bg-red-50 dark:bg-red-900/10 border border-red-300 dark:border-red-700 rounded-lg p-3">
                <p className="text-sm text-red-600 dark:text-red-400">{error}</p>
              </div>
            )}

            <DialogFooter>
              <Button
                variant="outline"
                onClick={() => { setStep('drop'); setError(null); }}
              >
                ← Back
              </Button>
              <Button variant="outline" onClick={onClose}>
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
