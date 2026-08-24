/**
 * The skill catalog, as the daemon serves it (#113).
 *
 * # What this replaces
 *
 * Every skill surface used to walk the filesystem itself, over
 * `ALL_SKILL_DIRS` — three roots. The backend discovers seven kinds, including
 * `~/.config/biorouter/extensions/<name>/skills`, so a skill bundled inside an
 * installed extension (BiorOffice's Word/Excel/PowerPoint, MarkItDown's
 * converter) was live for the model and had no row in the picker. A second
 * scanner with a different root list is not a bug you fix once; the lists drift
 * again the next time a root is added. So there is no scanner here — the
 * catalog is fetched.
 *
 * # The rule this hook exists to enforce
 *
 * **A switch reflects confirmed backend state, never local intent.** A per-chat
 * toggle used to write a `Map` in React state and raise a green toast. Nothing
 * left the renderer. Here the toggle is optimistic *for one frame* and then
 * replaced by the catalog the daemon returns; a failed write restores the
 * previous catalog and reports the error, and no success toast is raised in
 * that case. `applyResultIsAuthoritative` in the tests pins it.
 *
 * # Two scopes, deliberately different destinations
 *
 * * **Hub** (no session): the machine-wide preference, `skills-config.json`,
 *   through the existing `skillOverrides` store — the same file
 *   `biorouter skill enable/disable` writes. The catalog is then refetched
 *   rather than patched, because that file is read fresh on every catalog view.
 * * **A chat** (session id present): `POST /skills/session`, which persists
 *   `workspace_skills/v1` on the session row and touches no machine-wide file.
 *   Getting this backwards would make one chat's toggle change every other
 *   chat, window and CLI invocation.
 *
 * # `catalog:changed` (#112)
 *
 * Installing an extension can add a whole skill root
 * (`~/.config/biorouter/extensions/<name>/skills`), so this hook subscribes to
 * the machine-wide `catalog:changed` event and rescans on it.
 *
 * ⚠ **It keys off `revision` and reads nothing else from the payload.** The
 * event carries a `skills[]` list, and a consumer that repaired its inventory
 * from that list would drift the first time two events raced. A monotonic
 * revision that has advanced means "you are stale"; the answer to being stale
 * is to refetch.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import type { CatalogBundle, CatalogSkill, CatalogView, SkillRoot } from '../../api';
import { refreshSkillCatalog, setSessionSkills, skillCatalogHandler } from '../../api';
import { isContextSkill } from '../settings/contexts/contexts';
import {
  loadSkillOverrides,
  saveSkillOverrides,
  setSkillOverride,
} from '../../store/skillOverrides';

/** What one mutation did, with the daemon's own words when it refused. */
export type SkillMutationResult = { ok: true } | { ok: false; error: string };

/** One row in a skill picker: a standalone skill, or a whole bundle. */
export type SkillCatalogEntry =
  | { kind: 'single'; key: string; skill: CatalogSkill; enabled: boolean }
  | { kind: 'bundle'; key: string; bundle: CatalogBundle; enabled: boolean };

export interface SkillCatalogState {
  /** Rows for the picker: bundles first, then standalone skills, each sorted. */
  entries: SkillCatalogEntry[];
  /** Every skill, including bundle members — for search and for detail rows. */
  skills: CatalogSkill[];
  bundles: CatalogBundle[];
  roots: SkillRoot[];
  /** Bumped by the daemon on every rescan. */
  generation: number;
  loading: boolean;
  /** Set when the catalog could not be read at all. */
  error: string | null;
  /** Refetch. Pass `true` to make the daemon rescan the filesystem first. */
  reload: (rescan?: boolean) => Promise<void>;
  /**
   * Enable or disable one or more entries by key.
   *
   * ⚠ **The refusal message is carried in the RESULT, not in hook state.** It
   * was a `lastError` field once, and the caller read it immediately after
   * awaiting — from the render closure it had captured *before* the state
   * update, so every error toast said "The change was not saved." with the
   * reason silently dropped. A value that is only ever read by the awaiting
   * caller belongs to that call.
   *
   * Never throws, so a caller cannot forget the failure branch and leave a
   * stale switch on screen.
   */
  setEnabled: (keys: string[], enabled: boolean) => Promise<SkillMutationResult>;
}

const EMPTY: CatalogView = { generation: 0, roots: [], skills: [], bundles: [] };

/**
 * The machine-wide inventory-changed signal (#112). Named here as a constant so
 * the string is not spelled twice, and so a grep for the name finds the
 * consumer as well as the producer.
 */
export const CATALOG_CHANGED_EVENT = 'catalog:changed';

function errorText(err: unknown): string {
  if (err instanceof Error) return err.message;
  if (typeof err === 'string') return err;
  return 'the request failed.';
}

/**
 * Contexts ship with the app, so they are not what a user means by "skills".
 * Filtered here rather than in the daemon: the catalog is also what Settings
 * and the importer read, and both of those legitimately want the full set.
 */
function isPickerSkill(skill: CatalogSkill): boolean {
  return !isContextSkill(skill.name);
}

