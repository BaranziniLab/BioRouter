/**
 * Fail a bare `electron-forge package` / `make` with an actionable message.
 *
 * The release path never needs this: every `bundle:*` script runs
 * `prepare-platform-binaries.js` first, which BUILDS `src/web` and then
 * validates it. But `npm run package` and `npm run make` are the entry points a
 * developer reaches for directly, and they skip that preparer — so on a clean
 * checkout they die inside forge's copy step with
 *
 *     ENOENT: no such file or directory, lstat 'src/web'
 *
 * which names neither the cause (the browser interface bundle was never built)
 * nor the fix. `src/web` is an `extraResource` in forge.config.ts, so forge
 * fails on it long after the vite build has already run — the error arrives
 * minutes in and reads like a packaging bug.
 *
 * This only reports. It deliberately does NOT build the bundle: the preparer
 * owns that, and a second builder would be a second answer to "is the bundle
 * current".
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const desktopDir = path.join(path.dirname(fileURLToPath(import.meta.url)), '..');

/** The same three-part test `prepare-platform-binaries.js` applies: an
 *  index.html with no assets/ beside it serves a blank page, so "present" is
 *  not the question — "usable" is. */
function webBundleIsUsable(webDir) {
  const index = path.join(webDir, 'index.html');
  const assets = path.join(webDir, 'assets');
  try {
    return (
      fs.existsSync(index) &&
      fs.statSync(index).size > 0 &&
      fs.existsSync(assets) &&
      fs.readdirSync(assets).length > 0
    );
  } catch {
    return false;
  }
}

const webDir = path.join(desktopDir, 'src', 'web');
if (!webBundleIsUsable(webDir)) {
  console.error('\n❌ Cannot package: the browser interface bundle is missing or empty.');
  console.error(`   ${webDir}`);
  console.error('\nforge.config.ts ships it as an extraResource, and `biorouterd` serves it');
  console.error('from `<exe dir>/../web` for `biorouter serve`.\n');
  console.error('Build it, then re-run:');
  console.error('  npm run build:web');
  console.error('\nOr use the release entry point, which builds it for you:');
  console.error('  npm run bundle:default        (macOS arm64)');
  console.error('');
  process.exit(1);
}
