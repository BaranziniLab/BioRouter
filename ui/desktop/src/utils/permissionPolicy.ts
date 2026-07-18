import path from 'node:path';
import { fileURLToPath } from 'node:url';

export function isAppOrigin(candidate: string, appUrl: URL): boolean {
  let url: URL;
  try {
    url = new URL(candidate);
  } catch {
    return false;
  }

  if (appUrl.protocol !== 'file:') return url.origin === appUrl.origin;
  if (url.protocol !== 'file:') return false;

  try {
    const entry = fileURLToPath(appUrl);
    const rendererDir = path.dirname(entry);
    const target = path.resolve(fileURLToPath(url));
    return target === entry || target.startsWith(rendererDir + path.sep);
  } catch {
    return false;
  }
}

export function shouldOpenExternalNavigation(candidate: string, appUrl: URL): boolean {
  if (candidate.length > 8 * 1024) return false;
  if (isAppOrigin(candidate, appUrl)) return false;
  try {
    const url = new URL(candidate);
    return (
      (url.protocol === 'http:' || url.protocol === 'https:') &&
      url.username === '' &&
      url.password === ''
    );
  } catch {
    return false;
  }
}

export function isAllowedArtifactFrameNavigation(
  candidate: string,
  proxyBaseUrl?: string
): boolean {
  try {
    const url = new URL(candidate);
    if (candidate === 'about:srcdoc' || candidate === 'about:blank') return true;
    if (!proxyBaseUrl || url.username || url.password) return false;
    const proxy = new URL(`${proxyBaseUrl.replace(/\/+$/, '')}/mcp-ui-proxy`);
    if (
      url.origin !== proxy.origin ||
      url.pathname !== proxy.pathname ||
      url.hash ||
      url.searchParams.get('contentType') !== 'rawhtml'
    ) {
      return false;
    }
    for (const [key, value] of url.searchParams) {
      if (key === 'contentType' && value === 'rawhtml') continue;
      if (key === 'waitForRenderData' && value === 'true') continue;
      return false;
    }
    return true;
  } catch {
    return false;
  }
}

export function isAllowedRendererPermission(
  permission: string,
  requestingUrl: string,
  appUrl: URL,
  mediaTypes: ReadonlyArray<string>
): boolean {
  return (
    permission === 'media' &&
    mediaTypes.length > 0 &&
    mediaTypes.every((type) => type === 'audio') &&
    isAppOrigin(requestingUrl, appUrl)
  );
}
