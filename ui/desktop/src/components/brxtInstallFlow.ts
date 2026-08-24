// ui/desktop/src/components/brxtInstallFlow.ts
import type { ProviderTier } from '../api/types.gen';
import type { BrxtEnvVar, BrxtManifest } from '../types/brxt';

/**
 * Issue #116. **Where an install came from** — the fact `BrxtInstallModal`'s
 * state and copy follow, rather than "does a manifest happen to be loaded".
 *
 * The distinction is not cosmetic. A local file can arrive *already chosen*:
 * double-clicking a `.brxt` in Finder hands the path to the app over IPC and
 * `ExtensionsView` passes it as `preloadedFilePath`. So a preloaded path is
 * **not** evidence of a marketplace install and must never be read as one —
 * that conflation is exactly how the marketplace route ended up rendering
 * "Drop your .brxt file here" underneath a fully populated manifest card.
 *
 * Only the caller knows the origin, so the caller states it.
 */
export type BrxtInstallOrigin =
  | { kind: 'local-file' }
  | {
      kind: 'marketplace';
      /**
       * Issue #56 Task 43 (DR-23). Recorded beside the config entry so the
       * daemon re-derives the privacy tier from the stable registry id rather
       * than from a config name the user (or the model) can rename.
       */
      registrySource: { registryId: string; sourceUrl?: string };
      /**
       * What the marketplace row said. The modal opens the moment Add is
       * clicked — before any bundle exists — so this is the only thing it has
       * to name the extension with while the download is in flight.
       */
      entry: {
        name: string;
        organization?: string;
        version?: string;
        description?: string;
        /** The badge the marketplace row showed, via `effectivePrivacy`. */
        privacyTier?: ProviderTier;
      };
      /** True while the marketplace is still fetching the asset. */
      downloading: boolean;
      /** Set when the download failed. The modal then offers Retry / Back. */
      downloadError?: string | null;
      /** Re-run the download for the same registry entry. */
      onRetry: () => void;
    };

export type PrimaryActionKind = 'install' | 'configure' | 'optional';

export interface PrimaryAction {
  kind: PrimaryActionKind;
  label: string;
}

/**
 * What the first step's primary button *actually does*, said in its own label.
 *
 * `handleNext` has always installed immediately when a manifest declares no
 * environment variables, while the button read "Next: configure" regardless —
 * so the screenshot in issue #116 says "0 required env vars" directly above a
 * button promising a configuration step that will never appear. Three answers,
 * because there are three behaviours:
 *
 *   - required variables exist  -> a form the user MUST fill: "Next: configure"
 *   - only optional ones exist  -> a form the user MAY skip, named as optional
 *   - none at all               -> the button installs: "Install extension"
 */
export function primaryInstallAction(manifest: BrxtManifest | null): PrimaryAction {
  const envVars: BrxtEnvVar[] = manifest?.env_vars ?? [];
  if (envVars.some((v) => v.required)) return { kind: 'configure', label: 'Next: configure' };
  if (envVars.length > 0) return { kind: 'optional', label: 'Next: optional settings' };
  return { kind: 'install', label: 'Install extension' };
}

export type MarketplacePhase = 'downloading' | 'validating' | 'error' | 'ready';

/**
 * The marketplace route's own progress, which the local route has no analogue
 * for: the user clicked Add on a row and the bundle has to be fetched and read
 * before anything can be confirmed.
 *
 * An error — from either half — wins over a still-running phase, because the
 * recovery it offers (Retry / Back to marketplace) is the only useful thing on
 * screen at that point.
 */
export function marketplacePhase(state: {
  downloading: boolean;
  downloadError?: string | null;
  isValidating: boolean;
  validationError: string | null;
  hasManifest: boolean;
}): MarketplacePhase {
  if (state.downloadError || state.validationError) return 'error';
  if (state.downloading) return 'downloading';
  if (state.hasManifest && !state.isValidating) return 'ready';
  // The gap between "download finished" and "validation started" is one React
  // tick with no manifest and nothing in flight. Reporting `downloading` there
  // would run the progress backwards, so the later phase wins.
  return 'validating';
}

/**
 * The tier the install will actually produce, never lowered.
 *
 * `effectivePrivacy` (what the marketplace row rendered) is a superset of
 * `classifyExtension` (what the installed config will classify as), so the two
 * agree for any catalogue that came through `loadRegistry`. Where they could
 * ever diverge, private wins: the badge is the only warning a user gets before
 * a cohort reaches a public model, and a confirmation screen that lowers it is
 * the one direction §10.2's union rule forbids.
 */
export function resultingPrivacyTier(
  manifestTier: ProviderTier | null,
  originTier: ProviderTier | undefined
): ProviderTier {
  return manifestTier === 'private' || originTier === 'private' ? 'private' : 'public';
}
