#!/usr/bin/env node
/**
 * Assemble `Biorouter Authentication.app` from the `biorouter-authprompt`
 * binary, into `src/bin/` so `extraResource` carries it into
 * `Biorouter.app/Contents/Resources/`.
 *
 * ⚠ **Why an .app bundle at all, when the payload is one small executable.**
 * The daemon launches this through `/usr/bin/open`, and `open` takes bundles,
 * not bare executables. That is not incidental packaging taste — it is the
 * whole mechanism: `open` hands the launch to LaunchServices, which starts the
 * process under **launchd** rather than under the caller, and that is what lets
 * the prompt appear at all. A daemon started by the desktop app cannot raise
 * `LAContext.evaluatePolicy` itself; nor can Electron's own main process, even
 * holding a visible focused window. See the `biorouter-authprompt` crate root
 * for the measurements.
 *
 * ⚠ **`LSUIElement` is required.** Without it the helper takes focus and puts a
 * second icon in the Dock every time anyone declassifies a chat. With it, the
 * process can still present the system dialog — which is exactly the property
 * this whole design turns on — while remaining invisible otherwise.
 *
 * macOS only, and a no-op elsewhere: there is no prompt to raise on a platform
 * whose authentication already works in-process.
 */
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const desktop = path.resolve(here, '..');
const repo = path.resolve(desktop, '../..');

if (process.platform !== 'darwin') {
  console.log('[auth-helper] not macOS — nothing to build');
  process.exit(0);
}

const arch = process.env.ELECTRON_ARCH === 'x64' ? 'x64' : 'arm64';
const target = arch === 'x64' ? 'x86_64-apple-darwin' : null;
const built = target
  ? path.join(repo, 'target', target, 'release', 'biorouter-authprompt')
  : path.join(repo, 'target', 'release', 'biorouter-authprompt');

if (!fs.existsSync(built)) {
  // Loud, not silent. A missing helper does not break the app — the daemon
  // falls back to the in-process call — but on a packaged build that fallback
  // never works, so shipping without it means shipping the bug this fixes.
  console.error(`[auth-helper] MISSING: ${built}`);
  // Name the exact command for THIS arch. A generic hint costs a round trip on
  // the Intel path, where the `--target` is the whole point.
  const cmd = target
    ? `cargo build --release --target ${target} -p biorouter-authprompt`
    : 'cargo build --release -p biorouter-authprompt';
  console.error(`[auth-helper] build it first:  ${cmd}`);
  process.exit(1);
}

const bundle = path.join(desktop, 'src', 'bin', 'Biorouter Authentication.app');
fs.rmSync(bundle, { recursive: true, force: true });
fs.mkdirSync(path.join(bundle, 'Contents', 'MacOS'), { recursive: true });

fs.writeFileSync(
  path.join(bundle, 'Contents', 'Info.plist'),
  `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key><string>biorouter-authprompt</string>
  <key>CFBundleIdentifier</key><string>edu.ucsf.biorouter.authprompt</string>
  <key>CFBundleName</key><string>Biorouter Authentication</string>
  <key>CFBundleDisplayName</key><string>Biorouter</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${process.env.npm_package_version ?? '1.0.0'}</string>
  <key>LSUIElement</key><true/>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
</dict>
</plist>
`
);

fs.copyFileSync(built, path.join(bundle, 'Contents', 'MacOS', 'biorouter-authprompt'));
fs.chmodSync(path.join(bundle, 'Contents', 'MacOS', 'biorouter-authprompt'), 0o755);

// Ad-hoc now so the bundle is launchable in a dev tree; forge re-signs the whole
// app (this bundle included) with the Developer ID when APPLE_ID is set.
try {
  execFileSync('/usr/bin/codesign', ['--force', '--sign', '-', bundle], { stdio: 'pipe' });
} catch (err) {
  console.warn(`[auth-helper] adhoc sign failed (forge will re-sign): ${err.message}`);
}

console.log(`[auth-helper] built ${path.relative(desktop, bundle)} (${arch})`);
