import path from 'node:path';
import os from 'node:os';

/**
 * Expands tilde (~) to the user's home directory
 * @param filePath - The file path that may contain tilde
 * @returns The expanded path with tilde replaced by home directory
 */
export function expandTilde(filePath: string): string {
  if (!filePath || typeof filePath !== 'string') return filePath;
  // Support "~", "~/..." and "~\\..." on Windows
  if (filePath === '~') {
    return os.homedir();
  }
  if (filePath.startsWith('~/') || (process.platform === 'win32' && filePath.startsWith('~\\'))) {
    // Remove the leading "~" and any separator that follows, then join
    const remainder = filePath.slice(2);
    return path.join(os.homedir(), remainder);
  }
  if (filePath.startsWith('~')) {
    // Generic fallback: replace only the first leading tilde
    return path.join(os.homedir(), filePath.slice(1));
  }
  return filePath;
}

/**
 * Recover a `~/…` path that was never a home path.
 *
 * Models habitually write `~/` in front of a path, and when the chat's working
 * directory is itself outside the home tree that habit produces a link to a
 * file that does not exist. A session working in `/ws/projects/ucsf/ic` gets
 * `~/ws/projects/ucsf/ic/figure.png`, which [`expandTilde`] faithfully turns
 * into `$HOME/ws/projects/ucsf/ic/figure.png` — nothing is there, and the link
 * is dead. (The same response's other links, written without the tilde, are
 * fine, which is what makes the failure look arbitrary to the user.)
 *
 * So: if the home reading does not exist and dropping the tilde yields an
 * absolute path that does, the latter is unambiguously what was meant.
 *
 * ⚠ **The home reading always wins when it exists.** This only ever fires where
 * the current behaviour already resolves to nothing, so it cannot redirect a
 * path that works today, and it cannot invent access to a file that is not
 * there. Containment is still checked afterwards on whatever comes back — this
 * decides which path is *meant*, never whether it may be read.
 *
 * POSIX only. `~\ws\…` on Windows has no analogous absolute reading.
 */
export function reinterpretTildeAsAbsolute(
  original: string,
  expanded: string,
  exists: (candidate: string) => boolean
): string {
  if (process.platform === 'win32') return expanded;
  if (!original.startsWith('~/')) return expanded;
  if (exists(expanded)) return expanded;
  const asAbsolute = path.posix.resolve('/', original.slice(2));
  return exists(asAbsolute) ? asAbsolute : expanded;
}
