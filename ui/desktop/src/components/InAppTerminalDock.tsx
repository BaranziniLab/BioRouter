import React, { useCallback, useEffect, useRef, useState } from 'react';
import { Terminal as XTerm } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';
import { Plus, Terminal as TerminalIcon, X } from './icons/app-icons';
import { Button } from './ui/button';
import { useResolvedTheme, useThemeFamily } from '../contexts/ThemeContext';
import { registerNewTerminalPane } from '../utils/terminalFocus';
import { cn } from '../utils';

interface InAppTerminalDockProps {
  open: boolean;
  workingDir?: string;
  /** Hide the dock — the header X or the parent toggle. Panes stay alive. */
  onClose: () => void;
  /**
   * The user closed the LAST pane, so this terminal is now empty. Distinct from
   * onClose (hide): the parent should DESTROY the terminal here, not just hide
   * it. Falls back to onClose when omitted, which is the local (non-/pair) dock's
   * behaviour — there hiding and emptying are the same "set open false".
   */
  onEmptied?: () => void;
}

type TerminalPane = {
  id: string;
  title: string;
};

type ProposedDimensions = {
  cols: number;
  rows: number;
};

export const MAX_TERMINAL_PANES = 8;

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
 * the cool navy grounds, with a UCSF ANSI-16. Grounds are --background-muted for
 * the family: light #f2f3f4, dark #0d2a50.
 *
 * The dark ground used to be stated as #08213f. That is wrong twice over: it is
 * --background-default (the navy *card*, two steps darker than --background-muted),
 * and it is this palette's own `black`. The ratios were therefore measured against
 * a surface the dock never paints. Corrected here, which forced two follow-ons:
 * `black` was #0d2a50 — now the ground itself, i.e. invisible — so it moves to
 * --background-strong, and `cursor` fell to 4.06:1 on the true ground so it lifts
 * from the orchid accent to --background-accent-hover (5.29:1).
 *
 * Every colour clears WCAG AA (4.5:1) on its own ground (D-11), except `black`,
 * which is the ANSI dim slot and is a lifted ground by convention (1.55:1 here,
 * matching TERMINAL_THEMES.dark's 1.49:1). See docs/design/theming/alma-mater-theme-tokens.md.
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
    background: '#0d2a50', // --background-muted, alma-mater dark
    foreground: '#e1e3e5', // 11.15:1
    cursor: '#d07ee0', // --background-accent-hover orchid, 5.29:1
    cursorAccent: '#0d2a50',
    selectionBackground: '#163864',
    black: '#1e477f', // --background-strong — the ANSI dim slot, 1.55:1
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
  const terminalExitedRef = useRef(false);
  const fitEnabledRef = useRef(open && active);
  const focusFrameRef = useRef<number | null>(null);
  const focusTimerRef = useRef<number | null>(null);
  fitEnabledRef.current = open && active;

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
      if (!terminalExitedRef.current) pendingInputRef.current.push(data);
      return;
    }
    window.electron.writeTerminalSession(sessionId, data).catch(() => {});
  }, []);

  const fitAndFocus = useCallback(() => {
    if (focusFrameRef.current !== null) {
      window.cancelAnimationFrame(focusFrameRef.current);
      focusFrameRef.current = null;
    }
    if (focusTimerRef.current !== null) {
      window.clearTimeout(focusTimerRef.current);
      focusTimerRef.current = null;
    }
    const term = terminalRef.current;
    const fitAddon = fitAddonRef.current;
    if (!term || !fitAddon || !open || !active) return;
    focusFrameRef.current = window.requestAnimationFrame(() => {
      focusFrameRef.current = null;
      try {
        fitAddon.fit();
        const { cols, rows } = getTerminalSize(fitAddon);
        const sessionId = backendSessionIdRef.current;
        if (sessionId) {
          window.electron.resizeTerminalSession(sessionId, cols, rows).catch(() => {});
        }
        term.focus();
        focusTimerRef.current = window.setTimeout(() => {
          focusTimerRef.current = null;
          if (fitEnabledRef.current) term.focus();
        }, 30);
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
      backendSessionIdRef.current = null;
      terminalExitedRef.current = true;
      pendingInputRef.current = [];
      const suffix = event.signal ? ` (${event.signal})` : '';
      term.writeln(
        `\r\n\x1b[90m[terminal exited with code ${event.exitCode ?? 0}${suffix}]\x1b[0m`
      );
    });
    const inputDisposer = term.onData((data) => {
      writeToBackend(data);
    });

    const resizeObserver = new ResizeObserver(() => {
      if (!fitEnabledRef.current) return;
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
        terminalExitedRef.current = true;
        pendingInputRef.current = [];
        term.writeln(`\x1b[31mCould not start terminal: ${result.error}\x1b[0m`);
        return;
      }
      terminalExitedRef.current = false;
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
    return () => {
      if (focusFrameRef.current !== null) window.cancelAnimationFrame(focusFrameRef.current);
      if (focusTimerRef.current !== null) window.clearTimeout(focusTimerRef.current);
    };
  }, []);

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
        // The ground is painted ONCE, by the dock's terminal region (bg-background-muted).
        // This host is transparent and contributes only the inset xterm needs to
        // breathe, so the token — not a hex — is what actually reaches the screen;
        // the theme `background` above is the forced mirror xterm needs internally
        // (it cannot read a CSS var). The bg-transparent! overrides are what keep
        // the token authoritative: without them xterm's own layers would repaint
        // the ground and any drift between hex and token would show as a seam.
        // No border and no radius here — the dock's border-t is the only hairline
        // the terminal needs (D-11).
        className="h-full min-h-0 w-full flex-1 overflow-hidden px-2 py-1.5 [&_.xterm]:h-full [&_.xterm-viewport]:bg-transparent! [&_.xterm-screen]:bg-transparent!"
      />
    </div>
  );
};

