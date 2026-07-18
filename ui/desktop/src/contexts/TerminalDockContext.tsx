import { createContext, useCallback, useContext, useMemo, useState } from 'react';

/** One mounted terminal, identified by the chat tab it belongs to. */
export interface TerminalDockEntry {
  /** The chat tab's id — the terminal's scope. Stable across a session bind. */
  key: string;
  /** The cwd this terminal was opened in. Frozen for the terminal's lifetime. */
  workingDir: string | undefined;
  /** Whether the user wants this terminal shown (the toggle state). A hidden
   *  terminal stays mounted with its panes alive — hide is not destroy. */
  showing: boolean;
}

export interface TerminalDockContextValue {
  /** Is the terminal for `key` currently toggled on (showing)? */
  isOpenFor: (key: string) => boolean;
  /**
   * Show or hide the terminal for `key`.
   *
   * `open === true` shows it, creating the terminal on first open and CAPTURING
   * `workingDir` once at that moment (never overwriting a live terminal's frozen
   * cwd — see below). `open === false` HIDES it: the entry and its live panes
   * survive so a background tab's shell keeps running, exactly as hiding the
   * panel in a terminal app does. Destroying a terminal (disposing its panes) is
   * `remove`, reached only when the user closes its last pane.
   */
  setOpen: (key: string, open: boolean, workingDir?: string) => void;
  /** Destroy the terminal for `key` (its last pane was closed). */
  remove: (key: string) => void;
  /** Drop every terminal whose tab no longer exists, unmounting + disposing it. */
  retain: (liveKeys: readonly string[]) => void;
  /** Every mounted terminal, in the order it was first opened. The shell renders
   *  one dock per entry and shows only the active tab's. */
  terminals: readonly TerminalDockEntry[];
}

const TerminalDockContext = createContext<TerminalDockContextValue | null>(null);

/**
 * Returns null outside a provider — the ChatContext.tsx:74-77 pattern.
 *
 * That null is what makes the seam free: with no provider mounted (every surface
 * other than /pair — the Dashboard, /extensions, the Hub), BaseChat keeps its own
 * local dock state and its behaviour is byte-identical to before this existed.
 */
export function useTerminalDock(): TerminalDockContextValue | null {
  return useContext(TerminalDockContext);
}

/**
 * PER-CHAT-TAB terminals, keyed by tab id.
 *
 * Every chat tab has its OWN terminal — its own open/hidden state and its own
 * panes — and switching tabs switches which one you see. The dock used to be one
 * window-global panel shared by every tab, so opening it in one tab showed it in
 * all of them and its cwd was frozen to whichever tab happened to open it first.
 *
 * The state lives HERE, above the tab switch, rather than inside BaseChat,
 * because BaseChat is keyed by tab id and remounts on every tab switch — local
 * state would tear the pty down the moment you clicked another tab. The shell
 * renders one InAppTerminalDock per entry and toggles VISIBILITY (open) rather
 * than mounting, so a hidden tab's shell keeps running.
 *
 * ## The frozen-cwd contract
 *
 * A pane captures its cwd at creation and never re-reads it; InAppTerminalDock
 * recreates its panes when `workingDir` changes. So `workingDir` is captured
 * ONCE, when a terminal is first opened, and never overwritten while it lives —
 * a live terminal's shell must never be respawned under a running command. A
 * fresh open (after the previous terminal was destroyed) picks up the tab's
 * then-current session cwd, which is the moment the user actually asked for one.
 */
export function TerminalDockProvider({ children }: { children: React.ReactNode }) {
  const [entries, setEntries] = useState<TerminalDockEntry[]>([]);

  const setOpen = useCallback((key: string, open: boolean, requestedWorkingDir?: string) => {
    setEntries((current) => {
      const index = current.findIndex((entry) => entry.key === key);
      if (open) {
        // First open for this tab: create the terminal and capture its cwd once.
        if (index === -1) {
          return [...current, { key, workingDir: requestedWorkingDir, showing: true }];
        }
        // Already alive — just reveal it. NEVER touch the frozen cwd here (a
        // running shell would be respawned under the user's command).
        if (current[index].showing) return current;
        const next = current.slice();
        next[index] = { ...current[index], showing: true };
        return next;
      }
      // Hide, keeping the entry and its live panes.
      if (index === -1 || !current[index].showing) return current;
      const next = current.slice();
      next[index] = { ...current[index], showing: false };
      return next;
    });
  }, []);

  const remove = useCallback((key: string) => {
    setEntries((current) => {
      if (!current.some((entry) => entry.key === key)) return current;
      return current.filter((entry) => entry.key !== key);
    });
  }, []);

  const retain = useCallback((liveKeys: readonly string[]) => {
    setEntries((current) => {
      const live = new Set(liveKeys);
      const next = current.filter((entry) => live.has(entry.key));
      return next.length === current.length ? current : next;
    });
  }, []);

  const isOpenFor = useCallback(
    (key: string) => entries.some((entry) => entry.key === key && entry.showing),
    [entries]
  );

  const value = useMemo<TerminalDockContextValue>(
    () => ({ isOpenFor, setOpen, remove, retain, terminals: entries }),
    [isOpenFor, setOpen, remove, retain, entries]
  );

  return <TerminalDockContext.Provider value={value}>{children}</TerminalDockContext.Provider>;
}
