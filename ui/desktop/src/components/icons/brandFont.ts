/**
 * The one face the BR monogram and the wordmark are set in.
 *
 * Inter, bundled as an `@font-face` in `styles/main.css` — deliberately NOT the
 * app's native UI stack, so the mark matches the app icon on every platform
 * instead of drifting into whatever each OS picks. The native stack stays only
 * as a load-time fallback.
 *
 * It lives here because the mark and the wordmark are two drawings of ONE
 * identity: rendered side by side in the sidebar, a face that drifted in one of
 * them would be visible as a mismatch, and two files each declaring their own
 * copy is precisely how that happens. A shared constant makes the drift
 * impossible rather than merely detectable.
 */
export const BRAND_SANS =
  'Inter, -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, Roboto, "Helvetica Neue", Arial, sans-serif';