export const InAppTerminalDock: React.FC<InAppTerminalDockProps> = ({
  open,
  workingDir,
  onClose,
  onEmptied,
}) => {
  const [panes, setPanes] = useState<TerminalPane[]>([]);
  const [activePaneId, setActivePaneId] = useState<string | null>(null);
  const suppressAutoOpenRef = useRef(false);
  const pendingDockCloseRef = useRef(false);

  const addPane = useCallback(() => {
    setPanes((current) => {
      if (current.length >= MAX_TERMINAL_PANES) return current;
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

  const closePane = useCallback((paneId: string) => {
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
  }, []);

  useEffect(() => {
    if (!open || panes.length !== 0 || !pendingDockCloseRef.current) return;
    pendingDockCloseRef.current = false;
    // The last pane closed: DESTROY, don't merely hide. onEmptied lets the
    // per-tab shell drop this terminal entirely (disposing it); the local dock
    // has no onEmptied and falls back to onClose, its "set open false".
    (onEmptied ?? onClose)();
  }, [onClose, onEmptied, open, panes.length]);

  // While this dock is the VISIBLE one, own the "new terminal pane" gesture:
  // focus-aware Cmd+T (routed in App.tsx) adds a pane here instead of a chat
  // tab. Only the open dock registers, and the shell keeps exactly one open at a
  // time, so last-write-wins in the registry is correct.
  useEffect(() => {
    if (!open) return;
    return registerNewTerminalPane(addPane);
  }, [open, addPane]);

  useEffect(() => {
    if (!open || panes.length === 0) return;
    if (activePaneId && panes.some((pane) => pane.id === activePaneId)) return;
    setActivePaneId(panes[0].id);
  }, [activePaneId, open, panes]);

  if (!open && panes.length === 0) return null;

  return (
    <section
      data-testid="in-app-terminal-dock"
      className={cn(
        'no-drag flex min-h-[100px] flex-shrink-0 flex-col overflow-hidden border-t border-border-subtle bg-background-default text-text-default ',
        open
          ? 'animate-in slide-in-from-bottom-2 fade-in duration-[var(--motion-base)] ease-out'
          : 'hidden'
      )}
      style={{ height: 'min(42vh, 380px, max(100px, calc(100% - 200px)))' }}
    >
      {/* Safari-style tab strip (Ω/Ψ). .br-tabstrip owns the flex layout, the gap,
          the padding, the sidebar-coloured ground and the bottom hairline;
          .br-tab owns the tabs, the active pill and the divider between them.
          Nothing here may restyle any of that — the only additions are h-10 (the
          class leaves height to the host; the dock's strip is 40px) and
          flex-shrink-0, so the strip holds its height in the dock's flex column. */}
      <div className="br-tabstrip br-tabstrip--sm h-10 flex-shrink-0">
        <div
          className="flex min-w-0 flex-1 items-center overflow-x-auto"
          role="tablist"
          aria-label="Terminal sessions"
        >
          {panes.map((pane) => {
            const active = pane.id === activePaneId;
            return (
              <div
                key={pane.id}
                // .br-tab already caps at 190px and lets the label ellipsis;
                // flex-shrink-0 is the only addition, so a full strip scrolls
                // rather than crushing every tab.
                className="br-tab flex-shrink-0"
                data-active={active}
              >
                <button
                  type="button"
                  role="tab"
                  aria-selected={active}
                  onClick={() => setActivePaneId(pane.id)}
                  className="flex h-full min-w-0 flex-1 items-center gap-1.5 text-left"
                >
                  <TerminalIcon className="h-3.5 w-3.5 flex-shrink-0" />
                  <span className="br-tab__label">{pane.title}</span>
                </button>
                <button
                  type="button"
                  onMouseDown={(event) => event.stopPropagation()}
                  onClick={(event) => {
                    event.stopPropagation();
                    closePane(pane.id);
                  }}
                  className="flex h-5 w-5 flex-shrink-0 items-center justify-center rounded-sm text-text-muted transition-colors hover:bg-background-medium hover:text-text-default"
                  aria-label={`Close terminal tab ${pane.title}`}
                  title={`Close ${pane.title}`}
                >
                  <X className="h-3 w-3" />
                </button>
              </div>
            );
          })}
          {/* Left-aligned "+", immediately to the right of the last tab and
              scrolling WITH the tabs — the same placement (and the chat strip's
              .br-tab-new styling) as the chat tabs' new-tab button. It used to be
              pushed to the far right of the strip. */}
          <button
            type="button"
            onClick={addPane}
            disabled={panes.length >= MAX_TERMINAL_PANES}
            className="br-tab-new ml-0.5 flex h-6 w-6 flex-none items-center justify-center rounded-md text-text-muted transition-colors hover:bg-background-medium hover:text-text-default disabled:pointer-events-none disabled:opacity-40"
            aria-label="New terminal session"
            title={
              panes.length >= MAX_TERMINAL_PANES
                ? `Maximum of ${MAX_TERMINAL_PANES} terminal sessions reached`
                : 'New terminal session'
            }
          >
            <Plus className="h-4 w-4" />
          </button>
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
      {/* The single painted terminal ground. No gutter padding: the terminal
          bleeds to the dock edges, and the panes' own inset does the breathing. */}
      <div className="grid min-h-0 flex-1 grid-cols-1 grid-rows-1 bg-background-muted">
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
