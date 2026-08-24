import { execFile } from 'node:child_process';
import os from 'node:os';
import { promises as fs } from 'node:fs';
import path from 'node:path';
import { promisify } from 'node:util';
import log from 'electron-log';

const run = promisify(execFile);

/**
 * HEIC previews, via the operating system's own decoder.
 *
 * **Why the OS and not a bundled library.** Every practical HEIC decoder is
 * libheif underneath, which is LGPL — manageable, but the harder problem is
 * that HEVC patents attach to the *technique*, not the implementation, so no
 * library choice removes them. Access Advance's own FAQ answers "software that
 * is free for users to download" with *"In general, HEVC software downloaded by
 * users requires a license."* The market agrees: `sharp` excludes HEIC from its
 * prebuilts and calls it patent-encumbered, `wasm-vips` compiles libheif with
 * the HEVC decoder switched off, and Debian ships it as a plugin that is not
 * installed by default.
 *
 * Decoding through a decoder the OS vendor has already licensed is the one
 * route that plausibly sidesteps that, and since HEIC files are overwhelmingly
 * iPhone and Mac artefacts it covers most real traffic for nothing — no bundle,
 * no copyleft, no build change.
 *
 * Elsewhere the panel says plainly that it cannot decode the format
 * (`describeUnsupportedFormat`), which is better than a half-answer. Shipping a
 * bundled decoder for those platforms is a real option, and one that needs a
 * licence decision rather than a commit.
 */

/** `sips` ships with macOS itself, not with Xcode. */
const SIPS = '/usr/bin/sips';

let supportsHeic: boolean | null = null;

/**
 * Whether this machine can convert HEIC.
 *
 * Probed once, at runtime, rather than assumed from the platform: the published
 * `sips` man page predates High Sierra and does not list HEIC among its formats
 * even though ImageIO has read it since 10.13, so the documentation is not a
 * reliable guide and the runtime answer is.
 */
export async function canConvertHeic(): Promise<boolean> {
  if (supportsHeic !== null) return supportsHeic;
  if (process.platform !== 'darwin') {
    supportsHeic = false;
    return false;
  }
  try {
    await fs.access(SIPS);
    supportsHeic = true;
  } catch {
    supportsHeic = false;
  }
  return supportsHeic;
}

/**
 * Converts a HEIC/HEIF file to PNG, returning the bytes.
 *
 * Returns `null` rather than throwing when conversion is unavailable or fails,
 * because the caller's fallback — an honest "this panel cannot decode HEIC"
 * card — is a better outcome than an error dialog.
 */
export async function heicToPng(filePath: string): Promise<Buffer | null> {
  if (!(await canConvertHeic())) return null;

  const outDir = await fs.mkdtemp(path.join(os.tmpdir(), 'biorouter-heic-'));
  const outPath = path.join(outDir, 'preview.png');
  try {
    // Fixed argv through execFile — never a shell — so a filename containing
    // quotes or semicolons is an argument and not a command.
    await run(SIPS, ['-s', 'format', 'png', filePath, '--out', outPath], { timeout: 20_000 });
    return await fs.readFile(outPath);
  } catch (error) {
    log.info('[heic] conversion failed:', error);
    return null;
  } finally {
    await fs.rm(outDir, { recursive: true, force: true }).catch(() => {});
  }
}
