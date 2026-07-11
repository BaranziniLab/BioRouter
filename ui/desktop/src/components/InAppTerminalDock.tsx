import React, { useCallback, useEffect, useRef, useState } from 'react';
import { Terminal as XTerm } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';
import { Plus, Terminal as TerminalIcon, X } from './icons/app-icons';
import { Button } from './ui/button';
import { useResolvedTheme, useThemeFamily } from '../contexts/ThemeContext';
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

// xterm measures glyph widths itself, so it cannot read `var(--font-mono)`.
// This stack must stay byte-identical to --font-mono in styles/main.css, so a
// command pasted from a chat code block renders in exactly the same face.
const TERMINAL_FONT =
  'ui-monospace, "SF Mono", SFMono-Regular, "Cascadia Mono", Menlo, Consolas, "Liberation Mono", monospace';
const TERMINAL_FONT_SIZE = 13;
const TERMINAL_LINE_HEIGHT = 20 / 13; // design.md §3.2 — code/terminal is 13/20

/**
 * ANSI 16 for both themes (design.md §5.2, decision D-11).
 *
 * The ground is --background-muted, not a bespoke cream, so the terminal, the
 * chat code block and the page all share one surface. Previously a single light
 * theme was applied unconditionally, which left the terminal a glowing cream
 * rectangle in dark mode.
 *
 * Every colour clears WCAG AA (4.5:1) on its own ground. The old palette's
 * `blue` (4.45:1) and `brightBlack` — what most CLIs use for dimmed text
 * (4.32:1) — both failed; they are corrected here.
 */
const TERMINAL_THEMES = {
  light: {
    background: '#faf8f3',
    foreground: '#2d2a26',
    cursor: '#b85a32', // the focus-ring accent
    cursorAccent: '#faf8f3',
    selectionBackground: '#e4d9c3',
    black: '#2d2a26',
    red: '#b63f3f',
    green: '#22784f',
    yellow: '#9b6818',
    blue: '#255fb5',
    magenta: '#7847b8',
    cyan: '#16818c',
    white: '#574f46',
    brightBlack: '#6f6659',
    brightRed: '#d45252',
    brightGreen: '#1f7a3d',
    brightYellow: '#8a5a00',
    brightBlue: '#2f75d6',
    brightMagenta: '#9462d6',
    brightCyan: '#1f9aa6',
    brightWhite: '#2d2a26',
  },
  dark: {
    background: '#16120c',
    foreground: '#e8e1d2',
    cursor: '#e8895f',
    cursorAccent: '#16120c',
    selectionBackground: '#403928',
    black: '#3a3324',
    red: '#e2665c',
    green: '#7fbf6a',
    yellow: '#d9a441',
    blue: '#6f9fd8',
    magenta: '#b98ad6',
    cyan: '#5fb8b8',
    white: '#d4cab6',
    brightBlack: '#8d8266',
    brightRed: '#f0857b',
    brightGreen: '#9ad686',
    brightYellow: '#ecc063',
    brightBlue: '#8fb8e8',
    brightMagenta: '#d0a6e8',
    brightCyan: '#7fd0d0',
    brightWhite: '#e8e1d2',
  },
} as const;

/**
 * Alma Mater (UCSF) terminal palette — the same design as TERMINAL_THEMES but on
 * the cool navy grounds, with a UCSF ANSI-16. Grounds are --background-muted
 * (light #f2f3f4) and the navy card (dark #08213f); every colour clears WCAG AA
 * (4.5:1) on its own ground (D-11). See docs/design/alma-mater-theme.md.
 */
const ALMA_TERMINAL_THEMES = {
  light: {
    background: '#f2f3f4',
    foreground: '#052049',
    cursor: '#6c247c', // eggplant accent
    cursorAccent: '#f2f3f4',
    selectionBackground: '#d7dbe0',
    black: '#052049',
    red: '#c40d3e',
    green: '#007242',
    yellow: '#8a5a00',
    blue: '#0f388a',
    magenta: '#6c247c',
    cyan: '#0e5258',
    white: '#506380',
    brightBlack: '#586780',
    brightRed: '#d0143f',
    brightGreen: '#1f7a3d',
    brightYellow: '#8a5a00',
    brightBlue: '#255fb5',
    brightMagenta: '#8a1fa0',
    brightCyan: '#106a72',
    brightWhite: '#052049',
  },
  dark: {
    background: '#08213f',
    foreground: '#e1e3e5',
    cursor: '#c45ed8', // orchid accent
    cursorAccent: '#08213f',
    selectionBackground: '#163864',
    black: '#0d2a50',
    red: '#f5768a',
    green: '#5fbf74',
    yellow: '#feb80a',
    blue: '#7fb3e6',
    magenta: '#c58ad6',
    cyan: '#5cc6d0',
    white: '#b4b9bf',
    brightBlack: '#909aa6',
    brightRed: '#ff8fa0',
    brightGreen: '#7fd08f',
    brightYellow: '#ffca4a',
    brightBlue: '#a3c9f0',
    brightMagenta: '#d7a5e8',
    brightCyan: '#7fd8e0',
    brightWhite: '#f2f3f4',
  },
} as const;

