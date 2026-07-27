/**
 * Friendly messages for artifact-file read failures (#36).
 *
 * The artifact panel opens files the agent named in its output, so a file that
 * has since been moved, renamed, or deleted is an EXPECTED state — not an
 * internal error. The raw Node message ("ENOENT: no such file or directory,
 * stat '/…'") must never reach the panel; map the errno-style `code` to a
 * sentence a person can act on, and keep the code as structured data so the
 * viewer can pick an icon.
 *
 * Unrecognised (or absent) codes fall back to the caller's message untouched:
 * our own thrown errors ("Access denied: …") carry no `code` and are already
 * written for people.
 */

export type FriendlyArtifactFileError = {
  message: string;
  /** The recognised errno-style code (e.g. 'ENOENT'), absent for fallbacks. */
  code?: string;
};

export function friendlyArtifactFileError(
  code: string | undefined,
  fallback: string
): FriendlyArtifactFileError {
  switch (code) {
    case 'ENOENT':
      return {
        message: "This file was moved, renamed, or deleted, so it can't be previewed anymore.",
        code,
      };
    case 'EACCES':
    case 'EPERM':
      return {
        message: "Biorouter doesn't have permission to read this file.",
        code,
      };
    case 'EISDIR':
    case 'ENOTDIR':
      return {
        message: "This path isn't a previewable file.",
        code,
      };
    default:
      return { message: fallback };
  }
}
