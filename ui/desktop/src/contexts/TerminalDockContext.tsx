import { createContext, useCallback, useContext, useMemo, useRef, useState } from 'react';

export interface TerminalDockContextValue {
  isOpen: boolean;
  /** The cwd the dock was opened with. Frozen for the lifetime of the open. */
  workingDir: string | undefined;
  /**
   * Open or close the global dock. `workingDir` is a REQUEST, honoured only on
   * the false -> true transition — see the capture site for why.
   */
  setOpen: (open: boolean, workingDir?: string) => void;
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
 * ONE terminal dock for every chat group (spec: "the panel stays global, spans
 * all groups"). The shell renders the dock once, below the groups row; each
 * group's header toggle drives it through here.
 */
export function TerminalDockProvider({ children }: { children: React.ReactNode }) {
  const [isOpen, setIsOpen] = useState(false);
  const [workingDir, setWorkingDir] = useState<string | undefined>(undefined);
  const isOpenRef = useRef(false);

  const setOpen = useCallback((open: boolean, requestedWorkingDir?: string) => {
    // ===================================================================
    // CAPTURE THE CWD ONCE, ON THE false -> true TRANSITION ONLY.
    //
    // InAppTerminalDock RECREATES ITS PANES when `workingDir` changes: its
    // pane-factory useCallback lists workingDir in its deps
    // (InAppTerminalDock.tsx:444) and the reset effect keys off it. So a dock
    // whose workingDir live-tracked the ACTIVE GROUP would tear down and respawn
    // every shell the moment the user clicked into the other pane — mid-command,
    // with no warning and no way back. Someone's long-running job, killed by a
    // click on a tab.
    //
    // Once the dock is open its cwd is therefore frozen: the panes already
    // captured their cwd at creation and never re-read it, and this makes that
    // existing contract explicit at the only place that could break it. The next
    // OPEN picks up the then-active group's cwd, which is the moment the user
    // actually asked for a terminal somewhere.
    //
    // Do not "improve" this by adding workingDir to a dep array.
    // ===================================================================
    if (open && !isOpenRef.current) setWorkingDir(requestedWorkingDir);
    isOpenRef.current = open;
    setIsOpen(open);
  }, []);

  const value = useMemo<TerminalDockContextValue>(
    () => ({ isOpen, workingDir, setOpen }),
    [isOpen, workingDir, setOpen]
  );

  return <TerminalDockContext.Provider value={value}>{children}</TerminalDockContext.Provider>;
}
