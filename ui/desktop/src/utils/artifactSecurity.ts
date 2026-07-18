export const ARTIFACT_BROWSER_CSP = [
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
].join('; ');

export const ARTIFACT_WRAPPER_CSP = [
  "default-src 'none'",
  "style-src 'unsafe-inline'",
  "frame-src 'self'",
].join('; ');

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
