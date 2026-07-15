/**
 * BR-63: the composer's reasoning-effort control.
 *
 * The chat request carries the effort per turn, so the value has to be readable
 * from `chatStreamStore` (a plain class, not a React component) at submit time.
 * That is why this is a tiny module store rather than React context: the toggle
 * writes it, the submit path reads it, and subscribers re-render.
 *
 * `normal` is the default and is **never sent** — an omitted `reasoning_effort`
 * leaves the session's `/effort` setting (and the model's own default depth)
 * alone, so a user who never touches the control gets exactly the old behaviour.
 */
import type { ReasoningEffort } from '../api/types.gen';

export type { ReasoningEffort };

const STORAGE_KEY = 'biorouter.reasoningEffort';

export const REASONING_EFFORTS: ReasoningEffort[] = ['quick', 'normal', 'deep'];

export const DEFAULT_REASONING_EFFORT: ReasoningEffort = 'normal';

export const REASONING_EFFORT_LABELS: Record<ReasoningEffort, string> = {
  quick: 'Quick',
  normal: 'Normal',
  deep: 'Deep',
};

export const REASONING_EFFORT_DESCRIPTIONS: Record<ReasoningEffort, string> = {
  quick: 'Fast answers with minimal exploration.',
  normal: "The model's default depth.",
  deep: 'More thinking and exploration.',
};

function isReasoningEffort(value: unknown): value is ReasoningEffort {
  return typeof value === 'string' && (REASONING_EFFORTS as string[]).includes(value);
}

let current: ReasoningEffort = DEFAULT_REASONING_EFFORT;
let loaded = false;
const listeners = new Set<() => void>();

/** The effort the composer is set to. Reads through to localStorage once. */
export function getReasoningEffort(): ReasoningEffort {
  if (!loaded) {
    loaded = true;
    try {
      const stored = localStorage.getItem(STORAGE_KEY);
      if (isReasoningEffort(stored)) {
        current = stored;
      }
    } catch {
      // Storage unavailable (private mode, tests) — stay on the default.
    }
  }
  return current;
}

export function setReasoningEffort(effort: ReasoningEffort): void {
  loaded = true;
  if (current === effort) return;
  current = effort;
  try {
    localStorage.setItem(STORAGE_KEY, effort);
  } catch {
    // Non-fatal: the choice still applies to this session.
  }
  listeners.forEach((listener) => listener());
}

export function subscribeToReasoningEffort(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/**
 * What to put on the chat request. `normal` is omitted so it can't stomp a
 * session-level `/effort` the user set from the composer.
 */
export function reasoningEffortForRequest(): ReasoningEffort | undefined {
  const effort = getReasoningEffort();
  return effort === DEFAULT_REASONING_EFFORT ? undefined : effort;
}

/** Test-only: drop the cached value so a fresh localStorage read happens. */
export function resetReasoningEffortForTests(): void {
  current = DEFAULT_REASONING_EFFORT;
  loaded = false;
  listeners.clear();
}
