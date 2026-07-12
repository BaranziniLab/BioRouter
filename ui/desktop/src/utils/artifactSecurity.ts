export const ARTIFACT_BROWSER_CSP = [
  "default-src 'none'",
  "script-src 'unsafe-inline' 'unsafe-eval' blob:",
  "style-src 'unsafe-inline'",
  'img-src data: blob: https:',
  'connect-src http://127.0.0.1:* ws://127.0.0.1:* ws://localhost:*',
  'font-src data:',
  "frame-src 'self' data: blob:",
  'worker-src blob:',
  'media-src data: blob:',
  "form-action 'none'",
  "base-uri 'none'",
  "object-src 'none'",
].join('; ');

export function injectArtifactBrowserCsp(html: string): string {
  const meta = `<meta http-equiv="Content-Security-Policy" content="${ARTIFACT_BROWSER_CSP}">`;
  const head = /<head\b[^>]*>/i.exec(html);
  if (head?.index !== undefined) {
    const insertAt = head.index + head[0].length;
    return `${html.slice(0, insertAt)}${meta}${html.slice(insertAt)}`;
  }

  const documentElement = /<html\b[^>]*>/i.exec(html);
  if (documentElement?.index !== undefined) {
    const insertAt = documentElement.index + documentElement[0].length;
    return `${html.slice(0, insertAt)}<head>${meta}</head>${html.slice(insertAt)}`;
  }
  return `<head>${meta}</head>${html}`;
}

export function injectArtifactHostTheme(html: string, theme: 'light' | 'dark'): string {
  const tag = `<script>window.__BR_VIZ_HOST_THEME__=${JSON.stringify(theme)};</script>`;
  const head = /<head\b[^>]*>/i.exec(html);
  if (!head || head.index === undefined) return tag + html;
  const insertAt = head.index + head[0].length;
  return html.slice(0, insertAt) + tag + html.slice(insertAt);
}