/** Terminal palettes keyed by theme family, then resolved mode. */
const TERMINAL_THEMES_BY_FAMILY = {
  parchment: TERMINAL_THEMES,
  'alma-mater': ALMA_TERMINAL_THEMES,
} as const;

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

  // The xterm instance is created once, in an effect that must not re-run when
  // the theme flips (that would tear down the pty session). The ref lets the
  // constructor read the current theme; the effect below repaints on change.
  const resolvedTheme = useResolvedTheme();
  const resolvedThemeRef = useRef(resolvedTheme);
  resolvedThemeRef.current = resolvedTheme;
  const themeFamily = useThemeFamily();
  const themeFamilyRef = useRef(themeFamily);
  themeFamilyRef.current = themeFamily;

  useEffect(() => {
    const term = terminalRef.current;
    if (term) term.options.theme = TERMINAL_THEMES_BY_FAMILY[themeFamily][resolvedTheme];
  }, [resolvedTheme, themeFamily]);

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
      fontFamily: TERMINAL_FONT,
      fontSize: TERMINAL_FONT_SIZE,
      lineHeight: TERMINAL_LINE_HEIGHT,
      scrollback: 8000,
      theme: TERMINAL_THEMES_BY_FAMILY[themeFamilyRef.current][resolvedThemeRef.current],
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
      term.writeln(
        `\r\n\x1b[90m[terminal exited with code ${event.exitCode ?? 0}${suffix}]\x1b[0m`
      );
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
        // xterm paints its own ground from TERMINAL_THEMES; the viewport must stay
        // transparent so the theme — not a hardcoded cream — shows through.
        className="h-full min-h-0 w-full flex-1 overflow-hidden rounded-md border border-border-subtle bg-background-muted px-2 py-2 [&_.xterm]:h-full [&_.xterm-viewport]:bg-transparent! [&_.xterm-screen]:bg-transparent!"
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
        'no-drag flex min-h-[220px] flex-shrink-0 flex-col overflow-hidden border-t border-border-subtle bg-background-default text-text-default ',
        open
          ? 'animate-in slide-in-from-bottom-2 fade-in duration-[var(--motion-base)] ease-out'
          : 'hidden'
      )}
      style={{ height: 'min(42vh, 380px)' }}
    >
      <div className="flex h-11 flex-shrink-0 items-center gap-2 border-b border-border-subtle bg-background-muted px-2">
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
                  'relative flex h-7 min-w-0 max-w-[190px] items-center overflow-hidden rounded-md text-xs transition-colors before:transition-[background-color] before:duration-[var(--motion-base)] before:ease-[var(--ease-out)]',
                  active
                    ? 'bg-background-default text-text-default before:absolute before:inset-y-1 before:left-0 before:w-0.5 before:rounded-full before:bg-accent-bar'
                    : 'text-text-muted hover:bg-background-default/70 hover:text-text-default'
                )}
              >
                <button
                  type="button"
                  role="tab"
                  aria-selected={active}
                  onClick={() => setActivePaneId(pane.id)}
                  className="flex h-full min-w-0 flex-1 items-center gap-1.5 px-2 text-left"
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
                  className="mr-1 flex h-5 w-5 flex-shrink-0 items-center justify-center rounded-sm text-text-muted transition-colors hover:bg-background-medium hover:text-text-default"
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
            className="h-7 w-7 flex-shrink-0 p-0 text-text-muted hover:bg-background-default/70 hover:text-text-default"
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
          className="h-7 w-7 flex-shrink-0 p-0 text-text-muted hover:bg-background-default/70 hover:text-text-default"
          aria-label="Hide terminal"
          title="Hide terminal"
        >
          <X className="h-3.5 w-3.5" />
        </Button>
      </div>
      <div className="grid min-h-0 flex-1 grid-cols-1 grid-rows-1 bg-background-muted p-2">
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
