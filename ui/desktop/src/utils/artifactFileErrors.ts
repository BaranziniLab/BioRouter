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

/**
 * ENOENT for a path whose only evidence was assistant prose.
 *
 * The generic ENOENT copy asserts the file once existed ("moved, renamed, or
 * deleted"), which for a suggested path — "write the spec to
 * `~/Desktop/spec.md` and tell me the path" — is simply untrue and sends the
 * reader looking through their Trash for a file that has never been created.
 * `ArtifactSource.mentionedOnly` is only still set when NOTHING confirmed the
 * path: no tool call wrote it, and the panel's existence gate never found it
 * (it clears the flag when it does). So this claim is backed by the transcript,
 * not guessed from the read failure.
 */
export const NEVER_CREATED_MESSAGE =
  "This file doesn't exist. The assistant mentioned this path but never created it.";

/**
 * The message to show for a failed artifact-file read, given what is known
 * about where the path came from.
 *
 * Provenance only ever overrides ENOENT. A permission error, a directory, or an
 * allowlist refusal says something true about the path regardless of who named
 * it, and must not be replaced by a claim about creation.
 */
export function artifactFileErrorMessage(
  file: { error: string; code?: string },
  options: { mentionedOnly?: boolean } = {}
): string {
  if (options.mentionedOnly && file.code === 'ENOENT') return NEVER_CREATED_MESSAGE;
  return file.error;
}
