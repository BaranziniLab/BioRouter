/**
 * The policy an Auto Visualiser figure (or any generated artifact) runs under.
 *
 * `default-src 'none'` with a handful of explicit grants: inline scripts and
 * `blob:` workers so the chart runtime executes, `data:`/`blob:` images, fonts
 * and media so a figure can embed its own assets — and nothing that reaches the
 * network, navigates, submits, or nests a frame.
 */
const ARTIFACT_POLICY = [
  "default-src 'none'",
  "script-src 'unsafe-inline' 'unsafe-eval' blob:",
  "style-src 'unsafe-inline'",
  'img-src data: blob:',
  "connect-src 'none'",
  'font-src data:',
  "frame-src 'none'",
  'worker-src blob:',
  'media-src data: blob:',
  "navigate-to 'none'",
  "form-action 'none'",
  "base-uri 'none'",
  "object-src 'none'",
];

export const ARTIFACT_BROWSER_CSP = ARTIFACT_POLICY.join('; ');

/**
 * The policy for the standalone wrapper page — the artifact window, the
 * open-in-browser file, and the headless renderer's blob: tab — which holds the
 * figure in a `srcdoc` iframe.
 *
 * ⚠ **It must grant everything `ARTIFACT_BROWSER_CSP` grants, and it is derived
 * rather than written for exactly that reason.** A `srcdoc` document inherits
 * its parent's policy list and enforces *both*, so the guest's effective policy
 * is the intersection. A hand-written wrapper policy of
 * `default-src 'none'; style-src 'unsafe-inline'; frame-src 'self'` — which is
 * what this used to be — therefore did not "tighten the wrapper": it silently
 * governed the figure, and **no script in any artifact ran** on those three
 * surfaces. Not the chart runtime, and not the figure's own error handler, so
 * the failure was an empty card with no message.
 *
 * Tightening this does not constrain the guest, it breaks it. The guest is
 * contained by its own identical policy plus the sandbox attribute in
 * {@link wrapArtifactForBrowser} (no `allow-same-origin`, so it cannot script
 * this page; no `allow-top-navigation`), and this page contributes no script of
 * its own — its only variable content is the escaped `srcdoc`.
 *
 * The single deliberate difference is `frame-src`: the wrapper exists to embed
 * one frame, while a figure may embed none.
 */
export const ARTIFACT_WRAPPER_CSP = ARTIFACT_POLICY.map((directive) =>
  directive === "frame-src 'none'" ? "frame-src 'self'" : directive
).join('; ');

export function injectArtifactBrowserCsp(html: string): string {
  const meta = `<meta http-equiv="Content-Security-Policy" content="${ARTIFACT_BROWSER_CSP}">`;
  const documentElement = /<html\b[^>]*>/i.exec(html);
  if (documentElement?.index !== undefined) {
    const prefix = html.slice(0, documentElement.index).trim();
    const hasOnlyPreamble = !prefix || /^<!doctype\b[^>]*>\s*$/i.test(prefix);
    if (!hasOnlyPreamble) return `<head>${meta}</head>${html}`;

    const insertAt = documentElement.index + documentElement[0].length;
    const head = /^\s*(<head\b[^>]*>)/i.exec(html.slice(insertAt));
    if (head) {
      const headEnd = insertAt + head.index + head[0].length;
      return `${html.slice(0, headEnd)}${meta}${html.slice(headEnd)}`;
    }
    return `${html.slice(0, insertAt)}<head>${meta}</head>${html.slice(insertAt)}`;
  }

  const head = /^\s*(<head\b[^>]*>)/i.exec(html);
  if (head) {
    const insertAt = head.index + head[0].length;
    return `${html.slice(0, insertAt)}${meta}${html.slice(insertAt)}`;
  }
  return `<head>${meta}</head>${html}`;
}

export function wrapArtifactForBrowser(html: string): string {
  const secured = injectArtifactBrowserCsp(html);
  const srcdoc = secured.replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/\0/g, '\uFFFD');
  return (
    '<!doctype html><html><head><meta charset="utf-8">' +
    `<meta http-equiv="Content-Security-Policy" content="${ARTIFACT_WRAPPER_CSP}">` +
    '<style>html,body,iframe{width:100%;height:100%;margin:0;border:0;overflow:hidden}body{background:#fff}</style>' +
    '</head><body><iframe name="biorouter-artifact-preview" title="Biorouter artifact preview" ' +
    'credentialless referrerpolicy="no-referrer" ' +
    'sandbox="allow-scripts allow-downloads" ' +
    `srcdoc="${srcdoc}"></iframe></body></html>`
  );
}

export function injectArtifactHostTheme(html: string, theme: 'light' | 'dark'): string {
  const tag = `<script>window.__BR_VIZ_HOST_THEME__=${JSON.stringify(theme)};</script>`;
  const head = /<head\b[^>]*>/i.exec(html);
  if (!head || head.index === undefined) return tag + html;
  const insertAt = head.index + head[0].length;
  return html.slice(0, insertAt) + tag + html.slice(insertAt);
}
