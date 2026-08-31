export type ManagedAppPreviewBackend = {
  baseUrl: string;
  signal: AbortSignal;
};

export type ManagedAppPreviewScope = {
  backend: ManagedAppPreviewBackend;
  origin: string;
  port: number;
  appId: string;
  rootPath: string;
};

function safeUrl(candidate: string): URL | null {
  try {
    // Reject ambiguous spellings before URL normalizes dot segments/escapes.
    if (/[\\\s]/.test(candidate)) return null;
    const rawPath = candidate.replace(/^[a-z]+:\/\/[^/]+/i, '').split(/[?#]/, 1)[0];
    if (rawPath.includes('%') || rawPath.split('/').some((part) => part === '.' || part === '..')) {
      return null;
    }
    const url = new URL(candidate);
    return url.username || url.password ? null : url;
  } catch {
    return null;
  }
}

export function managedAppPreviewScope(
  candidate: string,
  backend?: ManagedAppPreviewBackend
): ManagedAppPreviewScope | null {
  if (!backend || backend.signal.aborted) return null;
  const base = safeUrl(backend.baseUrl);
  if (
    !base ||
    base.protocol !== 'http:' ||
    base.hostname !== '127.0.0.1' ||
    !base.port ||
    base.pathname !== '/' ||
    base.search ||
    base.hash ||
    backend.baseUrl.replace(/\/$/, '') !== base.origin
  ) {
    return null;
  }
  const url = safeUrl(candidate);
  if (!url || url.origin !== base.origin || !candidate.startsWith(`${base.origin}/`)) return null;
  const match = /^\/apps\/([A-Za-z0-9_-]{1,128})\/?$/.exec(url.pathname);
  if (!match) return null;
  return {
    backend,
    origin: base.origin,
    port: Number(base.port),
    appId: match[1],
    rootPath: `/apps/${match[1]}/`,
  };
}

export function isManagedAppNavigation(scope: ManagedAppPreviewScope, candidate: string): boolean {
  return managedAppPreviewScope(candidate, scope.backend)?.appId === scope.appId;
}

export function isManagedAppRequest(
  scope: ManagedAppPreviewScope,
  request: { url: string; method: string; resourceType: string }
): boolean {
  if (scope.backend.signal.aborted || request.method !== 'GET') return false;
  if (request.url.startsWith('data:')) {
    return (
      (request.resourceType === 'image' && /^data:image\//i.test(request.url)) ||
      (request.resourceType === 'font' && /^data:(?:font\/|application\/font-)/i.test(request.url))
    );
  }
  const url = safeUrl(request.url);
  if (!url) return false;
  const socket = request.resourceType === 'webSocket';
  const origin = socket ? scope.origin.replace(/^http:/, 'ws:') : scope.origin;
  if (url.origin !== origin || !request.url.startsWith(`${origin}/`)) return false;
  if (socket) return url.pathname === `${scope.rootPath}agent`;
  if (request.resourceType === 'mainFrame') return isManagedAppNavigation(scope, request.url);
  if (url.pathname === scope.rootPath || url.pathname === scope.rootPath.slice(0, -1)) return true;
  if (!url.pathname.startsWith(scope.rootPath)) return false;
  const tail = url.pathname.slice(scope.rootPath.length);
  return (
    tail === 'models' ||
    tail === 'runstate' ||
    /^(?:dist|assets)\/[A-Za-z0-9_.-]+(?:\/[A-Za-z0-9_.-]+)*$/.test(tail)
  );
}

// An additional policy intersects with (never replaces) the server's CSP.
export const MANAGED_APP_PREVIEW_CSP = "worker-src 'none'; object-src 'none'; form-action 'none'";
