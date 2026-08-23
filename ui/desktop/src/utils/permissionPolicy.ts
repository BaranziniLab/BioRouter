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

/**
 * Where an artifact preview frame is allowed to navigate: nowhere.
 *
 * Every surface that displays a generated artifact — the side panel, the
 * "open in browser" wrapper document — hands the figure to the frame as a
 * `srcdoc`, so the only legitimate destinations are `about:srcdoc` and the
 * `about:blank` the frame starts at. Anything else is the guest document trying
 * to move the frame somewhere, which is exactly what must not happen.
 *
 * This used to carry a second allowance: the daemon's `/mcp-ui-proxy`, which
 * served the figure to an inline iframe in the transcript. That surface is gone
 * — a figure is only ever displayed in the artifact panel now — and with it the
 * one reason this function needed to know the daemon's origin at all. Losing the
 * parameter is a tightening, not a regression: the policy is now a closed set of
 * two literals with nothing configurable about it.
 */
export function isAllowedArtifactFrameNavigation(candidate: string): boolean {
  return candidate === 'about:srcdoc' || candidate === 'about:blank';
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
