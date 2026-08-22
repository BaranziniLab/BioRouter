/**
 * The CDN→inline rewriter for Auto Visualiser artifacts.
 *
 * Every artifact is displayed under `ARTIFACT_BROWSER_CSP`
 * (`default-src 'none'`), so the document itself can never reach the network:
 * a remote `<script src=…>` is blocked and the figure comes up blank or with
 * "… library failed to load". CDN mode still exists because it is the *stored*
 * blob that matters — `BIOROUTER_AUTOVIS_CDN=1` (the desktop default, set in
 * `biorouterd.ts`) shrinks a persisted figure from megabytes to a few KB, which
 * is what makes a large diagram survive a session reload. The main process
 * closes the gap by pre-fetching each URL below and splicing its source into
 * the document as an inline `<script>`, *before* the CSP is applied.
 *
 * Two invariants follow, and both are asserted from the Rust side in
 * `crates/biorouter-mcp/tests/autovis_cdn_desktop_contract.rs`:
 *
 * 1. Every URL the Auto Visualiser can emit in CDN mode appears in
 *    `ARTIFACT_CDN_ASSETS`. A library that is missing here is simply never
 *    inlined, and the figure fails every time in the packaged app while passing
 *    every Rust test — which is exactly how Mermaid shipped broken.
 * 2. Each is emitted as `<script src="URL" …></script>` (or a `<link>` for CSS),
 *    because that is the only shape the patterns below match. In particular a
 *    `<script type="module">import x from '…/+esm'</script>` never matches, so
 *    an ESM entrypoint cannot be used here however valid the URL is.
 */
export const ARTIFACT_CDN_ASSETS = [
  'https://cdn.jsdelivr.net/npm/d3@7/dist/d3.min.js',
  'https://cdn.jsdelivr.net/npm/d3-sankey@0.12/dist/d3-sankey.min.js',
  'https://cdn.jsdelivr.net/npm/chart.js@4/dist/chart.umd.min.js',
  'https://cdn.jsdelivr.net/npm/leaflet@1.9.4/dist/leaflet.js',
  'https://cdn.jsdelivr.net/npm/leaflet@1.9.4/dist/leaflet.css',
  'https://cdn.jsdelivr.net/npm/leaflet.markercluster@1.5.3/dist/leaflet.markercluster.js',
  // The classic (IIFE) bundle, which ends in `globalThis["mermaid"] = …`. Not
  // the `/+esm` transform: the replacement below produces a *classic* script,
  // so module source spliced into it would be a syntax error.
  'https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.min.js',
];

const escapeRegExp = (value: string): string => value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

/** The `<script src="…"></script>` shape the rewriter can replace. */
export const artifactCdnScriptPattern = (url: string): RegExp =>
  new RegExp(`<script\\b[^>]*src=["']${escapeRegExp(url)}["'][^>]*>\\s*</script>`, 'g');

/** The `<link href="…">` shape the rewriter can replace. */
export const artifactCdnStylePattern = (url: string): RegExp =>
  new RegExp(`<link\\b[^>]*href=["']${escapeRegExp(url)}["'][^>]*>`, 'g');

export type ArtifactCdnAssetFetcher = (url: string) => Promise<string>;

/**
 * Replace every known CDN reference in `rawHtml` with the library's source,
 * inlined. Unknown or unfetchable references are left untouched — the figure
 * then fails visibly rather than silently rendering half a chart.
 */
export const inlineArtifactCdnAssets = async (
  rawHtml: string,
  fetchAsset: ArtifactCdnAssetFetcher,
  onError: (url: string, error: unknown) => void = () => {}
): Promise<string> => {
  let html = rawHtml;
  for (const url of ARTIFACT_CDN_ASSETS) {
    if (!html.includes(url)) continue;
    try {
      const asset = await fetchAsset(url);
      // Replacement must be a function: minified bundles contain `$&`, `$'`,
      // `$1`, `$$` sequences, which String.replace would expand as match
      // references and corrupt the inlined script.
      if (url.endsWith('.css')) {
        html = html.replace(artifactCdnStylePattern(url), () => `<style>${asset}</style>`);
      } else {
        html = html.replace(artifactCdnScriptPattern(url), () => `<script>${asset}</script>`);
      }
    } catch (error) {
      onError(url, error);
    }
  }
  return html;
};
