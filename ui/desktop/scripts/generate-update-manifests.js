#!/usr/bin/env node
/*
 * generate-update-manifests.js
 *
 * Produces the `latest-mac.yml` manifest that `electron-updater` needs to do a
 * one-click, in-place macOS auto-update from a GitHub release.
 *
 * Background: our releases ship signed + notarized `.dmg` installers plus the
 * `maker-zip` app archives (`BioRouter-darwin-<arch>-<ver>.zip`). electron-
 * updater's macOS path (Squirrel.Mac) installs from the ZIP, but only if the
 * release also carries a `latest-mac.yml` listing each ZIP with its base64
 * SHA-512 and byte size. Without it, `autoUpdater.checkForUpdates()` 404s and
 * the app falls back to the assisted "download to ~/Downloads" flow.
 *
 * electron-updater (6.x) selects the architecture-appropriate ZIP by checking
 * whether the filename contains "arm64" (see MacUpdater `isArm64`), so a single
 * manifest listing both the arm64 and x64 ZIPs serves both Apple Silicon and
 * Intel clients.
 *
 * Windows (plain .zip) and Linux (.deb/.rpm) have no electron-updater in-place
 * installer, so no manifest is emitted for them — they use the assisted
 * GitHub-download fallback in src/utils/githubUpdater.ts.
 *
 * Usage:
 *   node scripts/generate-update-manifests.js \
 *     --version 1.85.5 \
 *     --arm64-zip out/make/zip/darwin/arm64/BioRouter-darwin-arm64-1.85.5.zip \
 *     --x64-zip   out/make/zip/darwin/x64/BioRouter-darwin-x64-1.85.5.zip \
 *     --out out/make \
 *     [--release-date 2026-06-20T00:00:00.000Z]
 *
 * Either --arm64-zip or --x64-zip may be omitted (e.g. a single-arch build);
 * at least one is required.
 */

const fs = require('fs');
const path = require('path');
const crypto = require('crypto');

/** base64-encoded SHA-512 of a file (the digest format electron-updater uses). */
function sha512Base64(filePath) {
  const hash = crypto.createHash('sha512');
  hash.update(fs.readFileSync(filePath));
  return hash.digest('base64');
}

function fileEntry(filePath) {
  const stat = fs.statSync(filePath);
  return {
    url: path.basename(filePath),
    sha512: sha512Base64(filePath),
    size: stat.size,
  };
}

/**
 * Build the latest-mac.yml content for the given arch zips.
 * @param {{ version: string, arm64Zip?: string, x64Zip?: string, releaseDate?: string }} opts
 * @returns {{ yaml: string, manifest: object }}
 */
function buildMacManifest({ version, arm64Zip, x64Zip, releaseDate }) {
  if (!version) throw new Error('version is required');
  const files = [];
  if (arm64Zip) files.push(fileEntry(arm64Zip));
  if (x64Zip) files.push(fileEntry(x64Zip));
  if (files.length === 0) throw new Error('at least one of arm64Zip / x64Zip is required');

  // `path`/top-level sha512 are legacy fields; electron-updater downloads from
  // the arch-filtered `files` list. Point them at the first (arm64-preferred).
  const primary = files[0];
  const date = releaseDate || new Date().toISOString();

  const manifest = {
    version,
    files,
    path: primary.url,
    sha512: primary.sha512,
    releaseDate: date,
  };

  // Hand-rendered YAML matching electron-builder's output shape (2-space
  // indent, hyphenated file list). Avoids a js-yaml dependency in the build.
  const lines = [];
  lines.push(`version: ${version}`);
  lines.push('files:');
  for (const f of files) {
    lines.push(`  - url: ${f.url}`);
    lines.push(`    sha512: ${f.sha512}`);
    lines.push(`    size: ${f.size}`);
  }
  lines.push(`path: ${primary.url}`);
  lines.push(`sha512: ${primary.sha512}`);
  lines.push(`releaseDate: '${date}'`);
  const yaml = lines.join('\n') + '\n';

  return { yaml, manifest };
}

function parseArgs(argv) {
  const args = {};
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a.startsWith('--')) {
      const key = a.slice(2);
      const val = argv[i + 1] && !argv[i + 1].startsWith('--') ? argv[++i] : 'true';
      args[key] = val;
    }
  }
  return args;
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const version = args.version;
  const outDir = args.out || 'out/make';
  const arm64Zip = args['arm64-zip'];
  const x64Zip = args['x64-zip'];
  const releaseDate = args['release-date'];

  if (!version) {
    console.error('error: --version is required');
    process.exit(1);
  }
  for (const [label, p] of [
    ['--arm64-zip', arm64Zip],
    ['--x64-zip', x64Zip],
  ]) {
    if (p && !fs.existsSync(p)) {
      console.error(`error: ${label} not found: ${p}`);
      process.exit(1);
    }
  }

  const { yaml } = buildMacManifest({ version, arm64Zip, x64Zip, releaseDate });
  fs.mkdirSync(outDir, { recursive: true });
  const outPath = path.join(outDir, 'latest-mac.yml');
  fs.writeFileSync(outPath, yaml);
  console.log(`wrote ${outPath}`);
  console.log(yaml);
}

if (require.main === module) {
  main();
}

module.exports = { buildMacManifest, sha512Base64, fileEntry };
