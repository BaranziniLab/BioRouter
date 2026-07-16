#!/usr/bin/env node
/*
 * Standalone test for generate-update-manifests.js. Not part of the Vitest
 * suite (Vitest only scans src/); run directly:
 *   node scripts/generate-update-manifests.test.js
 *
 * Verifies the emitted latest-mac.yml matches the shape electron-updater's
 * MacUpdater parses: a `files` list with base64 SHA-512 + size for each arch
 * zip, plus the legacy top-level path/sha512.
 */
const assert = require('assert');
const fs = require('fs');
const os = require('os');
const path = require('path');
const crypto = require('crypto');
const { buildMacManifest, sha512Base64 } = require('./generate-update-manifests.js');

let passed = 0;
function test(name, fn) {
  fn();
  passed++;
  console.log(`  ✓ ${name}`);
}

const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'br-manifest-'));
const armPath = path.join(tmp, 'Biorouter-darwin-arm64-1.86.0.zip');
const x64Path = path.join(tmp, 'Biorouter-darwin-x64-1.86.0.zip');
const armBytes = Buffer.from('pretend-arm64-zip-contents');
const x64Bytes = Buffer.from('pretend-x64-zip-contents-longer');
fs.writeFileSync(armPath, armBytes);
fs.writeFileSync(x64Path, x64Bytes);

const expectArmSha = crypto.createHash('sha512').update(armBytes).digest('base64');
const expectX64Sha = crypto.createHash('sha512').update(x64Bytes).digest('base64');

console.log('generate-update-manifests');

test('sha512Base64 matches node crypto base64 digest', () => {
  assert.strictEqual(sha512Base64(armPath), expectArmSha);
});

test('manifest lists both arch zips with correct sha512 + size', () => {
  const { manifest } = buildMacManifest({
    version: '1.86.0',
    arm64Zip: armPath,
    x64Zip: x64Path,
    releaseDate: '2026-06-20T00:00:00.000Z',
  });
  assert.strictEqual(manifest.version, '1.86.0');
  assert.strictEqual(manifest.files.length, 2);

  const arm = manifest.files.find((f) => f.url.includes('arm64'));
  const x64 = manifest.files.find((f) => f.url.endsWith('x64-1.86.0.zip'));
  assert.ok(arm && x64, 'both arch entries present');
  assert.strictEqual(arm.url, 'Biorouter-darwin-arm64-1.86.0.zip');
  assert.strictEqual(arm.sha512, expectArmSha);
  assert.strictEqual(arm.size, armBytes.length);
  assert.strictEqual(x64.sha512, expectX64Sha);
  assert.strictEqual(x64.size, x64Bytes.length);

  // Legacy top-level fields point at the primary (arm64) entry.
  assert.strictEqual(manifest.path, 'Biorouter-darwin-arm64-1.86.0.zip');
  assert.strictEqual(manifest.sha512, expectArmSha);
  assert.strictEqual(manifest.releaseDate, '2026-06-20T00:00:00.000Z');
});

test('arch filenames are distinguishable (electron-updater isArm64 check)', () => {
  const { manifest } = buildMacManifest({ version: '1.86.0', arm64Zip: armPath, x64Zip: x64Path });
  const arm = manifest.files.find((f) => f.url.includes('arm64'));
  const x64 = manifest.files.find((f) => !f.url.includes('arm64'));
  // MacUpdater picks arm64 files by `url.pathname.includes('arm64')`.
  assert.ok(arm.url.includes('arm64'));
  assert.ok(!x64.url.includes('arm64'));
});

test('yaml is well-formed and parseable back to the same values', () => {
  const { yaml } = buildMacManifest({
    version: '1.86.0',
    arm64Zip: armPath,
    x64Zip: x64Path,
    releaseDate: '2026-06-20T00:00:00.000Z',
  });
  // Minimal structural assertions (no js-yaml dependency).
  assert.ok(yaml.startsWith('version: 1.86.0\n'));
  assert.ok(yaml.includes('files:\n'));
  assert.ok(yaml.includes('  - url: Biorouter-darwin-arm64-1.86.0.zip\n'));
  assert.ok(yaml.includes(`    sha512: ${expectArmSha}\n`));
  assert.ok(yaml.includes(`    size: ${armBytes.length}\n`));
  assert.ok(yaml.includes("releaseDate: '2026-06-20T00:00:00.000Z'\n"));
  assert.ok(yaml.endsWith('\n'));
});

test('single-arch build (arm64 only) still produces a valid manifest', () => {
  const { manifest } = buildMacManifest({ version: '1.86.0', arm64Zip: armPath });
  assert.strictEqual(manifest.files.length, 1);
  assert.strictEqual(manifest.path, 'Biorouter-darwin-arm64-1.86.0.zip');
});

test('throws when no zips provided', () => {
  assert.throws(() => buildMacManifest({ version: '1.86.0' }), /arm64Zip|x64Zip/);
});

test('throws when version missing', () => {
  assert.throws(() => buildMacManifest({ arm64Zip: armPath }), /version/);
});

fs.rmSync(tmp, { recursive: true, force: true });
console.log(`\n${passed} passed`);
