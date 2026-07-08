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

function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  return ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName);
}

function keyEventToTerminalData(event: KeyboardEvent): string | null {
  if (event.metaKey) return null;
  if (event.ctrlKey && event.key.length === 1) {
    const code = event.key.toUpperCase().charCodeAt(0);
    if (code >= 64 && code <= 95) return String.fromCharCode(code - 64);
  }
  if (event.altKey || event.ctrlKey) return null;

  switch (event.key) {
    case 'Enter':
      return '\r';
    case 'Backspace':
      return '\x7f';
    case 'Tab':
      return '\t';
    case 'Escape':
      return '\x1b';
    case 'ArrowUp':
      return '\x1b[A';
    case 'ArrowDown':
      return '\x1b[B';
    case 'ArrowRight':
      return '\x1b[C';
    case 'ArrowLeft':
      return '\x1b[D';
    case 'Delete':
      return '\x1b[3~';
    case 'Home':
      return '\x1b[H';
    case 'End':
      return '\x1b[F';
    default:
      return event.key.length === 1 ? event.key : null;
  }
}

const terminalTheme = {
  background: '#fffdf7',
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
  cursor: '#1f1a14',
  cursorAccent: '#fffdf7',
  cyan: '#16818c',
  foreground: '#2d2a26',
  green: '#22784f',
  magenta: '#7847b8',
  red: '#b63f3f',
  selectionBackground: '#eadfcf',
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

  const focusTerminal = useCallback(() => {
    terminalRef.current?.focus();
  }, []);

  const writeToBackend = useCallback((data: string) => {
    const sessionId = backendSessionIdRef.current;
    if (!sessionId) {
      pendingInputRef.current.push(data);
      return;
    }
    window.electron.writeTerminalSession(sessionId, data).catch(() => {});
  }, []);

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
        window.setTimeout(() => term.focus(), 30);
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
      cursorStyle: 'block',
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
      writeToBackend(data);
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
  }, [paneId, workingDir, writeToBackend]);

  useEffect(() => {
    fitAndFocus();
  }, [fitAndFocus]);

  useEffect(() => {
    if (!open || !active) return;

    const handleKeyDown = (event: KeyboardEvent) => {
      const host = terminalHostRef.current;
      if (isEditableTarget(event.target) && (!host || !host.contains(event.target as Node))) {
        return;
      }
      const data = keyEventToTerminalData(event);
      if (!data) return;
      event.preventDefault();
      event.stopPropagation();
      writeToBackend(data);
      focusTerminal();
    };

    window.addEventListener('keydown', handleKeyDown, true);
    return () => window.removeEventListener('keydown', handleKeyDown, true);
  }, [active, focusTerminal, open, writeToBackend]);

  return (
    <div
      className={cn(
        'col-start-1 row-start-1 flex min-h-0 overflow-hidden',
        !active && 'pointer-events-none invisible'
      )}
      data-terminal-pane={paneId}
    >
      <div
        ref={terminalHostRef}
        onMouseDown={(event) => {
          event.preventDefault();
          focusTerminal();
        }}
        className="h-full min-h-0 w-full flex-1 overflow-hidden rounded-md border border-[#e3d7c6] bg-[#fffdf7] px-2 py-2 shadow-inner shadow-black/[0.025] outline-none focus-visible:ring-1 focus-visible:ring-[#b98b52]/35 [&_.xterm]:h-full [&_.xterm-viewport]:!bg-[#fffdf7] [&_.xterm-screen]:!bg-[#fffdf7]"
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
  const dockRef = useRef<HTMLElement | null>(null);
  const suppressAutoOpenRef = useRef(false);
  const pendingDockCloseRef = useRef(false);

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
    if (!open) {
      suppressAutoOpenRef.current = false;
      pendingDockCloseRef.current = false;
      return;
    }
    if (panes.length === 0 && !suppressAutoOpenRef.current) {
      addPane();
    }
  }, [addPane, open, panes.length]);

  const closePane = useCallback(
    (paneId: string) => {
      setPanes((current) => {
        const closedIndex = current.findIndex((pane) => pane.id === paneId);
        if (closedIndex === -1) return current;

        const next = current.filter((pane) => pane.id !== paneId);
        if (next.length === 0) {
          suppressAutoOpenRef.current = true;
          pendingDockCloseRef.current = true;
          setActivePaneId(null);
          return [];
        }

        setActivePaneId((activeId) => {
          if (activeId && activeId !== paneId && next.some((pane) => pane.id === activeId)) {
            return activeId;
          }
          return next[Math.min(closedIndex, next.length - 1)].id;
        });
        return next;
      });
    },
    [onClose]
  );

  useEffect(() => {
    if (!open || panes.length !== 0 || !pendingDockCloseRef.current) return;
    pendingDockCloseRef.current = false;
    onClose();
  }, [onClose, open, panes.length]);

  useEffect(() => {
    if (!open || panes.length === 0) return;
    if (activePaneId && panes.some((pane) => pane.id === activePaneId)) return;
    setActivePaneId(panes[0].id);
  }, [activePaneId, open, panes]);

  useEffect(() => {
    if (!open) return;
    const animationFrame = window.requestAnimationFrame(() => {
      dockRef.current?.focus({ preventScroll: true });
    });
    return () => window.cancelAnimationFrame(animationFrame);
  }, [open, activePaneId]);

  if (!open && panes.length === 0) return null;

  return (
    <section
      ref={dockRef}
      data-testid="in-app-terminal-dock"
      tabIndex={-1}
      className={cn(
        'no-drag flex min-h-[220px] flex-shrink-0 flex-col overflow-hidden border-t border-border-subtle bg-[#f7f1e8] text-text-default shadow-[0_-14px_36px_rgba(62,42,15,0.10)] outline-none',
        open ? 'animate-in slide-in-from-bottom-2 fade-in duration-200' : 'hidden'
      )}
      style={{ height: 'min(42vh, 380px)' }}
    >
      <div className="flex h-11 flex-shrink-0 items-center gap-2 border-b border-[#e0d3c1] bg-[#f2e7d8] px-2">
        <div
          className="flex min-w-0 flex-1 items-center gap-1"
          role="tablist"
          aria-label="Terminal sessions"
        >
          {panes.map((pane) => {
            const active = pane.id === activePaneId;
            return (
              <div
                key={pane.id}
                className={cn(
                  'relative flex h-7 min-w-0 max-w-[190px] items-center overflow-hidden rounded-md text-xs transition-colors',
                  active
                    ? 'bg-background-default text-text-default shadow-sm shadow-black/[0.04] before:absolute before:inset-y-1 before:left-0 before:w-0.5 before:rounded-full before:bg-[#b98b52]'
                    : 'text-[#6f6255] hover:bg-background-default/70 hover:text-text-default'
                )}
              >
                <button
                  type="button"
                  role="tab"
                  aria-selected={active}
                  onClick={() => setActivePaneId(pane.id)}
                  className="flex h-full min-w-0 flex-1 items-center gap-1.5 px-2 text-left outline-none focus-visible:ring-1 focus-visible:ring-[#b98b52]/35"
                >
                  <TerminalIcon className="h-3.5 w-3.5 flex-shrink-0" />
                  <span className="truncate">{pane.title}</span>
                </button>
                <button
                  type="button"
                  onMouseDown={(event) => event.stopPropagation()}
                  onClick={(event) => {
                    event.stopPropagation();
                    closePane(pane.id);
                  }}
                  className="mr-1 flex h-5 w-5 flex-shrink-0 items-center justify-center rounded text-[#7a6a5a] transition-colors hover:bg-[#eadfce] hover:text-text-default focus-visible:ring-1 focus-visible:ring-[#b98b52]/35"
                  aria-label={`Close terminal tab ${pane.title}`}
                  title={`Close ${pane.title}`}
                >
                  <X className="h-3 w-3" />
                </button>
              </div>
            );
          })}
          <Button
            type="button"
            variant="ghost"
            size="sm"
            shape="round"
            onClick={addPane}
            className="h-7 w-7 flex-shrink-0 p-0 text-[#6f6255] hover:bg-background-default/70 hover:text-text-default"
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
          className="h-7 w-7 flex-shrink-0 p-0 text-[#6f6255] hover:bg-background-default/70 hover:text-text-default"
          aria-label="Hide terminal"
          title="Hide terminal"
        >
          <X className="h-3.5 w-3.5" />
        </Button>
      </div>
      <div className="grid min-h-0 flex-1 grid-cols-1 grid-rows-1 bg-[#fbf6ed] p-2">
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
