import fsSync from 'node:fs';
import path from 'node:path';

/**
 * Pure containment logic behind the main process's file-access allowlist
 * (`isAllowedFilePath` in main.ts). Split out so it can be unit-tested without
 * importing the Electron main module.
 *
 * Canonicalization matters in BOTH directions. macOS's `/tmp` → `/private/tmp`
 * and `/var` → `/private/var` symlinks defeat naive prefix matching: `/tmp/x`
 * never string-matches a `/private/tmp` root (false DENIAL — observed live as
 * "Access denied: path '/tmp/qa-r1b/hi.txt' is outside allowed directories"
 * for a file the session's own tool call had just written), and a symlink
 * planted inside an allowed root can point outside it (false ADMIT).
 */
export function canonicalizeForContainment(p: string): string {
  try {
    return fsSync.realpathSync(p);
  } catch {
    // Path may not exist yet (e.g. a write target): canonicalize the deepest
    // existing ancestor and re-append the remainder lexically.
    const parent = path.dirname(p);
    if (parent === p) return p;
    return path.join(canonicalizeForContainment(parent), path.basename(p));
  }
}

/** True iff `candidate` (already path.resolve'd) lives under one of `roots`. */
export function isPathContained(candidate: string, roots: string[]): boolean {
  const canonical = canonicalizeForContainment(candidate);
  return roots.some((root) => {
    const canonicalRoot = canonicalizeForContainment(root);
    return canonical.startsWith(canonicalRoot + path.sep) || canonical === canonicalRoot;
  });
}
