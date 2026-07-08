import React, { useCallback, useEffect, useRef, useState } from 'react';
import { Terminal as XTerm } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';
import { Plus, Terminal as TerminalIcon, X } from './icons/app-icons';
import { Button } from './ui/button';
import { cn } from '../utils';

interface InAppTerminalDockProps {
  open: boolean;
  workingDir?: string;
  onClose: () => void;
}

type TerminalPane = {
  id: string;
  title: string;
};

type ProposedDimensions = {
  cols: number;
  rows: number;
};

function getTerminalSize(fitAddon: FitAddon): ProposedDimensions {
  const proposed = (
    fitAddon as FitAddon & { proposeDimensions?: () => ProposedDimensions | undefined }
  ).proposeDimensions?.();
  return {
    cols: Math.max(24, proposed?.cols ?? 80),
    rows: Math.max(8, proposed?.rows ?? 18),
  };
}

function basename(path?: string) {
  if (!path) return 'terminal';
  return path.replace(/\/+$/, '').split('/').pop() || 'terminal';
}

function formatCwd(cwd?: string) {
  if (!cwd) return 'Session terminal';
  const home = window.appConfig?.get('BIOROUTER_HOME_DIR') as string | undefined;
  if (home && cwd.startsWith(home)) return cwd.replace(home, '~');
  return cwd;
}

function makePaneTitle(workingDir: string | undefined, index: number) {
  const name = basename(workingDir);
  return index === 1 ? name : `${name} ${index}`;
}

const terminalTheme = {
  background: '#fff9ed',
  black: '#2d2a26',
  blue: '#2f75d6',
  brightBlack: '#81776b',
  brightBlue: '#5a93e8',
  brightCyan: '#1f9aa6',
  brightGreen: '#2f9d67',
  brightMagenta: '#9462d6',
  brightRed: '#d45252',
  brightWhite: '#2d2a26',
  brightYellow: '#b57919',
  cursor: '#2d2a26',
  cyan: '#16818c',
  foreground: '#2d2a26',
  green: '#22784f',
  magenta: '#7847b8',
  red: '#b63f3f',
  selectionBackground: '#ead7b8',
  white: '#574f46',
  yellow: '#9b6818',
};

const TerminalPaneView: React.FC<{
  active: boolean;
  open: boolean;
  paneId: string;
  workingDir?: string;
}> = ({ active, open, paneId, workingDir }) => {
  const terminalHostRef = useRef<HTMLDivElement | null>(null);
  const terminalRef = useRef<XTerm | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const backendSessionIdRef = useRef<string | null>(null);
  const pendingInputRef = useRef<string[]>([]);

  const fitAndFocus = useCallback(() => {
    const term = terminalRef.current;
    const fitAddon = fitAddonRef.current;
    if (!term || !fitAddon || !open || !active) return;
    window.requestAnimationFrame(() => {
      try {
        fitAddon.fit();
        const { cols, rows } = getTerminalSize(fitAddon);
        const sessionId = backendSessionIdRef.current;
        if (sessionId) {
          window.electron.resizeTerminalSession(sessionId, cols, rows).catch(() => {});
        }
        term.focus();
      } catch {
        /* xterm can throw while the hidden dock has no measurable dimensions */
      }
    });
  }, [active, open]);

  useEffect(() => {
    const host = terminalHostRef.current;
    if (!host) return;

    let disposed = false;
    let animationFrame = 0;
    const fitAddon = new FitAddon();
    const term = new XTerm({
      allowProposedApi: true,
      allowTransparency: true,
      convertEol: true,
      cursorBlink: true,
      cursorStyle: 'bar',
      fontFamily: 'Menlo, Monaco, Consolas, "Liberation Mono", monospace',
      fontSize: 12.5,
      lineHeight: 1.25,
      scrollback: 8000,
      theme: terminalTheme,
    });

    terminalRef.current = term;
    fitAddonRef.current = fitAddon;
    term.loadAddon(fitAddon);
    term.open(host);

    const flushResize = () => {
      if (disposed) return;
      try {
        fitAddon.fit();
        const { cols, rows } = getTerminalSize(fitAddon);
        const sessionId = backendSessionIdRef.current;
        if (sessionId) {
          window.electron.resizeTerminalSession(sessionId, cols, rows).catch(() => {});
        }
      } catch {
        /* ignored while hidden */
      }
    };

    const dataDisposer = window.electron.onTerminalData((event) => {
      if (event.sessionId !== backendSessionIdRef.current) return;
      term.write(event.data);
    });
    const exitDisposer = window.electron.onTerminalExit((event) => {
      if (event.sessionId !== backendSessionIdRef.current) return;
      const suffix = event.signal ? ` (${event.signal})` : '';
      term.writeln(`\r\n\x1b[90m[terminal exited with code ${event.exitCode ?? 0}${suffix}]\x1b[0m`);
    });
    const inputDisposer = term.onData((data) => {
      const sessionId = backendSessionIdRef.current;
      if (!sessionId) {
        pendingInputRef.current.push(data);
        return;
      }
      window.electron.writeTerminalSession(sessionId, data).catch(() => {});
    });

    const resizeObserver = new ResizeObserver(() => {
      window.cancelAnimationFrame(animationFrame);
      animationFrame = window.requestAnimationFrame(flushResize);
    });
    resizeObserver.observe(host);

    const start = async () => {
      const { cols, rows } = getTerminalSize(fitAddon);
      const result = await window.electron.createTerminalSession({ workingDir, cols, rows });
      if (disposed) {
        if (result.success) {
          await window.electron.disposeTerminalSession(result.sessionId).catch(() => {});
        }
        return;
      }
      if (!result.success) {
        term.writeln(`\x1b[31mCould not start terminal: ${result.error}\x1b[0m`);
        return;
      }
      backendSessionIdRef.current = result.sessionId;
      pendingInputRef.current.splice(0).forEach((data) => {
        window.electron.writeTerminalSession(result.sessionId, data).catch(() => {});
      });
      flushResize();
      if (open && active) term.focus();
    };

    start().catch((error) => {
      if (!disposed) {
        term.writeln(`\x1b[31mCould not start terminal: ${String(error)}\x1b[0m`);
      }
    });

    return () => {
      disposed = true;
      window.cancelAnimationFrame(animationFrame);
      resizeObserver.disconnect();
      dataDisposer();
      exitDisposer();
      inputDisposer.dispose();
      if (backendSessionIdRef.current) {
        window.electron.disposeTerminalSession(backendSessionIdRef.current).catch(() => {});
      }
      term.dispose();
      terminalRef.current = null;
      fitAddonRef.current = null;
      backendSessionIdRef.current = null;
    };
  }, [active, open, paneId, workingDir]);

  useEffect(() => {
    fitAndFocus();
  }, [fitAndFocus]);

  return (
    <div
      className={cn('h-full min-h-0 w-full overflow-hidden', !active && 'hidden')}
      data-terminal-pane={paneId}
    >
      <div
        ref={terminalHostRef}
        className="h-full min-h-0 w-full overflow-hidden rounded-md border border-[#d8c7aa] bg-[#fbf7ed] px-2 py-2 shadow-inner"
      />
    </div>
  );
};

