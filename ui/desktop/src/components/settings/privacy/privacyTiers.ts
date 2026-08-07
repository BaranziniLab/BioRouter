/**
 * The master privacy switch, as the renderer sees it (issue #56, DR-15).
 *
 * ⚠ **Its own module, and not `PrivacyPanel.tsx`.** These three values are read
 * by `ConfigContext`, which every surface in the app mounts, and by
 * `PrivacyBadge`, which is a leaf `ui/` component. Leaving them in the panel
 * would pull the whole Settings → Privacy screen — switch, input, buttons — into
 * every session row and every model chip, and would make `ConfigContext` import
 * a settings screen that imports `ConfigContext`.
 */

/**
 * The config key that holds the master switch. One spelling, shared with the
 * daemon's `biorouter::privacy::PRIVACY_TIERS_CONFIG_KEY`.
 */
export const PRIVACY_TIERS_KEY = 'BIOROUTER_PRIVACY_TIERS';

/**
 * The phrase the user must type to turn the feature off, byte-for-byte the
 * daemon's `PRIVACY_TIERS_DISABLE_PHRASE`. The daemon compares it EXACTLY, so a
 * panel that lower-cased or trimmed it would produce a 403 the user cannot
 * explain.
 */
export const DISABLE_PHRASE = 'DISABLE PRIVACY TIERS';

/**
 * `off` / `false` / `no` disable; anything else, including an absent key, is on.
 *
 * Mirrors `biorouter::privacy::privacy_tiers_value_is_on` — the daemon is the
 * authority and this is only what the renderer paints before the next read. The
 * `trim()` is part of that mirror, not a nicety: without the same surrounding
 * whitespace rule on both sides, a hand-edited `BIOROUTER_PRIVACY_TIERS: " off "`
 * renders as off in the panel while the daemon goes on enforcing, and the user
 * is told something false about a control they just used.
 */
export function privacyTiersEnabledFromConfig(value: unknown): boolean {
  if (typeof value === 'boolean') return value;
  if (typeof value !== 'string') return true;
  const v = value.trim().toLowerCase();
  return !(v === 'off' || v === 'false' || v === 'no');
}
