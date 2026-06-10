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
  const [cli, setCli] = useState<{ bundled: string | null; onPath: boolean } | null>(null);
  const [cliState, setCliState] = useState<InstallState>('idle');
  const [cliOutput, setCliOutput] = useState('');
  const [cliError, setCliError] = useState('');

  // On startup, offer to install the CLI if it isn't already callable from a
  // terminal (once per launch).
  useEffect(() => {
    let cancelled = false;
    window.electron
      .cliStatus()
      .then((s) => {
        if (cancelled || !s) return;
        if (s.bundled && !s.onPath && !sessionStorage.getItem('cli-install-dismissed')) {
          setCli({ bundled: s.bundled, onPath: s.onPath });
          setVisible(true);
        }
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  const handleInstallCli = async () => {
    setCliState('running');
    setCliError('');
    const res = await window.electron.installCli();
    if (res.success) {
      setCliOutput(res.output);
      setCliState('done');
      setCli((c) => (c ? { ...c, onPath: true } : c));
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
          missing.map((info) => ({ info, installState: 'idle' as InstallState, output: '', errorMsg: '' })),
        );
        setVisible(true);
      }

      if (payload.type === 'install-start' && payload.dep) {
        setDeps((prev) =>
          prev.map((d) => (d.info.name === payload.dep ? { ...d, installState: 'running' as InstallState, output: '', errorMsg: '' } : d)),
        );
      }

      if (payload.type === 'install-output' && payload.dep) {
        setDeps((prev) =>
          prev.map((d) =>
            d.info.name === payload.dep ? { ...d, output: d.output + (payload.output ?? '') } : d,
          ),
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
              return { ...d, installState: 'done' as InstallState, info: { ...d.info, installed: true, version: payload.version ?? null } };
            }
            return { ...d, installState: 'error' as InstallState, errorMsg: 'Install completed but tool still not detected. Try opening a new terminal and re-running BioRouter.' };
          }),
        );
      }

      if (payload.type === 'install-error' && payload.dep) {
        setDeps((prev) =>
          prev.map((d) =>
            d.info.name === payload.dep ? { ...d, installState: 'error' as InstallState, errorMsg: payload.error ?? 'Unknown error' } : d,
          ),
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

  const showCli = !!cli && !cli.onPath && cliState !== 'done';
  if (!visible || (deps.length === 0 && !cli)) return null;

  const allDone = deps.length > 0 && deps.every((d) => d.installState === 'done' || d.info.installed);

  return (
    <div className="fixed inset-0 z-[200] flex items-center justify-center bg-black/50">
      <div className="bg-background-default border border-border-subtle rounded-xl shadow-xl w-[560px] max-h-[85vh] flex flex-col">
        {/* Header */}
        <div className="px-6 pt-5 pb-4 border-b border-border-subtle flex items-start justify-between">
          <div>
            <h2 className="text-base font-semibold">Missing Dependencies</h2>
            <p className="text-xs text-text-muted mt-0.5">
              The following tools are required for BioRouter features. Install them to continue.
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
                      : 'Call `biorouter` from any terminal.'}
                  </p>
                </div>
                {cliState !== 'running' && cliState !== 'done' && (
                  <Button variant="default" size="sm" className="h-7 text-xs" onClick={handleInstallCli}>
                    Install
                  </Button>
                )}
                {cliState === 'running' && <span className="text-xs text-text-muted">Installing…</span>}
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
                    ref={(el) => { outputRefs.current[info.name] = el; }}
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