export const InAppTerminalDock: React.FC<InAppTerminalDockProps> = ({
  open,
  workingDir,
  onClose,
}) => {
  const [panes, setPanes] = useState<TerminalPane[]>([]);
  const [activePaneId, setActivePaneId] = useState<string | null>(null);

  const addPane = useCallback(() => {
    setPanes((current) => {
      const index = current.length + 1;
      const pane = {
        id: window.crypto.randomUUID(),
        title: makePaneTitle(workingDir, index),
      };
      setActivePaneId(pane.id);
      return [...current, pane];
    });
  }, [workingDir]);

  useEffect(() => {
    if (open && panes.length === 0) {
      addPane();
    }
  }, [addPane, open, panes.length]);

  if (!open && panes.length === 0) return null;

  return (
    <section
      data-testid="in-app-terminal-dock"
      className={cn(
        'no-drag flex min-h-[220px] flex-shrink-0 flex-col overflow-hidden border-t border-[#b98948] bg-[#e7d4b4] text-text-default shadow-[0_-14px_36px_rgba(62,42,15,0.16)]',
        open ? 'animate-in slide-in-from-bottom-2 fade-in duration-200' : 'hidden'
      )}
      style={{ height: 'min(42vh, 380px)' }}
    >
      <div className="flex h-11 flex-shrink-0 items-end gap-2 border-b border-[#bd9154] bg-[#ddc49d] px-2 pt-1.5">
        <div
          className="flex min-w-0 flex-1 items-end gap-1"
          role="tablist"
          aria-label="Terminal sessions"
        >
          {panes.map((pane) => {
            const active = pane.id === activePaneId;
            return (
              <button
                key={pane.id}
                type="button"
                role="tab"
                aria-selected={active}
                onClick={() => setActivePaneId(pane.id)}
                className={cn(
                  'relative flex h-8 min-w-0 max-w-[180px] items-center gap-1.5 rounded-t-md border border-b-0 px-2 text-xs shadow-sm transition-colors',
                  active
                    ? 'border-[#9f7130] bg-[#fff9ed] text-text-default after:absolute after:inset-x-0 after:-bottom-px after:h-0.5 after:bg-[#fff9ed] before:absolute before:inset-x-2 before:top-0 before:h-0.5 before:rounded-full before:bg-[#9f7130]'
                    : 'border-[#caa46e] bg-[#ccb083] text-[#665033] hover:border-[#aa7d41] hover:bg-[#f1dfc1] hover:text-text-default'
                )}
              >
                <TerminalIcon className="h-3.5 w-3.5 flex-shrink-0" />
                <span className="truncate">{pane.title}</span>
              </button>
            );
          })}
          <Button
            type="button"
            variant="ghost"
            size="sm"
            shape="round"
            onClick={addPane}
            className="mb-0.5 h-7 w-7 flex-shrink-0 border border-[#caa46e] bg-[#d4bc95] p-0 text-[#665033] hover:bg-[#fff9ed] hover:text-text-default"
            aria-label="New terminal session"
            title="New terminal session"
          >
            <Plus className="h-3.5 w-3.5" />
          </Button>
        </div>
        <span className="hidden min-w-0 max-w-[38%] truncate text-xs text-text-muted sm:block">
          {formatCwd(workingDir)}
        </span>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          shape="round"
          onClick={onClose}
          className="mb-0.5 h-7 w-7 flex-shrink-0 p-0 text-[#665033] hover:bg-[#fff9ed] hover:text-text-default"
          aria-label="Hide terminal"
          title="Hide terminal"
        >
          <X className="h-3.5 w-3.5" />
        </Button>
      </div>
      <div className="min-h-0 flex-1 border-t border-[#ead6b6] bg-[#f3e4cc] p-2">
        {panes.map((pane) => (
          <TerminalPaneView
            key={pane.id}
            active={pane.id === activePaneId}
            open={open}
            paneId={pane.id}
            workingDir={workingDir}
          />
        ))}
      </div>
    </section>
  );
};

export default InAppTerminalDock;
