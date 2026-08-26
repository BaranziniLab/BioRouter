/**
 * How the agent finds out what the user is looking at.
 *
 * The artifact panel's state is deliberately local — `useArtifactPanel` holds
 * `presentedArtifact` in a plain `useState`, and three surfaces mount three
 * independent panels. There is no context and no store, so there was no way to
 * ask "what is on screen?" from anywhere else.
 *
 * This is the narrowest thing that fixes that: a session-keyed registry of
 * *accessors*, in the same shape as `workspaceCommandRegistry` next door. The
 * panel registers what it can answer; the workspace channel routes a tool call
 * to it. Nothing else gains a way to read panel state, and the panel keeps
 * owning its own data.
 */

export type PanelDescriptor = {
  /** Whether a panel is open at all. */
  open: boolean;
  /** What kind of thing is shown: 'html' | 'file' | 'webPage' | 'mcpResource'. */
  kind?: string;
  title?: string;
  /** Absolute path for a file, or the URL for a live page. */
  locator?: string;
  sourceRevision?: string;
  /** How many tabs the panel is holding. */
  tabCount?: number;
};

export type PanelTextSnapshot = {
  kind: string;
  title: string;
  locator?: string;
  sourceRevision?: string;
  text: string;
  /** True when `text` was cut short. */
  truncated: boolean;
};

export type PanelAccessor = {
  /**
   * A cheap summary. Rides the existing workspace echo, so answering "what is
   * displayed?" costs no round-trip at all.
   */
  describe: () => PanelDescriptor;
  /**
   * The textual content of what is shown, bounded.
   *
   * This is the channel the agent should reach for first — a structured read is
   * both cheaper and more actionable than an image, and a screenshot cannot be
   * acted on. Returns `null` when the content is not textual.
   */
  readText: (maxChars: number) => Promise<PanelTextSnapshot | null>;
  /**
   * A PNG of the panel, as a file path.
   *
   * Out of band deliberately: the workspace channel caps an inbound frame at
   * 128 KiB and hands stored echoes to the model verbatim, so pixels must not
   * travel through it.
   */
  capture: () => Promise<{ path: string; width: number; height: number } | null>;
};

const accessors = new Map<string, PanelAccessor>();

/** Registers this session's panel. Returns the unregister function. */
export function registerPanelAccess(sessionId: string, accessor: PanelAccessor): () => void {
  accessors.set(sessionId, accessor);
  return () => {
    // Only clear if we are still the owner: a re-register during a remount
    // must not be undone by the previous effect's cleanup.
    if (accessors.get(sessionId) === accessor) accessors.delete(sessionId);
  };
}

export function panelAccessFor(sessionId: string): PanelAccessor | null {
  return accessors.get(sessionId) ?? null;
}

/**
 * The descriptor for a session, or a closed panel.
 *
 * Never throws: this is called while building the workspace echo, which runs on
 * every commit, and an exception there would take down the channel that carries
 * every other workspace command.
 */
export function describePanel(sessionId: string | null | undefined): PanelDescriptor {
  if (!sessionId) return { open: false };
  try {
    return accessors.get(sessionId)?.describe() ?? { open: false };
  } catch {
    return { open: false };
  }
}

/** Tests only — the singleton must not leak across cases. */
export function resetPanelAccessRegistry(): void {
  accessors.clear();
}
