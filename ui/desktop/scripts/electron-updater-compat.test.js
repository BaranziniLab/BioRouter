#!/usr/bin/env node
/*
 * Integration test: prove the latest-mac.yml we generate is consumed correctly
 * by the REAL electron-updater code (not a mock). We feed our manifest through
 * electron-updater's actual `parseUpdateInfo` / `resolveFiles` / `findFile`, and
 * replicate MacUpdater's exact architecture filter, asserting:
 *   • the manifest parses into a valid UpdateInfo with both arch files,
 *   • GitHub asset URLs resolve to the right download paths,
 *   • an arm64 client selects the arm64 zip and an x64 client the x64 zip,
 *   • every selected file carries the sha512 electron-updater verifies against.
 *
 * Run: node scripts/electron-updater-compat.test.js
 */
const assert = require('assert');
const fs = require('fs');
const os = require('os');
const path = require('path');
const crypto = require('crypto');

const {
  parseUpdateInfo,
  resolveFiles,
  findFile,
} = require('electron-updater/out/providers/Provider.js');
const { buildMacManifest } = require('./generate-update-manifests.js');

let passed = 0;
function test(name, fn) {
  fn();
  passed++;
  console.log(`  ✓ ${name}`);
}

// Replica of MacUpdater's arch filter (out/MacUpdater.js): arm64 macs take the
// arm64 file when present; everyone else takes the non-arm64 file. The ZIP is
// then chosen by findFile(files, "zip", ["pkg","dmg"]).
function isArm64File(file) {
  return file.url.pathname.includes('arm64') || (file.info.url && file.info.url.includes('arm64'));
}
function selectMacZip(resolved, isArm64Mac) {
  let files = resolved;
  if (isArm64Mac && files.some(isArm64File)) {
    files = files.filter((f) => isArm64Mac === isArm64File(f));
  } else {
    files = files.filter((f) => !isArm64File(f));
  }
  return findFile(files, 'zip', ['pkg', 'dmg']);
}

// Build a real manifest from on-disk zip fixtures.
const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'br-eu-'));
const armPath = path.join(tmp, 'BioRouter-darwin-arm64-1.86.0.zip');
const x64Path = path.join(tmp, 'BioRouter-darwin-x64-1.86.0.zip');
const armBytes = Buffer.from('arm64-app-archive');
const x64Bytes = Buffer.from('x64-app-archive-bytes');
fs.writeFileSync(armPath, armBytes);
fs.writeFileSync(x64Path, x64Bytes);
const expectArmSha = crypto.createHash('sha512').update(armBytes).digest('base64');
const expectX64Sha = crypto.createHash('sha512').update(x64Bytes).digest('base64');

const { yaml } = buildMacManifest({
  version: '1.86.0',
  arm64Zip: armPath,
  x64Zip: x64Path,
  releaseDate: '2026-06-20T00:00:00.000Z',
});

const baseUrl = new URL(
  'https://github.com/BaranziniLab/biorouter/releases/download/v1.86.0/'
);

console.log('electron-updater manifest compatibility');

let info;
test('real parseUpdateInfo() parses our latest-mac.yml', () => {
  info = parseUpdateInfo(yaml, 'latest-mac.yml', baseUrl.href);
  assert.strictEqual(info.version, '1.86.0');
  assert.strictEqual(info.files.length, 2);
  assert.ok(info.path && info.sha512, 'legacy path/sha512 present');
});

let resolved;
test('real resolveFiles() yields correct GitHub asset URLs', () => {
  resolved = resolveFiles(info, baseUrl);
  const urls = resolved.map((r) => r.url.href);
  assert.ok(
    urls.includes(`${baseUrl.href}BioRouter-darwin-arm64-1.86.0.zip`),
    'arm64 url resolved'
  );
  assert.ok(
    urls.includes(`${baseUrl.href}BioRouter-darwin-x64-1.86.0.zip`),
    'x64 url resolved'
  );
});

test('every resolved file carries an sha512 checksum (verification gate)', () => {
  for (const r of resolved) {
    assert.ok(r.info.sha512, `sha512 present for ${r.url.pathname}`);
  }
  const arm = resolved.find((r) => r.url.pathname.includes('arm64'));
  const x64 = resolved.find((r) => !r.url.pathname.includes('arm64'));
  assert.strictEqual(arm.info.sha512, expectArmSha);
  assert.strictEqual(x64.info.sha512, expectX64Sha);
});

test('arm64 Mac selects the arm64 zip (MacUpdater arch filter)', () => {
  const chosen = selectMacZip(resolved, /* isArm64Mac */ true);
  assert.ok(chosen.url.pathname.endsWith('BioRouter-darwin-arm64-1.86.0.zip'));
  assert.strictEqual(chosen.info.sha512, expectArmSha);
});

test('Intel Mac selects the x64 zip (MacUpdater arch filter)', () => {
  const chosen = selectMacZip(resolved, /* isArm64Mac */ false);
  assert.ok(chosen.url.pathname.endsWith('BioRouter-darwin-x64-1.86.0.zip'));
  assert.strictEqual(chosen.info.sha512, expectX64Sha);
});

test('a tampered checksum is detectable (manifest is the source of truth)', () => {
  // Simulate a corrupted download: recompute the hash of altered bytes and
  // confirm it no longer matches the manifest's sha512 (what electron-updater
  // compares before installing).
  const tamperedSha = crypto
    .createHash('sha512')
    .update(Buffer.concat([armBytes, Buffer.from('x')]))
    .digest('base64');
  const arm = resolved.find((r) => r.url.pathname.includes('arm64'));
  assert.notStrictEqual(tamperedSha, arm.info.sha512);
});

fs.rmSync(tmp, { recursive: true, force: true });
console.log(`\n${passed} passed`);