export function useSkillCatalog(sessionId: string | null): SkillCatalogState {
  const [view, setView] = useState<CatalogView>(EMPTY);
  // ⚠ **The rollback target is a ref, not the render closure's `view`.**
  // Two toggles can be made before React re-renders, and a callback created in
  // the first render captures the catalog as it was *then*. So when the second
  // toggle was refused, the rollback restored the state from before the FIRST —
  // silently undoing a change the daemon had already accepted, on screen only.
  // A ref written wherever the catalog is committed is always the last thing
  // this hook actually published, whatever the render timing.
  const committed = useRef<CatalogView>(EMPTY);
  const commit = useCallback((next: CatalogView) => {
    committed.current = next;
    setView(next);
  }, []);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  // Serialises mutations so two fast clicks cannot land out of order and leave
  // the switch showing the older of the two answers.
  const queue = useRef<Promise<unknown>>(Promise.resolve());

  const reload = useCallback(
    async (rescan = false) => {
      setLoading(true);
      try {
        const query = sessionId ? { session_id: sessionId } : {};
        const response = rescan
          ? await refreshSkillCatalog<true>({ query, throwOnError: true })
          : await skillCatalogHandler<true>({ query, throwOnError: true });
        commit(response.data);
        setError(null);
      } catch (err) {
        setError(`Could not read the skill catalog: ${errorText(err)}`);
      } finally {
        setLoading(false);
      }
    },
    [commit, sessionId]
  );

  useEffect(() => {
    void reload();
  }, [reload]);

  // A `window` event rather than React state, so this works from any surface —
  // including one outside the provider tree, or in a second window.
  const appliedRevision = useRef(0);
  useEffect(() => {
    const onCatalogChanged = (event: Event) => {
      const revision = (event as CustomEvent<{ revision?: number }>).detail?.revision;
      if (typeof revision === 'number') {
        if (revision <= appliedRevision.current) return;
        appliedRevision.current = revision;
      }
      void reload(true);
    };
    window.addEventListener(CATALOG_CHANGED_EVENT, onCatalogChanged);
    return () => window.removeEventListener(CATALOG_CHANGED_EVENT, onCatalogChanged);
  }, [reload]);

  const setEnabled = useCallback(
    (keys: string[], enabled: boolean): Promise<SkillMutationResult> => {
      if (keys.length === 0) return Promise.resolve({ ok: true });

      const run = async (): Promise<SkillMutationResult> => {
        const previous = committed.current;
        // Optimistic, so the switch does not lag the click — and reverted below
        // if the write is refused.
        commit(applyOptimistically(previous, keys, enabled, sessionId !== null));

        try {
          if (sessionId) {
            const response = await setSessionSkills<true>({
              body: {
                sessionId,
                add: enabled ? keys : [],
                remove: enabled ? [] : keys,
              },
              throwOnError: true,
            });
            commit(response.data.catalog);
          } else {
            keys.forEach((key) => setSkillOverride(key, enabled));
            await saveSkillOverrides();
            // `skills-config.json` is read fresh on every catalog view, so a
            // plain refetch already reflects the write — no rescan needed.
            const response = await skillCatalogHandler<true>({
              query: {},
              throwOnError: true,
            });
            commit(response.data);
          }
          return { ok: true };
        } catch (err) {
          commit(previous);
          if (!sessionId) {
            // Put the in-memory store back in step with the file we failed to
            // write, or the next save would persist this failed edit.
            await loadSkillOverrides();
          }
          return { ok: false, error: errorText(err) };
        }
      };

      const next = queue.current.then(run, run);
      queue.current = next;
      return next;
    },
    // ⚠ No `view` here — see `committed`. Depending on it would also give
    // `setEnabled` a new identity on every catalog change, which every caller
    // then has in ITS dependency array.
    [commit, sessionId]
  );

  const entries = useMemo((): SkillCatalogEntry[] => {
    const bundles: SkillCatalogEntry[] = view.bundles
      .map((bundle) => ({
        kind: 'bundle' as const,
        key: bundle.name,
        bundle,
        enabled: bundle.state.effective,
      }))
      .sort((a, b) => a.bundle.displayName.localeCompare(b.bundle.displayName));

    const singles: SkillCatalogEntry[] = view.skills
      .filter((skill) => !skill.bundle && isPickerSkill(skill))
      .map((skill) => ({
        kind: 'single' as const,
        key: skill.name,
        skill,
        enabled: skill.state.effective,
      }))
      .sort((a, b) => a.skill.name.localeCompare(b.skill.name));

    return [...bundles, ...singles];
  }, [view]);

  return {
    entries,
    skills: view.skills,
    bundles: view.bundles,
    roots: view.roots,
    generation: view.generation,
    loading,
    error,
    reload,
    setEnabled,
  };
}

/**
 * The one-frame optimistic view.
 *
 * ⚠ It edits `state.effective` **only**. `machineEnabled` and `session` are the
 * daemon's answer about where the change was persisted, and guessing at them
 * would let the interface claim a per-chat toggle had changed the machine-wide
 * preference. The authoritative catalog replaces this a moment later either
 * way; this exists so the switch does not visibly lag the pointer.
 *
 * Exported for the tests, which assert exactly that restriction.
 */
export function applyOptimistically(
  view: CatalogView,
  keys: string[],
  enabled: boolean,
  scopedToSession: boolean
): CatalogView {
  const targeted = new Set(keys);
  const bundleNames = new Set(
    view.bundles.filter((bundle) => targeted.has(bundle.name)).map((bundle) => bundle.name)
  );
  const hit = (name: string, bundle?: string | null) =>
    targeted.has(name) || (bundle != null && bundleNames.has(bundle));

  return {
    ...view,
    skills: view.skills.map((skill) =>
      hit(skill.name, skill.bundle)
        ? {
            ...skill,
            state: {
              ...skill.state,
              effective: enabled,
              // A per-chat toggle cannot move the machine-wide answer, and a
              // machine-wide one cannot invent a per-chat deviation.
              machineEnabled: scopedToSession ? skill.state.machineEnabled : enabled,
            },
          }
        : skill
    ),
    bundles: view.bundles.map((bundle) =>
      targeted.has(bundle.name)
        ? {
            ...bundle,
            state: {
              ...bundle.state,
              effective: enabled,
              machineEnabled: scopedToSession ? bundle.state.machineEnabled : enabled,
            },
          }
        : bundle
    ),
  };
}
