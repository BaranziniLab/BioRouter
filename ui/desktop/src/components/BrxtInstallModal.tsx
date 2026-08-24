// ui/desktop/src/components/BrxtInstallModal.tsx
import { useState, useCallback, useRef, useEffect } from 'react';
import { Package } from './icons/app-icons';
import { ModalShell } from './ModalShell';
import { Button } from './ui/button';
import { PrivacyBadge } from './ui/PrivacyBadge';
import { BrxtEnvVar, BrxtManifest } from '../types/brxt';
import { useConfig } from './ConfigContext';
import { activateExtensionDefault } from './settings/extensions';
import { classifyExtension } from './settings/extensions/extensionPrivacy';
import { upsertConfig } from '../api';
import { userActionHeaders } from '../utils/userAction';
import { toastService } from '../toasts';
import { DependencyErrorBanner } from './DependencyErrorBanner';
import {
  marketplacePhase,
  primaryInstallAction,
  resultingPrivacyTier,
  type BrxtInstallOrigin,
} from './brxtInstallFlow';

interface EnvEntry {
  key: string;
  value: string;
  secret: boolean;
  required: boolean;
  description: string;
  auto_propagate: boolean;
}

/**
 * Issue #116. The two entry modes share this component's validation and install
 * internals and nothing else, so the first step is named for what it *is* in
 * each — picking a file, or reviewing the extension the user already picked —
 * rather than for the local route's drop zone.
 */
type Step = 'select' | 'configure';

/**
 * A label has to point at its input. These fields carry a credential and are
 * the one part of this modal a screen-reader user must be able to navigate
 * unambiguously, and an unassociated `<label>` gives them nothing to land on.
 */
const envFieldId = (key: string) => `brxt-env-${key}`;

interface Props {
  onClose: () => void;
  onInstalled: () => void;
  preloadedFilePath?: string;
  /**
   * Where this install came from. Defaults to the local-file route.
   *
   * ⚠ **Not derivable from `preloadedFilePath`.** Finder hands a double-clicked
   * `.brxt` to the app over IPC and `ExtensionsView` preloads its path, so a
   * preloaded path means "a file is already chosen", never "this came from the
   * marketplace". See `brxtInstallFlow.ts`.
   */
  origin?: BrxtInstallOrigin;
}

const NO_LOCAL_PATH_MESSAGE =
  'Biorouter is running on another machine, so it cannot read a file you drop here. Copy the file onto that machine and install it with `biorouter extension install <path>`.';

