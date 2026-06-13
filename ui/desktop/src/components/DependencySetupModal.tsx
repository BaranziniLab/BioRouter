/**
 * DependencySetupModal.tsx
 *
 * Shown automatically when the startup dependency check finds missing tools.
 * Lets the user install each missing dependency individually from inside the app,
 * with live output streaming and re-check after install.
 *
 * Dismissed state is stored in sessionStorage so re-opening the app re-checks
 * again (user may have installed things in the meantime).
 */

import { useState, useEffect, useRef } from 'react';
import { Button } from './ui/button';
import type { DependencyInfo, DependencyEvent } from '../utils/dependencyChecker';

type InstallState = 'idle' | 'running' | 'done' | 'error' | 'installed';

interface DepState {
  info: DependencyInfo;
  installState: InstallState;
  output: string;
  errorMsg: string;
}

export default function DependencySetupModal() {
  const [deps, setDeps] = useState<DepState[]>([]);
  const [visible, setVisible] = useState(false);
  const outputRefs = useRef<Record<string, HTMLDivElement | null>>({});

  // Biorouter CLI install state (the bundled `biorouter` onto PATH).
  type CliStatus = {
    bundled: string | null;
    onPath: boolean;
    bundledVersion: string | null;
    pathVersion: string | null;
    needsUpdate: boolean;
    brokenOnPath: boolean;
  };
  const [cli, setCli] = useState<CliStatus | null>(null);
  const [cliState, setCliState] = useState<InstallState>('idle');
  const [cliOutput, setCliOutput] = useState('');
  const [cliError, setCliError] = useState('');

  // Whether a given status warrants showing the card: not installed at all, a
  // stale (older) install after an app upgrade, or a broken/dangling entry.
  const cliNeedsAttention = (s: CliStatus | null): boolean =>
    !!s && !!s.bundled && (!s.onPath || s.needsUpdate || s.brokenOnPath);

  // Dismissing a specific version-pair upgrade prompt persists across app
  // launches (localStorage) — a *new* app version prompts again, and the
  // toolbar CLI button can always re-open the card explicitly.
  const cliDismissKey = (s: CliStatus | null): string =>
    `cli-update-dismissed:${s?.pathVersion ?? 'none'}->${s?.bundledVersion ?? 'unknown'}`;

  // On startup, offer to install/upgrade the CLI if it isn't callable from a
  // terminal or is older than what this app ships (once per launch).
  useEffect(() => {
    let cancelled = false;
    window.electron
      .cliStatus()
      .then((s) => {
        if (cancelled || !s) return;
        if (
          cliNeedsAttention(s) &&
          !sessionStorage.getItem('cli-install-dismissed') &&
          !localStorage.getItem(cliDismissKey(s))
        ) {
          setCli(s);
          setVisible(true);
        }
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // The toolbar CLI button dispatches this event when the CLI isn't installed
  // (or is stale) — re-open the modal with the install/update card even if it
  // was dismissed earlier this session.
  useEffect(() => {
    const handler = () => {
      window.electron
        .cliStatus()
        .then((s) => {
          if (!s) return;
          if (cliNeedsAttention(s)) {
            setCli(s);
            setCliState('idle');
            setVisible(true);
          }
        })
        .catch(() => {});
    };
    window.addEventListener('biorouter:open-cli-install', handler);
    return () => window.removeEventListener('biorouter:open-cli-install', handler);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const handleInstallCli = async () => {
    setCliState('running');
    setCliError('');
    const res = await window.electron.installCli();
    if (res.success) {
      setCliOutput(res.output);
      setCliState('done');
      // After install/update the on-PATH binary now matches the bundled one.
      setCli((c) =>
        c
          ? {
              ...c,
              onPath: true,
              needsUpdate: false,
              brokenOnPath: false,
              pathVersion: c.bundledVersion,
            }
          : c
      );
    } else {
      setCliError(res.error);
      setCliState('error');
    }
  };

  // Listen for the push event from main process
  useEffect(() => {
    const handler = (_event: Electron.IpcRendererEvent, ...args: unknown[]) => {
      const payload = args[0] as DependencyEvent;
      if (payload.type === 'check-results' && payload.deps) {
        const missing = payload.deps.filter((d) => !d.installed);
        if (missing.length === 0) return;
        setDeps(
          missing.map((info) => ({
            info,
            installState: 'idle' as InstallState,
            output: '',
            errorMsg: '',
          }))
        );
        setVisible(true);
      }

      if (payload.type === 'install-start' && payload.dep) {
        setDeps((prev) =>
          prev.map((d) =>
            d.info.name === payload.dep
              ? { ...d, installState: 'running' as InstallState, output: '', errorMsg: '' }
              : d
          )
        );
      }

      if (payload.type === 'install-output' && payload.dep) {
        setDeps((prev) =>
          prev.map((d) =>
            d.info.name === payload.dep ? { ...d, output: d.output + (payload.output ?? '') } : d
          )
        );
        // Auto-scroll
        const el = outputRefs.current[payload.dep];
        if (el) el.scrollTop = el.scrollHeight;
      }

      if ((payload.type === 'install-done' || payload.type === 'recheck-results') && payload.dep) {
        setDeps((prev) =>
          prev.map((d) => {
            if (d.info.name !== payload.dep) return d;
            if (payload.installed) {
              return {
                ...d,
                installState: 'done' as InstallState,
                info: { ...d.info, installed: true, version: payload.version ?? null },
              };
            }
            return {
              ...d,
              installState: 'error' as InstallState,
              errorMsg:
                'Install completed but tool still not detected. Try opening a new terminal and re-running BioRouter.',
            };
          })
        );
      }

      if (payload.type === 'install-error' && payload.dep) {
        setDeps((prev) =>
          prev.map((d) =>
            d.info.name === payload.dep
              ? {
                  ...d,
                  installState: 'error' as InstallState,
                  errorMsg: payload.error ?? 'Unknown error',
                }
              : d
          )
        );
      }
    };

    return window.electron.on('dependency-event', handler);
  }, []);

  // Auto-dismiss when all are installed
  useEffect(() => {
    if (deps.length > 0 && deps.every((d) => d.installState === 'done' || d.info.installed)) {
      const t = setTimeout(() => setVisible(false), 1500);
      return () => clearTimeout(t);
    }
    return undefined;
  }, [deps]);

  const handleInstall = async (depName: string) => {
    await window.electron.installDependency(depName);
  };

  const handleOpenUrl = (url: string) => {
    if (url) window.electron.openExternal(url);
  };

  const showCli = cliNeedsAttention(cli) && cliState !== 'done';
  // Fixing an existing entry (stale version or broken/dangling) vs a first
  // install. `needsUpdate` implies it's on PATH; `brokenOnPath` means an entry
  // exists but won't run.
  const cliIsUpdate = !!cli && (cli.needsUpdate || cli.brokenOnPath);
  const cliButtonLabel = cli?.brokenOnPath ? 'Reinstall' : cliIsUpdate ? 'Update' : 'Install';
  const cliProgressLabel = cli?.brokenOnPath
    ? 'Reinstalling…'
    : cliIsUpdate
      ? 'Updating…'
      : 'Installing…';
  if (!visible || (deps.length === 0 && !cli)) return null;

  const allDone =
    deps.length > 0 && deps.every((d) => d.installState === 'done' || d.info.installed);

  return (
    <div className="fixed inset-0 z-[200] flex items-center justify-center bg-black/50">
      <div className="bg-background-default border border-border-subtle rounded-xl shadow-xl w-[560px] max-h-[85vh] flex flex-col">
        {/* Header */}
        <div className="px-6 pt-5 pb-4 border-b border-border-subtle flex items-start justify-between">
          <div>
            <h2 className="text-base font-semibold">
              {deps.length === 0 && cliIsUpdate
                ? cli?.brokenOnPath
                  ? 'Biorouter CLI Needs Repair'
                  : 'Biorouter CLI Update'
                : 'Missing Dependencies'}
            </h2>
            <p className="text-xs text-text-muted mt-0.5">
              {deps.length === 0 && cliIsUpdate
                ? cli?.brokenOnPath
                  ? 'The `biorouter` on your PATH no longer runs. Reinstall it from this app.'
                  : 'Your terminal `biorouter` is older than this app. Update it to match.'
                : 'The following tools are required for BioRouter features. Install them to continue.'}
            </p>
          </div>
          {allDone ? null : (
            <Button
              variant="ghost"
              size="sm"
              className="h-7 w-7 p-0 shrink-0 ml-3"
              title="Dismiss (you can install later)"
              onClick={() => {
                sessionStorage.setItem('cli-install-dismissed', '1');
                // Don't re-prompt for this same version pair on future
                // launches; missing required deps still re-open the modal.
                if (cliIsUpdate) {
                  localStorage.setItem(cliDismissKey(cli), '1');
                }
                setVisible(false);
              }}
            >
              ✕
            </Button>
          )}
        </div>

        {/* Dep list */}
        <div className="flex flex-col gap-3 p-6 overflow-y-auto">
          {/* Biorouter CLI install card */}
          {(showCli || cliState === 'done') && (
            <div
              className={`rounded-lg border px-4 py-3 ${
                cliState === 'done'
                  ? 'border-border-success/40 bg-background-success/10'
                  : cliState === 'error'
                    ? 'border-destructive/30 bg-destructive/5'
                    : 'border-border-subtle bg-background-medium/20'
              }`}
            >
              <div className="flex items-center justify-between">
                <div>
                  <p className="text-sm font-semibold flex items-center gap-1.5">
                    {cliState === 'done' && <span className="text-text-success">✓</span>}
                    {cliState === 'error' && <span className="text-destructive">✗</span>}
                    {cliState === 'running' && (
                      <span className="inline-block w-3 h-3 rounded-full border-2 border-text-muted border-t-transparent animate-spin" />
                    )}
                    Biorouter CLI
                  </p>
                  <p className="text-[11px] text-text-muted mt-0.5">
                    {cliState === 'done'
                      ? 'Installed — open a new terminal and run `biorouter`.'
                      : cli?.brokenOnPath
                        ? 'The `biorouter` on your PATH won’t run — reinstall it.'
                        : cliIsUpdate
                          ? `Update ${cli?.pathVersion ?? 'older'} → ${cli?.bundledVersion ?? 'latest'} to match this app.`
                          : 'Call `biorouter` from any terminal.'}
                  </p>
                </div>
                {cliState !== 'running' && cliState !== 'done' && (
                  <Button
                    variant="default"
                    size="sm"
                    className="h-7 text-xs"
                    onClick={handleInstallCli}
                  >
                    {cliButtonLabel}
                  </Button>
                )}
                {cliState === 'running' && (
                  <span className="text-xs text-text-muted">{cliProgressLabel}</span>
                )}
              </div>
              {cliOutput && (
                <div className="mt-2 font-mono text-[11px] text-text-muted bg-background-medium/40 rounded p-2 max-h-28 overflow-y-auto whitespace-pre-wrap">
                  {cliOutput}
                </div>
              )}
              {cliState === 'error' && cliError && (
                <p className="mt-2 text-xs text-destructive">{cliError}</p>
              )}
            </div>
          )}

          {deps.map(({ info, installState, output, errorMsg }) => {
            const installed = installState === 'done' || info.installed;
            return (
              <div
                key={info.name}
                className={`rounded-lg border px-4 py-3 ${
                  installed
                    ? 'border-border-success/40 bg-background-success/10'
                    : installState === 'error'
                      ? 'border-destructive/30 bg-destructive/5'
                      : 'border-border-subtle bg-background-medium/20'
                }`}
              >
                <div className="flex items-center justify-between">
                  <div>
                    <p className="text-sm font-semibold flex items-center gap-1.5">
                      {installed && <span className="text-text-success">✓</span>}
                      {installState === 'error' && <span className="text-destructive">✗</span>}
                      {installState === 'running' && (
                        <span className="inline-block w-3 h-3 rounded-full border-2 border-text-muted border-t-transparent animate-spin" />
                      )}
                      {info.displayName}
                    </p>
                    {installed && info.version && (
                      <p className="text-[11px] text-text-muted mt-0.5">{info.version}</p>
                    )}
                    {!installed && installState === 'idle' && (
                      <p className="text-[11px] text-text-muted font-mono mt-0.5 break-all">
                        {info.installCmd}
                      </p>
                    )}
                  </div>
                  <div className="flex items-center gap-2 shrink-0 ml-3">
                    {!installed && info.downloadUrl && installState !== 'running' && (
                      <Button
                        variant="ghost"
                        size="sm"
                        className="h-7 text-xs text-text-muted"
                        onClick={() => handleOpenUrl(info.downloadUrl)}
                        title="Open download page"
                      >
                        Download
                      </Button>
                    )}
                    {!installed && installState !== 'running' && (
                      <Button
                        variant="default"
                        size="sm"
                        className="h-7 text-xs"
                        onClick={() => handleInstall(info.name)}
                      >
                        Install
                      </Button>
                    )}
                    {installState === 'running' && (
                      <span className="text-xs text-text-muted">Installing…</span>
                    )}
                  </div>
                </div>

                {/* Live output */}
                {output && (
                  <div
                    ref={(el) => {
                      outputRefs.current[info.name] = el;
                    }}
                    className="mt-2 font-mono text-[11px] text-text-muted bg-background-medium/40 rounded p-2 max-h-28 overflow-y-auto whitespace-pre-wrap"
                  >
                    {output}
                  </div>
                )}

                {/* Error */}
                {installState === 'error' && errorMsg && (
                  <p className="mt-2 text-xs text-destructive">{errorMsg}</p>
                )}

                {/* Linux sudo note */}
                {info.requiresSudo && !installed && installState !== 'running' && (
                  <p className="mt-1 text-[11px] text-text-warning">
                    Requires sudo — you may be prompted for your password in a terminal.
                  </p>
                )}
              </div>
            );
          })}
        </div>

        {/* Footer */}
        <div className="px-6 py-4 border-t border-border-subtle flex items-center justify-between">
          <p className="text-xs text-text-muted">
            {allDone
              ? 'All dependencies installed. ✓'
              : deps.length === 0 && cliIsUpdate
                ? 'Updating keeps the terminal CLI in sync with the desktop app.'
                : deps.length === 0
                  ? 'Install the CLI to use `biorouter` from any terminal.'
                  : 'BioRouter features may be limited until these are installed.'}
          </p>
          <Button variant="outline" size="sm" onClick={() => setVisible(false)}>
            {allDone ? 'Done' : 'Dismiss'}
          </Button>
        </div>
      </div>
    </div>
  );
}
