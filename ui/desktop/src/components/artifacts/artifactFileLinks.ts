const WINDOWS_ABSOLUTE = /^(?:[a-z]:[\\/]|\\\\)/i;

function hasControlCharacter(value: string): boolean {
  for (let index = 0; index < value.length; index++) {
    const code = value.charCodeAt(index);
    if (code <= 0x1f || code === 0x7f) return true;
  }
  return false;
}

export type FileLinkLocation = { path: string; line?: number };
export type KnownFilePaths = readonly string[] | ((basename: string) => readonly string[]);
export type FileLinkResolution =
  | ({ kind: 'resolved' } & FileLinkLocation)
  | { kind: 'unresolved'; reason: string };

export function isAbsoluteFilePath(path: string): boolean {
  return path.startsWith('/') || /^~[\\/]/.test(path) || WINDOWS_ABSOLUTE.test(path);
}

export function localFileBasename(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() || path || 'Artifact';
}

export function isLocalFileReference(value: string): boolean {
  return (
    /^file:\/\//i.test(value) ||
    WINDOWS_ABSOLUTE.test(value) ||
    (!value.startsWith('#') && !/^[a-z][a-z\d+.-]*:/i.test(value)) ||
    /^[^/\\:]+\.[a-z\d]+:\d+(?::\d+)?$/i.test(value)
  );
}

/** Link syntax is parsed before decoding, so `%23L42` remains a filename. */
export function parseFileLink(value: string): FileLinkLocation | null {
  let rawPath = value.trim();
  if (!rawPath || hasControlCharacter(rawPath)) return null;
  let rawLine: string | undefined;

  if (/^file:\/\//i.test(rawPath)) {
    try {
      const url = new URL(rawPath);
      if ((url.hostname && url.hostname !== 'localhost') || url.search) return null;
      if (url.hash) {
        const match = /^#L(\d+)$/.exec(url.hash);
        if (!match) return null;
        rawLine = match[1];
      }
      rawPath = url.pathname;
      if (/^\/[a-z]:\//i.test(rawPath)) rawPath = rawPath.slice(1);
    } catch {
      return null;
    }
  }

  const location = /(?::(\d+)|#L(\d+))$/.exec(rawPath);
  if (location) {
    if (rawLine) return null;
    rawLine = location[1] ?? location[2];
    rawPath = rawPath.slice(0, location.index);
    if (/(?::\d+|#L\d+)$/.test(rawPath)) return null;
  }
  if (!WINDOWS_ABSOLUTE.test(rawPath) && /^[a-z][a-z\d+.-]*:/i.test(rawPath)) return null;

  let path = rawPath;
  try {
    path = decodeURIComponent(rawPath);
  } catch {
    // A literal percent sign is a valid filesystem character.
  }
  if (!path || hasControlCharacter(path)) return null;
  if (rawLine) {
    const line = Number(rawLine);
    if (!Number.isSafeInteger(line) || line < 1) return null;
    return { path, line };
  }
  return { path };
}

/** Resolve filesystem text, not URL syntax. Absolute identities are untouched. */
export function resolveLocalFilePath(path: string, workingDir?: string): string | null {
  if (!path || hasControlCharacter(path)) return null;
  if (isAbsoluteFilePath(path)) return path;
  if (!workingDir || !isAbsoluteFilePath(workingDir)) return null;
  const parts: string[] = [];
  for (const part of path.split(/[\\/]/)) {
    if (!part || part === '.') continue;
    if (part === '..') {
      if (!parts.length) return null;
      parts.pop();
    } else {
      parts.push(part);
    }
  }
  if (!parts.length) return null;
  const separator = WINDOWS_ABSOLUTE.test(workingDir) ? '\\' : '/';
  return `${workingDir.replace(/[\\/]+$/, '')}${separator}${parts.join(separator)}`;
}

export function resolveFileLink(
  value: string,
  workingDir?: string,
  knownPaths: KnownFilePaths = []
): FileLinkResolution {
  const parsed = parseFileLink(value);
  if (!parsed) return { kind: 'unresolved', reason: 'This file reference is not supported.' };
  if (isAbsoluteFilePath(parsed.path)) return { kind: 'resolved', ...parsed };

  const relativeParts = parsed.path.split(/[\\/]/);
  if (relativeParts.includes('..')) {
    const path = resolveLocalFilePath(parsed.path, workingDir);
    return path
      ? { kind: 'resolved', ...parsed, path }
      : { kind: 'unresolved', reason: 'This relative file reference leaves its workspace.' };
  }

  if (relativeParts.length === 1 && parsed.path !== '.') {
    const matches =
      typeof knownPaths === 'function'
        ? knownPaths(parsed.path)
        : [...new Set(knownPaths)].filter((path) => localFileBasename(path) === parsed.path);
    if (matches.length > 1) {
      return {
        kind: 'unresolved',
        reason: 'This file name matches multiple earlier artifacts. Use its full path.',
      };
    }
    if (matches.length === 1) return { kind: 'resolved', ...parsed, path: matches[0] };
  }

  const path = resolveLocalFilePath(parsed.path, workingDir);
  return path
    ? { kind: 'resolved', ...parsed, path }
    : {
        kind: 'unresolved',
        reason: 'The session workspace is not available for this relative file.',
      };
}
