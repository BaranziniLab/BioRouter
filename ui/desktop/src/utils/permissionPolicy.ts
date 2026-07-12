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