export function BrxtInstallModal({ onClose, onInstalled, preloadedFilePath, origin }: Props) {
  const installOrigin: BrxtInstallOrigin = origin ?? { kind: 'local-file' };
  const isMarketplace = installOrigin.kind === 'marketplace';
  const registrySource =
    installOrigin.kind === 'marketplace' ? installOrigin.registrySource : undefined;

  const [step, setStep] = useState<Step>('select');
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
    // An empty path means the surface could not supply one -- a browser tab
    // cannot know where a dropped file lives, and the daemon that would open it
    // is on another machine. Refusing here is the point: passing the bare file
    // name on would make the daemon resolve it against ITS working directory
    // and possibly validate some unrelated bundle.
    if (!fp) {
      setError(NO_LOCAL_PATH_MESSAGE);
      return;
    }
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
        setError('Drop a .brxt file');
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

  /**
   * Issue #116. One decision, read by both the button's label and its click, so
   * a screen that says "Install extension" cannot open a configuration step and
   * a screen that says "Next: configure" cannot install.
   */
  const primaryAction = primaryInstallAction(manifest);

  const handleNext = () => {
    if (primaryAction.kind === 'install') {
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
      const result = await window.electron.installBrxtBundle(
        filePath,
        manifest.name,
        registrySource
      );

      if ('error' in result) {
        setError(result.error);
        return;
      }

      const { installDir } = result;

      // Store secrets in Biorouter's keyring; track which ones succeeded
      const secretEnvKeys: string[] = [];
      for (const entry of envEntries.filter((e) => e.secret && e.value.trim())) {
        const res = await upsertConfig({
          body: { is_secret: true, key: entry.key, value: entry.value },
          // Issue #56 DR-16: the key here is whatever the extension's manifest
          // declared, so it can collide with a capability key — an extension
          // that talks to Ollama would plausibly declare OLLAMA_HOST. The guard
          // is on the key name and does not look at `is_secret`, so without this
          // the user clicking Install would be refused as though a model had
          // made the call. This IS the user, so it carries the proof.
          headers: await userActionHeaders(),
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

  /**
   * Issue #56 §13.5. The badge this install is going to produce, said out loud
   * before the user commits.
   *
   * ⚠ **The disclosure is about the RESULT, not about the route.** §13.5's
   * sentence is written for the file-drop case and is true there — an extension
   * installed from a file is Public under R11(ii), because the install records
   * no provenance for the daemon to treat as private. But this component is not
   * only the file-drop case:
   *
   *   - `BrowseExtensionsModal` renders THIS component for a marketplace
   *     install, so a row badged Private led straight into a confirmation that
   *     said "always Public";
   *   - and the task's own Step 3 records that a bundle merely NAMED
   *     `ucsfomopagent` inherits the private badge — "fail-closed, and fine",
   *     which it only is if the last screen before Install did not promise the
   *     opposite.
   *
   * Three screens with two answers is worse than either answer alone, so once a
   * manifest is in hand the modal states the tier the install will actually
   * produce, resolved through `classifyExtension` — the same union the Settings
   * card, the Browse row and the composer all read, so they cannot disagree.
   *
   * Issue #116 adds the marketplace's own answer to the same union, for the
   * window where the bundle is still downloading and there is no manifest to
   * classify: the row the user clicked already rendered a badge, and the modal
   * that opens on top of it must not contradict the row it came from. Private
   * from either source wins — see `resultingPrivacyTier`.
   *
   * Before a file is chosen on the LOCAL route there is no name to classify and
   * the route IS the only fact available, so §13.5's sentence stands verbatim:
   * a user who has not yet picked a bundle should already know what dropping
   * one in here means.
   *
   * One sentence, one element: the assertions match on the normalised text of a
   * single node, and splitting a phrase into a nested `<strong>` would take it
   * out of that node.
   */
  const resultingTier = resultingPrivacyTier(
    manifest ? classifyExtension(manifest.name) : null,
    installOrigin.kind === 'marketplace' ? installOrigin.entry.privacyTier : undefined
  );
  const publicNotice = isMarketplace
    ? 'The Biorouter marketplace publishes this name as public. Any model, including commercial models hosted outside your institution, will be able to call this extension.'
    : 'Extensions installed from a file are always Public. Any model, including commercial models hosted outside your institution, will be able to call this extension.';
  const badgeNotice =
    resultingTier === 'private'
      ? 'The Biorouter marketplace publishes this name as private, so this extension will be Private: only private models will be able to call it.'
      : publicNotice;

  const requiredVars = envEntries.filter((e) => e.required);
  const optionalVars = envEntries.filter((e) => !e.required);
  const requiredMissing = requiredVars.some((e) => !e.value.trim());

  /**
   * Issue #116. The marketplace route's own progress. `downloading` is the
   * parent's — `BrowseExtensionsModal` opens this modal the instant Add is
   * clicked, so the download runs *inside* the install rather than in front of
   * it, and a failure lands on a screen that can offer Retry.
   */
  const phase = marketplacePhase({
    downloading: installOrigin.kind === 'marketplace' ? installOrigin.downloading : false,
    downloadError: installOrigin.kind === 'marketplace' ? installOrigin.downloadError : null,
    isValidating,
    validationError: error,
    hasManifest: manifest !== null,
  });
  const downloadError =
    installOrigin.kind === 'marketplace' ? (installOrigin.downloadError ?? null) : null;
  const isBusy =
    isValidating ||
    isInstalling ||
    (installOrigin.kind === 'marketplace' && installOrigin.downloading);

  const marketplaceName = isMarketplace
    ? (manifest?.display_name ??
      (installOrigin.kind === 'marketplace' ? installOrigin.entry.name : ''))
    : '';
  const marketplaceMeta =
    installOrigin.kind === 'marketplace'
      ? [installOrigin.entry.organization, manifest ? `v${manifest.version}` : undefined]
          .filter(Boolean)
          .join(' · ')
      : '';

  const privacyPanel = (
    <div className="biorouter-modal-panel rounded-lg p-3">
      <PrivacyBadge tier={resultingTier} />
      <p className="text-supporting text-text-muted mt-1.5 leading-relaxed">{badgeNotice}</p>
    </div>
  );

  const manifestSummary = manifest && !error && (
    <div className="biorouter-modal-panel rounded-xl p-4">
      <p className="text-xs text-text-muted uppercase tracking-wide mb-2">
        {isMarketplace ? 'From the Biorouter marketplace' : 'Detected from bundle'}
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
        {optionalVars.length > 0 ? `, ${optionalVars.length} optional` : ''}
      </p>
      <p className="text-sm text-text-default mt-2">{manifest.description}</p>
      {skillsPreview.length > 0 && (
        <div className="mt-2 pt-2 shadow-[inset_0_1px_0_color-mix(in_srgb,var(--border-subtle)_45%,transparent)]">
          <p className="text-xs font-semibold text-text-muted uppercase tracking-wide mb-1">
            Skills included
          </p>
          {skillsPreview.map((skill) => (
            <p key={skill.slug} className="text-xs text-text-muted leading-relaxed">
              · <span className="font-medium">{skill.name}</span>: {skill.description}
            </p>
          ))}
        </div>
      )}
    </div>
  );

  const dependencyBanner = error && (
    <DependencyErrorBanner
      error={error}
      failure={{
        kind: 'extension',
        name: manifest?.name ?? 'extension',
        displayName: manifest?.display_name ?? manifest?.name,
        command: 'uv sync',
      }}
    />
  );

  let title: React.ReactNode;
  let subtitle: React.ReactNode;
  if (step === 'configure') {
    title = `Configure ${manifest?.display_name ?? ''}`;
    subtitle = 'Fill in required credentials. Optional fields are pre-filled with defaults.';
  } else if (isMarketplace) {
    title = `Install ${marketplaceName}`;
    subtitle = marketplaceMeta
      ? `From the Biorouter marketplace · ${marketplaceMeta}`
      : 'From the Biorouter marketplace.';
  } else {
    title = 'Add extension';
    subtitle = 'Install a Biorouter extension bundle (.brxt file).';
  }

  /**
   * Issue #116. On the marketplace route Cancel, Back and the × are all one
   * thing — returning to the list the user came from — so there is one control
   * and it says so. Offering a second "Cancel" beside it would imply the two
   * differ.
   */
  const backToMarketplace = (
    <Button variant="outline" onClick={onClose} disabled={isInstalling}>
      Back to marketplace
    </Button>
  );

  let footer: React.ReactNode;
  if (step === 'select' && isMarketplace) {
    footer = (
      <>
        {backToMarketplace}
        {phase === 'error' ? (
          <Button
            disabled={isBusy}
            onClick={() => {
              setError(null);
              if (downloadError) {
                if (installOrigin.kind === 'marketplace') installOrigin.onRetry();
              } else if (preloadedFilePath) {
                processFile(preloadedFilePath);
              }
            }}
          >
            Retry
          </Button>
        ) : (
          <Button disabled={!manifest || isBusy} onClick={handleNext}>
            {isInstalling ? 'Installing…' : primaryAction.label}
          </Button>
        )}
      </>
    );
  } else if (step === 'select') {
    footer = (
      <>
        <Button variant="outline" onClick={onClose} disabled={isBusy}>
          Cancel
        </Button>
        <Button disabled={!manifest || !!error || isBusy} onClick={handleNext}>
          {isInstalling ? 'Installing…' : primaryAction.label}
        </Button>
      </>
    );
  } else {
    footer = (
      <>
        <Button
          variant="outline"
          disabled={isBusy}
          onClick={() => {
            setStep('select');
            setError(null);
          }}
        >
          Back
        </Button>
        {isMarketplace ? (
          backToMarketplace
        ) : (
          <Button variant="outline" onClick={onClose} disabled={isBusy}>
            Cancel
          </Button>
        )}
        <Button disabled={requiredMissing || isInstalling} onClick={handleInstall}>
          {isInstalling ? 'Installing…' : 'Install extension'}
        </Button>
      </>
    );
  }

  return (
    <ModalShell
      open
      onOpenChange={(open) => !open && onClose()}
      // A form: Escape and the × still leave, but a misclick on the scrim can
      // no longer throw away typed credentials. While a bundle is being read or
      // installed it is `required` — nothing may interrupt it.
      size="md"
      purpose={isBusy ? 'required' : 'form'}
      scrollBody
      title={title}
      subtitle={subtitle}
      footer={footer}
    >
      {step === 'select' && isMarketplace && (
        /* Issue #116. The marketplace route renders NO local-file controls: no
           drop zone, no file input, no "Browse file…". The user chose this
           extension on the previous screen and Biorouter is fetching it — there
           is nothing for them to supply. */
        <div className="py-3 space-y-4">
          {privacyPanel}

          {phase === 'downloading' && (
            <div className="biorouter-modal-panel rounded-xl p-8 text-center">
              <Package className="w-10 h-10 mx-auto mb-2 text-text-muted" />
              <p className="text-sm text-text-muted animate-pulse">
                Downloading {marketplaceName}…
              </p>
            </div>
          )}

          {phase === 'validating' && (
            <div className="biorouter-modal-panel rounded-xl p-8 text-center">
              <Package className="w-10 h-10 mx-auto mb-2 text-text-muted" />
              <p className="text-sm text-text-muted animate-pulse">Reading bundle…</p>
            </div>
          )}

          {phase === 'error' &&
            (downloadError ? (
              <DependencyErrorBanner
                error={downloadError}
                hideDebugAction
                failure={{
                  kind: 'extension',
                  name:
                    installOrigin.kind === 'marketplace' ? installOrigin.entry.name : 'extension',
                  command: 'download',
                }}
              />
            ) : (
              dependencyBanner
            ))}

          {phase === 'ready' && manifestSummary}
        </div>
      )}

      {step === 'select' && !isMarketplace && (
        <div className="py-3 space-y-4">
          {/* §13.5: the tier this install will actually produce, stated before
              a bundle has even been chosen — someone who has not yet picked one
              should already know what dropping it in here means. */}
          {privacyPanel}

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
                <p className="text-sm font-medium mb-1">
                  {manifest ? 'Drop a different .brxt file here' : 'Drop your .brxt file here'}
                </p>
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

          {/* Error banner. A `.brxt` install fails most often on `uv sync` —
              a Python dependency that will not build on this machine — which is
              precisely the kind of thing a session with a shell can work out. */}
          {dependencyBanner}

          {/* Manifest preview card */}
          {manifestSummary}
        </div>
      )}

      {step === 'configure' && manifest && (
        <div className="py-3 space-y-4">
          {requiredVars.length > 0 && (
            <div className="space-y-3">
              <p className="text-xs font-semibold text-text-default uppercase tracking-wide">
                Required
              </p>
              {requiredVars.map((entry) => (
                <div key={entry.key}>
                  <label
                    htmlFor={envFieldId(entry.key)}
                    className="block text-xs font-semibold mb-1"
                  >
                    {entry.key} <span className="text-text-danger">*</span>
                  </label>
                  <input
                    id={envFieldId(entry.key)}
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

          {/* Issue #116. When an extension declares only optional variables the
              first step's button says so, and this step must not then hide them
              behind a disclosure the user has to find. They are open. */}
          {optionalVars.length > 0 && requiredVars.length === 0 && (
            <div className="space-y-3">
              <p className="text-xs font-semibold text-text-muted uppercase tracking-wide">
                Optional
              </p>
              <p className="text-xs text-text-muted">
                {manifest.display_name} needs no credentials to run. These {optionalVars.length}{' '}
                optional setting{optionalVars.length !== 1 ? 's are' : ' is'} pre-filled with
                defaults — install without changing anything if you are not sure.
              </p>
              {optionalVars.map((entry) => (
                <div key={entry.key}>
                  <label
                    htmlFor={envFieldId(entry.key)}
                    className="block text-xs font-medium text-text-muted mb-1"
                  >
                    {entry.key}
                  </label>
                  <input
                    id={envFieldId(entry.key)}
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

          {optionalVars.length > 0 && requiredVars.length > 0 && (
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
                      <label
                        htmlFor={envFieldId(entry.key)}
                        className="block text-xs font-medium text-text-muted mb-1"
                      >
                        {entry.key}
                      </label>
                      <input
                        id={envFieldId(entry.key)}
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

          {dependencyBanner}

          {/* §13.5: the resulting badge, above the Install button — the footer
              sits directly below this body, so it is the last thing read before
              the button that commits the install. */}
          {privacyPanel}
        </div>
      )}
    </ModalShell>
  );
}
